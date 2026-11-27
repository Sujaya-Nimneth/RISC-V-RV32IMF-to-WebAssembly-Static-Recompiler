import React, { useEffect, useMemo, useState } from 'react';
import type { VmClient } from '../lib/vmClient';
import type { FnSummary, InspectOutput, OptConfig } from '../types';
import { CfgGraph } from './CfgGraph';

interface Props {
  client: VmClient;
  elfBytes: Uint8Array | null;
  functions: FnSummary[];
  optConfig: OptConfig;
}

export const CompilerExplorer: React.FC<Props> = ({
  client,
  elfBytes,
  functions,
  optConfig,
}) => {
  const [search, setSearch] = useState('');
  const [selectedAddr, setSelectedAddr] = useState<number | null>(null);
  const [inspectData, setInspectData] = useState<InspectOutput | null>(null);
  const [irMode, setIrMode] = useState<'post' | 'pre'>('post');
  const [showCfg, setShowCfg] = useState(false);
  const [selectedBlock, setSelectedBlock] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return functions;
    return functions.filter(
      (f) => f.name.toLowerCase().includes(q) || f.addr.toString(16).includes(q)
    );
  }, [functions, search]);

  useEffect(() => {
    if (functions.length > 0 && (selectedAddr === null || !functions.some((f) => f.addr === selectedAddr))) {
      const mainFn = functions.find((f) => f.name === 'main') ?? functions[0];
      setSelectedAddr(mainFn.addr);
    }
  }, [functions, selectedAddr]);

  useEffect(() => {
    if (!elfBytes || selectedAddr === null) return;
    let cancelled = false;
    setError(null);
    client
      .inspectFn(elfBytes, selectedAddr, optConfig)
      .then((res) => {
        if (!cancelled) setInspectData(res);
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [client, elfBytes, selectedAddr, optConfig]);

  return (
    <div className="explorer-layout">
      <div className="fn-sidebar">
        <input
          className="fn-search"
          placeholder="Search functions or hex PC..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div className="fn-list">
          {filtered.map((f) => (
            <button
              key={f.addr}
              className={`fn-item ${selectedAddr === f.addr ? 'active' : ''}`}
              onClick={() => setSelectedAddr(f.addr)}
            >
              <span className="fn-item-name">{f.name}</span>
              <span className="fn-item-meta">
                0x{f.addr.toString(16).padStart(8, '0')} · {f.block_count} bb · {f.inst_count} inst
                {f.is_indirect_target ? ' · tbl' : ''}
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="explorer-main">
        <div className="panel-header" style={{ background: 'var(--bg-1)' }}>
          <span>
            {inspectData
              ? `${inspectData.name} @ 0x${inspectData.addr.toString(16).padStart(8, '0')}`
              : 'Select a function'}
          </span>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              className="btn"
              onClick={() => setIrMode((m) => (m === 'post' ? 'pre' : 'post'))}
            >
              IR: {irMode === 'post' ? 'Post-Opt (-O2)' : 'Pre-Opt (-O0)'}
            </button>
            <button
              className={`btn ${showCfg ? 'btn-primary' : ''}`}
              onClick={() => setShowCfg((s) => !s)}
            >
              {showCfg ? '3-Pane Code View' : 'CFG Graph View'}
            </button>
          </div>
        </div>

        {error && (
          <div style={{ padding: 12, color: 'var(--red)', fontFamily: 'var(--mono)' }}>{error}</div>
        )}

        {showCfg && inspectData ? (
          <CfgGraph
            nodes={inspectData.cfg.nodes}
            edges={inspectData.cfg.edges}
            selectedBlock={selectedBlock}
            onSelectBlock={setSelectedBlock}
          />
        ) : (
          <div className="panes-grid">
            <div className="code-pane">
              <div className="panel-header">RISC-V Disassembly (RV32IMF)</div>
              <pre className="code-scroll">{inspectData?.disasm ?? 'Loading...'}</pre>
            </div>
            <div className="code-pane">
              <div className="panel-header">
                Lifted SSA IR ({irMode === 'post' ? 'Optimized' : 'Unoptimized'})
              </div>
              <pre className="code-scroll">
                {inspectData
                  ? irMode === 'post'
                    ? inspectData.ir_post_opt
                    : inspectData.ir_pre_opt
                  : 'Loading...'}
              </pre>
            </div>
            <div className="code-pane">
              <div className="panel-header">Emitted WebAssembly Text (WAT)</div>
              <pre className="code-scroll">{inspectData?.fn_wat ?? 'Loading...'}</pre>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
