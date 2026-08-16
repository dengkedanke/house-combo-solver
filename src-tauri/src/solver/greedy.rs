/// 多起点贪心算法：按多种排序策略尝试分配，取总使用套数最大的方案。
/// 当 ILP 求解器不可用时作为降级方案。
pub fn solve_greedy(usage: &[Vec<u32>], remaining: &[u32]) -> Vec<u32> {
    let n = usage.len();
    if n == 0 {
        return vec![];
    }

    let totals: Vec<u32> = usage.iter().map(|u| u.iter().sum()).collect();

    // 多种排序策略
    let mut orders: Vec<Vec<usize>> = Vec::new();

    // 策略 1：按总套数降序（"大"组合优先）
    let mut o1: Vec<usize> = (0..n).collect();
    o1.sort_by(|&a, &b| totals[b].cmp(&totals[a]).then(a.cmp(&b)));
    orders.push(o1);

    // 策略 2：按总套数升序（"小"组合优先，更容易精确填充剩余）
    let mut o2: Vec<usize> = (0..n).collect();
    o2.sort_by(|&a, &b| totals[a].cmp(&totals[b]).then(a.cmp(&b)));
    orders.push(o2);

    // 策略 3：原始顺序
    orders.push((0..n).collect());

    // 策略 4：按平均每户型套数升序（轻组合优先）
    let mut o4: Vec<usize> = (0..n).collect();
    o4.sort_by(|&a, &b| {
        let cnt_a = usage[a].iter().filter(|&&c| c > 0).count().max(1) as f64;
        let cnt_b = usage[b].iter().filter(|&&c| c > 0).count().max(1) as f64;
        let avg_a = totals[a] as f64 / cnt_a;
        let avg_b = totals[b] as f64 / cnt_b;
        avg_a
            .partial_cmp(&avg_b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    orders.push(o4);

    let mut best: Option<(Vec<u32>, u32, f64)> = None;
    for order in &orders {
        let mut rem = remaining.to_vec();
        let mut xs = vec![0u32; n];
        for &k in order {
            // 跳过未完整定义的组合（全 0），否则条件恒真导致死循环
            if usage[k].iter().all(|&c| c == 0) {
                continue;
            }
            // 批量分配：一步算出该组合可分配的最大数量（最小需求比率），避免逐份循环
            let max_k = usage[k]
                .iter()
                .zip(rem.iter())
                .filter(|(&c, _)| c > 0)
                .map(|(&c, &r)| r / c)
                .min()
                .unwrap_or(0);
            if max_k > 0 {
                for j in 0..rem.len() {
                    rem[j] -= usage[k][j].saturating_mul(max_k);
                }
                xs[k] = max_k;
            }
        }
        let used: u32 = xs
            .iter()
            .zip(usage.iter())
            .map(|(x, u)| x * u.iter().sum::<u32>())
            .sum();
        let variance = xs_variance(&xs);
        // 平局时优先方差小（组合数量更均衡），消除"先定义组合优先"偏见
        let better = match &best {
            Some((_, best_used, best_var)) => {
                used > *best_used || (used == *best_used && variance < *best_var)
            }
            None => true,
        };
        if better {
            best = Some((xs, used, variance));
        }
    }

    best.map(|(xs, _, _)| xs).unwrap_or_else(|| vec![0; n])
}

/// 组合数量的方差（用于平局公平性比较）
fn xs_variance(xs: &[u32]) -> f64 {
    let n = xs.len();
    if n == 0 {
        return 0.0;
    }
    let mean = xs.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    xs.iter()
        .map(|&x| (x as f64 - mean) * (x as f64 - mean))
        .sum::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_greedy() {
        let usage = vec![vec![1, 1], vec![2, 1]];
        let remaining = vec![10, 10];
        let xs = solve_greedy(&usage, &remaining);
        let used: u32 = xs
            .iter()
            .zip(usage.iter())
            .map(|(x, u)| x * u.iter().sum::<u32>())
            .sum();
        assert_eq!(used, 20);
    }

    #[test]
    fn cannot_fit() {
        let usage = vec![vec![3]];
        let remaining = vec![2];
        let xs = solve_greedy(&usage, &remaining);
        assert_eq!(xs[0], 0);
    }

    #[test]
    fn empty_combo_does_not_loop() {
        // 组合未完整定义（全 0）时不得死循环
        let usage = vec![vec![0, 0, 0], vec![1, 1, 1]];
        let remaining = vec![10, 10, 10];
        let xs = solve_greedy(&usage, &remaining);
        assert_eq!(xs[0], 0);
        assert!(xs[1] >= 1);
    }
}
