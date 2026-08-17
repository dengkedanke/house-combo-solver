import { useAppStore } from '../store/useAppStore';
import { getCombinationColor } from '../utils/colors';
import { UNUSED_COLOR } from '../utils/colors';

export default function ResultPanel() {
  const solveResult = useAppStore((s) => s.solveResult);
  const houseTypes = useAppStore((s) => s.houseTypes);
  const combinations = useAppStore((s) => s.combinations);
  const solving = useAppStore((s) => s.calculating);
  // 备选方案
  const solutions = useAppStore((s) => s.solutions);
  const activeSolutionIndex = useAppStore((s) => s.activeSolutionIndex);
  const enumerating = useAppStore((s) => s.enumerating);
  const selectSolution = useAppStore((s) => s.selectSolution);
  const enumerateTruncated = useAppStore((s) => s.enumerateTruncated);
  // "≥1"约束（结果中标注该组合数量最小值为 1）
  const minOneIds = useAppStore((s) => s.minOneIds);

  // 优先展示当前选中的备选方案；否则展示单次计算结果
  const result =
    activeSolutionIndex !== null && solutions[activeSolutionIndex]
      ? solutions[activeSolutionIndex]
      : solveResult;

  if (!result) {
    return (
      <aside className="result-panel">
        <h2>计算结果</h2>
        <div className="result-empty">
          {enumerating
            ? '正在遍历备选方案…'
            : solving
              ? '计算中…'
              : '暂无结果，点击"计算最优解"'}
        </div>
      </aside>
    );
  }

  const hasPlans = solutions.length > 0;
  const planLabel =
    activeSolutionIndex !== null ? `方案 ${activeSolutionIndex + 1}` : null;

  return (
    <aside className="result-panel">
      <div className="section-header">
        <h2>计算结果</h2>
        {planLabel && <span className="tag-plan">{planLabel}</span>}
      </div>

      {/* 备选方案标签栏：点击切换，选中高亮；方案多时竖向滚动 */}
      {hasPlans && (
        <>
          <div className="solution-tabs" role="tablist" aria-label="备选方案">
            {solutions.map((_, i) => (
              <button
                key={i}
                role="tab"
                aria-selected={i === activeSolutionIndex}
                className={`solution-tab ${i === activeSolutionIndex ? 'active' : ''}`}
                onClick={() => selectSolution(i)}
              >
                方案{i + 1}
              </button>
            ))}
          </div>
          {/* E2：超时截断提示——区分"已穷尽"与"仅部分方案" */}
          {enumerateTruncated && (
            <p className="enumerate-truncated">已截断：求解超时，仅显示部分方案</p>
          )}
        </>
      )}

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
                {minOneIds.includes(a.combinationId) && <span className="tag-minone">≥1</span>}
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
