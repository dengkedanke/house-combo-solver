import { useState } from 'react';
import { useAppStore } from '../store/useAppStore';
import { getCombinationColor } from '../utils/colors';
import ConfirmDialog from './ConfirmDialog';
import type { Combination } from '../types';

/* ---------- 户型类型编辑器 ---------- */
function HouseTypeEditor() {
  const houseTypes = useAppStore((s) => s.houseTypes);
  const addHouseType = useAppStore((s) => s.addHouseType);
  const updateHouseType = useAppStore((s) => s.updateHouseType);
  const removeHouseType = useAppStore((s) => s.removeHouseType);
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState('');
  const [quantity, setQuantity] = useState('10');

  const submit = () => {
    const n = name.trim();
    const q = Math.max(0, Math.floor(Number(quantity) || 0));
    if (!n || q <= 0) return;
    addHouseType(n, q);
    setName('');
    setQuantity('10');
    setShowForm(false);
  };

  return (
    <section className="panel-section">
      <div className="section-header">
        <h3>房源类型</h3>
        <button className="btn btn-ghost btn-sm" onClick={() => setShowForm((v) => !v)}>
          + 添加
        </button>
      </div>

      {showForm && (
        <div className="inline-form">
          <input
            value={name}
            placeholder="如 50㎡"
            onChange={(e) => setName(e.target.value)}
          />
          <input
            type="number"
            min={1}
            value={quantity}
            onChange={(e) => setQuantity(e.target.value)}
          />
          <button className="btn btn-primary btn-sm" onClick={submit}>
            确定
          </button>
        </div>
      )}

      {houseTypes.length === 0 && <p className="empty-hint">暂无户型，点击"添加"创建</p>}

      <ul className="type-list">
        {houseTypes.map((t) => (
          <li key={t.id} className="type-item">
            <input
              className="type-name-input"
              value={t.name}
              onChange={(e) => updateHouseType(t.id, { name: e.target.value })}
            />
            <div className="type-qty">
              <span className="muted">×</span>
              <input
                type="number"
                min={0}
                className="qty-input"
                value={t.quantity}
                onChange={(e) =>
                  updateHouseType(t.id, { quantity: Math.max(0, Number(e.target.value) || 0) })
                }
              />
              <span className="muted">套</span>
            </div>
            <button className="btn btn-ghost btn-icon" title="删除" onClick={() => removeHouseType(t.id)}>
              ×
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/* ---------- 组合方案编辑器 ---------- */
function CombinationEditor() {
  const houseTypes = useAppStore((s) => s.houseTypes);
  const combinations = useAppStore((s) => s.combinations);
  const addCombination = useAppStore((s) => s.addCombination);
  const removeCombination = useAppStore((s) => s.removeCombination);
  const setCombinationItem = useAppStore((s) => s.setCombinationItem);
  // #11：从 hook 解构，避免在 JSX 中直接 getState()（保持响应式绑定）
  const updateCombination = useAppStore((s) => s.updateCombination);
  const updateComboRange = useAppStore((s) => s.updateComboRange);
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState('');
  // 待删除的组合（非 null 时弹确认框）
  const [pendingDelete, setPendingDelete] = useState<Combination | null>(null);

  const comboTotal = (comboId: string) => {
    const c = combinations.find((x) => x.id === comboId);
    return c ? c.items.reduce((s, i) => s + i.count, 0) : 0;
  };

  const submit = () => {
    if (houseTypes.length === 0) return;
    addCombination(name.trim(), []);
    setName('');
    setShowForm(false);
  };

  return (
    <section className="panel-section">
      <div className="section-header">
        <h3>组合定义</h3>
        <button className="btn btn-ghost btn-sm" onClick={() => setShowForm((v) => !v)} disabled={houseTypes.length === 0}>
          + 添加
        </button>
      </div>

      {showForm && (
        <div className="inline-form">
          <input value={name} placeholder="如 组合A" onChange={(e) => setName(e.target.value)} />
          <button className="btn btn-primary btn-sm" onClick={submit}>
            创建
          </button>
        </div>
      )}

      {combinations.length === 0 && <p className="empty-hint">暂无组合，先添加房源类型再创建组合</p>}

      <ul className="combo-list">
        {combinations.map((c, idx) => (
          <li key={c.id} className="combo-item">
            <div className="combo-head">
              <span className="color-dot" style={{ background: getCombinationColor(idx) }} />
              <input
                className="combo-name-input"
                value={c.name}
                onChange={(e) => updateCombination(c.id, { name: e.target.value })}
              />
              <span className="muted combo-total">{comboTotal(c.id)} 套</span>
              <button
                className="btn btn-ghost btn-icon"
                title="删除"
                onClick={() => setPendingDelete(c)}
              >
                ×
              </button>
            </div>
            {/* 数量区间（替换原 ≥1 勾选）：下限/上限，store 兜底保证 0 ≤ min ≤ max */}
            <div className="combo-range">
              <label className="range-field">
                <span className="muted">下限</span>
                <input
                  type="number"
                  min={0}
                  className="qty-input"
                  value={c.min ?? 0}
                  onChange={(e) => {
                    const raw = Number(e.target.value);
                    const min = Number.isFinite(raw) && raw >= 0 ? Math.floor(raw) : 0;
                    updateComboRange(c.id, min, c.max ?? 999);
                  }}
                />
              </label>
              <label className="range-field">
                <span className="muted">上限</span>
                <input
                  type="number"
                  min={0}
                  className="qty-input"
                  value={c.max ?? 999}
                  onChange={(e) => {
                    const raw = Number(e.target.value);
                    const max = Number.isFinite(raw) && raw >= 0 ? Math.floor(raw) : 0;
                    updateComboRange(c.id, c.min ?? 0, max);
                  }}
                />
              </label>
            </div>
            <div className="combo-items">
              {houseTypes.map((t) => {
                const item = c.items.find((i) => i.typeId === t.id);
                return (
                  <label key={t.id} className="combo-row">
                    <span className="combo-type-name">{t.name}</span>
                    <input
                      type="number"
                      min={0}
                      className="qty-input"
                      value={item?.count ?? 0}
                      onChange={(e) =>
                        setCombinationItem(c.id, t.id, Math.max(0, Number(e.target.value) || 0))
                      }
                    />
                    <span className="muted">套</span>
                  </label>
                );
              })}
              {c.items.length === 0 && <span className="muted combo-empty">点击下方数字设置户型</span>}
            </div>
          </li>
        ))}
      </ul>

      <ConfirmDialog
        open={pendingDelete !== null}
        title="删除组合"
        message={`确定要删除"${pendingDelete?.name ?? ''}"吗？删除后不可恢复`}
        confirmText="确认删除"
        onConfirm={() => {
          if (pendingDelete) removeCombination(pendingDelete.id);
          setPendingDelete(null);
        }}
        onCancel={() => setPendingDelete(null)}
      />
    </section>
  );
}

/* ---------- 组合权重偏好（分层优化阶段二） ---------- */
function WeightPreference() {
  const combinations = useAppStore((s) => s.combinations);
  const updateComboWeight = useAppStore((s) => s.updateComboWeight);

  if (combinations.length === 0) return null;

  return (
    <section className="panel-section">
      <div className="section-header">
        <h3>偏好设置</h3>
      </div>
      <div className="weight-list">
        {combinations.map((c, idx) => (
          <label key={c.id} className="weight-row">
            <span className="color-dot" style={{ background: getCombinationColor(idx) }} />
            <span className="weight-name">{c.name}</span>
            <input
              type="range"
              min={1}
              max={10}
              step={1}
              value={c.weight ?? 5}
              onChange={(e) => updateComboWeight(c.id, Number(e.target.value))}
              className="weight-slider"
            />
            <span className="weight-value">{c.weight ?? 5}</span>
          </label>
        ))}
      </div>
      <p className="weight-hint">💡 权重越高，算法越优先使用该组合（不影响"用完房源"的最终目标）</p>
    </section>
  );
}

/* ---------- 手动输入（固定数量优先于数量区间） ---------- */
function ManualInput() {
  const combinations = useAppStore((s) => s.combinations);
  const manualInputs = useAppStore((s) => s.manualInputs);
  const setManualQuantity = useAppStore((s) => s.setManualQuantity);
  const clearManual = useAppStore((s) => s.clearManual);
  const solve = useAppStore((s) => s.solve);
  const calculating = useAppStore((s) => s.calculating);
  const enumerateSolutions = useAppStore((s) => s.enumerateSolutions);
  const enumerating = useAppStore((s) => s.enumerating);
  const hasManual = manualInputs.some((m) => m.quantity > 0);

  return (
    <section className="panel-section">
      <div className="section-header">
        <h3>指定组合数量</h3>
        {hasManual && (
          <button className="btn btn-ghost btn-sm" onClick={clearManual}>
            清空
          </button>
        )}
      </div>

      {combinations.length === 0 && <p className="empty-hint">暂无组合</p>}

      <div className="manual-list">
        {combinations.map((c, idx) => {
          const v = manualInputs.find((m) => m.combinationId === c.id)?.quantity ?? 0;
          return (
            <label key={c.id} className="manual-row">
              <span className="color-dot" style={{ background: getCombinationColor(idx) }} />
              <span className="manual-name">{c.name}</span>
              <input
                type="number"
                min={0}
                className="qty-input"
                value={v}
                placeholder="自动"
                onChange={(e) => setManualQuantity(c.id, Math.max(0, Number(e.target.value) || 0))}
              />
              <span className="muted">个</span>
            </label>
          );
        })}
      </div>

      <button className="btn btn-primary btn-block" onClick={() => solve()} disabled={calculating}>
        {calculating ? '计算中…' : '计算最优解'}
      </button>

      <button
        className="btn btn-block btn-enumerate"
        onClick={() => enumerateSolutions()}
        disabled={enumerating || calculating}
        title="循环 ILP 求解，生成多个互不相同的备选方案"
      >
        {enumerating ? '遍历中…' : '遍历备选方案'}
      </button>
    </section>
  );
}

/* ---------- 输入面板容器 ---------- */
export default function InputPanel() {
  const total = useAppStore((s) => s.houseTypes.reduce((acc, t) => acc + t.quantity, 0));
  return (
    <aside className="input-panel">
      <div className="panel-summary">
        <span>房源总数</span>
        <strong>{total}</strong>
        <span className="muted">套</span>
      </div>
      <HouseTypeEditor />
      <CombinationEditor />
      <WeightPreference />
      <ManualInput />
      <footer className="panel-footer">Vibed by DengKe with DS-V4-Flash</footer>
    </aside>
  );
}
