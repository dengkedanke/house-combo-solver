// 网格布局计算：尽量正方形排列
export interface GridLayout {
  rows: number;
  cols: number;
}

export function computeGridLayout(total: number): GridLayout {
  if (total <= 0) return { rows: 0, cols: 0 };
  const cols = Math.ceil(Math.sqrt(total));
  const rows = Math.ceil(total / cols);
  return { rows, cols };
}

export function uid(prefix: string): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
}
