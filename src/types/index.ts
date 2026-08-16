// 与 Rust 后端 models.rs 保持一致的数据类型

export interface HouseType {
  id: string;
  name: string;
  quantity: number;
}

export interface CombinationItem {
  typeId: string;
  count: number;
}

export interface Combination {
  id: string;
  name: string;
  color?: string;
  items: CombinationItem[];
}

export interface ManualInput {
  combinationId: string;
  quantity: number;
}

export interface SolveRequest {
  houseTypes: HouseType[];
  combinations: Combination[];
  manualInputs: ManualInput[];
}

export interface CombinationAssignment {
  combinationId: string;
  combinationName: string;
  quantity: number;
  isManual: boolean;
}

export interface RemainingItem {
  typeId: string;
  typeName: string;
  remaining: number;
}

export interface SolveResult {
  assignments: CombinationAssignment[];
  remaining: RemainingItem[];
  totalUsed: number;
  totalRemaining: number;
  solveTimeMs: number;
  algorithm: string;
}

// 持久化配置
export interface AppConfig {
  houseTypes: HouseType[];
  combinations: Combination[];
}

// 网格格子（供可视化使用）
export interface GridCell {
  index: number;
  combinationId: string | null; // null = 未使用
  typeId: string | null;
  color: string | null;
}
