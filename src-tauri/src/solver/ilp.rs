use good_lp::{constraint, default_solver, variable, variables, Expression, Solution, SolverModel, Variable};
use std::sync::mpsc;
use std::time::Duration;

/// ILP 求解超时阈值：minilp 分支定界无内部时间限制，超时后降级贪心，避免界面挂起
const ILP_TIMEOUT_MS: u64 = 2000;

/// 整数线性规划求解：在剩余库存约束下，最大化各自由组合的总使用套数
///
/// usage[k][j] = 组合 k 需要户型 j 的套数
/// remaining[j] = 户型 j 的剩余可用库存
/// 返回各组合的最优数量（与 usage 行序对应）
pub fn solve_ilp(usage: &[Vec<u32>], remaining: &[u32]) -> Option<Vec<u32>> {
    let n = usage.len();
    if n == 0 {
        return Some(vec![]);
    }

    // 组合数量过多时 minilp 分支定界可能变慢，直接交由贪心处理
    if n > 60 {
        return None;
    }

    // 放入线程执行并限时：超时立即返回 None（调用方降级贪心），
    // 后台线程继续运行至结束（minilp 无法强制中断，但不会阻塞主流程）
    let (tx, rx) = mpsc::channel();
    let usage_owned = usage.to_vec();
    let remaining_owned = remaining.to_vec();
    std::thread::spawn(move || {
        let result = solve_ilp_inner(&usage_owned, &remaining_owned);
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_millis(ILP_TIMEOUT_MS)) {
        Ok(Some(v)) => Some(v),
        _ => None,
    }
}

fn solve_ilp_inner(usage: &[Vec<u32>], remaining: &[u32]) -> Option<Vec<u32>> {
    let n = usage.len();
    let m = remaining.len();
    let mut vars = variables!();
    let xs: Vec<Variable> = (0..n).map(|_| vars.add(variable().min(0).integer())).collect();

    // 目标函数：最大化 Σ(x_k × 组合k的总套数)
    let mut objective = Expression::default();
    for (k, x) in xs.iter().enumerate() {
        let total_units = usage[k].iter().sum::<u32>() as f64;
        if total_units > 0.0 {
            objective += total_units * (*x);
        }
    }

    let mut model = vars.maximise(objective).using(default_solver);

    // 约束：每种户型的消耗不超过剩余库存
    for j in 0..m {
        let mut expr = Expression::default();
        for (k, x) in xs.iter().enumerate() {
            let c = usage[k][j] as f64;
            if c > 0.0 {
                expr += c * (*x);
            }
        }
        if expr != Expression::default() {
            model = model.with(constraint!(expr <= remaining[j] as f64));
        }
    }

    match model.solve() {
        Ok(solution) => {
            let vals: Vec<u32> = xs
                .iter()
                .map(|x| solution.value(*x).max(0.0).round() as u32)
                .collect();
            Some(vals)
        }
        Err(_) => None,
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
}
