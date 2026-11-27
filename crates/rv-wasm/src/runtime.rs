use rv_abi::{
    DEFAULT_FB_HEIGHT, DEFAULT_FB_WIDTH, FB_BASE, FB_MAX_HEIGHT, FB_MAX_WIDTH, HEAP_LIMIT,
    HostEnv, SYS_CLOCK_MS, SYS_EXIT, SYS_POLL_INPUT, SYS_PRESENT_FRAME, SYS_SBRK, SYS_WRITE,
    write_mmio_input,
};
use thiserror::Error;
use wasmtime::{Caller, Config, Engine, Linker, Module, Store};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("wasmtime error: {0}")]
    Wasmtime(String),
    #[error("invalid guest pc trapped: 0x{0:08x}")]
    InvalidPc(u32),
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub regs: [u32; 32],
    pub fregs_bits: [u32; 32],
    pub pc: u32,
    pub instret: u64,
    pub exit_code: i32,
    pub memory_crc32: u32,
    pub exec_ms: f64,
}

#[derive(Debug)]
struct ExitSignal(i32);

impl std::fmt::Display for ExitSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "guest exited with code {}", self.0)
    }
}

impl std::error::Error for ExitSignal {}

struct HostState<H: HostEnv> {
    host: H,
    heap_brk: u32,
    initial_brk: u32,
    fb_width: u32,
    fb_height: u32,
    frame_count: u32,
    exit_code: Option<i32>,
}

/// Execute a compiled `.wasm` module using Wasmtime (`tail_call` enabled).
pub fn execute_wasm<H: HostEnv + 'static>(wasm_bytes: &[u8], host: H) -> Result<(ExecutionResult, H), RuntimeError> {
    let mut config = Config::new();
    config.wasm_tail_call(true);
    let engine = Engine::new(&config).map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;
    let module = Module::new(&engine, wasm_bytes).map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;

    let state = HostState {
        host,
        heap_brk: rv_abi::TEXT_BASE,
        initial_brk: rv_abi::TEXT_BASE,
        fb_width: DEFAULT_FB_WIDTH,
        fb_height: DEFAULT_FB_HEIGHT,
        frame_count: 0,
        exit_code: None,
    };
    let mut store = Store::new(&engine, state);
    let mut linker = Linker::new(&engine);

    linker
        .func_wrap(
            "env",
            "ecall",
            |mut caller: Caller<'_, HostState<H>>,
             id: i32,
             a0: i32,
             a1: i32,
             a2: i32,
             _a3: i32|
             -> wasmtime::Result<i32> {
                let id_u = id as u32;
                let a0_u = a0 as u32;
                let a1_u = a1 as u32;
                let a2_u = a2 as u32;
                let mem = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .ok_or_else(|| wasmtime::Error::msg("missing memory export"))?;

                match id_u {
                    SYS_WRITE => {
                        let start = a1_u as usize;
                        let len = a2_u as usize;
                        let data = mem.data(&caller);
                        let end = start.saturating_add(len);
                        if end > data.len() {
                            return Ok(-1);
                        }
                        let bytes = data[start..end].to_vec();
                        let written = caller.data_mut().host.write_fd(a0_u, &bytes);
                        Ok(written)
                    }
                    SYS_EXIT => {
                        let code = a0;
                        caller.data_mut().exit_code = Some(code);
                        caller.data_mut().host.on_exit(code);
                        Err(wasmtime::Error::new(ExitSignal(code)))
                    }
                    SYS_CLOCK_MS => {
                        let ms = caller.data_mut().host.clock_ms();
                        Ok(ms as i32)
                    }
                    SYS_PRESENT_FRAME => {
                        if a0_u > 0 && a0_u <= FB_MAX_WIDTH {
                            caller.data_mut().fb_width = a0_u;
                        }
                        if a1_u > 0 && a1_u <= FB_MAX_HEIGHT {
                            caller.data_mut().fb_height = a1_u;
                        }
                        caller.data_mut().frame_count = caller.data_mut().frame_count.wrapping_add(1);
                        let w = caller.data().fb_width;
                        let h = caller.data().fb_height;
                        let fb_len = (w as usize) * (h as usize) * 4;
                        let fb_start = FB_BASE as usize;
                        let fb_bytes = mem.data(&caller)[fb_start..fb_start + fb_len].to_vec();
                        caller.data_mut().host.present_frame(w, h, &fb_bytes);
                        Ok(0)
                    }
                    SYS_POLL_INPUT => {
                        let input = caller.data_mut().host.poll_input();
                        let ms = caller.data_mut().host.clock_ms();
                        let (w, h, fc) = {
                            let d = caller.data();
                            (d.fb_width, d.fb_height, d.frame_count)
                        };
                        write_mmio_input(mem.data_mut(&mut caller), input, ms, w, h, fc);
                        Ok(input.keys_lo as i32)
                    }
                    SYS_SBRK => {
                        let old_brk = caller.data().heap_brk;
                        let initial_brk = caller.data().initial_brk;
                        let new_brk = (old_brk as i64).saturating_add(a0 as i64);
                        if new_brk < (initial_brk as i64) || new_brk > (HEAP_LIMIT as i64) {
                            Ok(-1)
                        } else {
                            caller.data_mut().heap_brk = new_brk as u32;
                            Ok(old_brk as i32)
                        }
                    }
                    _ => Err(wasmtime::Error::msg(format!("unsupported syscall {id_u}"))),
                }
            },
        )
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;

    linker
        .func_wrap("env", "trap_invalid_pc", |_caller: Caller<'_, HostState<H>>, pc: i32| -> wasmtime::Result<()> {
            Err(wasmtime::Error::msg(format!("invalid indirect PC: 0x{:08x}", pc as u32)))
        })
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;

    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;

    if let Ok(get_brk) = instance.get_typed_func::<(), i32>(&mut store, "get_initial_brk") {
        if let Ok(brk) = get_brk.call(&mut store, ()) {
            store.data_mut().initial_brk = brk as u32;
            store.data_mut().heap_brk = brk as u32;
        }
    }

    let start = instance
        .get_typed_func::<(), i32>(&mut store, "_start")
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;

    let t_exec0 = std::time::Instant::now();
    let exit_code = match start.call(&mut store, ()) {
        Ok(code) => code,
        Err(err) => {
            if let Some(sig) = err.downcast_ref::<ExitSignal>() {
                sig.0
            } else {
                return Err(RuntimeError::Wasmtime(format!("{err:?}")));
            }
        }
    };
    let exec_ms = t_exec0.elapsed().as_secs_f64() * 1000.0;

    let get_reg = instance
        .get_typed_func::<i32, i32>(&mut store, "get_reg")
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;
    let get_freg_bits = instance
        .get_typed_func::<i32, i32>(&mut store, "get_freg_bits")
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;
    let get_pc = instance
        .get_typed_func::<(), i32>(&mut store, "get_pc")
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;
    let get_instret = instance
        .get_typed_func::<(), i64>(&mut store, "get_instret")
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))?;

    let mut regs = [0u32; 32];
    for r in 1..32 {
        regs[r] = get_reg
            .call(&mut store, r as i32)
            .map_err(|e| RuntimeError::Wasmtime(e.to_string()))? as u32;
    }
    let mut fregs_bits = [0u32; 32];
    for r in 0..32 {
        fregs_bits[r] = get_freg_bits
            .call(&mut store, r as i32)
            .map_err(|e| RuntimeError::Wasmtime(e.to_string()))? as u32;
    }
    let pc = get_pc
        .call(&mut store, ())
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))? as u32;
    let instret = get_instret
        .call(&mut store, ())
        .map_err(|e| RuntimeError::Wasmtime(e.to_string()))? as u64;

    let mem = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| RuntimeError::Wasmtime("missing memory export".into()))?;
    let memory_crc32 = crc32fast::hash(mem.data(&store));

    let state = store.into_data();
    Ok((
        ExecutionResult {
            regs,
            fregs_bits,
            pc,
            instret,
            exit_code,
            memory_crc32,
            exec_ms,
        },
        state.host,
    ))
}
