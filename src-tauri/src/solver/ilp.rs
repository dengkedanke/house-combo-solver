use good_lp::{constraint, default_solver, variable, variables, Expression, Solution, SolverModel, Variable};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

/// ILP 求解超时阈值：minilp 分支定界无内部时间限制，超时后降级贪心，避免界面挂起
const ILP_TIMEOUT_MS: u64 = 2000;

/// 自由组合数防御上限（#7 修复）：超过即快速失败交给超时机制/贪心降级。
/// 阈值从 60 提升至 500：稀疏约束的较大问题有机会通过真实 recv_timeout 求解，
/// 不再被一刀切拒绝；极端规模仍快速降级。
const MAX_COMBOS: usize = 500;

/// ILP 后台线程并发上限（#2 修复）：minilp 无法强制中断，超时线程会继续运行至结束，
/// 此上限防止高频枚举下线程无界堆积。
/// 取值说明：单用户桌面应用的并发求解请求远低于此（前端防抖 + 枚举串行循环
/// 单次至多 1 个活跃后台线程）；上限仅为防止"极端高频调用"下的无界增长。
/// cargo test 并行执行 ~30 个测试也不会触顶，避免测试互相干扰。
const MAX_ACTIVE_ILP_THREADS: usize = 64;

static ACTIVE_ILP_THREADS: AtomicUsize = AtomicUsize::new(0);

/// 当前活跃的 ILP 后台线程数（诊断/测试用）
#[cfg_attr(not(test), allow(dead_code))]
pub fn active_ilp_threads() -> usize {
    ACTIVE_ILP_THREADS.load(Ordering::Relaxed)
}

/// 增量约束（no-good cut），用于"遍历备选方案"——屏蔽已求得的解，迫使求解器寻找下一个不同方案。
#[derive(Debug, Clone)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Cut {
    /// 线性不等式：Σ(coeffs[k] × x_k) ≤ rhs。
    /// 仅用于单分量场景/测试；对一般整数向量会过度排除"正分量更满"的解。
    Linear { coeffs: Vec<i64>, rhs: i64 },
    /// 精确排除单个整数解 v（大 M 线性化，#9 修复）：
    /// 每个分量要么 < v_k 要么 > v_k，且至少一个分量不同（Σz_k ≥ 1）。
    /// 只排除"恰好等于 v"的解，不误伤其他合法备选方案。
    Exact { v: Vec<u32> },
}

/// 整数线性规划求解：在剩余库存约束下，最大化各自由组合的总使用套数
///
/// usage[k][j] = 组合 k 需要户型 j 的套数
/// remaining[j] = 户型 j 的剩余可用库存
/// 返回各组合的最优数量（与 usage 行序对应）
/// 注：主流程使用 solve_ilp_with_cuts（支持下界/上界），本函数保留为便捷 API（测试使用）
#[allow(dead_code)]
pub fn solve_ilp(usage: &[Vec<u32>], remaining: &[u32]) -> Option<Vec<u32>> {
    let upper = vec![999u32; usage.len()];
    solve_ilp_with_cuts(usage, remaining, &[], &upper, &[])
}

/// ILP 求解三态结果（用于区分"无解"与"超时"）
#[derive(Debug, Clone, PartialEq)]
pub enum IlpOutcome {
    Solved(Vec<u32>),
    Infeasible,
    TimedOut,
}

/// 带下界/上界与增量约束（cuts）的 ILP 求解：
/// - lower[k]：组合 k 的数量下界（组合 min，默认 0）
/// - upper[k]：组合 k 的数量上界（组合 max，默认 999）
/// - cuts：增量约束（no-good cut，用于枚举备选方案）
pub fn solve_ilp_with_cuts(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    upper: &[u32],
    cuts: &[Cut],
) -> Option<Vec<u32>> {
    solve_ilp_with_cuts_weighted(usage, remaining, lower, upper, &[], cuts)
}

/// 带权重偏好的两阶段求解（solve_ilp_with_cuts + weights）。
/// weights[k]：组合 k 的权重（1-10），仅影响阶段二的加权目标，不影响利用率最大化。
pub fn solve_ilp_with_cuts_weighted(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    upper: &[u32],
    weights: &[u8],
    cuts: &[Cut],
) -> Option<Vec<u32>> {
    match solve_ilp_detailed(usage, remaining, lower, upper, weights, cuts, ILP_TIMEOUT_MS) {
        IlpOutcome::Solved(v) => Some(v),
        _ => None,
    }
}

/// 带可配超时的 ILP 求解，返回三态结果（两阶段：利用率最大化 + 权重偏好）。
/// 用于单方案求解；"遍历备选方案"使用更快的 solve_ilp_phase1_detailed（仅阶段一）。
pub fn solve_ilp_detailed(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    upper: &[u32],
    weights: &[u8],
    cuts: &[Cut],
    timeout_ms: u64,
) -> IlpOutcome {
    solve_ilp_detailed_impl(usage, remaining, lower, upper, weights, cuts, timeout_ms, true)
}

/// 枚举专用：仅阶段一（最大化利用率 + cuts）的 ILP 求解。
/// 备选方案的目标是按利用率降序列出互不相同的解，权重偏好阶段对枚举无意义；
/// 去掉阶段二可大幅降低每轮耗时（避免大 M 精确割下两阶段 MIP 双重开销）。
pub fn solve_ilp_phase1_detailed(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    upper: &[u32],
    cuts: &[Cut],
    timeout_ms: u64,
) -> IlpOutcome {
    solve_ilp_detailed_impl(usage, remaining, lower, upper, &[], cuts, timeout_ms, false)
}

fn solve_ilp_detailed_impl(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    upper: &[u32],
    weights: &[u8],
    cuts: &[Cut],
    timeout_ms: u64,
    weighted: bool,
) -> IlpOutcome {
    let n = usage.len();
    if n == 0 {
        return IlpOutcome::Solved(vec![]);
    }

    // #7 修复：组合数防御上限（原 60 硬阈值对稀疏大问题过于粗暴）与
    // #2 修复：并发线程上限，任一超限即返回 TimedOut（上层决定降级/截断）。
    // 组合数在阈值内时完全依赖下方真实 recv_timeout 超时判断，行为更可预期。
    if n > MAX_COMBOS || ACTIVE_ILP_THREADS.load(Ordering::Relaxed) >= MAX_ACTIVE_ILP_THREADS {
        return IlpOutcome::TimedOut;
    }

    // 放入线程执行并限时：超时返回 TimedOut（调用方决定降级/截断），
    // 后台线程继续运行至结束（minilp 无法强制中断，但不会阻塞主流程，
    // 且并发数受 MAX_ACTIVE_ILP_THREADS 上限约束）
    let (tx, rx) = mpsc::channel();
    let usage_owned = usage.to_vec();
    let remaining_owned = remaining.to_vec();
    let lower_owned = lower.to_vec();
    let upper_owned = upper.to_vec();
    let weights_owned = weights.to_vec();
    let cuts_owned = cuts.to_vec();
    ACTIVE_ILP_THREADS.fetch_add(1, Ordering::Relaxed);
    std::thread::spawn(move || {
        let result = solve_ilp_inner(
            &usage_owned,
            &remaining_owned,
            &lower_owned,
            &upper_owned,
            &cuts_owned,
            &weights_owned,
            weighted,
        );
        let _ = tx.send(result);
        ACTIVE_ILP_THREADS.fetch_sub(1, Ordering::Relaxed);
    });

    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(Some(v)) => IlpOutcome::Solved(v),
        Ok(None) => IlpOutcome::Infeasible,
        Err(_) => IlpOutcome::TimedOut,
    }
}

fn solve_ilp_inner(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    upper: &[u32],
    cuts: &[Cut],
    weights: &[u8],
    weighted: bool,
) -> Option<Vec<u32>> {
    let n = usage.len();
    let m = remaining.len();
    // 大 M 取"总库存 + 1"：保证大于任何 x_k 可能取值与 v_k 之差（#9）
    let total_houses: u64 = remaining.iter().map(|&r| r as u64).sum();
    let big_m: f64 = (total_houses + 1) as f64;

    // ---------- 阶段一：最大化利用率 ----------
    // 在库存 + 下界/上界 + no-good cut 约束下，最大化已分配总套数 maxUtil
    let mut vars1 = variables!();
    let xs1: Vec<Variable> = (0..n)
        .map(|k| {
            vars1.add(
                variable()
                    .min(lower.get(k).copied().unwrap_or(0) as f64)
                    .max(upper.get(k).copied().unwrap_or(999) as f64)
                    .integer(),
            )
        })
        .collect();
    // #9：为每个 Exact cut 预分配二元变量（必须在 maximise 之前添加到模型）。
    // 每分量：v_k>0 需要 a（上翻 x≥v+1）与 b（下翻 x≤v−1）两个；
    // v_k=0 时下翻不可能（x≥0），只需上翻变量（x≥1）。
    let exact_zs1: Vec<Vec<Variable>> = cuts
        .iter()
        .map(|cut| match cut {
            Cut::Exact { v } => v
                .iter()
                .map(|&x| {
                    let cnt = if x > 0 { 2 } else { 1 };
                    (0..cnt).map(|_| vars1.add(variable().binary())).collect::<Vec<_>>()
                })
                .flatten()
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    let mut objective1 = Expression::default();
    for (k, x) in xs1.iter().enumerate() {
        let total_units = usage[k].iter().sum::<u32>() as f64;
        if total_units > 0.0 {
            objective1 += total_units * (*x);
        }
    }
    let mut model1 = vars1.maximise(objective1).using(default_solver);
    // 库存约束
    for j in 0..m {
        let mut expr = Expression::default();
        for (k, x) in xs1.iter().enumerate() {
            let c = usage[k][j] as f64;
            if c > 0.0 {
                expr += c * (*x);
            }
        }
        if expr != Expression::default() {
            model1 = model1.with(constraint!(expr <= remaining[j] as f64));
        }
    }
    // no-good cuts
    for (i, cut) in cuts.iter().enumerate() {
        match cut {
            Cut::Linear { coeffs, rhs } => {
                let mut expr = Expression::default();
                for (k, x) in xs1.iter().enumerate() {
                    let c = coeffs.get(k).copied().unwrap_or(0) as f64;
                    if c != 0.0 {
                        expr += c * (*x);
                    }
                }
                if expr != Expression::default() {
                    model1 = model1.with(constraint!(expr <= *rhs as f64));
                }
            }
            Cut::Exact { v } => {
                // #9 精确割（大 M 线性化）：每个分量要么 < v_k 要么 > v_k，
                // Σ(全部二元变量) ≥ 1 保证至少一个分量与 v 不同。
                // a_k=1 → x_k ≥ v_k+1（上翻）；b_k=1 → x_k ≤ v_k−1（下翻）。
                // v_k=0 的分量只有上翻变量（下翻到 ≤−1 不可能）。
                let mut offset = 0usize;
                for (k, x) in xs1.iter().enumerate() {
                    let vk = v[k];
                    let a = exact_zs1[i][offset];
                    offset += 1;
                    // a_k=1 → x_k ≥ v_k + 1：x_k − M·a_k ≥ v_k + 1 − M（a=0 时宽松）
                    let mut lhs_up = Expression::default();
                    lhs_up += 1.0 * (*x);
                    lhs_up += -big_m * a;
                    model1 = model1.with(constraint!(lhs_up >= vk as f64 + 1.0 - big_m));
                    if vk > 0 {
                        // b_k=1 → x_k ≤ v_k − 1：x_k + M·b_k ≤ v_k − 1 + M（b=0 时宽松）
                        let b = exact_zs1[i][offset];
                        offset += 1;
                        let mut lhs_dn = Expression::default();
                        lhs_dn += 1.0 * (*x);
                        lhs_dn += big_m * b;
                        model1 = model1.with(constraint!(lhs_dn <= vk as f64 - 1.0 + big_m));
                    }
                }
                let mut zsum = Expression::default();
                for z in exact_zs1[i].iter() {
                    zsum += 1.0 * (*z);
                }
                model1 = model1.with(constraint!(zsum >= 1.0));
            }
        }
    }
    let solution1 = model1.solve().ok()?;
    // 最大利用率
    let max_util: f64 = (0..n)
        .map(|k| solution1.value(xs1[k]).max(0.0) * usage[k].iter().sum::<u32>() as f64)
        .sum();
    if max_util <= 0.0 {
        // 无可分配方案：返回全 0（上层据此判定无解/空方案）
        return Some(vec![0u32; n]);
    }
    // 枚举专用（weighted=false）：阶段一的最优解即"利用率最高的下一个不同方案"，
    // 无需再跑权重偏好阶段二（权重只对单方案求解有意义）
    if !weighted {
        return Some(
            xs1.iter()
                .map(|x| solution1.value(*x).max(0.0).round() as u32)
                .collect(),
        );
    }

    // ---------- 阶段二：加权偏好优化（分层优化） ----------
    // 硬约束：已分配总套数 ≥ 阶段一的最大利用率（绝对不牺牲"用完房源"）
    // 目标：最大化 Σ (weightCoeff_k × 组合总套数_k × x_k)
    // weightCoeff = 1.0 + (weight - 1) * 0.001 —— 1~10 只带来 0~0.9% 微小差异
    let mut vars2 = variables!();
    let xs2: Vec<Variable> = (0..n)
        .map(|k| {
            vars2.add(
                variable()
                    .min(lower.get(k).copied().unwrap_or(0) as f64)
                    .max(upper.get(k).copied().unwrap_or(999) as f64)
                    .integer(),
            )
        })
        .collect();
    let utilized = vars2.add(variable().min(0.0));
    // #9：阶段二同样为 Exact cut 预分配二元变量（v_k=0 分量仅上翻变量）
    let exact_zs2: Vec<Vec<Variable>> = cuts
        .iter()
        .map(|cut| match cut {
            Cut::Exact { v } => v
                .iter()
                .map(|&x| {
                    let cnt = if x > 0 { 2 } else { 1 };
                    (0..cnt).map(|_| vars2.add(variable().binary())).collect::<Vec<_>>()
                })
                .flatten()
                .collect(),
            _ => Vec::new(),
        })
        .collect();

    let mut objective2 = Expression::default();
    let mut util_expr = Expression::default();
    for (k, x) in xs2.iter().enumerate() {
        let total_units = usage[k].iter().sum::<u32>() as f64;
        if total_units > 0.0 {
            let w = weights.get(k).copied().unwrap_or(5) as f64;
            let coeff = 1.0 + (w - 1.0) * 0.001;
            objective2 += (coeff * total_units) * (*x);
            util_expr += total_units * (*x);
        }
    }
    let mut model2 = vars2.maximise(objective2).using(default_solver);
    // 库存约束（同阶段一）
    for j in 0..m {
        let mut expr = Expression::default();
        for (k, x) in xs2.iter().enumerate() {
            let c = usage[k][j] as f64;
            if c > 0.0 {
                expr += c * (*x);
            }
        }
        if expr != Expression::default() {
            model2 = model2.with(constraint!(expr <= remaining[j] as f64));
        }
    }
    // no-good cuts
    for (i, cut) in cuts.iter().enumerate() {
        match cut {
            Cut::Linear { coeffs, rhs } => {
                let mut expr = Expression::default();
                for (k, x) in xs2.iter().enumerate() {
                    let c = coeffs.get(k).copied().unwrap_or(0) as f64;
                    if c != 0.0 {
                        expr += c * (*x);
                    }
                }
                if expr != Expression::default() {
                    model2 = model2.with(constraint!(expr <= *rhs as f64));
                }
            }
            Cut::Exact { v } => {
                let mut offset = 0usize;
                for (k, x) in xs2.iter().enumerate() {
                    let vk = v[k];
                    let a = exact_zs2[i][offset];
                    offset += 1;
                    let mut lhs_up = Expression::default();
                    lhs_up += 1.0 * (*x);
                    lhs_up += -big_m * a;
                    model2 = model2.with(constraint!(lhs_up >= vk as f64 + 1.0 - big_m));
                    if vk > 0 {
                        let b = exact_zs2[i][offset];
                        offset += 1;
                        let mut lhs_dn = Expression::default();
                        lhs_dn += 1.0 * (*x);
                        lhs_dn += big_m * b;
                        model2 = model2.with(constraint!(lhs_dn <= vk as f64 - 1.0 + big_m));
                    }
                }
                let mut zsum = Expression::default();
                for z in exact_zs2[i].iter() {
                    zsum += 1.0 * (*z);
                }
                model2 = model2.with(constraint!(zsum >= 1.0));
            }
        }
    }
    // 利用率定义与硬约束：utilized == Σ(总套数×x) 且 utilized ≥ max_util
    model2 = model2.with(constraint!(util_expr - utilized == 0.0));
    model2 = model2.with(constraint!(utilized >= max_util));

    match model2.solve() {
        Ok(solution2) => {
            let vals: Vec<u32> = xs2
                .iter()
                .map(|x| solution2.value(*x).max(0.0).round() as u32)
                .collect();
            Some(vals)
        }
        // 阶段二异常（浮点精度等）：回退阶段一解，保证至少给出最优利用率方案
        Err(_) => {
            let vals: Vec<u32> = xs1
                .iter()
                .map(|x| solution1.value(*x).max(0.0).round() as u32)
                .collect();
            Some(vals)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_solve() {
        // 两种户型各 10 套
        let usage = vec![vec![1, 1], vec![2, 1]];
        let remaining = vec![10, 10];
        let xs = solve_ilp(&usage, &remaining).unwrap();
        assert!(xs[0] * 1 + xs[1] * 2 <= 10);
        assert!(xs[0] * 1 + xs[1] * 1 <= 10);
        let used: u32 = xs
            .iter()
            .zip(usage.iter())
            .map(|(x, u)| x * u.iter().sum::<u32>())
            .sum();
        // 10 套户型1 + 10 套户型2 应该能被完全用完：x0=10,x1=0 或 x0=0,x1=5...
        assert_eq!(used, 20);
    }

    #[test]
    fn impossible_case() {
        // 组合 1 需要 3 套户型 0，但只有 2 套 → 无法使用
        let usage = vec![vec![3]];
        let remaining = vec![2];
        let xs = solve_ilp(&usage, &remaining).unwrap();
        assert_eq!(xs[0], 0);
    }

    #[test]
    fn cut_excludes_previous_solution() {
        // 单户型 6 套；两个相同组合（各 1 套）→ 任意最优解满足 x0+x1=6
        let usage = vec![vec![1], vec![1]];
        let remaining = vec![6];
        let upper = vec![999, 999];
        let first = solve_ilp(&usage, &remaining).unwrap();
        assert_eq!(first[0] + first[1], 6, "首次求解必须为最优");
        // 屏蔽 first 后，应得到不同的（仍最优的）解
        let cut = Cut::Linear {
            coeffs: first.iter().map(|&x| if x > 0 { 1 } else { -1 }).collect(),
            rhs: first.iter().map(|&x| x as i64).sum::<i64>() - 1,
        };
        let second = solve_ilp_with_cuts(&usage, &remaining, &[], &upper, &[cut]).unwrap();
        assert_ne!(second, first);
        assert_eq!(second[0] + second[1], 6, "第二个方案仍应最大化总套数");
    }

    #[test]
    fn all_solutions_exhausted() {
        // 单户型 3 套；组合1=(1)。目标最大化 → x1=3；逐级加割递减直至无解
        let usage = vec![vec![1]];
        let remaining = vec![3];
        let upper = vec![999];
        let first = solve_ilp(&usage, &remaining).unwrap();
        assert_eq!(first, vec![3]);
        let cut = Cut::Linear { coeffs: vec![1], rhs: 2 };
        let second = solve_ilp_with_cuts(&usage, &remaining, &[], &upper, &[cut.clone()]).unwrap();
        assert_eq!(second, vec![2]);
        let cut2 = Cut::Linear { coeffs: vec![1], rhs: 1 };
        let third = solve_ilp_with_cuts(&usage, &remaining, &[], &upper, &[cut.clone(), cut2.clone()]).unwrap();
        assert_eq!(third, vec![1]);
        let cut3 = Cut::Linear { coeffs: vec![1], rhs: 0 };
        let fourth = solve_ilp_with_cuts(&usage, &remaining, &[], &upper, &[cut.clone(), cut2.clone(), cut3]).unwrap();
        assert_eq!(fourth, vec![0]);
        let cut4 = Cut::Linear { coeffs: vec![1], rhs: -1 };
        assert!(solve_ilp_with_cuts(&usage, &remaining, &[], &upper, &[cut, cut2, cut4]).is_none());
    }

    // ---- #9 回归：Exact cut 只排除目标解，不误伤"正分量更满"的合法解 ----
    #[test]
    fn exact_cut_does_not_exclude_fuller_solutions() {
        // t1=6；c1=(1)、c2=(2)。最优利用率 6。
        // v=(2,2)：旧式 Linear cut（x1+x2≤3）会一并排除同档解 (4,1)；
        // Exact cut 只排除 (2,2)，(4,1)（利用率 6）必须仍可达。
        let usage = vec![vec![1], vec![2]];
        let remaining = vec![6];
        let upper = vec![999, 999];
        let cut = Cut::Exact { v: vec![2, 2] };
        let xs = solve_ilp_with_cuts(&usage, &remaining, &[0, 0], &upper, &[cut]).unwrap();
        let used = xs[0] + xs[1] * 2;
        assert_eq!(used, 6, "Exact cut 不得牺牲利用率: xs={xs:?}");
        assert_ne!(xs.as_slice(), &[2, 2], "Exact cut 必须排除目标解本身");
        // 求解器最优解可能是 (4,1)/(6,0)/(0,3) 任一；关键在于 (2,2) 被排除且利用率保持最优
    }

    // ---- #2 回归：并发求解全部成功，线程计数始终受上限约束 ----
    #[test]
    fn concurrent_solves_respect_limit_and_release() {
        let usage = vec![vec![1, 1], vec![2, 1]];
        let remaining = vec![10, 10];
        let upper = vec![999, 999];
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let u = usage.clone();
                let r = remaining.clone();
                let up = upper.clone();
                std::thread::spawn(move || {
                    matches!(
                        solve_ilp_detailed(&u, &r, &[0, 0], &up, &[5, 5], &[], 3000),
                        IlpOutcome::Solved(_)
                    )
                })
            })
            .collect();
        for h in handles {
            assert!(h.join().unwrap(), "并发求解应全部成功");
        }
        // 上限约束始终成立（防线程无界堆积；测试并行时其他测试的后台线程也计入，
        // 因此只断言不超上限，不做全局归零断言）
        assert!(
            active_ilp_threads() <= MAX_ACTIVE_ILP_THREADS,
            "活跃线程数不得超过上限"
        );
    }
}
