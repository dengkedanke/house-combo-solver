// 组合颜色：与规划文档一致，9 种高对比度颜色，超出用 HSL 色环生成
export const COMBINATION_COLORS = [
  '#378ADD', // 蓝
  '#639922', // 绿
  '#D85A30', // 珊瑚
  '#7F77DD', // 紫
  '#EF9F27', // 琥珀
  '#1D9E75', // 青
  '#D4537E', // 粉
  '#E24B4A', // 红
  '#888780', // 灰
];

export const UNUSED_COLOR = '#E2E0DA'; // 未使用方格

const GOLDEN_ANGLE = 137.508;

export function getCombinationColor(index: number): string {
  if (index < COMBINATION_COLORS.length) {
    return COMBINATION_COLORS[index];
  }
  const hue = (index * GOLDEN_ANGLE) % 360;
  return `hsl(${Math.round(hue)}, 60%, 45%)`;
}
