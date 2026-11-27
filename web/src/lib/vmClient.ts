import type { ExecMode, InspectOutput, OptConfig, RaceResult, RecompileMeta } from '../types';

export interface FrameEvent {
  rgba: Uint8ClampedArray;
  width: number;
  height: number;
  fps: number;
  mips: number;
  instret: number;
}

export interface FinishEvent {
  exitCode: number;
  instret: number;
  elapsedMs: number;
  mips: number;
  regs: number[];
  fregs: number[];
}

export class VmClient {
  private worker: Worker;
  private nextId = 1;
  private pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();
  private sab: SharedArrayBuffer | null = null;
  private sabView: Int32Array | null = null;

  public onStdout?: (text: string) => void;
  public onFrame?: (ev: FrameEvent) => void;

  constructor() {
    this.worker = new Worker(new URL('../worker/vm.worker.ts', import.meta.url), {
      type: 'module',
    });

    if (typeof SharedArrayBuffer !== 'undefined' && self.crossOriginIsolated) {
      this.sab = new SharedArrayBuffer(32);
      this.sabView = new Int32Array(this.sab);
      this.worker.postMessage({ type: 'initSab', sab: this.sab });
    }

    this.worker.onmessage = (ev: MessageEvent) => {
      const msg = ev.data;
      if (msg.type === 'stdout') {
        this.onStdout?.(msg.text);
        return;
      }
      if (msg.type === 'frame') {
        this.onFrame?.({
          rgba: msg.rgba,
          width: msg.width,
          height: msg.height,
          fps: msg.fps,
          mips: msg.mips,
          instret: msg.instret,
        });
        return;
      }
      const req = this.pending.get(msg.id);
      if (!req) return;
      this.pending.delete(msg.id);
      if (msg.type === 'error') {
        req.reject(new Error(msg.error));
      } else {
        req.resolve(msg);
      }
    };
  }

  public get sabEnabled(): boolean {
    return this.sab !== null;
  }

  public updateInput(
    keysLo: number,
    keysHi: number,
    mouseX: number,
    mouseY: number,
    mouseBtn: number
  ) {
    if (this.sabView) {
      Atomics.store(this.sabView, 0, keysLo | 0);
      Atomics.store(this.sabView, 1, keysHi | 0);
      Atomics.store(this.sabView, 2, mouseX | 0);
      Atomics.store(this.sabView, 3, mouseY | 0);
      Atomics.store(this.sabView, 4, mouseBtn | 0);
    } else {
      this.worker.postMessage({
        type: 'input',
        input: { keysLo, keysHi, mouseX, mouseY, mouseBtn },
      });
    }
  }

  public stop() {
    if (this.sabView) {
      Atomics.store(this.sabView, 5, 1);
      Atomics.notify(this.sabView, 6);
    }
    this.worker.postMessage({ type: 'stop' });
  }

  private call<T>(payload: Record<string, unknown>, transfer: Transferable[] = []): Promise<T> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, ...payload }, transfer);
    });
  }

  public async compile(
    elfBytes: Uint8Array,
    optConfig: OptConfig
  ): Promise<{ meta: RecompileMeta; wasmBytes: Uint8Array }> {
    const copy = elfBytes.slice();
    const res = await this.call<{ meta: RecompileMeta; wasmBytes: Uint8Array }>(
      { type: 'compile', elfBytes: copy.buffer, optConfig },
      [copy.buffer]
    );
    return { meta: res.meta, wasmBytes: res.wasmBytes };
  }

  public async inspectFn(
    elfBytes: Uint8Array,
    fnAddr: number,
    optConfig: OptConfig
  ): Promise<InspectOutput> {
    const copy = elfBytes.slice();
    const res = await this.call<{ data: InspectOutput }>(
      { type: 'inspect', elfBytes: copy.buffer, fnAddr, optConfig },
      [copy.buffer]
    );
    return res.data;
  }

  public async toWat(wasmBytes: Uint8Array): Promise<string> {
    const copy = wasmBytes.slice();
    const res = await this.call<{ wat: string }>(
      { type: 'toWat', wasmBytes: copy.buffer },
      [copy.buffer]
    );
    return res.wat;
  }

  public async run(
    elfBytes: Uint8Array,
    mode: ExecMode,
    optConfig: OptConfig,
    interactive: boolean
  ): Promise<FinishEvent> {
    const copy = elfBytes.slice();
    return this.call<FinishEvent>(
      { type: 'run', elfBytes: copy.buffer, mode, optConfig, interactive },
      [copy.buffer]
    );
  }

  public async race(elfBytes: Uint8Array, optConfig: OptConfig): Promise<RaceResult> {
    const copy = elfBytes.slice();
    const res = await this.call<{ race: RaceResult }>(
      { type: 'race', elfBytes: copy.buffer, optConfig },
      [copy.buffer]
    );
    return res.race;
  }

  public dispose() {
    this.worker.terminate();
  }
}
