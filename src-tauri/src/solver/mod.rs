pub mod greedy;
pub mod ilp;

use crate::models::*;
use std::collections::HashMap;

/// 均衡化后处理（公平性）：在保持总套数（最优值）不变的前提下，
/// 通过"总量守恒转移"使各组合数量尽可能均衡（组合数量方差最小）。
///
/// 转移单位：对组合对 (a,b)，g×totals[a] == h×totals[b] 时，
/// x_a 减 g、x_b 加 h 不改变总套数（均值不变），Σx² 减小即方差减小。
pub fn balance_solution(usage: &[Vec<u32>], remaining: &[u32], totals: &[u32], xs: &mut [u32]) {
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

/// 求解入口：
/// 1. 校验输入合法性
/// 2. 扣除手动指定组合占用的房源
/// 3. ILP 求解自由组合（失败降级为贪心）
/// 4. 组装结果
pub fn solve(request: &SolveRequest) -> Result<SolveResult, String> {
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
            return Err(format!("组合 {} 未包含任何房源，无法手动指定数量", request.combinations[k].name));
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

    // 求解自由组合（仅有效组合，未定义的空组合自动跳过，数量为 0）
    let free_indices: Vec<usize> = (0..num_combos).filter(|&k| !manual[k] && valid[k]).collect();
    let algorithm: &str;

    if free_indices.is_empty() {
        algorithm = "manual-only";
    } else {
        let free_usage: Vec<Vec<u32>> = free_indices.iter().map(|&k| usage[k].clone()).collect();
        let free_remaining = remaining.clone();
        match ilp::solve_ilp(&free_usage, &free_remaining) {
            Some(v) => {
                for (i, &k) in free_indices.iter().enumerate() {
                    xs[k] = v[i];
                }
                algorithm = "ilp";
            }
            None => {
                let v = greedy::solve_greedy(&free_usage, &free_remaining);
                for (i, &k) in free_indices.iter().enumerate() {
                    xs[k] = v[i];
                }
                algorithm = "greedy";
            }
        }
        // 公平性：保持总套数不变，均衡各组合数量（方差最小）
        let free_totals: Vec<u32> = free_usage.iter().map(|u| u.iter().sum()).collect();
        let mut free_xs: Vec<u32> = free_indices.iter().map(|&k| xs[k]).collect();
        balance_solution(&free_usage, &free_remaining, &free_totals, &mut free_xs);
        for (i, &k) in free_indices.iter().enumerate() {
            xs[k] = free_xs[i];
        }
    }

    // 组装结果
    let mut assignments = Vec::new();
    for k in 0..num_combos {
        if xs[k] > 0 || manual[k] {
            assignments.push(CombinationAssignment {
                combination_id: request.combinations[k].id.clone(),
                combination_name: request.combinations[k].name.clone(),
                quantity: xs[k],
                is_manual: manual[k],
            });
        }
    }

    let mut used_by_type = vec![0u32; num_types];
    for k in 0..num_combos {
        for j in 0..num_types {
            used_by_type[j] += xs[k].saturating_mul(usage[k][j]);
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

    Ok(SolveResult {
        assignments,
        remaining: remaining_items,
        total_used,
        total_remaining,
        solve_time_ms: 0,
        algorithm: algorithm.to_string(),
    })
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
}
