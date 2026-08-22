use serde::{Deserialize, Serialize};

/// 户型类型
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HouseType {
    pub id: String,
    pub name: String,
    pub quantity: u32,
}

/// 组合中的一项（某个户型需要几套）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CombinationItem {
    pub type_id: String,
    pub count: u32,
}

/// 房源组合方案
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Combination {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub items: Vec<CombinationItem>,
    /// 组合权重偏好（1-10，默认 5）。分层优化阶段二在保持最大利用率下优先高权重组合。
    #[serde(default = "default_weight")]
    pub weight: u8,
    /// 数量区间下限（默认 0）：该组合至少使用几套（替换原"≥1"勾选，可自由设置）
    #[serde(default)]
    pub min: u32,
    /// 数量区间上限（默认 999）：该组合最多使用几套
    #[serde(default = "default_max")]
    pub max: u32,
}

fn default_weight() -> u8 {
    5
}

fn default_max() -> u32 {
    999
}

/// 手动输入：某个组合指定数量
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ManualInput {
    pub combination_id: String,
    pub quantity: u32,
}

/// 求解请求
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SolveRequest {
    pub house_types: Vec<HouseType>,
    pub combinations: Vec<Combination>,
    pub manual_inputs: Vec<ManualInput>,
}

/// 单个组合的分配结果
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CombinationAssignment {
    pub combination_id: String,
    pub combination_name: String,
    pub quantity: u32,
    pub is_manual: bool,
}

/// 某户型剩余数量
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemainingItem {
    pub type_id: String,
    pub type_name: String,
    pub remaining: u32,
}

/// 求解结果
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SolveResult {
    pub assignments: Vec<CombinationAssignment>,
    pub remaining: Vec<RemainingItem>,
    pub total_used: u32,
    pub total_remaining: u32,
    pub solve_time_ms: u64,
    pub algorithm: String,
}

/// 备选方案遍历结果
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EnumerateResponse {
    pub solutions: Vec<SolveResult>,
    /// 是否因超时截断（true 表示未完整遍历所有方案）
    pub truncated: bool,
}
