// 旋转沙漏"计算中"指示组件
export default function Sandglass() {
  return (
    <div className="sandglass-overlay" aria-live="polite">
      <div className="sandglass-bg">
        <svg
          className="sandglass-spin"
          viewBox="0 0 24 24"
          width="34"
          height="34"
          role="img"
          aria-label="计算中"
        >
          <path
            d="M6.5 2.5h11v2.2l-4.6 7.3 4.6 7.3v2.2h-11v-2.2l4.6-7.3-4.6-7.3z"
            fill="none"
            stroke="#1d9e75"
            strokeWidth="1.7"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <path
            d="M7.2 5.5h9.6M7.2 18.5h9.6"
            fill="none"
            stroke="#1d9e75"
            strokeWidth="1.2"
            strokeLinecap="round"
          />
          <circle cx="12" cy="12" r="1.6" fill="#1d9e75" />
        </svg>
      </div>
      <span className="sandglass-text">计算中…</span>
    </div>
  );
}
