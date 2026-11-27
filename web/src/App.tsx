import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { CompilerExplorer } from './components/CompilerExplorer';
import { OptPassToggles } from './components/OptPassToggles';
import { PresetBar } from './components/PresetBar';
import { TelemetryBar } from './components/TelemetryBar';
import { VmClient } from './lib/vmClient';
import {
  DEFAULT_OPT_O2,
  PRESETS,
  type ExecMode,
  type OptConfig,
  type PresetInfo,
  type RaceResult,
  type RecompileMeta,
} from './types';

const KEY_BITS: Record<string, number> = {
  KeyW: 1 << 0,
  KeyA: 1 << 1,
  KeyS: 1 << 2,
  KeyD: 1 << 3,
  ArrowUp: 1 << 4,
  ArrowDown: 1 << 5,
  ArrowLeft: 1 << 6,
  ArrowRight: 1 << 7,
  Space: 1 << 8,
  Enter: 1 << 9,
  Escape: 1 << 10,
  ShiftLeft: 1 << 11,
  ShiftRight: 1 << 11,
  ControlLeft: 1 << 12,
  KeyR: 1 << 13,
  KeyP: 1 << 14,
};

const ABI_REG_NAMES = [
  'zero', 'ra', 'sp', 'gp', 'tp', 't0', 't1', 't2',
  's0', 's1', 'a0', 'a1', 'a2', 'a3', 'a4', 'a5',
  'a6', 'a7', 's2', 's3', 's4', 's5', 's6', 's7',
  's8', 's9', 's10', 's11', 't3', 't4', 't5', 't6',
];

export const App: React.FC = () => {
  const client = useMemo(() => new VmClient(), []);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const [tab, setTab] = useState<'runner' | 'explorer'>('runner');
  const [activePreset, setActivePreset] = useState<PresetInfo>(PRESETS[0]);
  const [customName, setCustomName] = useState<string | null>(null);
  const [elfBytes, setElfBytes] = useState<Uint8Array | null>(null);
  const [wasmBytes, setWasmBytes] = useState<Uint8Array | null>(null);
  const [meta, setMeta] = useState<RecompileMeta | null>(null);

  const [mode, setMode] = useState<ExecMode>('recomp-o2');
  const [optConfig, setOptConfig] = useState<OptConfig>(DEFAULT_OPT_O2);
  const [running, setRunning] = useState(false);
  const [stdout, setStdout] = useState('');
  const [fps, setFps] = useState(0);
  const [mips, setMips] = useState(0);
  const [instret, setInstret] = useState(0);
  const [speedup, setSpeedup] = useState<number | null>(null);
  const [raceResult, setRaceResult] = useState<RaceResult | null>(null);
  const [regs, setRegs] = useState<number[]>(new Array(32).fill(0));
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const keysLoRef = useRef(0);
  const mouseRef = useRef({ x: 160, y: 100, btn: 0 });

  useEffect(() => {
    client.onStdout = (txt) => setStdout((prev) => prev + txt);
    client.onFrame = ({ rgba, width, height, fps: f, mips: m, instret: ir }) => {
      const cvs = canvasRef.current;
      if (cvs) {
        if (cvs.width !== width || cvs.height !== height) {
          cvs.width = width;
          cvs.height = height;
        }
        const ctx = cvs.getContext('2d');
        if (ctx) {
          const img = new ImageData(new Uint8ClampedArray(rgba), width, height);
          ctx.putImageData(img, 0, 0);
        }
      }
      setFps(f);
      setMips(m);
      setInstret(ir);
    };
    return () => client.dispose();
  }, [client]);

  const compileCurrent = useCallback(
    async (bytes: Uint8Array, cfg: OptConfig) => {
      setErrorMsg(null);
      try {
        const res = await client.compile(bytes, cfg);
        setMeta(res.meta);
        setWasmBytes(res.wasmBytes);
      } catch (e) {
        setErrorMsg(e instanceof Error ? e.message : String(e));
      }
    },
    [client]
  );

  const loadPreset = useCallback(
    async (preset: PresetInfo) => {
      client.stop();
      setRunning(false);
      setCustomName(null);
      setActivePreset(preset);
      setRaceResult(null);
      setSpeedup(null);
      setStdout('');
      setFps(0);
      setMips(0);
      setInstret(0);

      const resp = await fetch(preset.path);
      const buf = new Uint8Array(await resp.arrayBuffer());
      setElfBytes(buf);
      await compileCurrent(buf, optConfig);
    },
    [client, compileCurrent, optConfig]
  );

  useEffect(() => {
    void loadPreset(PRESETS[0]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (elfBytes) {
      void compileCurrent(elfBytes, optConfig);
    }
  }, [elfBytes, optConfig, compileCurrent]);

  // Keyboard MMIO capture
  useEffect(() => {
    const syncInput = () => {
      const m = mouseRef.current;
      client.updateInput(keysLoRef.current, 0, m.x, m.y, m.btn);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      const bit = KEY_BITS[e.code];
      if (bit && running) {
        e.preventDefault();
        keysLoRef.current |= bit;
        syncInput();
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      const bit = KEY_BITS[e.code];
      if (bit) {
        keysLoRef.current &= ~bit;
        syncInput();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
    };
  }, [client, running]);

  const handleRun = async () => {
    if (!elfBytes || running) return;
    setStdout('');
    setErrorMsg(null);
    setRunning(true);
    setFps(0);
    const interactive = !customName && activePreset.kind === 'interactive';
    try {
      const res = await client.run(elfBytes, mode, optConfig, interactive);
      setInstret(res.instret);
      setMips(res.mips);
      setRegs(res.regs);
      setStdout((prev) => `${prev}\n[exit ${res.exitCode} · ${res.elapsedMs.toFixed(2)} ms · ${res.mips.toFixed(1)} MIPS]\n`);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  };

  const handleStop = () => {
    client.stop();
    setRunning(false);
  };

  const handleRace = async () => {
    if (!elfBytes || running) return;
    setRunning(true);
    setErrorMsg(null);
    try {
      const res = await client.race(elfBytes, optConfig);
      setRaceResult(res);
      setSpeedup(res.speedup);
      setInstret(res.instret);
      setMips(res.o2Mips);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  };

  const handleDownloadWasm = () => {
    if (!wasmBytes) return;
    const blob = new Blob([new Uint8Array(wasmBytes)], { type: 'application/wasm' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${customName ?? activePreset.id}.wasm`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleDownloadWat = async () => {
    if (!wasmBytes) return;
    const wat = await client.toWat(wasmBytes);
    const blob = new Blob([wat], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${customName ?? activePreset.id}.wat`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-badge">RV32IMF -&gt; WASM</span>
          <span className="brand-title">Static Binary Recompiler &amp; Compiler Explorer</span>
        </div>
        <div className="tabs">
          <button
            className={`tab-btn ${tab === 'runner' ? 'active' : ''}`}
            onClick={() => setTab('runner')}
          >
            Live Runner &amp; Benchmarks
          </button>
          <button
            className={`tab-btn ${tab === 'explorer' ? 'active' : ''}`}
            onClick={() => setTab('explorer')}
          >
            3-Pane Compiler Explorer &amp; CFG
          </button>
        </div>
      </header>

      <PresetBar
        activePresetId={activePreset.id}
        customName={customName}
        onSelectPreset={(p) => void loadPreset(p)}
        onUploadElf={(name, bytes) => {
          client.stop();
          setRunning(false);
          setCustomName(name);
          setElfBytes(bytes);
          setStdout('');
          setRaceResult(null);
          setSpeedup(null);
        }}
      />

      <TelemetryBar
        meta={meta}
        mode={mode}
        fps={fps}
        mips={mips}
        instret={instret}
        speedup={speedup}
        sabEnabled={client.sabEnabled}
      />

      {errorMsg && (
        <div style={{ padding: '6px 16px', background: 'rgba(248,113,113,0.12)', color: 'var(--red)' }}>
          {errorMsg}
        </div>
      )}

      <main className="main-body">
        {tab === 'runner' ? (
          <div className="runner-layout">
            <div className="panel">
              <div className="panel-header">
                <span>Framebuffer (320x200 RGBA8888)</span>
                <span style={{ fontWeight: 400, color: 'var(--text-2)' }}>
                  {customName ?? activePreset.description}
                </span>
              </div>
              <div className="canvas-wrap">
                <canvas
                  ref={canvasRef}
                  width={320}
                  height={200}
                  className="vm-canvas"
                  onMouseMove={(e) => {
                    const rect = e.currentTarget.getBoundingClientRect();
                    const x = Math.round(((e.clientX - rect.left) / rect.width) * e.currentTarget.width);
                    const y = Math.round(((e.clientY - rect.top) / rect.height) * e.currentTarget.height);
                    mouseRef.current.x = x;
                    mouseRef.current.y = y;
                    client.updateInput(keysLoRef.current, 0, x, y, mouseRef.current.btn);
                  }}
                  onMouseDown={(e) => {
                    mouseRef.current.btn |= 1 << e.button;
                    client.updateInput(
                      keysLoRef.current,
                      0,
                      mouseRef.current.x,
                      mouseRef.current.y,
                      mouseRef.current.btn
                    );
                  }}
                  onMouseUp={(e) => {
                    mouseRef.current.btn &= ~(1 << e.button);
                    client.updateInput(
                      keysLoRef.current,
                      0,
                      mouseRef.current.x,
                      mouseRef.current.y,
                      mouseRef.current.btn
                    );
                  }}
                />
                <div className="controls-row">
                  <select
                    className="select-mode"
                    value={mode}
                    onChange={(e) => setMode(e.target.value as ExecMode)}
                    disabled={running}
                  >
                    <option value="recomp-o2">Static Recompiler (-O2)</option>
                    <option value="recomp-o0">Static Recompiler (-O0)</option>
                    <option value="interp">Baseline Interpreter</option>
                  </select>

                  {!running ? (
                    <button className="btn btn-primary" onClick={() => void handleRun()}>
                      Run
                    </button>
                  ) : (
                    <button className="btn btn-danger" onClick={handleStop}>
                      Stop
                    </button>
                  )}

                  {(customName || activePreset.kind === 'compute') && (
                    <button className="btn" onClick={() => void handleRace()} disabled={running}>
                      Run Speedup Race
                    </button>
                  )}

                  <button className="btn" onClick={handleDownloadWasm} disabled={!wasmBytes}>
                    Download .wasm
                  </button>
                  <button className="btn" onClick={() => void handleDownloadWat()} disabled={!wasmBytes}>
                    Download .wat
                  </button>
                </div>
              </div>

              {raceResult && (
                <div className="race-card">
                  <div>
                    <div className="metric-label">INTERP</div>
                    <div>{raceResult.interpMs.toFixed(2)} ms</div>
                    <div className="metric-label">{raceResult.interpMips.toFixed(1)} MIPS</div>
                  </div>
                  <div>
                    <div className="metric-label">RECOMP -O0</div>
                    <div>{raceResult.o0Ms.toFixed(2)} ms</div>
                    <div className="metric-label">{raceResult.o0Mips.toFixed(1)} MIPS</div>
                  </div>
                  <div>
                    <div className="metric-label">RECOMP -O2</div>
                    <div>{raceResult.o2Ms.toFixed(2)} ms</div>
                    <div className="metric-label">{raceResult.o2Mips.toFixed(1)} MIPS</div>
                  </div>
                  <div>
                    <div className="metric-label">SPEEDUP</div>
                    <div style={{ color: 'var(--amber)', fontWeight: 700 }}>
                      {raceResult.speedup.toFixed(1)}x
                    </div>
                  </div>
                </div>
              )}
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: 12, minHeight: 0 }}>
              <div className="panel">
                <div className="panel-header">Optimization Passes (Live Recompile)</div>
                <OptPassToggles config={optConfig} onChange={setOptConfig} />
              </div>

              <div className="panel" style={{ flex: 1 }}>
                <div className="panel-header">
                  <span>Console Output (ecall sys_write)</span>
                  <button className="btn" onClick={() => setStdout('')}>Clear</button>
                </div>
                <div className="console-box">{stdout || 'Click Run to execute guest ELF...'}</div>
              </div>

              <div className="panel">
                <div className="panel-header">Integer Registers (x0–x31)</div>
                <div className="regs-grid">
                  {regs.map((v, i) => (
                    <div key={i} className="reg-cell">
                      <span className="reg-name">{ABI_REG_NAMES[i]}</span>
                      <span>0x{(v >>> 0).toString(16).padStart(8, '0')}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        ) : (
          <CompilerExplorer
            client={client}
            elfBytes={elfBytes}
            functions={meta?.functions ?? []}
            optConfig={optConfig}
          />
        )}
      </main>
    </div>
  );
};
