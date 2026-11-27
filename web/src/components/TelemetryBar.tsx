import React from 'react';
import type { ExecMode, RecompileMeta } from '../types';

interface Props {
  meta: RecompileMeta | null;
  mode: ExecMode;
  fps: number;
  mips: number;
  instret: number;
  speedup: number | null;
  sabEnabled: boolean;
}

export const TelemetryBar: React.FC<Props> = ({
  meta,
  mode,
  fps,
  mips,
  instret,
  speedup,
  sabEnabled,
}) => {
  return (
    <div className="telemetry-bar">
      <div className="metric">
        <span className="metric-label">MODE</span>
        <span className="metric-val">{mode.toUpperCase()}</span>
      </div>
      <div className="metric">
        <span className="metric-label">COMPILE</span>
        <span className="metric-val">{meta ? `${meta.compile_time_ms.toFixed(2)} ms` : '—'}</span>
      </div>
      <div className="metric">
        <span className="metric-label">SIZE</span>
        <span className="metric-val">
          {meta
            ? `${(meta.elf_size / 1024).toFixed(1)} KB -> ${(meta.wasm_size / 1024).toFixed(1)} KB`
            : '—'}
        </span>
      </div>
      <div className="metric">
        <span className="metric-label">FNS</span>
        <span className="metric-val">{meta ? meta.function_count : '—'}</span>
      </div>
      <div className="metric">
        <span className="metric-label">FPS</span>
        <span className="metric-val">{fps > 0 ? fps.toFixed(1) : '—'}</span>
      </div>
      <div className="metric">
        <span className="metric-label">MIPS</span>
        <span className="metric-val">{mips > 0 ? mips.toFixed(1) : '—'}</span>
      </div>
      <div className="metric">
        <span className="metric-label">INSTRET</span>
        <span className="metric-val">{instret > 0 ? instret.toLocaleString() : '—'}</span>
      </div>
      {speedup !== null && (
        <div className="metric">
          <span className="metric-label">SPEEDUP</span>
          <span className="metric-val" style={{ color: 'var(--amber)' }}>
            {speedup.toFixed(1)}x
          </span>
        </div>
      )}
      <div className="metric" style={{ marginLeft: 'auto' }}>
        <span className="metric-label">SAB</span>
        <span className="metric-val">{sabEnabled ? 'ON' : 'FALLBACK'}</span>
      </div>
      {meta && (
        <span className={meta.valid ? 'badge-valid' : 'badge-invalid'}>
          {meta.valid ? 'wasmparser VALID' : 'INVALID'}
        </span>
      )}
    </div>
  );
};
