// 程序 Logo：3×3 房源网格，四种组合色块 + 高亮中心格
// 呼应产品核心可视化（总套数 = 方格数、每种组合一种颜色）
export default function Logo({ size = 24 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 48 48"
      role="img"
      aria-label="房源组合最优解计算器"
    >
      <defs>
        <linearGradient id="logo-bg" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#2f6fed" />
          <stop offset="1" stopColor="#0f8a68" />
        </linearGradient>
      </defs>
      {/* 圆角背景 */}
      <rect width="48" height="48" rx="11" fill="url(#logo-bg)" />
      {/* 3×3 网格：代表房源单元，颜色代表所属组合 */}
      <rect x="3.5" y="3.5" width="13" height="13" rx="3" fill="#378ADD" />
      <rect x="17.5" y="3.5" width="13" height="13" rx="3" fill="#EF9F27" />
      <rect x="31.5" y="3.5" width="13" height="13" rx="3" fill="rgba(255,255,255,0.30)" />
      <rect x="3.5" y="17.5" width="13" height="13" rx="3" fill="#639922" />
      <rect x="17.5" y="17.5" width="13" height="13" rx="3" fill="#ffffff" />
      <rect x="31.5" y="17.5" width="13" height="13" rx="3" fill="#D85A30" />
      <rect x="3.5" y="31.5" width="13" height="13" rx="3" fill="rgba(255,255,255,0.30)" />
      <rect x="17.5" y="31.5" width="13" height="13" rx="3" fill="rgba(255,255,255,0.55)" />
      <rect x="31.5" y="31.5" width="13" height="13" rx="3" fill="rgba(255,255,255,0.30)" />
    </svg>
  );
}
