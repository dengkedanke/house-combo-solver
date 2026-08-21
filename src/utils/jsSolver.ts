// 浏览器环境的降级求解器（贪心算法），用于 vite dev 无 Tauri 时预览。
// 生产环境由 Rust 后端 ILP 求解器提供精确解。
import type { EnumerateResponse, SolveRequest, SolveResult } from '../types';

function solveGreedyOneOrder(
  usage: number[][],
  remaining: number[],
  order: number[],
): { xs: number[]; used: number } {
  const n = usage.length;
  const rem = [...remaining];
  const xs = new Array(n).fill(0);
  for (const k of order) {
    const u = usage[k];
    // 跳过未完整定义的组合（全 0），否则 while 条件恒真导致死循环
    if (u.every((c) => c === 0)) continue;
    // 批量分配：一步算出该组合可分配的最大数量（最小需求比率）
    // #15：用 Infinity 作"未受限"哨兵，语义更清晰（Math.min 处理一致）
    let maxK = Infinity;
    for (let j = 0; j < rem.length; j++) {
      if (u[j] > 0) maxK = Math.min(maxK, Math.floor(rem[j] / u[j]));
    }
    if (maxK === Infinity) maxK = 0;
    if (maxK > 0) {
      for (let j = 0; j < rem.length; j++) rem[j] -= u[j] * maxK;
      xs[k] = maxK;
    }
  }
  const used = xs.reduce((acc, x, k) => acc + x * usage[k].reduce((s, c) => s + c, 0), 0);
  return { xs, used };
}

function xsVariance(xs: number[]): number {
  const n = xs.length;
  if (n === 0) return 0;
  const mean = xs.reduce((s, x) => s + x, 0) / n;
  return xs.reduce((s, x) => s + (x - mean) * (x - mean), 0);
}

function gcd(a: number, b: number): number {
  while (b !== 0) {
    const t = b;
    b = a % b;
    a = t;
  }
  return a;
}

// 均衡化后处理（与 Rust balance_solution 一致）：
// 保持总套数（最优值）不变，通过"总量守恒转移"使组合数量方差最小；尊重 lower 下界
function balanceSolution(
  usage: number[][],
  remaining: number[],
  totals: number[],
  lower: number[],
  xs: number[],
): void {
  const n = xs.length;
  if (n < 2) return;
  let improved = true;
  while (improved) {
    improved = false;
    for (let a = 0; a < n; a++) {
      if (xs[a] === 0 || totals[a] === 0) continue;
      for (let b = 0; b < n; b++) {
        if (a === b || totals[b] === 0) continue;
        const g_cd = gcd(totals[a], totals[b]);
        const lcm = (totals[a] / g_cd) * totals[b];
        const g = lcm / totals[a];
        const h = lcm / totals[b];
        if (g > xs[a]) continue;
        const newXa = xs[a] - g;
        const newXb = xs[b] + h;
        // 下界保护：转移后不得低于"≥1"等数量下界
        if (newXa < (lower[a] ?? 0)) continue;
        // 校验新解不违反任何户型库存约束
        let feasible = true;
        for (let j = 0; j < remaining.length; j++) {
          let totalUse = 0;
          for (let k = 0; k < n; k++) {
            const v = k === a ? newXa : k === b ? newXb : xs[k];
            totalUse += v * usage[k][j];
          }
          if (totalUse > remaining[j]) {
            feasible = false;
            break;
          }
        }
        if (!feasible) continue;
        const curSumSq = xs[a] * xs[a] + xs[b] * xs[b];
        const newSumSq = newXa * newXa + newXb * newXb;
        if (newSumSq < curSumSq) {
          xs[a] = newXa;
          xs[b] = newXb;
          improved = true;
        }
      }
    }
  }
}

export function jsSolve(req: SolveRequest): SolveResult {
  const start = performance.now();
  const typeIds = req.houseTypes.map((t) => t.id);
  const typeIdx = new Map(typeIds.map((id, i) => [id, i]));

  // 使用矩阵
  const usage = req.combinations.map((c) => {
    const row = new Array(typeIds.length).fill(0);
    for (const item of c.items) {
      const j = typeIdx.get(item.typeId);
      if (j !== undefined) row[j] = item.count;
    }
    return row;
  });

  // 手动输入
  const comboIdx = new Map(req.combinations.map((c, i) => [c.id, i]));
  const xs = new Array(req.combinations.length).fill(0);
  const manual = new Array(req.combinations.length).fill(false);
  for (const mi of req.manualInputs) {
    const k = comboIdx.get(mi.combinationId);
    if (k !== undefined) {
      xs[k] = mi.quantity;
      manual[k] = true;
    }
  }

  // 剩余库存
  const remaining = req.houseTypes.map((t, j) => {
    let used = 0;
    usage.forEach((row, k) => {
      used += xs[k] * row[j];
    });
    return Math.max(0, t.quantity - used);
  });

  // 自由组合贪心（尊重"≥1"下界：先预分配下界，再对剩余库存贪心）
  const freeIdx = req.combinations.map((_, i) => i).filter((i) => !manual[i]);
  const freeUsage = freeIdx.map((k) => usage[k]);
  // 自由组合下界（勾选 ≥1 的组合为 1）
  const lower = freeIdx.map((gi) =>
    req.minOneCombinationIds?.includes(req.combinations[gi].id) ? 1 : 0,
  );
  // 自由组合权重偏好（1-10，默认 5）
  const weights = freeIdx.map((gi) => {
    const w = req.combinations[gi].weight ?? 5;
    return Math.min(10, Math.max(1, Math.round(w)));
  });
  // 加权分数（分层优化阶段二的目标系数：1.0 + (w-1)*0.001）
  const weightCoeff = weights.map((w) => 1.0 + (w - 1) * 0.001);
  // 预分配下界并扣减库存
  const lowerXs = new Array(freeIdx.length).fill(0);
  const remAfterLower = [...remaining];
  freeIdx.forEach((_, i) => {
    if (lower[i] > 0) {
      for (let j = 0; j < remAfterLower.length; j++) {
        remAfterLower[j] -= freeUsage[i][j] * lower[i];
      }
      lowerXs[i] = lower[i];
    }
  });

  let best = { xs: freeIdx.map(() => 0), used: 0, variance: Infinity, score: -Infinity };
  if (freeIdx.length > 0) {
    const orders: number[][] = [];
    const totals = freeUsage.map((u) => u.reduce((s, c) => s + c, 0));
    // freeIdx 元素是全局组合索引，而 totals 按 freeUsage 下标排列，
    // 必须通过 pos 映射，否则存在手动输入时索引错位产生 NaN 比较器
    const pos = new Map(freeIdx.map((k, i) => [k, i]));
    orders.push([...freeIdx].sort((a, b) => totals[pos.get(b)!] - totals[pos.get(a)!] || a - b)); // 大优先
    orders.push([...freeIdx].sort((a, b) => totals[pos.get(a)!] - totals[pos.get(b)!] || a - b)); // 小优先
    orders.push([...freeIdx]); // 原始顺序
    // 策略4：平均每户型套数升序（轻组合优先），与 Rust 对齐
    orders.push(
      [...freeIdx].sort((a, b) => {
        const ia = pos.get(a)!;
        const ib = pos.get(b)!;
        const cntA = freeUsage[ia].filter((c) => c > 0).length || 1;
        const cntB = freeUsage[ib].filter((c) => c > 0).length || 1;
        const avgA = totals[ia] / cntA;
        const avgB = totals[ib] / cntB;
        return avgA - avgB || a - b;
      }),
    );
    // 策略5：权重降序（高权重组合优先，预览近似体现分层优化阶段二）
    orders.push([...freeIdx].sort((a, b) => weights[pos.get(b)!] - weights[pos.get(a)!] || a - b));
    for (const order of orders) {
      const r = solveGreedyOneOrder(freeUsage, remAfterLower, order.map((k) => pos.get(k)!));
      // 叠加下界预分配
      const full = r.xs.map((x, i) => x + lowerXs[i]);
      const variance = xsVariance(full);
      const used = full.reduce((acc, x, i) => acc + x * totals[i], 0);
      // 加权分数：同利用下高权重组合优先（分层优化阶段二目标）
      const score = full.reduce((acc, x, i) => acc + weightCoeff[i] * totals[i] * x, 0);
      // 优先：利用率大 > 加权分数大 > 方差小
      if (
        used > best.used ||
        (used === best.used && score > best.score) ||
        (used === best.used && score === best.score && variance < best.variance)
      ) {
        best = { xs: full, used, variance, score };
      }
    }
    freeIdx.forEach((k, i) => {
      xs[k] = best.xs[i];
    });
    // 公平性：仅当无权重偏好时均衡化（否则均衡转移会抵消高权重优先意图）
    const hasPreference = weights.some((w) => w !== 5);
    if (!hasPreference) {
      balanceSolution(freeUsage, remAfterLower, totals, lower, best.xs);
    }
    freeIdx.forEach((k, i) => {
      xs[k] = best.xs[i];
    });
  }

  return buildSolveResult(
    req,
    usage,
    xs,
    manual,
    freeIdx.length === 0 ? 'manual-only' : 'greedy (preview)',
    Math.round(performance.now() - start),
  );
}

/** 组装 SolveResult（jsSolve 与 jsEnumerate 共用） */
function buildSolveResult(
  req: SolveRequest,
  usage: number[][],
  xs: number[],
  manual: boolean[],
  algorithm: string,
  solveTimeMs: number,
): SolveResult {
  const assignments = req.combinations
    .map((c, k) => ({
      combinationId: c.id,
      combinationName: c.name,
      quantity: xs[k],
      isManual: manual[k],
    }))
    .filter((a) => a.quantity > 0 || a.isManual);

  const remainingItems = req.houseTypes.map((t, j) => {
    let used = 0;
    usage.forEach((row, k) => {
      used += xs[k] * row[j];
    });
    return { typeId: t.id, typeName: t.name, remaining: Math.max(0, t.quantity - used) };
  });

  const totalUsed = remainingItems.reduce(
    (acc, r, j) => acc + (req.houseTypes[j].quantity - r.remaining),
    0,
  );
  const totalRemaining = remainingItems.reduce((acc, r) => acc + r.remaining, 0);

  return {
    assignments,
    remaining: remainingItems,
    totalUsed,
    totalRemaining,
    solveTimeMs,
    algorithm,
  };
}

/**
 * 浏览器预览降级：遍历备选方案（近似实现）
 * 桌面版由 Rust ILP + no-good cut 精确遍历；此处用"多策略贪心 + 自由组合子集变体"
 * 生成互不相同的方案，供浏览器预览体验。
 */
export function jsEnumerate(req: SolveRequest, max = 50): EnumerateResponse {
  const typeIds = req.houseTypes.map((t) => t.id);
  const typeIdx = new Map(typeIds.map((id, i) => [id, i]));
  const usage = req.combinations.map((c) => {
    const row = new Array(typeIds.length).fill(0);
    for (const item of c.items) {
      const j = typeIdx.get(item.typeId);
      if (j !== undefined) row[j] = item.count;
    }
    return row;
  });

  const comboIdx = new Map(req.combinations.map((c, i) => [c.id, i]));
  const manual = new Array(req.combinations.length).fill(false);
  const manualQs = new Array(req.combinations.length).fill(0);
  for (const mi of req.manualInputs) {
    const k = comboIdx.get(mi.combinationId);
    if (k !== undefined) {
      manualQs[k] = mi.quantity;
      manual[k] = true;
    }
  }
  const freeIdx = req.combinations.map((_, i) => i).filter((i) => !manual[i]);
  if (freeIdx.length === 0) {
    // 全手动 → 仅一种方案（预览近似，无截断）
    return { solutions: [jsSolve(req)], truncated: false };
  }

  const freeUsage = freeIdx.map((k) => usage[k]);
  const remaining = req.houseTypes.map((t, j) => {
    let used = 0;
    usage.forEach((row, k) => {
      used += manualQs[k] * row[j];
    });
    return Math.max(0, t.quantity - used);
  });
  const pos = new Map(freeIdx.map((k, i) => [k, i]));
  const totals = freeUsage.map((u) => u.reduce((s, c) => s + c, 0));

  // "≥1"下界：预分配下界组合，剩余库存供贪心/子集变体使用
  const lower = freeIdx.map((gi) =>
    req.minOneCombinationIds?.includes(req.combinations[gi].id) ? 1 : 0,
  );
  const lowerXs = new Array(freeIdx.length).fill(0);
  const remAfterLower = [...remaining];
  freeIdx.forEach((_, i) => {
    if (lower[i] > 0) {
      for (let j = 0; j < remAfterLower.length; j++) {
        remAfterLower[j] -= freeUsage[i][j];
      }
      lowerXs[i] = 1;
    }
  });
  // 非下界自由组合（子集变体只在这些组合上枚举，保证下界始终满足）
  const freeNonLower = freeIdx.filter((_, i) => lower[i] === 0);

  const seen = new Set<string>();
  const results: SolveResult[] = [];
  const start = performance.now();

  const trySolution = (freeXs: number[]) => {
    const k = freeXs.join(',');
    if (seen.has(k)) return;
    seen.add(k);
    // 自由组合全 0 且无手动输入 → 无可行正解，跳过（不生成空方案标签）
    if (freeXs.every((x) => x === 0) && manualQs.every((q) => q === 0)) return;
    const full = new Array(req.combinations.length).fill(0);
    freeIdx.forEach((gi, i) => {
      full[gi] = freeXs[i];
    });
    manual.forEach((m, gi) => {
      if (m) full[gi] = manualQs[gi];
    });
    results.push(
      buildSolveResult(
        req,
        usage,
        full,
        manual,
        // 与 jsSolve 的 'greedy (preview)' 保持同一 'preview' 标记，
        // 供 P0-2/P1-4 的引擎判定（includes('preview')）正确识别 JS 产物
        'greedy (preview 枚举)',
        Math.round(performance.now() - start),
      ),
    );
  };

  // 1) 多策略贪心解（叠加下界预分配）
  const orders: number[][] = [];
  orders.push([...freeIdx].sort((a, b) => totals[pos.get(b)!] - totals[pos.get(a)!] || a - b));
  orders.push([...freeIdx].sort((a, b) => totals[pos.get(a)!] - totals[pos.get(b)!] || a - b));
  orders.push([...freeIdx]);
  orders.push(
    [...freeIdx].sort((a, b) => {
      const ia = pos.get(a)!;
      const ib = pos.get(b)!;
      const cntA = freeUsage[ia].filter((c) => c > 0).length || 1;
      const cntB = freeUsage[ib].filter((c) => c > 0).length || 1;
      return totals[ia] / cntA - totals[ib] / cntB || a - b;
    }),
  );
  for (const order of orders) {
    const r = solveGreedyOneOrder(freeUsage, remAfterLower, order.map((k) => pos.get(k)!));
    trySolution(r.xs.map((x, i) => x + lowerXs[i]));
    if (results.length >= max) return { solutions: results, truncated: false };
  }

  // 2) 非下界自由组合子集变体（组合数 ≤ 6 时枚举所有非空子集，生成更多备选）
  if (freeNonLower.length <= 6) {
    for (let mask = 1; mask < 1 << freeNonLower.length && results.length < max; mask++) {
      const subset = freeNonLower.filter((_, i) => (mask & (1 << i)) !== 0);
      const subUsage = subset.map((k) => usage[k]);
      const r = solveGreedyOneOrder(
        subUsage,
        [...remAfterLower],
        subset.map((_, i) => i),
      );
      const fullSub = lowerXs.slice(); // 下界预分配 + 子集贪心
      // r.xs 按下标顺序对应 subset
      subset.forEach((gi, si) => {
        fullSub[pos.get(gi)!] = lowerXs[pos.get(gi)!] + r.xs[si];
      });
      trySolution(fullSub);
    }
  }

  return { solutions: results, truncated: false };
}
