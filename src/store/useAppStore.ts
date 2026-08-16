import { create } from 'zustand';
import type {
  AppConfig,
  Combination,
  HouseType,
  ManualInput,
  SolveRequest,
  SolveResult,
} from '../types';
import { invoke } from '../utils/tauri';
import { jsSolve } from '../utils/jsSolver';
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

interface AppState {
  houseTypes: HouseType[];
  combinations: Combination[];
  manualInputs: ManualInput[];
  solveResult: SolveResult | null;
  calculating: boolean;
  rendering: boolean; // 网格渲染动画进行中
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
  removeCombination: (id: string) => void;
  setCombinationItem: (comboId: string, typeId: string, count: number) => void;

  // 手动输入
  setManualQuantity: (combinationId: string, quantity: number) => void;
  clearManual: () => void;

  // 求解
  setAutoCalc: (v: boolean) => void;
  setRendering: (v: boolean) => void;
  solve: () => Promise<void>;

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

  return {
    houseTypes: [],
    combinations: [],
    manualInputs: [],
    solveResult: null,
    calculating: false,
    rendering: false,
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
          },
        ],
      })),

    updateCombination: (id, patch) =>
      set((s) => ({
        combinations: s.combinations.map((c) => (c.id === id ? { ...c, ...patch } : c)),
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
      set({ calculating: true, error: null });
      try {
        const result = await safeSolve(req);
        set({
          solveResult: result,
          calculating: false,
          lastSolvedBy: result.algorithm.includes('preview') ? 'js' : 'rust',
        });
      } catch (e) {
        set({ calculating: false, error: String(e) });
      }
    },

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
            items: [
              { typeId: 't50', count: 1 },
              { typeId: 't70', count: 2 },
              { typeId: 't90', count: 1 },
            ],
          },
          {
            id: 'cB',
            name: '组合B',
            items: [
              { typeId: 't120', count: 1 },
              { typeId: 't150', count: 1 },
            ],
          },
          {
            id: 'cC',
            name: '组合C',
            items: [
              { typeId: 't70', count: 1 },
              { typeId: 't90', count: 2 },
            ],
          },
        ],
        manualInputs: [{ combinationId: 'cA', quantity: 5 }],
        solveResult: null,
        error: null,
      }),
  };
});
