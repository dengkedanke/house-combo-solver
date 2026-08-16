// 浏览器环境的降级求解器（贪心算法），用于 vite dev 无 Tauri 时预览。
// 生产环境由 Rust 后端 ILP 求解器提供精确解。
import type { SolveRequest, SolveResult } from '../types';

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
    let maxK = Number.MAX_SAFE_INTEGER;
    for (let j = 0; j < rem.length; j++) {
      if (u[j] > 0) maxK = Math.min(maxK, Math.floor(rem[j] / u[j]));
    }
    if (maxK === Number.MAX_SAFE_INTEGER) maxK = 0;
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
// 保持总套数（最优值）不变，通过"总量守恒转移"使组合数量方差最小
function balanceSolution(
  usage: number[][],
  remaining: number[],
  totals: number[],
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

  // 自由组合贪心
  const freeIdx = req.combinations.map((_, i) => i).filter((i) => !manual[i]);
  const freeUsage = freeIdx.map((k) => usage[k]);

  let best = { xs: freeIdx.map(() => 0), used: 0, variance: Infinity };
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
    for (const order of orders) {
      const r = solveGreedyOneOrder(freeUsage, remaining, order.map((k) => pos.get(k)!));
      const variance = xsVariance(r.xs);
      // 平局时优先方差小（组合数量更均衡），消除"先定义组合优先"偏见
      if (r.used > best.used || (r.used === best.used && variance < best.variance)) {
        best = { xs: r.xs, used: r.used, variance };
      }
    }
    freeIdx.forEach((k, i) => {
      xs[k] = best.xs[i];
    });
    // 公平性：保持总套数不变，均衡各组合数量（方差最小）
    balanceSolution(freeUsage, remaining, totals, best.xs);
    freeIdx.forEach((k, i) => {
      xs[k] = best.xs[i];
    });
  }

  // 组装结果
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
    solveTimeMs: Math.round(performance.now() - start),
    algorithm: freeIdx.length === 0 ? 'manual-only' : 'greedy (preview)',
  };
}
