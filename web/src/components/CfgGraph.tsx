import React, { useMemo, useState } from 'react';
import type { CfgEdgeInfo, CfgNodeInfo } from '../types';

interface Props {
  nodes: CfgNodeInfo[];
  edges: CfgEdgeInfo[];
  selectedBlock: number | null;
  onSelectBlock: (id: number) => void;
}

export const CfgGraph: React.FC<Props> = ({ nodes, edges, selectedBlock, onSelectBlock }) => {
  const [zoom, setZoom] = useState(1);

  const layout = useMemo(() => {
    const nodeW = 240;
    const nodeH = 74;
    const gapX = 40;
    const gapY = 44;

    // Simple layered BFS layout
    const adj = new Map<number, number[]>();
    for (const n of nodes) adj.set(n.id, []);
    for (const e of edges) {
      if (e.kind !== 'backedge') {
        adj.get(e.from)?.push(e.to);
      }
    }

    const level = new Map<number, number>();
    if (nodes.length > 0) {
      const q = [nodes[0].id];
      level.set(nodes[0].id, 0);
      while (q.length > 0) {
        const cur = q.shift()!;
        const lv = level.get(cur) ?? 0;
        for (const nxt of adj.get(cur) ?? []) {
          if (!level.has(nxt)) {
            level.set(nxt, lv + 1);
            q.push(nxt);
          }
        }
      }
      for (const n of nodes) {
        if (!level.has(n.id)) level.set(n.id, 0);
      }
    }

    const byLevel = new Map<number, CfgNodeInfo[]>();
    for (const n of nodes) {
      const lv = level.get(n.id) ?? 0;
      const arr = byLevel.get(lv) ?? [];
      arr.push(n);
      byLevel.set(lv, arr);
    }

    const pos = new Map<number, { x: number; y: number }>();
    let maxW = 600;
    let maxH = 320;
    for (const [lv, arr] of byLevel.entries()) {
      arr.forEach((n, idx) => {
        const x = 32 + idx * (nodeW + gapX);
        const y = 32 + lv * (nodeH + gapY);
        pos.set(n.id, { x, y });
        maxW = Math.max(maxW, x + nodeW + 40);
        maxH = Math.max(maxH, y + nodeH + 40);
      });
    }
    return { pos, nodeW, nodeH, maxW, maxH };
  }, [nodes, edges]);

  return (
    <div className="cfg-wrap">
      <div style={{ position: 'sticky', top: 8, left: 8, zIndex: 2, padding: 8, display: 'flex', gap: 6 }}>
        <button className="btn" onClick={() => setZoom((z) => Math.min(2, z + 0.15))}>+</button>
        <button className="btn" onClick={() => setZoom((z) => Math.max(0.4, z - 0.15))}>-</button>
        <button className="btn" onClick={() => setZoom(1)}>100%</button>
      </div>
      <svg
        width={layout.maxW * zoom}
        height={layout.maxH * zoom}
        viewBox={`0 0 ${layout.maxW} ${layout.maxH}`}
      >
        <defs>
          <marker id="arrow-edge" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto">
            <path d="M0,0 L0,6 L7,3 z" fill="#38bdf8" />
          </marker>
          <marker id="arrow-back" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto">
            <path d="M0,0 L0,6 L7,3 z" fill="#fbbf24" />
          </marker>
        </defs>

        {edges.map((e, i) => {
          const from = layout.pos.get(e.from);
          const to = layout.pos.get(e.to);
          if (!from || !to) return null;
          const x1 = from.x + layout.nodeW / 2;
          const y1 = from.y + layout.nodeH;
          const x2 = to.x + layout.nodeW / 2;
          const y2 = to.y;
          const isBack = e.kind === 'backedge';
          const d = isBack
            ? `M ${x1} ${y1} C ${x1 + 90} ${y1 + 40}, ${x2 + 90} ${y2 - 40}, ${x2} ${y2}`
            : `M ${x1} ${y1} L ${x2} ${y2}`;
          return (
            <path
              key={i}
              d={d}
              fill="none"
              stroke={isBack ? '#fbbf24' : '#38bdf8'}
              strokeWidth={1.6}
              strokeDasharray={isBack ? '5 3' : undefined}
              markerEnd={isBack ? 'url(#arrow-back)' : 'url(#arrow-edge)'}
            />
          );
        })}

        {nodes.map((n) => {
          const p = layout.pos.get(n.id);
          if (!p) return null;
          const active = selectedBlock === n.id;
          return (
            <g
              key={n.id}
              transform={`translate(${p.x}, ${p.y})`}
              onClick={() => onSelectBlock(n.id)}
              style={{ cursor: 'pointer' }}
            >
              <rect
                width={layout.nodeW}
                height={layout.nodeH}
                rx={6}
                fill={active ? '#16283d' : '#101520'}
                stroke={active ? '#38bdf8' : '#242e42'}
                strokeWidth={active ? 2 : 1}
              />
              <text x={10} y={20} fill="#38bdf8" fontFamily="JetBrains Mono, monospace" fontSize={12} fontWeight={600}>
                {`bb${n.id}  0x${n.guest_pc.toString(16).padStart(8, '0')}`}
              </text>
              <text x={10} y={40} fill="#93a4bd" fontFamily="JetBrains Mono, monospace" fontSize={11}>
                {`${n.lines.length} RV32 insts -> ${n.inst_count} IR ops`}
              </text>
              <text x={10} y={58} fill="#62728d" fontFamily="JetBrains Mono, monospace" fontSize={10}>
                {n.lines[0]?.text ?? ''}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
};
