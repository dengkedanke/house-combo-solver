use good_lp::{constraint, default_solver, variable, variables, Expression, Solution, SolverModel, Variable};
use std::sync::mpsc;
use std::time::Duration;

/// ILP 求解超时阈值：minilp 分支定界无内部时间限制，超时后降级贪心，避免界面挂起
const ILP_TIMEOUT_MS: u64 = 2000;

/// 增量约束（no-good cut）：形如 Σ(coeffs[k] × x_k) ≤ rhs。
/// 用于"遍历备选方案"——屏蔽已求得的解，迫使求解器寻找下一个不同方案。
#[derive(Debug, Clone)]
pub struct Cut {
    pub coeffs: Vec<i64>,
    pub rhs: i64,
}

/// 整数线性规划求解：在剩余库存约束下，最大化各自由组合的总使用套数
///
/// usage[k][j] = 组合 k 需要户型 j 的套数
/// remaining[j] = 户型 j 的剩余可用库存
/// 返回各组合的最优数量（与 usage 行序对应）
/// 注：主流程使用 solve_ilp_with_cuts（支持下界），本函数保留为便捷 API（测试使用）
#[allow(dead_code)]
pub fn solve_ilp(usage: &[Vec<u32>], remaining: &[u32]) -> Option<Vec<u32>> {
    solve_ilp_with_cuts(usage, remaining, &[], &[])
}

/// ILP 求解三态结果（用于区分"无解"与"超时"）
#[derive(Debug, Clone, PartialEq)]
pub enum IlpOutcome {
    Solved(Vec<u32>),
    Infeasible,
    TimedOut,
}

/// 带下界与增量约束（cuts）的 ILP 求解：
/// - lower[k]：组合 k 的数量下界（如"≥1"约束，默认 0）
/// - cuts：增量约束（no-good cut，用于枚举备选方案）
/// lower/cuts 为空时等价于 solve_ilp。
pub fn solve_ilp_with_cuts(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    cuts: &[Cut],
) -> Option<Vec<u32>> {
    solve_ilp_with_cuts_weighted(usage, remaining, lower, &[], cuts)
}

/// 带权重偏好的两阶段求解（solve_ilp_with_cuts + weights）。
/// weights[k]：组合 k 的权重（1-10），仅影响阶段二的加权目标，不影响利用率最大化。
pub fn solve_ilp_with_cuts_weighted(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    weights: &[u8],
    cuts: &[Cut],
) -> Option<Vec<u32>> {
    match solve_ilp_detailed(usage, remaining, lower, weights, cuts, ILP_TIMEOUT_MS) {
        IlpOutcome::Solved(v) => Some(v),
        _ => None,
    }
}

/// 带可配超时的 ILP 求解，返回三态结果。
/// 用于"遍历备选方案"：可区分无可行解（穷尽）与超时（截断），
/// 并支持每轮/总时间预算控制（E1/E2 修复）。
pub fn solve_ilp_detailed(
    usage: &[Vec<u32>],
    remaining: &[u32],
    lower: &[u32],
    weights: &[u8],
    cuts: &[Cut],
    timeout_ms: u64,
) -> IlpOutcome {
    let n = usage.len();
    if n == 0 {
        return IlpOutcome::Solved(vec![]);
    }

    // 组合数量过多时 minilp 分支定界可能变慢：返回 TimedOut 而非 Infeasible。
    // 语义区别（P1-3 修复）：
    // - 单方案求解：TimedOut 与 Infeasible 都会被 solve_ilp_with_cuts_weighted
    //   映射为 None，从而降级到贪心，行为不变；
    // - 枚举流程：Infeasible 被解释为"已穷尽所有方案"（提前结束），
    //   而 TimedOut 被解释为"超时截断"（置 truncated=true），
    //   避免把"算不动"误判为"已遍历完"，导致漏掉真实存在的方案。
    if n > 60 {
        return IlpOutcome::TimedOut;
    }

    // 放入线程执行并限时：超时返回 TimedOut（调用方决定降级/截断），
    // 后台线程继续运行至结束（minilp 无法强制中断，但不会阻塞主流程）
    let (tx, rx) = mpsc::channel();
    let usage_owned = usage.to_vec();
    let remaining_owned = remaining.to_vec();
    let lower_owned = lower.to_vec();
    let weights_owned = weights.to_vec();
    let cuts_owned = cuts.to_vec();
    std::thread::spawn(move || {
        let result = solve_ilp_inner(
            &usage_owned,
            &remaining_owned,
            &lower_owned,
            &cuts_owned,
            &weights_owned,
        );
        let _ = tx.send(result);
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
    cuts: &[Cut],
    weights: &[u8],
) -> Option<Vec<u32>> {
    let n = usage.len();
    let m = remaining.len();

    // ---------- 阶段一：最大化利用率 ----------
    // 在库存 + 下界 + no-good cut 约束下，最大化已分配总套数 maxUtil
    let mut vars1 = variables!();
    let xs1: Vec<Variable> = (0..n)
        .map(|k| vars1.add(variable().min(lower.get(k).copied().unwrap_or(0) as f64).integer()))
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
    for cut in cuts {
        let mut expr = Expression::default();
        for (k, x) in xs1.iter().enumerate() {
            let c = cut.coeffs.get(k).copied().unwrap_or(0) as f64;
            if c != 0.0 {
                expr += c * (*x);
            }
        }
        if expr != Expression::default() {
            model1 = model1.with(constraint!(expr <= cut.rhs as f64));
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

    // ---------- 阶段二：加权偏好优化（分层优化） ----------
    // 硬约束：已分配总套数 ≥ 阶段一的最大利用率（绝对不牺牲"用完房源"）
    // 目标：最大化 Σ (weightCoeff_k × 组合总套数_k × x_k)
    // weightCoeff = 1.0 + (weight - 1) * 0.001 —— 1~10 只带来 0~0.9% 微小差异
    let mut vars2 = variables!();
    let xs2: Vec<Variable> = (0..n)
        .map(|k| vars2.add(variable().min(lower.get(k).copied().unwrap_or(0) as f64).integer()))
        .collect();
    let utilized = vars2.add(variable().min(0.0));

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
    for cut in cuts {
        let mut expr = Expression::default();
        for (k, x) in xs2.iter().enumerate() {
            let c = cut.coeffs.get(k).copied().unwrap_or(0) as f64;
            if c != 0.0 {
                expr += c * (*x);
            }
        }
        if expr != Expression::default() {
            model2 = model2.with(constraint!(expr <= cut.rhs as f64));
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
        let first = solve_ilp(&usage, &remaining).unwrap();
        assert_eq!(first[0] + first[1], 6, "首次求解必须为最优");
        // 屏蔽 first 后，应得到不同的（仍最优的）解
        let cut = Cut {
            coeffs: first.iter().map(|&x| if x > 0 { 1 } else { -1 }).collect(),
            rhs: first.iter().map(|&x| x as i64).sum::<i64>() - 1,
        };
        let second = solve_ilp_with_cuts(&usage, &remaining, &[], &[cut]).unwrap();
        assert_ne!(second, first);
        assert_eq!(second[0] + second[1], 6, "第二个方案仍应最大化总套数");
    }

    #[test]
    fn all_solutions_exhausted() {
        // 单户型 3 套；组合1=(1)。目标最大化 → x1=3；逐级加割递减直至无解
        let usage = vec![vec![1]];
        let remaining = vec![3];
        let first = solve_ilp(&usage, &remaining).unwrap();
        assert_eq!(first, vec![3]);
        let cut = Cut { coeffs: vec![1], rhs: 2 };
        let second = solve_ilp_with_cuts(&usage, &remaining, &[], &[cut.clone()]).unwrap();
        assert_eq!(second, vec![2]);
        let cut2 = Cut { coeffs: vec![1], rhs: 1 };
        let third = solve_ilp_with_cuts(&usage, &remaining, &[], &[cut.clone(), cut2.clone()]).unwrap();
        assert_eq!(third, vec![1]);
        let cut3 = Cut { coeffs: vec![1], rhs: 0 };
        let fourth = solve_ilp_with_cuts(&usage, &remaining, &[], &[cut.clone(), cut2.clone(), cut3]).unwrap();
        assert_eq!(fourth, vec![0]);
        let cut4 = Cut { coeffs: vec![1], rhs: -1 };
        assert!(solve_ilp_with_cuts(&usage, &remaining, &[], &[cut, cut2, cut4]).is_none());
    }
}
