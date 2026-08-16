import { useEffect, useMemo, useRef, useState } from 'react';
import { useAppStore } from '../store/useAppStore';
import { computeGridLayout } from '../utils/grid';
import { getCombinationColor, UNUSED_COLOR } from '../utils/colors';
import type { Combination, HouseType, SolveResult } from '../types';

interface Cell {
  typeId: string;
  typeName: string;
  comboId: string | null;
  comboName: string | null;
  color: string | null;
}

interface Tooltip {
  x: number;
  y: number;
  text: string;
}

function buildCells(
  houseTypes: HouseType[],
  combinations: Combination[],
  result: SolveResult | null,
): Cell[] {
  // 格子顺序 = 按户型顺序排列的房源单元
  const typeSeq: { typeId: string; typeName: string }[] = [];
  for (const t of houseTypes) {
    for (let i = 0; i < t.quantity; i++) {
      typeSeq.push({ typeId: t.id, typeName: t.name });
    }
  }

  // 每格的颜色归属，初始为 null（未使用）
  const colorByIndex: ({ comboId: string; comboName: string; color: string } | null)[] =
    new Array(typeSeq.length).fill(null);

  if (result) {
    // 每种户型的格子索引列表
    const typeIndexMap = new Map<string, number[]>();
    typeSeq.forEach((t, i) => {
      const list = typeIndexMap.get(t.typeId) ?? [];
      list.push(i);
      typeIndexMap.set(t.typeId, list);
    });
    // 每种户型已占用格数（从前往后分配）
    const typeUsed = new Map<string, number>();

    combinations.forEach((c, idx) => {
      const assign = result.assignments.find((a) => a.combinationId === c.id);
      const qty = assign?.quantity ?? 0;
      if (qty <= 0) return;
      const color = getCombinationColor(idx);
      // 按组合包含的户型逐项分配：该户型需要 count × qty 个格子
      for (const item of c.items) {
        if (item.count === 0) continue;
        const need = item.count * qty;
        const indices = typeIndexMap.get(item.typeId) ?? [];
        const used = typeUsed.get(item.typeId) ?? 0;
        for (let k = 0; k < need && used + k < indices.length; k++) {
          colorByIndex[indices[used + k]] = {
            comboId: c.id,
            comboName: c.name,
            color,
          };
        }
        typeUsed.set(item.typeId, used + need);
      }
    });
  }

  return typeSeq.map((t, i) => {
    const color = colorByIndex[i];
    return {
      typeId: t.typeId,
      typeName: t.typeName,
      comboId: color?.comboId ?? null,
      comboName: color?.comboName ?? null,
      color: color?.color ?? null,
    };
  });
}

export default function GridCanvas() {
  const houseTypes = useAppStore((s) => s.houseTypes);
  const combinations = useAppStore((s) => s.combinations);
  const solveResult = useAppStore((s) => s.solveResult);
  const lastSolvedBy = useAppStore((s) => s.lastSolvedBy);
  const setRendering = useAppStore((s) => s.setRendering);

  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number>(0);
  const [size, setSize] = useState({ w: 600, h: 600 });
  const [tooltip, setTooltip] = useState<Tooltip | null>(null);
  const hoverRef = useRef<{ col: number; row: number } | null>(null);

  const total = useMemo(
    () => houseTypes.reduce((acc, t) => acc + t.quantity, 0),
    [houseTypes],
  );
  const cells = useMemo(
    () => buildCells(houseTypes, combinations, solveResult),
    [houseTypes, combinations, solveResult],
  );

  // 自适应容器尺寸
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      setSize({ w: width, h: height });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const layout = computeGridLayout(total);

  const drawFrame = (progress: number) => {
    const canvas = canvasRef.current;
    if (!canvas || layout.rows === 0 || layout.cols === 0) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const pad = 8;
    const gap = Math.min(2, (Math.min(size.w, size.h) - pad * 2) / (Math.max(layout.rows, layout.cols) * 8));
    const cell = Math.min(
      (size.w - pad * 2) / layout.cols - gap,
      (size.h - pad * 2) / layout.rows - gap,
    );
    const gridW = layout.cols * (cell + gap) - gap;
    const gridH = layout.rows * (cell + gap) - gap;
    const offsetX = (size.w - gridW) / 2;
    const offsetY = (size.h - gridH) / 2;

    // 高 DPI 适配
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== size.w * dpr || canvas.height !== size.h * dpr) {
      canvas.width = size.w * dpr;
      canvas.height = size.h * dpr;
      canvas.style.width = `${size.w}px`;
      canvas.style.height = `${size.h}px`;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.w, size.h);

    const visibleCount = Math.floor(progress * cells.length);
    const hover = hoverRef.current;

    for (let i = 0; i < cells.length; i++) {
      const row = Math.floor(i / layout.cols);
      const col = i % layout.cols;
      const x = offsetX + col * (cell + gap);
      const y = offsetY + row * (cell + gap);
      const isHover = hover !== null && hover.row === row && hover.col === col;

      const cellData = cells[i];
      // 动画已到达且有归属组合的格子涂组合色，其余（未到达/未使用）涂浅灰
      const fill =
        i < visibleCount && cellData.color !== null ? cellData.color : UNUSED_COLOR;

      ctx.fillStyle = fill;
      const r = Math.max(1.5, cell * 0.18);
      ctx.beginPath();
      ctx.roundRect(x, y, cell, cell, r);
      ctx.fill();

      if (isHover) {
        ctx.strokeStyle = '#185FA5';
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    }
  };

  // 动画渲染：依赖 cells 引用（而非仅 length），
  // 保证 solveResult 变化但房源总数不变时也会立即重绘
  useEffect(() => {
    if (rafRef.current) cancelAnimationFrame(rafRef.current);
    if (layout.rows === 0) return;
    // 格子数过多时跳过逐帧动画，一次性绘制，避免界面卡顿
    if (cells.length > 30000) {
      drawFrame(1);
      return;
    }
    setRendering(true);
    const duration = Math.min(450, cells.length * 2 + 150);
    const start = performance.now();
    const tick = (now: number) => {
      const p = Math.min(1, (now - start) / duration);
      // easeOutCubic
      const eased = 1 - Math.pow(1 - p, 3);
      drawFrame(eased);
      if (p < 1) {
        rafRef.current = requestAnimationFrame(tick);
      } else {
        drawFrame(1);
        setRendering(false);
      }
    };
    rafRef.current = requestAnimationFrame(tick);
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      setRendering(false);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cells, layout.rows, layout.cols, size.w, size.h]);

  // 立即显示最终状态（中断动画，用于 hover 交互，避免闪烁）
  const drawFinal = () => {
    if (rafRef.current) cancelAnimationFrame(rafRef.current);
    setRendering(false);
    drawFrame(1);
  };

  const handleMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas || layout.rows === 0 || layout.cols === 0) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    // 重新计算布局参数（与 drawFrame 一致）
    const pad = 8;
    const gap = Math.min(2, (Math.min(size.w, size.h) - pad * 2) / (Math.max(layout.rows, layout.cols) * 8));
    const cell = Math.min(
      (size.w - pad * 2) / layout.cols - gap,
      (size.h - pad * 2) / layout.rows - gap,
    );
    const gridW = layout.cols * (cell + gap) - gap;
    const gridH = layout.rows * (cell + gap) - gap;
    const offsetX = (size.w - gridW) / 2;
    const offsetY = (size.h - gridH) / 2;

    const col = Math.floor((x - offsetX) / (cell + gap));
    const row = Math.floor((y - offsetY) / (cell + gap));
    if (col < 0 || row < 0 || col >= layout.cols || row >= layout.rows) {
      hoverRef.current = null;
      setTooltip(null);
      drawFinal();
      return;
    }
    const index = row * layout.cols + col;
    if (index >= cells.length) {
      hoverRef.current = null;
      setTooltip(null);
      drawFinal();
      return;
    }
    hoverRef.current = { col, row };
    const c = cells[index];
    const text = c.comboName
      ? `${c.typeName} · ${c.comboName}`
      : `${c.typeName} · 未使用`;
    setTooltip({ x: e.clientX - rect.left + 12, y: e.clientY - rect.top + 12, text });
    drawFinal();
  };

  const handleLeave = () => {
    hoverRef.current = null;
    setTooltip(null);
    drawFinal();
  };

  return (
    <main className="grid-canvas-wrap">
      <div className="canvas-header">
        <h2>房源网格</h2>
        <div className="canvas-meta">
          <span>
            共 <strong>{total}</strong> 套
          </span>
          {lastSolvedBy && (
            <span className="muted">
              {lastSolvedBy === 'rust' ? 'Rust ILP' : 'JS 预览'}
            </span>
          )}
        </div>
      </div>

      <div className="canvas-container" ref={containerRef}>
        {total === 0 ? (
          <div className="canvas-empty">请在左侧添加房源类型</div>
        ) : (
          <canvas
            ref={canvasRef}
            onMouseMove={handleMove}
            onMouseLeave={handleLeave}
          />
        )}
        {tooltip && (
          <div
            className="canvas-tooltip"
            style={{ left: tooltip.x, top: tooltip.y }}
          >
            {tooltip.text}
          </div>
        )}
      </div>

      <div className="canvas-legend">
        {combinations.map((c, idx) => {
          const assign = solveResult?.assignments.find((a) => a.combinationId === c.id);
          const qty = assign?.quantity ?? 0;
          if (qty <= 0) return null;
          return (
            <span key={c.id} className="legend-item">
              <span className="legend-dot" style={{ background: getCombinationColor(idx) }} />
              {c.name} × {qty}
            </span>
          );
        })}
        {solveResult && solveResult.totalRemaining > 0 && (
          <span className="legend-item">
            <span className="legend-dot" style={{ background: UNUSED_COLOR }} />
            剩余 {solveResult.totalRemaining}
          </span>
        )}
      </div>
    </main>
  );
}
