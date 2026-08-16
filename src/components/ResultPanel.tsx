import { useAppStore } from '../store/useAppStore';
import { getCombinationColor } from '../utils/colors';
import { UNUSED_COLOR } from '../utils/colors';

export default function ResultPanel() {
  const solveResult = useAppStore((s) => s.solveResult);
  const houseTypes = useAppStore((s) => s.houseTypes);
  const combinations = useAppStore((s) => s.combinations);
  const solving = useAppStore((s) => s.calculating);

  if (!solveResult) {
    return (
      <aside className="result-panel">
        <h2>计算结果</h2>
        <div className="result-empty">
          {solving ? '计算中…' : '暂无结果，点击"计算最优解"'}
        </div>
      </aside>
    );
  }

  const result = solveResult;

  return (
    <aside className="result-panel">
      <div className="section-header">
        <h2>计算结果</h2>
      </div>

      <div className="metric-grid">
        <div className="metric-card">
          <span className="metric-label">已使用</span>
          <strong className="metric-value">{result.totalUsed}</strong>
          <span className="muted">套</span>
        </div>
        <div className="metric-card">
          <span className="metric-label">剩余</span>
          <strong className="metric-value warn">{result.totalRemaining}</strong>
          <span className="muted">套</span>
        </div>
      </div>

      <div className="result-block">
        <h4>组合分配</h4>
        <ul className="result-list">
          {result.assignments.map((a) => {
            const idx = combinations.findIndex((c) => c.id === a.combinationId);
            const color = idx >= 0 ? getCombinationColor(idx) : '#888';
            return (
              <li key={a.combinationId} className="result-row">
                <span className="color-dot" style={{ background: color }} />
                <span className="result-name">{a.combinationName}</span>
                <span className="result-qty">{a.quantity}</span>
                <span className="muted">个</span>
                {a.isManual && <span className="tag-manual">手动</span>}
              </li>
            );
          })}
        </ul>
      </div>

      <div className="result-block">
        <h4>剩余明细</h4>
        <ul className="result-list">
          {result.remaining.map((r) => (
            <li key={r.typeId} className="result-row">
              <span className="legend-dot" style={{ background: UNUSED_COLOR }} />
              <span className="result-name">{r.typeName}</span>
              <span className={`result-qty ${r.remaining > 0 ? 'warn' : ''}`}>{r.remaining}</span>
              <span className="muted">套</span>
            </li>
          ))}
          {houseTypes.length === 0 && <li className="muted">暂无户型</li>}
        </ul>
      </div>

      <div className="result-meta">
        <span className="muted">算法: {result.algorithm}</span>
        <span className="muted">耗时: {result.solveTimeMs} ms</span>
      </div>
    </aside>
  );
}
