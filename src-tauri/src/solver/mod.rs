pub mod greedy;
pub mod ilp;

use crate::models::*;
use std::collections::HashMap;

/// 求解前的公共准备数据（校验、使用矩阵、手动输入、剩余库存、自由组合索引）
struct Prepared {
    usage: Vec<Vec<u32>>,   // usage[k][j] = 组合 k 需要户型 j 的套数
    manual: Vec<bool>,      // 组合是否被手动指定
    xs: Vec<u32>,           // 各组合数量（初始为手动指定值）
    remaining: Vec<u32>,    // 扣除手动后各户型剩余库存
    free_indices: Vec<usize>, // 参与自动求解的自由组合索引
    lower: Vec<u32>,        // 自由组合数量下界（"≥1"约束，与 free_indices 对齐）
}

/// 校验输入并构造求解所需数据结构
fn prepare(request: &SolveRequest) -> Result<Prepared, String> {
    let num_types = request.house_types.len();
    let num_combos = request.combinations.len();
    if num_types == 0 {
        return Err("请先添加房源类型".to_string());
    }
    if num_combos == 0 {
        return Err("请先定义房源组合".to_string());
    }

    // 户型 id → 索引
    let mut type_idx: HashMap<&str, usize> = HashMap::new();
    for (i, t) in request.house_types.iter().enumerate() {
        type_idx.insert(t.id.as_str(), i);
    }

    // 组合 id → 索引
    let mut combo_idx: HashMap<&str, usize> = HashMap::new();
    for (i, c) in request.combinations.iter().enumerate() {
        combo_idx.insert(c.id.as_str(), i);
    }

    // 构造使用矩阵 usage[k][j]，空组合（未定义任何房源）标记为不可用并跳过
    let mut usage = vec![vec![0u32; num_types]; num_combos];
    let mut valid = vec![false; num_combos];
    for (k, c) in request.combinations.iter().enumerate() {
        let mut total = 0u32;
        for item in &c.items {
            let Some(&j) = type_idx.get(item.type_id.as_str()) else {
                return Err(format!("组合 {} 引用了不存在的房源类型", c.name));
            };
            if item.count == 0 {
                continue;
            }
            usage[k][j] = item.count;
            total = total.saturating_add(item.count);
        }
        if total > 0 {
            valid[k] = true;
        }
    }

    // 处理手动输入：扣减库存并记录（引用未定义组合视为输入不合法）
    let mut manual = vec![false; num_combos];
    let mut xs = vec![0u32; num_combos];
    for mi in &request.manual_inputs {
        let Some(&k) = combo_idx.get(mi.combination_id.as_str()) else {
            return Err(format!("手动输入引用了不存在的组合: {}", mi.combination_id));
        };
        if !valid[k] {
            return Err(format!(
                "组合 {} 未包含任何房源，无法手动指定数量",
                request.combinations[k].name
            ));
        }
        if mi.quantity == 0 {
            continue;
        }
        for j in 0..num_types {
            let need = usage[k][j].saturating_mul(mi.quantity);
            if request.house_types[j].quantity < need {
                return Err(format!(
                    "组合 {} 数量 {} 超出户型 {} 的库存",
                    request.combinations[k].name, mi.quantity, request.house_types[j].name
                ));
            }
        }
        xs[k] = mi.quantity;
        manual[k] = true;
    }

    // 剩余库存（扣除手动后）
    let mut remaining = vec![0u32; num_types];
    for j in 0..num_types {
        let mut used = 0u32;
        for k in 0..num_combos {
            used += xs[k].saturating_mul(usage[k][j]);
        }
        remaining[j] = request.house_types[j].quantity.saturating_sub(used);
    }

    // 自由组合（未手动指定且有效）
    let free_indices: Vec<usize> = (0..num_combos).filter(|&k| !manual[k] && valid[k]).collect();

    // 处理"≥1"下界约束：勾选的组合必须纳入计算（数量 ≥ 1）
    let mut lower_full = vec![0u32; num_combos];
    for cid in &request.min_one_combination_ids {
        let Some(&k) = combo_idx.get(cid.as_str()) else {
            return Err(format!("≥1 约束引用了不存在的组合: {}", cid));
        };
        if !valid[k] {
            return Err(format!(
                "组合 {} 未包含任何房源，无法设置 ≥1",
                request.combinations[k].name
            ));
        }
        if manual[k] {
            // 手动指定数量已 ≥ 1，天然满足，无需额外下界（不冲突）
            continue;
        }
        lower_full[k] = 1;
    }
    // 校验所有下界组合（与手动占用合计）不超库存，保证 ILP/贪心均有可行解
    let mut after_lower = remaining.clone();
    for k in 0..num_combos {
        if lower_full[k] == 0 {
            continue;
        }
        for j in 0..num_types {
            if after_lower[j] < usage[k][j] {
                return Err(format!(
                    "≥1 约束超出户型 {} 的库存（剩余 {} 套）",
                    request.house_types[j].name, after_lower[j]
                ));
            }
            after_lower[j] -= usage[k][j];
        }
    }
    // 自由组合下界（与 free_indices 对齐）
    let lower: Vec<u32> = free_indices.iter().map(|&k| lower_full[k]).collect();

    Ok(Prepared {
        usage,
        manual,
        xs,
        remaining,
        free_indices,
        lower,
    })
}

/// 将求解结果组装为 SolveResult
fn assemble(request: &SolveRequest, prep: &Prepared, xs: &[u32], algorithm: &str) -> SolveResult {
    let num_types = request.house_types.len();

    let mut assignments = Vec::new();
    for k in 0..request.combinations.len() {
        if xs[k] > 0 || prep.manual[k] {
            assignments.push(CombinationAssignment {
                combination_id: request.combinations[k].id.clone(),
                combination_name: request.combinations[k].name.clone(),
                quantity: xs[k],
                is_manual: prep.manual[k],
            });
        }
    }

    let mut used_by_type = vec![0u32; num_types];
    for k in 0..request.combinations.len() {
        for j in 0..num_types {
            used_by_type[j] += xs[k].saturating_mul(prep.usage[k][j]);
        }
    }

    let mut remaining_items = Vec::new();
    let mut total_used = 0u32;
    for j in 0..num_types {
        total_used += used_by_type[j];
        remaining_items.push(RemainingItem {
            type_id: request.house_types[j].id.clone(),
            type_name: request.house_types[j].name.clone(),
            remaining: request.house_types[j].quantity.saturating_sub(used_by_type[j]),
        });
    }
    let total_remaining: u32 = remaining_items.iter().map(|r| r.remaining).sum();

    SolveResult {
        assignments,
        remaining: remaining_items,
        total_used,
        total_remaining,
        solve_time_ms: 0,
        algorithm: algorithm.to_string(),
    }
}

/// 生成 no-good cut：屏蔽解 v（v 中各组合数量）
/// 割约束：Σ_{k:v_k>0} x_k − Σ_{k:v_k=0} x_k ≤ (Σ v_k) − 1
/// 当 x = v 时左侧 = Σ v_k > 右侧，被禁止；其他解不受影响（或仅排除"更挤"的同优解）
fn no_good_cut(v: &[u32]) -> ilp::Cut {
    let mut coeffs = Vec::with_capacity(v.len());
    let mut sum_positive: i64 = 0;
    for &x in v {
        if x > 0 {
            coeffs.push(1);
            sum_positive += x as i64;
        } else {
            coeffs.push(-1);
        }
    }
    ilp::Cut {
        coeffs,
        rhs: sum_positive - 1,
    }
}

/// 求解入口：
/// 1. 校验输入合法性
/// 2. 扣除手动指定组合占用的房源
/// 3. ILP 求解自由组合（失败降级为贪心）
/// 4. 组装结果
pub fn solve(request: &SolveRequest) -> Result<SolveResult, String> {
    let prep = prepare(request)?;
    let mut xs = prep.xs.clone();
    let algorithm: &str;

    if prep.free_indices.is_empty() {
        algorithm = "manual-only";
    } else {
        let free_usage: Vec<Vec<u32>> =
            prep.free_indices.iter().map(|&k| prep.usage[k].clone()).collect();
        let free_remaining = prep.remaining.clone();
        let free_lower = prep.lower.clone();
        match ilp::solve_ilp_with_cuts(&free_usage, &free_remaining, &free_lower, &[]) {
            Some(v) => {
                for (i, &k) in prep.free_indices.iter().enumerate() {
                    xs[k] = v[i];
                }
                algorithm = "ilp";
            }
            None => {
                // 贪心降级：先预分配下界（保证 ≥1），再对剩余库存贪心
                let mut v = vec![0u32; free_usage.len()];
                let mut rem2 = free_remaining.clone();
                for (i, &lb) in free_lower.iter().enumerate() {
                    if lb > 0 {
                        for j in 0..free_usage[i].len() {
                            rem2[j] -= free_usage[i][j] * lb;
                        }
                        v[i] = lb;
                    }
                }
                let g = greedy::solve_greedy(&free_usage, &rem2);
                for i in 0..free_usage.len() {
                    v[i] += g[i];
                }
                for (i, &k) in prep.free_indices.iter().enumerate() {
                    xs[k] = v[i];
                }
                algorithm = "greedy";
            }
        }
        // 公平性：保持总套数不变，均衡各组合数量（方差最小，尊重下界）
        let free_totals: Vec<u32> = free_usage.iter().map(|u| u.iter().sum()).collect();
        let mut free_xs: Vec<u32> = prep.free_indices.iter().map(|&k| xs[k]).collect();
        balance_solution(&free_usage, &free_remaining, &free_totals, &free_lower, &mut free_xs);
        for (i, &k) in prep.free_indices.iter().enumerate() {
            xs[k] = free_xs[i];
        }
    }

    Ok(assemble(request, &prep, &xs, algorithm))
}

/// 遍历备选方案：
/// 循环执行 ILP，每得到一个方案立即保存，并追加 no-good cut 屏蔽该解，
/// 直至无新方案可求（或达到 max_solutions 上限 / 总时间预算耗尽）。
/// 返回 (方案列表, 是否因超时截断)。
pub fn enumerate_solutions(
    request: &SolveRequest,
    max_solutions: usize,
) -> Result<(Vec<SolveResult>, bool), String> {
    let prep = prepare(request)?;
    let max = max_solutions.max(1);

    // 无自由组合（全部手动指定）：只有一种方案，无截断
    if prep.free_indices.is_empty() {
        let xs = prep.xs.clone();
        return Ok((vec![assemble(request, &prep, &xs, "manual-only")], false));
    }

    let free_usage: Vec<Vec<u32>> = prep
        .free_indices
        .iter()
        .map(|&k| prep.usage[k].clone())
        .collect();
    let free_remaining = prep.remaining.clone();

    // E1 修复：枚举总时间预算 + 单轮超时上限，防止线程堆积与时长失控
    const TOTAL_BUDGET_MS: u64 = 10_000;
    const ROUND_TIMEOUT_MS: u64 = 1_000;

    let mut cuts: Vec<ilp::Cut> = Vec::new();
    let mut results: Vec<SolveResult> = Vec::new();
    // 已保存的自由组合数量向量（防御性去重；no-good cut 已保证互异）
    let mut seen: std::collections::HashSet<Vec<u32>> = std::collections::HashSet::new();
    let mut xs = prep.xs.clone();
    let mut budget_left = TOTAL_BUDGET_MS;
    let mut truncated = false;

    while results.len() < max {
        let round_timeout = ROUND_TIMEOUT_MS.min(budget_left);
        if round_timeout == 0 {
            truncated = true;
            break;
        }
        let t0 = std::time::Instant::now();
        match ilp::solve_ilp_detailed(
            &free_usage,
            &free_remaining,
            &prep.lower,
            &cuts,
            round_timeout,
        ) {
            ilp::IlpOutcome::Solved(v) => {
                // 自由组合全 0 且无手动输入 → 无可行正解，视为遍历结束（不保存空方案）
                if v.iter().all(|&x| x == 0) && prep.xs.iter().all(|&x| x == 0) {
                    break;
                }
                // 保存 ILP 原始解（不做均衡化：均衡化的等价类转移可能把不同原始解
                // 映射到同一展示解，导致重复；互不相同的备选方案是枚举的目标）
                if !seen.insert(v.clone()) {
                    // 防御：理论上不会发生（no-good cut 已屏蔽该解）
                    break;
                }
                // 屏蔽当前解，进入下一轮
                cuts.push(no_good_cut(&v));
                for (i, &k) in prep.free_indices.iter().enumerate() {
                    xs[k] = v[i];
                }
                results.push(assemble(request, &prep, &xs, "ilp (备选方案)"));
            }
            // E2 修复：区分"穷尽"（无解）与"截断"（超时）
            ilp::IlpOutcome::Infeasible => break, // 已穷尽所有方案
            ilp::IlpOutcome::TimedOut => {
                truncated = true;
                break;
            }
        }
        budget_left = budget_left.saturating_sub(t0.elapsed().as_millis() as u64);
    }

    Ok((results, truncated))
}

/// 均衡化后处理（公平性）：在保持总套数（最优值）不变的前提下，
/// 通过"总量守恒转移"使各组合数量尽可能均衡（组合数量方差最小）。
///
/// 转移单位：对组合对 (a,b)，g×totals[a] == h×totals[b] 时，
/// x_a 减 g、x_b 加 h 不改变总套数（均值不变），Σx² 减小即方差减小。
pub fn balance_solution(
    usage: &[Vec<u32>],
    remaining: &[u32],
    totals: &[u32],
    lower: &[u32],
    xs: &mut [u32],
) {
    let n = xs.len();
    if n < 2 {
        return;
    }
    loop {
        let mut improved = false;
        for a in 0..n {
            if xs[a] == 0 || totals[a] == 0 {
                continue;
            }
            for b in 0..n {
                if a == b || totals[b] == 0 {
                    continue;
                }
                let (g, h) = transfer_unit(totals[a], totals[b]);
                if g > xs[a] {
                    continue;
                }
                let new_xa = xs[a] - g;
                let new_xb = xs[b] + h;
                // 下界保护：转移后不得低于"≥1"等数量下界
                if new_xa < lower.get(a).copied().unwrap_or(0) {
                    continue;
                }
                // 校验新解不违反任何户型库存约束
                let mut feasible = true;
                'check: for j in 0..remaining.len() {
                    let mut total_use: u64 = 0;
                    for k in 0..n {
                        let v = if k == a {
                            new_xa
                        } else if k == b {
                            new_xb
                        } else {
                            xs[k]
                        };
                        total_use += v as u64 * usage[k][j] as u64;
                    }
                    if total_use > remaining[j] as u64 {
                        feasible = false;
                        break 'check;
                    }
                }
                if !feasible {
                    continue;
                }
                // 均值不变时 Σx² 减小 ⟺ 方差减小
                let cur_sum_sq = xs[a] as i64 * xs[a] as i64 + xs[b] as i64 * xs[b] as i64;
                let new_sum_sq = new_xa as i64 * new_xa as i64 + new_xb as i64 * new_xb as i64;
                if new_sum_sq < cur_sum_sq {
                    xs[a] = new_xa;
                    xs[b] = new_xb;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
}

/// 求 (g, h)：g×ta == h×tb，且为最小正整数组合（基于最小公倍数）
fn transfer_unit(ta: u32, tb: u32) -> (u32, u32) {
    let g_cd = gcd(ta, tb) as u64;
    let lcm = (ta as u64 / g_cd) * tb as u64;
    ((lcm / ta as u64) as u32, (lcm / tb as u64) as u32)
}

fn gcd(a: u32, b: u32) -> u32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> SolveRequest {
        SolveRequest {
            house_types: vec![
                HouseType { id: "t1".into(), name: "50㎡".into(), quantity: 20 },
                HouseType { id: "t2".into(), name: "70㎡".into(), quantity: 30 },
                HouseType { id: "t3".into(), name: "90㎡".into(), quantity: 25 },
            ],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "组合A".into(),
                    color: None,
                    items: vec![
                        CombinationItem { type_id: "t1".into(), count: 1 },
                        CombinationItem { type_id: "t2".into(), count: 2 },
                        CombinationItem { type_id: "t3".into(), count: 1 },
                    ],
                },
                Combination {
                    id: "c2".into(),
                    name: "组合B".into(),
                    color: None,
                    items: vec![
                        CombinationItem { type_id: "t2".into(), count: 1 },
                        CombinationItem { type_id: "t3".into(), count: 2 },
                    ],
                },
            ],
            manual_inputs: vec![],
            min_one_combination_ids: vec![],
        }
    }

    #[test]
    fn solves_without_manual() {
        let req = setup();
        let res = solve(&req).unwrap();
        // 20+30+25 = 75 套
        assert_eq!(res.total_remaining + res.total_used, 75);
        assert!(!res.assignments.is_empty());
    }

    #[test]
    fn solves_with_manual() {
        let mut req = setup();
        req.manual_inputs = vec![ManualInput { combination_id: "c1".into(), quantity: 2 }];
        let res = solve(&req).unwrap();
        // 组合A 用掉 2 个：t1=2, t2=4, t3=2
        // 剩余 t1=18, t2=26, t3=23
        let c1 = res.assignments.iter().find(|a| a.combination_id == "c1").unwrap();
        assert_eq!(c1.quantity, 2);
        assert!(c1.is_manual);
    }

    #[test]
    fn rejects_invalid_manual() {
        let mut req = setup();
        // 组合A 需要 1 个 t1，库存 20，50 个显然超出 t1 库存
        req.manual_inputs = vec![ManualInput { combination_id: "c1".into(), quantity: 999 }];
        assert!(solve(&req).is_err());
    }

    #[test]
    fn empty_combo_is_skipped_not_hang() {
        // 空组合（未定义任何房源）自动跳过、数量为 0，不阻塞求解
        let mut req = setup();
        req.combinations = vec![
            Combination {
                id: "c1".into(),
                name: "组合A".into(),
                color: None,
                items: vec![
                    CombinationItem { type_id: "t1".into(), count: 1 },
                    CombinationItem { type_id: "t2".into(), count: 2 },
                    CombinationItem { type_id: "t3".into(), count: 1 },
                ],
            },
            Combination {
                id: "c_empty".into(),
                name: "空组合".into(),
                color: None,
                items: vec![],
            },
        ];
        let res = solve(&req).unwrap();
        assert!(!res.assignments.iter().any(|a| a.combination_id == "c_empty"));
        assert!(res.total_used > 0);
        assert_eq!(res.total_used + res.total_remaining, 75);
    }

    #[test]
    fn balance_prefers_even_distribution() {
        // 实验2场景：单户型 10 套；组合1=(1) 1套、组合2=(2) 2套
        // 所有 x1 + 2×x2 = 10 均为最优；均衡化应得到方差最小的 x1=2, x2=4
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 10 }],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "组合1".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
                },
                Combination {
                    id: "c2".into(),
                    name: "组合2".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 2 }],
                },
            ],
            manual_inputs: vec![],
            min_one_combination_ids: vec![],
        };
        let res = solve(&req).unwrap();
        assert_eq!(res.total_used, 10, "总套数必须保持最优");
        let x1 = res.assignments.iter().find(|a| a.combination_id == "c1").map(|a| a.quantity).unwrap_or(0);
        let x2 = res.assignments.iter().find(|a| a.combination_id == "c2").map(|a| a.quantity).unwrap_or(0);
        assert_eq!(x1 + 2 * x2, 10);
        // 方差最小解为 (2, 4)；至少不得是独占解 (10,0) 或 (0,5)
        assert!(x1 >= 1 && x2 >= 3, "均衡化失败: x1={x1}, x2={x2}");
    }

    #[test]
    fn balance_removes_order_bias() {
        // 实验3场景：两个相同组合（各 1 套同户型），库存 6
        // 均衡化应均分 3/3，而非"先定义 6、后定义 0"
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 6 }],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "先定义".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
                },
                Combination {
                    id: "c2".into(),
                    name: "后定义".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
                },
            ],
            manual_inputs: vec![],
            min_one_combination_ids: vec![],
        };
        let res = solve(&req).unwrap();
        let x1 = res.assignments.iter().find(|a| a.combination_id == "c1").map(|a| a.quantity).unwrap_or(0);
        let x2 = res.assignments.iter().find(|a| a.combination_id == "c2").map(|a| a.quantity).unwrap_or(0);
        assert_eq!(x1 + x2, 6);
        assert_eq!(x1, 3);
        assert_eq!(x2, 3);
    }

    #[test]
    fn transfer_unit_math() {
        assert_eq!(transfer_unit(1, 2), (2, 1)); // 2×1 == 1×2
        assert_eq!(transfer_unit(2, 3), (3, 2)); // 3×2 == 2×3
        assert_eq!(transfer_unit(4, 4), (1, 1));
        assert_eq!(transfer_unit(6, 4), (2, 3)); // lcm=12
    }

    #[test]
    fn min_one_forces_inclusion() {
        // 单户型 10 套；组合1=(1)、组合2=(2)
        // 未勾选时组合1 可为 0（最优解可能全部由组合2 构成）；
        // 勾选组合1 "≥1" 后，组合1 数量必须 ≥ 1
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 10 }],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "组合1".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
                },
                Combination {
                    id: "c2".into(),
                    name: "组合2".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 2 }],
                },
            ],
            manual_inputs: vec![],
            min_one_combination_ids: vec!["c1".into()],
        };
        let res = solve(&req).unwrap();
        let x1 = res.assignments.iter().find(|a| a.combination_id == "c1").map(|a| a.quantity).unwrap_or(0);
        assert!(x1 >= 1, "勾选 ≥1 后组合1 数量不得为 0，实际 {x1}");
        assert_eq!(res.total_used, 10, "总套数仍应保持最优");
    }

    #[test]
    fn min_one_insufficient_stock_errors() {
        // 单户型 2 套；组合1 需要 3 套，勾选 ≥1 → 库存不足应报错
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 2 }],
            combinations: vec![Combination {
                id: "c1".into(),
                name: "组合1".into(),
                color: None,
                items: vec![CombinationItem { type_id: "t1".into(), count: 3 }],
            }],
            manual_inputs: vec![],
            min_one_combination_ids: vec!["c1".into()],
        };
        assert!(solve(&req).is_err());
    }

    #[test]
    fn min_one_overlap_errors() {
        // 单户型 4 套；组合1=(3)、组合2=(3) 均勾选 ≥1 → 合计 6 > 4，应报错
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 4 }],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "组合1".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 3 }],
                },
                Combination {
                    id: "c2".into(),
                    name: "组合2".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 3 }],
                },
            ],
            manual_inputs: vec![],
            min_one_combination_ids: vec!["c1".into(), "c2".into()],
        };
        assert!(solve(&req).is_err());
    }

    #[test]
    fn min_one_with_manual_no_conflict() {
        // 手动指定组合1 = 5（已 ≥1），同时勾选 ≥1 → 不冲突，数量仍为 5
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 10 }],
            combinations: vec![Combination {
                id: "c1".into(),
                name: "组合1".into(),
                color: None,
                items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
            }],
            manual_inputs: vec![ManualInput { combination_id: "c1".into(), quantity: 5 }],
            min_one_combination_ids: vec!["c1".into()],
        };
        let res = solve(&req).unwrap();
        let x1 = res.assignments.iter().find(|a| a.combination_id == "c1").unwrap();
        assert_eq!(x1.quantity, 5);
        assert!(x1.is_manual);
    }

    #[test]
    fn min_one_with_enumerate_preserved() {
        // 枚举备选方案时，"≥1"下界在每个方案中都保留
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 10 }],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "组合1".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
                },
                Combination {
                    id: "c2".into(),
                    name: "组合2".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 2 }],
                },
            ],
            manual_inputs: vec![],
            min_one_combination_ids: vec!["c1".into()],
        };
        let (sols, truncated) = enumerate_solutions(&req, 50).unwrap();
        assert!(!truncated, "穷尽场景不应标记截断");
        assert!(sols.len() >= 2, "应至少 2 个方案");
        for s in &sols {
            let x1 = s.assignments.iter().find(|a| a.combination_id == "c1").map(|a| a.quantity).unwrap_or(0);
            assert!(x1 >= 1, "枚举方案中组合1 数量不得为 0");
        }
    }

    #[test]
    fn enumerate_returns_distinct_solutions() {
        // 单户型 10 套；组合1=(1)、组合2=(2)
        // 首个方案为最优（总套数 10），后续为不同（可能次优）方案，且互不相同
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 10 }],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "组合1".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
                },
                Combination {
                    id: "c2".into(),
                    name: "组合2".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 2 }],
                },
            ],
            manual_inputs: vec![],
            min_one_combination_ids: vec![],
        };
        let (sols, truncated) = enumerate_solutions(&req, 50).unwrap();
        assert!(!truncated, "穷尽场景不应标记截断");
        assert!(!sols.is_empty());
        // 首方案必须为最优（总套数 10）
        assert_eq!(sols[0].total_used, 10);
        // 方案互不相同（按 组合id+数量 对比较，避免不同组合数量巧合相同）
        let key = |s: &SolveResult| -> Vec<(String, u32)> {
            s.assignments
                .iter()
                .map(|a| (a.combination_id.clone(), a.quantity))
                .collect()
        };
        for i in 0..sols.len() {
            for j in i + 1..sols.len() {
                assert_ne!(key(&sols[i]), key(&sols[j]), "方案 {i} 与 {j} 重复");
            }
        }
        assert!(sols.len() >= 2, "应至少得到 2 个不同方案");
    }

    #[test]
    fn enumerate_exhausts_until_no_new() {
        // 单户型 3 套；组合1=(1) → 方案：3、2、1 共 3 个；全 0 空方案被过滤后无解
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 3 }],
            combinations: vec![Combination {
                id: "c1".into(),
                name: "组合1".into(),
                color: None,
                items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
            }],
            manual_inputs: vec![],
            min_one_combination_ids: vec![],
        };
        let (sols, truncated) = enumerate_solutions(&req, 50).unwrap();
        assert!(!truncated, "穷尽场景不应标记截断");
        assert_eq!(sols.len(), 3);
        let q: Vec<u32> = sols.iter().map(|s| s.total_used).collect();
        assert_eq!(q, vec![3, 2, 1]);
    }

    #[test]
    fn enumerate_respects_max() {
        let req = setup();
        let (sols, _truncated) = enumerate_solutions(&req, 2).unwrap();
        assert!(sols.len() <= 2);
    }

    #[test]
    fn enumerate_manual_only_single() {
        // 两个组合均手动指定 → 无自由组合 → 仅一个方案
        let req = SolveRequest {
            house_types: vec![HouseType { id: "t1".into(), name: "A".into(), quantity: 10 }],
            combinations: vec![
                Combination {
                    id: "c1".into(),
                    name: "组合1".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 1 }],
                },
                Combination {
                    id: "c2".into(),
                    name: "组合2".into(),
                    color: None,
                    items: vec![CombinationItem { type_id: "t1".into(), count: 2 }],
                },
            ],
            manual_inputs: vec![
                ManualInput { combination_id: "c1".into(), quantity: 2 },
                ManualInput { combination_id: "c2".into(), quantity: 3 },
            ],
            min_one_combination_ids: vec![],
        };
        let (sols, _truncated) = enumerate_solutions(&req, 10).unwrap();
        assert_eq!(sols.len(), 1);
        assert_eq!(sols[0].total_used, 2 + 6);
    }
}
