import { create } from 'zustand';
import type {
  AppConfig,
  Combination,
  EnumerateResponse,
  HouseType,
  ManualInput,
  SolveRequest,
  SolveResult,
} from '../types';
import { invoke } from '../utils/tauri';
import { jsSolve, jsEnumerate } from '../utils/jsSolver';
import { uid } from '../utils/grid';

// 带超时的 Rust 求解调用：超时或失败时降级到 JS 贪心算法，保证界面永不卡死
function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('solve timeout')), ms);
    p.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      },
    );
  });
}

// #4 修复：请求序列号。自动计算模式下防抖只能减少触发频率、不能保证串行
//（前一个 Rust 求解最长 5s 未返回时用户继续改输入会并发多个 solve），
// 序列号确保只接受"最新一次"请求的结果，过期结果直接丢弃，避免展示过期数据。
let solveSeq = 0;
let enumerateSeq = 0;

async function safeSolve(req: SolveRequest): Promise<SolveResult> {
  try {
    const result = await withTimeout(
      invoke<SolveResult>('solve_optimal', { request: req }),
      5000,
    );
    return result;
  } catch (e) {
    // 浏览器预览环境：无 Tauri IPC，使用 JS 贪心降级
    if (e instanceof Error && e.message === 'NOT_IN_TAURI') {
      return jsSolve(req);
    }
    // 求解超时：降级 JS 并标记（保证界面不卡死）
    if (e instanceof Error && e.message === 'solve timeout') {
      const r = jsSolve(req);
      return { ...r, algorithm: `${r.algorithm} (超时降级)` };
    }
    // 真实求解错误（如库存不足）：抛给上层展示，不再静默掩盖
    throw e;
  }
}

/** 遍历备选方案：优先 Rust ILP（精确），浏览器预览降级 JS（近似） */
async function safeEnumerate(req: SolveRequest): Promise<EnumerateResponse> {
  try {
    // 枚举可能多轮求解，给更宽松的超时（30s）
    // #10：透传 maxSolutions（Rust 端上限保护 clamp 1..200，默认 50）
    const result = await withTimeout(
      invoke<EnumerateResponse>('enumerate_solutions', { request: req, maxSolutions: 50 }),
      30000,
    );
    return result;
  } catch (e) {
    if (e instanceof Error && (e.message === 'NOT_IN_TAURI' || e.message === 'solve timeout')) {
      return jsEnumerate(req);
    }
    throw e;
  }
}

interface AppState {
  houseTypes: HouseType[];
  combinations: Combination[];
  manualInputs: ManualInput[];
  solveResult: SolveResult | null;
  calculating: boolean;
  rendering: boolean; // 网格渲染动画进行中
  // 备选方案遍历
  solutions: SolveResult[];
  activeSolutionIndex: number | null;
  enumerating: boolean;
  /** 枚举是否因超时截断（未完整遍历） */
  enumerateTruncated: boolean;
  autoCalc: boolean;
  error: string | null;
  lastSolvedBy: 'rust' | 'js' | null;

  // 户型操作
  addHouseType: (name: string, quantity: number) => void;
  updateHouseType: (id: string, patch: Partial<HouseType>) => void;
  removeHouseType: (id: string) => void;

  // 组合操作
  addCombination: (name: string, items: { typeId: string; count: number }[]) => void;
  updateCombination: (id: string, patch: Partial<Combination>) => void;
  /** 更新组合权重偏好（1-10） */
  updateComboWeight: (comboId: string, weight: number) => void;
  /** 更新组合数量区间（下限/上限，0 ≤ min ≤ max） */
  updateComboRange: (comboId: string, min: number, max: number) => void;
  removeCombination: (id: string) => void;
  setCombinationItem: (comboId: string, typeId: string, count: number) => void;

  // 手动输入（固定数量优先于区间）
  setManualQuantity: (combinationId: string, quantity: number) => void;
  clearManual: () => void;

  // 求解
  setAutoCalc: (v: boolean) => void;
  setRendering: (v: boolean) => void;
  solve: () => Promise<void>;
  // 备选方案
  enumerateSolutions: () => Promise<void>;
  selectSolution: (index: number) => void;

  // 持久化
  saveConfig: () => Promise<void>;
  loadConfig: () => Promise<void>;

  // 数据导入（演示样例）
  loadSample: () => void;
}

export const useAppStore = create<AppState>((set, get) => {
  const buildRequest = () => {
    const { houseTypes, combinations, manualInputs } = get();
    return {
      houseTypes,
      combinations,
      manualInputs: manualInputs.filter((m) => m.quantity > 0),
    };
  };

  // 预校验：所有组合 min 下界对应的最低房源需求是否超过总库存。
  // 手动固定数量的组合按固定值扣减后剩余库存参与校验。
  const validateMinStock = (): string | null => {
    const { houseTypes, combinations, manualInputs } = get();
    if (houseTypes.length === 0 || combinations.length === 0) return null;
    // 先扣除手动固定数量占用的库存
    const remaining = houseTypes.map((t) => t.quantity);
    for (const m of manualInputs) {
      if (m.quantity <= 0) continue;
      const c = combinations.find((x) => x.id === m.combinationId);
      if (!c) continue;
      for (const item of c.items) {
        const j = houseTypes.findIndex((t) => t.id === item.typeId);
        if (j >= 0) remaining[j] -= item.count * m.quantity;
      }
    }
    // 累加各组合 min 的房源需求（自动组合，跳过手动固定）
    const manualIds = new Set(manualInputs.filter((m) => m.quantity > 0).map((m) => m.combinationId));
    for (const c of combinations) {
      const min = c.min ?? 0;
      if (min <= 0 || manualIds.has(c.id)) continue;
      for (const item of c.items) {
        const j = houseTypes.findIndex((t) => t.id === item.typeId);
        if (j >= 0 && remaining[j] < item.count * min) {
          return `组合「${c.name}」的下限 ${min} 超出户型「${houseTypes[j].name}」的库存（剩余 ${Math.max(0, remaining[j])} 套）`;
        }
        if (j >= 0) remaining[j] -= item.count * min;
      }
    }
    return null;
  };

  return {
    houseTypes: [],
    combinations: [],
    manualInputs: [],
    solveResult: null,
    calculating: false,
    rendering: false,
    solutions: [],
    activeSolutionIndex: null,
    enumerating: false,
    enumerateTruncated: false,
    autoCalc: true,
    error: null,
    lastSolvedBy: null,

    addHouseType: (name, quantity) =>
      set((s) => ({
        houseTypes: [...s.houseTypes, { id: uid('t'), name, quantity }],
      })),

    updateHouseType: (id, patch) =>
      set((s) => ({
        houseTypes: s.houseTypes.map((t) => (t.id === id ? { ...t, ...patch } : t)),
      })),

    removeHouseType: (id) =>
      set((s) => ({
        houseTypes: s.houseTypes.filter((t) => t.id !== id),
        // 同时清理组合中对已删除户型的引用
        combinations: s.combinations.map((c) => ({
          ...c,
          items: c.items.filter((i) => i.typeId !== id),
        })),
      })),

    addCombination: (name, items) =>
      set((s) => ({
        combinations: [
          ...s.combinations,
          {
            id: uid('c'),
            name: name || `组合${String.fromCharCode(65 + s.combinations.length)}`,
            items: items.map((i) => ({ ...i })),
            weight: 5,
            min: 0,
            max: 999,
          },
        ],
      })),

    updateCombination: (id, patch) =>
      set((s) => ({
        combinations: s.combinations.map((c) => (c.id === id ? { ...c, ...patch } : c)),
      })),

    updateComboWeight: (comboId, weight) =>
      set((s) => ({
        combinations: s.combinations.map((c) =>
          c.id === comboId ? { ...c, weight: Math.min(10, Math.max(1, Math.round(weight))) } : c,
        ),
      })),

    // 数量区间：保证 0 ≤ min ≤ max（输入钳制由 UI 处理，此处兜底规范化）
    updateComboRange: (comboId, min, max) =>
      set((s) => ({
        combinations: s.combinations.map((c) => {
          if (c.id !== comboId) return c;
          const lo = Math.max(0, Math.min(Math.round(min || 0), Math.round(max ?? 999)));
          const hi = Math.max(lo, Math.round(max ?? 999));
          return { ...c, min: lo, max: hi };
        }),
      })),

    removeCombination: (id) =>
      set((s) => ({
        combinations: s.combinations.filter((c) => c.id !== id),
        manualInputs: s.manualInputs.filter((m) => m.combinationId !== id),
      })),

    setCombinationItem: (comboId, typeId, count) =>
      set((s) => ({
        combinations: s.combinations.map((c) => {
          if (c.id !== comboId) return c;
          const existing = c.items.find((i) => i.typeId === typeId);
          let items: typeof c.items;
          if (count <= 0) {
            items = c.items.filter((i) => i.typeId !== typeId);
          } else if (existing) {
            items = c.items.map((i) => (i.typeId === typeId ? { ...i, count } : i));
          } else {
            items = [...c.items, { typeId, count }];
          }
          return { ...c, items };
        }),
      })),

    setManualQuantity: (combinationId, quantity) =>
      set((s) => {
        const others = s.manualInputs.filter((m) => m.combinationId !== combinationId);
        if (quantity <= 0) return { manualInputs: others };
        return { manualInputs: [...others, { combinationId, quantity }] };
      }),

    clearManual: () => set({ manualInputs: [] }),

    setAutoCalc: (v) => set({ autoCalc: v }),
    setRendering: (v) => set({ rendering: v }),

    solve: async () => {
      const req = buildRequest();
      // 无数据时不计算；组合未完整定义时由求解器自动跳过（数量=0），不影响其他组合
      if (req.houseTypes.length === 0 || req.combinations.length === 0) {
        set({ solveResult: null, calculating: false });
        return;
      }
      // 预校验：所有组合 min 下界的最低房源需求不得超出总库存（弹窗阻止计算）
      const invalid = validateMinStock();
      if (invalid) {
        set({ calculating: false, error: invalid });
        window.alert(invalid);
        return;
      }
      // #4：登记本次请求序列号，仅最新请求的结果可写入状态
      const seq = ++solveSeq;
      // 输入变化后旧的备选方案已失效，清空（避免展示过期数据）
      set({
        calculating: true,
        error: null,
        solutions: [],
        activeSolutionIndex: null,
        enumerateTruncated: false,
      });
      try {
        const result = await safeSolve(req);
        if (seq !== solveSeq) return; // 过期结果：丢弃（更新的请求在途/已完成）
        // 判定求解引擎：JS 预览（algorithm 含 'preview'）或 JS 超时降级（含 '超时降级'，
        // 仅 JS 路径会追加该标记）均归为 'js'；Rust ILP / Rust 贪心兜底归为 'rust'。
        // 注意：不能仅凭 'greedy' 判定，否则会把 Rust 原生贪心兜底误判为 JS。
        const isJs =
          result.algorithm.includes('preview') || result.algorithm.includes('超时降级');
        set({
          solveResult: result,
          calculating: false,
          lastSolvedBy: isJs ? 'js' : 'rust',
        });
      } catch (e) {
        if (seq !== solveSeq) return;
        // #6：保留可读错误消息（Error 取 message，其余兜底 String）
        const msg = e instanceof Error ? e.message : String(e);
        set({ calculating: false, error: msg });
      }
    },

    enumerateSolutions: async () => {
      const req = buildRequest();
      if (req.houseTypes.length === 0 || req.combinations.length === 0) {
        set({ error: '请先添加房源类型和组合', enumerating: false });
        return;
      }
      // 预校验：与单方案求解一致（弹窗阻止）
      const invalid = validateMinStock();
      if (invalid) {
        set({ enumerating: false, error: invalid });
        window.alert(invalid);
        return;
      }
      // #4：枚举同样只接受最新一次请求的结果
      const seq = ++enumerateSeq;
      set({ enumerating: true, error: null });
      try {
        const resp = await safeEnumerate(req);
        if (seq !== enumerateSeq) return;
        set({
          solutions: resp.solutions,
          activeSolutionIndex: resp.solutions.length > 0 ? 0 : null,
          enumerateTruncated: resp.truncated,
          enumerating: false,
        });
        // 首次即无可行解：明确提示且不生成标签
        if (resp.solutions.length === 0) {
          set({ error: '未找到可行方案' });
        }
      } catch (e) {
        if (seq !== enumerateSeq) return;
        // 异常时保留已保存的方案，仅提示错误（#6 可读消息）
        const msg = e instanceof Error ? e.message : String(e);
        set({ enumerating: false, error: msg });
      }
    },

    selectSolution: (index) => set({ activeSolutionIndex: index }),

    saveConfig: async () => {
      const { houseTypes, combinations } = get();
      const config: AppConfig = { houseTypes, combinations };
      try {
        await invoke('save_config', { config });
      } catch {
        // 浏览器环境静默忽略
      }
    },

    loadConfig: async () => {
      try {
        const config = await invoke<AppConfig | null>('load_config');
        if (config) {
          set({
            houseTypes: config.houseTypes ?? [],
            combinations: config.combinations ?? [],
            manualInputs: [],
          });
        }
      } catch {
        // 无配置或浏览器环境忽略
      }
    },

    loadSample: () =>
      set({
        houseTypes: [
          { id: 't50', name: '50㎡', quantity: 20 },
          { id: 't70', name: '70㎡', quantity: 30 },
          { id: 't90', name: '90㎡', quantity: 25 },
          { id: 't120', name: '120㎡', quantity: 15 },
          { id: 't150', name: '150㎡', quantity: 10 },
        ],
        combinations: [
          {
            id: 'cA',
            name: '组合A',
            weight: 5,
            items: [
              { typeId: 't50', count: 1 },
              { typeId: 't70', count: 2 },
              { typeId: 't90', count: 1 },
            ],
          },
          {
            id: 'cB',
            name: '组合B',
            weight: 5,
            items: [
              { typeId: 't120', count: 1 },
              { typeId: 't150', count: 1 },
            ],
          },
          {
            id: 'cC',
            name: '组合C',
            weight: 5,
            items: [
              { typeId: 't70', count: 1 },
              { typeId: 't90', count: 2 },
            ],
          },
        ],
        manualInputs: [{ combinationId: 'cA', quantity: 5 }],
        solutions: [],
        activeSolutionIndex: null,
        solveResult: null,
        error: null,
      }),
  };
});
