import init, {
  recompile_elf,
  inspect_fn,
  to_wat,
  InterpreterSession,
} from '../pkg/rv_web_bridge.js';
import type { ExecMode, OptConfig, RecompileMeta, RaceResult } from '../types';
import { OPT_O0 } from '../types';

const FB_BASE = 0x0100_0000;
const HEAP_LIMIT = 0x00e0_0000;
const MMIO_BASE = 0x0120_0000;

class GuestExitSignal extends Error {
  code: number;
  constructor(code: number) {
    super(`exit(${code})`);
    this.code = code;
  }
}

class GuestStopSignal extends Error {
  constructor() {
    super('stopped');
  }
}

let bridgeReady: Promise<void> | null = null;
function ensureBridge(): Promise<void> {
  if (!bridgeReady) {
    bridgeReady = init().then(() => undefined);
  }
  return bridgeReady;
}

let sabView: Int32Array | null = null;
let fallbackInput = { keysLo: 0, keysHi: 0, mouseX: 160, mouseY: 100, mouseBtn: 0 };
let stopRequested = false;

function readInput() {
  if (sabView) {
    return {
      keysLo: Atomics.load(sabView, 0) >>> 0,
      keysHi: Atomics.load(sabView, 1) >>> 0,
      mouseX: Atomics.load(sabView, 2),
      mouseY: Atomics.load(sabView, 3),
      mouseBtn: Atomics.load(sabView, 4) >>> 0,
      stop: Atomics.load(sabView, 5) !== 0,
    };
  }
  return { ...fallbackInput, stop: stopRequested };
}

async function runRecompiledWasm(
  wasmBytes: Uint8Array,
  interactive: boolean,
  onStdout: (text: string) => void,
  onFrame: (rgba: Uint8ClampedArray, w: number, h: number, fps: number, mips: number, instret: number) => void
): Promise<{ exitCode: number; instret: number; elapsedMs: number; regs: number[]; fregs: number[] }> {
  const mod = await WebAssembly.compile(new Uint8Array(wasmBytes));
  let heapBrk = 0x0002_0000;
  let initialBrk = 0x0002_0000;
  let fbWidth = 320;
  let fbHeight = 200;
  let frameCount = 0;
  const startWall = performance.now();
  let lastFrameWall = startWall;
  let lastInstret = 0;
  let fpsAvg = 0;

  let instance!: WebAssembly.Instance;
  const decoder = new TextDecoder();

  const imports: WebAssembly.Imports = {
    env: {
      ecall(id: number, a0: number, a1: number, a2: number, _a3: number): number {
        const inp = readInput();
        if (inp.stop || stopRequested) {
          throw new GuestStopSignal();
        }
        const mem = instance.exports.memory as WebAssembly.Memory;
        const u8 = new Uint8Array(mem.buffer);
        const dv = new DataView(mem.buffer);

        switch (id >>> 0) {
          case 1: {
            // sys_write
            const ptr = a1 >>> 0;
            const len = a2 >>> 0;
            if (ptr + len > u8.length) return -1;
            const text = decoder.decode(u8.subarray(ptr, ptr + len));
            onStdout(text);
            return len | 0;
          }
          case 2: {
            // sys_exit
            throw new GuestExitSignal(a0 | 0);
          }
          case 3: {
            // sys_clock_ms
            return (performance.now() - startWall) | 0;
          }
          case 4: {
            // sys_present_frame
            const wArg = a0 >>> 0;
            const hArg = a1 >>> 0;
            if (wArg > 0 && wArg <= 640) fbWidth = wArg;
            if (hArg > 0 && hArg <= 480) fbHeight = hArg;
            frameCount = (frameCount + 1) >>> 0;

            const byteLen = fbWidth * fbHeight * 4;
            const frameCopy = new Uint8ClampedArray(u8.subarray(FB_BASE, FB_BASE + byteLen));

            const now = performance.now();
            const dt = Math.max(0.001, now - lastFrameWall);
            const curInstret = Number((instance.exports.get_instret as () => bigint)());
            const dInst = curInstret - lastInstret;
            const instFps = 1000.0 / dt;
            fpsAvg = fpsAvg === 0 ? instFps : fpsAvg * 0.8 + instFps * 0.2;
            const mips = dInst / (dt * 1000.0);
            lastFrameWall = now;
            lastInstret = curInstret;

            onFrame(frameCopy, fbWidth, fbHeight, fpsAvg, mips, curInstret);

            if (interactive) {
              if (sabView) {
                const elapsed = performance.now() - now;
                const sleepMs = Math.max(1, 16.6 - elapsed);
                Atomics.wait(sabView, 6, 0, sleepMs);
                if (Atomics.load(sabView, 5) !== 0) {
                  throw new GuestStopSignal();
                }
              }
            }
            return 0;
          }
          case 5: {
            // sys_poll_input
            const nowMs = (performance.now() - startWall) >>> 0;
            dv.setUint32(MMIO_BASE + 0x00, inp.keysLo, true);
            dv.setUint32(MMIO_BASE + 0x04, inp.keysHi, true);
            dv.setInt32(MMIO_BASE + 0x08, inp.mouseX, true);
            dv.setInt32(MMIO_BASE + 0x0c, inp.mouseY, true);
            dv.setUint32(MMIO_BASE + 0x10, inp.mouseBtn, true);
            dv.setUint32(MMIO_BASE + 0x14, nowMs, true);
            dv.setUint32(MMIO_BASE + 0x18, fbWidth, true);
            dv.setUint32(MMIO_BASE + 0x1c, fbHeight, true);
            dv.setUint32(MMIO_BASE + 0x20, frameCount, true);
            return inp.keysLo | 0;
          }
          case 6: {
            // sys_sbrk
            const incr = a0 | 0;
            const oldBrk = heapBrk;
            const nextBrk = oldBrk + incr;
            if (nextBrk < initialBrk || nextBrk > HEAP_LIMIT) return -1;
            heapBrk = nextBrk;
            return oldBrk | 0;
          }
          default:
            return -1;
        }
      },
      trap_invalid_pc(pc: number): void {
        throw new Error(`Invalid indirect PC: 0x${(pc >>> 0).toString(16)}`);
      },
    },
  };

  instance = await WebAssembly.instantiate(mod, imports);
  if (typeof instance.exports.get_initial_brk === 'function') {
    initialBrk = (instance.exports.get_initial_brk as () => number)() >>> 0;
    heapBrk = initialBrk;
  }

  const t0 = performance.now();
  let exitCode = 0;
  try {
    exitCode = (instance.exports._start as () => number)() | 0;
  } catch (e) {
    if (e instanceof GuestExitSignal) {
      exitCode = e.code;
    } else if (e instanceof GuestStopSignal) {
      exitCode = 0;
    } else {
      throw e;
    }
  }
  const elapsedMs = performance.now() - t0;
  const instret = Number((instance.exports.get_instret as () => bigint)());
  const getReg = instance.exports.get_reg as (idx: number) => number;
  const getFReg = instance.exports.get_freg_bits as (idx: number) => number;
  const regs: number[] = [];
  const fregs: number[] = [];
  for (let i = 0; i < 32; i++) {
    regs.push(getReg(i) >>> 0);
    fregs.push(getFReg(i) >>> 0);
  }

  return { exitCode, instret, elapsedMs, regs, fregs };
}

self.onmessage = async (ev: MessageEvent) => {
  const msg = ev.data;
  try {
    await ensureBridge();

    if (msg.type === 'initSab') {
      sabView = msg.sab ? new Int32Array(msg.sab) : null;
      return;
    }

    if (msg.type === 'input') {
      fallbackInput = msg.input;
      return;
    }

    if (msg.type === 'stop') {
      stopRequested = true;
      return;
    }

    if (msg.type === 'compile') {
      const elfBytes = new Uint8Array(msg.elfBytes);
      const t0 = performance.now();
      const out = recompile_elf(elfBytes, JSON.stringify(msg.optConfig as OptConfig));
      const wallMs = performance.now() - t0;
      const meta = JSON.parse(out.metadata_json) as RecompileMeta;
      if (meta.compile_time_ms === 0) {
        meta.compile_time_ms = wallMs;
      }
      const wasmBytes = out.wasm_bytes;
      out.free();
      self.postMessage(
        { id: msg.id, type: 'compiled', meta, wasmBytes },
        { transfer: [wasmBytes.buffer] }
      );
      return;
    }

    if (msg.type === 'inspect') {
      const elfBytes = new Uint8Array(msg.elfBytes);
      const json = inspect_fn(elfBytes, msg.fnAddr >>> 0, JSON.stringify(msg.optConfig));
      self.postMessage({ id: msg.id, type: 'inspected', data: JSON.parse(json) });
      return;
    }

    if (msg.type === 'toWat') {
      const wat = to_wat(new Uint8Array(msg.wasmBytes));
      self.postMessage({ id: msg.id, type: 'wat', wat });
      return;
    }

    if (msg.type === 'run') {
      stopRequested = false;
      if (sabView) Atomics.store(sabView, 5, 0);
      const mode: ExecMode = msg.mode;
      const elfBytes = new Uint8Array(msg.elfBytes);

      if (mode === 'interp') {
        const session = new InterpreterSession(elfBytes);
        const startWall = performance.now();
        let lastFrameWall = startWall;
        let lastInst = 0;
        let fpsAvg = 0;
        let halted = false;
        let exitCode = 0;

        while (!halted && !stopRequested) {
          const inp = readInput();
          if (inp.stop) break;
          session.set_input(
            inp.keysLo,
            inp.keysHi,
            inp.mouseX,
            inp.mouseY,
            inp.mouseBtn,
            (performance.now() - startWall) >>> 0
          );
          const stepJson = session.step(200_000);
          const stepRes = JSON.parse(stepJson) as {
            executed: number;
            frame_presented: boolean;
            halted: boolean;
            exit_code: number;
          };
          const outText = session.take_stdout();
          if (outText) {
            self.postMessage({ type: 'stdout', text: outText });
          }
          if (stepRes.frame_presented) {
            const w = session.fb_width();
            const h = session.fb_height();
            const fb = new Uint8ClampedArray(session.get_framebuffer());
            const now = performance.now();
            const dt = Math.max(0.001, now - lastFrameWall);
            const curInst = Number(session.get_instret());
            const instFps = 1000.0 / dt;
            fpsAvg = fpsAvg === 0 ? instFps : fpsAvg * 0.8 + instFps * 0.2;
            const mips = (curInst - lastInst) / (dt * 1000.0);
            lastFrameWall = now;
            lastInst = curInst;
            self.postMessage(
              { type: 'frame', rgba: fb, width: w, height: h, fps: fpsAvg, mips, instret: curInst },
              { transfer: [fb.buffer] }
            );
          } else if (msg.interactive) {
            // Yield briefly so stop/input messages can arrive even between frames
            await new Promise((r) => setTimeout(r, 0));
          }
          halted = stepRes.halted;
          exitCode = stepRes.exit_code;
        }

        const elapsedMs = performance.now() - startWall;
        const instret = Number(session.get_instret());
        const mips = instret / Math.max(1, elapsedMs * 1000);
        const regs = Array.from(session.get_regs());
        const fregs = Array.from(session.get_fregs_bits());
        session.free();

        self.postMessage({
          id: msg.id,
          type: 'finished',
          exitCode,
          instret,
          elapsedMs,
          mips,
          regs,
          fregs,
        });
        return;
      }

      // Recompiled Wasm (-O2 or -O0)
      const opt: OptConfig = mode === 'recomp-o0' ? OPT_O0 : msg.optConfig;
      const out = recompile_elf(elfBytes, JSON.stringify(opt));
      const wasmBytes = out.wasm_bytes;
      out.free();

      const res = await runRecompiledWasm(
        wasmBytes,
        Boolean(msg.interactive),
        (text) => self.postMessage({ type: 'stdout', text }),
        (rgba, width, height, fps, mips, instret) =>
          self.postMessage(
            { type: 'frame', rgba, width, height, fps, mips, instret },
            { transfer: [rgba.buffer] }
          )
      );

      const mips = res.instret / Math.max(0.001, res.elapsedMs * 1000);
      self.postMessage({
        id: msg.id,
        type: 'finished',
        exitCode: res.exitCode,
        instret: res.instret,
        elapsedMs: res.elapsedMs,
        mips,
        regs: res.regs,
        fregs: res.fregs,
      });
      return;
    }

    if (msg.type === 'race') {
      stopRequested = false;
      const elfBytes = new Uint8Array(msg.elfBytes);

      // 1. Interpreter
      const session = new InterpreterSession(elfBytes);
      const tI0 = performance.now();
      let halted = false;
      while (!halted) {
        const s = JSON.parse(session.step(1_000_000));
        halted = s.halted;
      }
      const interpMs = Math.max(0.01, performance.now() - tI0);
      const instret = Number(session.get_instret());
      session.free();

      // 2. -O0
      const out0 = recompile_elf(elfBytes, JSON.stringify(OPT_O0));
      const w0 = out0.wasm_bytes;
      out0.free();
      const res0 = await runRecompiledWasm(w0, false, () => {}, () => {});
      const o0Ms = Math.max(0.01, res0.elapsedMs);

      // 3. -O2
      const out2 = recompile_elf(elfBytes, JSON.stringify(msg.optConfig));
      const w2 = out2.wasm_bytes;
      out2.free();
      const res2 = await runRecompiledWasm(w2, false, () => {}, () => {});
      const o2Ms = Math.max(0.01, res2.elapsedMs);

      const race: RaceResult = {
        interpMs,
        interpMips: instret / (interpMs * 1000),
        o0Ms,
        o0Mips: res0.instret / (o0Ms * 1000),
        o2Ms,
        o2Mips: res2.instret / (o2Ms * 1000),
        speedup: interpMs / o2Ms,
        instret,
      };
      self.postMessage({ id: msg.id, type: 'raced', race });
    }
  } catch (err) {
    self.postMessage({
      id: msg.id,
      type: 'error',
      error: err instanceof Error ? err.message : String(err),
    });
  }
};
