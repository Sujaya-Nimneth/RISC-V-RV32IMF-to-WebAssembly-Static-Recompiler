//! wasm-bindgen bridge exposing the recompiler, inspector, WAT printer, and reference interpreter.

use rv_abi::{HeadlessHost, InputState};
use rv_decode::{LoadedElf, disassemble, discover_cfg};
use rv_interp::Interpreter;
use rv_ir::{OptConfig, lift_program, optimize_function, print_function};
use rv_wasm::{compile_to_wasm_with_opts, validate_wasm};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct FnSummary {
    addr: u32,
    name: String,
    block_count: usize,
    inst_count: usize,
    is_indirect_target: bool,
}

#[derive(Serialize)]
struct RecompileMeta {
    valid: bool,
    validation_error: Option<String>,
    compile_time_ms: f64,
    elf_size: usize,
    wasm_size: usize,
    entry_pc: u32,
    function_count: usize,
    dispatch_table_size: usize,
    functions: Vec<FnSummary>,
}

#[wasm_bindgen]
pub struct RecompileOutput {
    wasm_bytes: Vec<u8>,
    metadata_json: String,
}

#[wasm_bindgen]
impl RecompileOutput {
    #[wasm_bindgen(getter)]
    pub fn wasm_bytes(&self) -> Vec<u8> {
        self.wasm_bytes.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn metadata_json(&self) -> String {
        self.metadata_json.clone()
    }
}

fn parse_opt_config(opt_config_json: &str) -> Result<OptConfig, JsValue> {
    if opt_config_json.trim().is_empty() {
        return Ok(OptConfig::o2());
    }
    serde_json::from_str(opt_config_json)
        .map_err(|e| JsValue::from_str(&format!("invalid OptConfig JSON: {e}")))
}

#[wasm_bindgen]
pub fn recompile_elf(elf_bytes: &[u8], opt_config_json: &str) -> Result<RecompileOutput, JsValue> {
    let opt = parse_opt_config(opt_config_json)?;
    let t0 = now_ms();

    let elf = LoadedElf::parse(elf_bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let cfg = discover_cfg(&elf).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let ir = lift_program(&cfg, &opt);
    let compiled = compile_to_wasm_with_opts(&ir, &elf, opt.local_coalescing)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let compile_time_ms = (now_ms() - t0).max(0.0);

    let functions: Vec<FnSummary> = cfg
        .functions
        .values()
        .map(|f| FnSummary {
            addr: f.entry,
            name: f.name.clone(),
            block_count: f.blocks.len(),
            inst_count: f.blocks.values().map(|b| b.insts.len()).sum(),
            is_indirect_target: f.is_indirect_target,
        })
        .collect();

    let meta = RecompileMeta {
        valid: compiled.valid,
        validation_error: compiled.validation_error,
        compile_time_ms,
        elf_size: elf_bytes.len(),
        wasm_size: compiled.wasm_bytes.len(),
        entry_pc: elf.entry,
        function_count: compiled.function_count,
        dispatch_table_size: compiled.dispatch_table_size,
        functions,
    };

    let metadata_json =
        serde_json::to_string(&meta).map_err(|e| JsValue::from_str(&e.to_string()))?;

    Ok(RecompileOutput {
        wasm_bytes: compiled.wasm_bytes,
        metadata_json,
    })
}

#[derive(Serialize)]
struct DisasmLine {
    pc: u32,
    word: u32,
    text: String,
}

#[derive(Serialize)]
struct CfgNodeInfo {
    id: u32,
    guest_pc: u32,
    end_pc: u32,
    inst_count: usize,
    lines: Vec<DisasmLine>,
}

#[derive(Serialize)]
struct CfgEdgeInfo {
    from: u32,
    to: u32,
    kind: String,
}

#[derive(Serialize)]
struct CfgGraphInfo {
    nodes: Vec<CfgNodeInfo>,
    edges: Vec<CfgEdgeInfo>,
}

#[derive(Serialize)]
struct InspectOutput {
    name: String,
    addr: u32,
    disasm: String,
    ir_pre_opt: String,
    ir_post_opt: String,
    cfg: CfgGraphInfo,
    fn_wat: String,
}

#[wasm_bindgen]
pub fn disassemble_fn(elf_bytes: &[u8], fn_addr: u32) -> Result<String, JsValue> {
    let elf = LoadedElf::parse(elf_bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let cfg = discover_cfg(&elf).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let fcfg = cfg
        .functions
        .get(&fn_addr)
        .ok_or_else(|| JsValue::from_str(&format!("function 0x{fn_addr:08x} not found")))?;

    let mut out = String::new();
    for bb in fcfg.blocks.values() {
        out.push_str(&format!("; block 0x{:08x}\n", bb.start_pc));
        for di in &bb.insts {
            out.push_str(&format!(
                "0x{:08x}:  {:08x}  {}\n",
                di.pc,
                di.word,
                disassemble(di.pc, &di.inst)
            ));
        }
    }
    Ok(out)
}

#[wasm_bindgen]
pub fn inspect_fn(
    elf_bytes: &[u8],
    fn_addr: u32,
    opt_config_json: &str,
) -> Result<String, JsValue> {
    let opt = parse_opt_config(opt_config_json)?;
    let elf = LoadedElf::parse(elf_bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let cfg = discover_cfg(&elf).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let fcfg = cfg
        .functions
        .get(&fn_addr)
        .ok_or_else(|| JsValue::from_str(&format!("function 0x{fn_addr:08x} not found")))?;

    let mut disasm = String::new();
    for bb in fcfg.blocks.values() {
        disasm.push_str(&format!("; bb 0x{:08x}\n", bb.start_pc));
        for di in &bb.insts {
            disasm.push_str(&format!(
                "0x{:08x}:  {}\n",
                di.pc,
                disassemble(di.pc, &di.inst)
            ));
        }
    }

    let ir_pre_mod = lift_program(&cfg, &OptConfig::o0());
    let pre_fn = &ir_pre_mod.functions[&fn_addr];
    let ir_pre_opt = print_function(pre_fn);

    let mut post_fn = pre_fn.clone();
    optimize_function(&mut post_fn, &opt);
    let ir_post_opt = print_function(&post_fn);

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for bb in &post_fn.blocks {
        let mut lines = Vec::new();
        if let Some(f_bb) = fcfg.blocks.get(&bb.guest_pc) {
            for di in &f_bb.insts {
                lines.push(DisasmLine {
                    pc: di.pc,
                    word: di.word,
                    text: disassemble(di.pc, &di.inst),
                });
            }
        }
        nodes.push(CfgNodeInfo {
            id: bb.id.0,
            guest_pc: bb.guest_pc,
            end_pc: fcfg
                .blocks
                .get(&bb.guest_pc)
                .map(|b| b.end_pc)
                .unwrap_or(bb.guest_pc),
            inst_count: bb.insts.len(),
            lines,
        });
        for succ in bb.term.successors() {
            let kind = if succ.0 <= bb.id.0 {
                "backedge"
            } else {
                "edge"
            };
            edges.push(CfgEdgeInfo {
                from: bb.id.0,
                to: succ.0,
                kind: kind.to_string(),
            });
        }
    }

    // Build a single-function module WAT snippet for focused inspection.
    let single_ir = lift_program(&cfg, &opt);
    let compiled = compile_to_wasm_with_opts(&single_ir, &elf, opt.local_coalescing)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let full_wat =
        wasmprinter::print_bytes(&compiled.wasm_bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let fn_wat = extract_wat_function(&full_wat, &cfg.functions, fn_addr);

    let out = InspectOutput {
        name: fcfg.name.clone(),
        addr: fn_addr,
        disasm,
        ir_pre_opt,
        ir_post_opt,
        cfg: CfgGraphInfo { nodes, edges },
        fn_wat,
    };

    serde_json::to_string(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}

fn extract_wat_function(
    full_wat: &str,
    functions: &std::collections::BTreeMap<u32, rv_decode::FunctionCfg>,
    fn_addr: u32,
) -> String {
    let idx_in_guest = functions.keys().position(|&pc| pc == fn_addr).unwrap_or(0);
    // Guest functions start at Wasm function index 15 (FN_GUEST_BASE = 15).
    let target_marker = format!("(func (;{};)", 15 + idx_in_guest);
    if let Some(start) = full_wat.find(&target_marker) {
        let rest = &full_wat[start..];
        let mut depth = 0i32;
        let mut end_off = rest.len();
        for (i, ch) in rest.char_indices() {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
                if depth == 0 {
                    end_off = i + 1;
                    break;
                }
            }
        }
        return rest[..end_off].to_string();
    }
    full_wat.chars().take(4096).collect()
}

#[wasm_bindgen]
pub fn to_wat(wasm_bytes: &[u8]) -> Result<String, JsValue> {
    validate_wasm(wasm_bytes).map_err(|e| JsValue::from_str(&e))?;
    wasmprinter::print_bytes(wasm_bytes).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[derive(Serialize)]
struct StepResult {
    executed: u64,
    frame_presented: bool,
    halted: bool,
    exit_code: i32,
}

#[wasm_bindgen]
pub struct InterpreterSession {
    interp: Interpreter,
    host: HeadlessHost,
}

#[wasm_bindgen]
impl InterpreterSession {
    #[wasm_bindgen(constructor)]
    pub fn new(elf_bytes: &[u8]) -> Result<InterpreterSession, JsValue> {
        let elf = LoadedElf::parse(elf_bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self {
            interp: Interpreter::from_elf(&elf),
            host: HeadlessHost::new(),
        })
    }

    pub fn set_input(
        &mut self,
        keys_lo: u32,
        keys_hi: u32,
        mouse_x: i32,
        mouse_y: i32,
        mouse_btn: u32,
        clock_ms: u32,
    ) {
        self.host.input = InputState {
            keys_lo,
            keys_hi,
            mouse_x,
            mouse_y,
            mouse_btn,
        };
        self.host.clock_ms_value = clock_ms;
    }

    pub fn step(&mut self, max_instructions: u32) -> Result<String, JsValue> {
        let mut executed = 0u64;
        let mut frame_presented = false;
        while !self.interp.cpu.halted && executed < max_instructions as u64 {
            let presented = self
                .interp
                .step_one(&mut self.host)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            executed += 1;
            if presented {
                frame_presented = true;
                break;
            }
        }
        let res = StepResult {
            executed,
            frame_presented,
            halted: self.interp.cpu.halted,
            exit_code: self.interp.cpu.exit_code,
        };
        serde_json::to_string(&res).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn get_framebuffer(&self) -> Vec<u8> {
        self.interp.framebuffer_slice().to_vec()
    }

    pub fn fb_width(&self) -> u32 {
        self.interp.fb_width
    }

    pub fn fb_height(&self) -> u32 {
        self.interp.fb_height
    }

    pub fn take_stdout(&mut self) -> String {
        let s = self.host.stdout_str();
        self.host.stdout.clear();
        s
    }

    pub fn get_regs(&self) -> Vec<u32> {
        self.interp.cpu.x.to_vec()
    }

    pub fn get_fregs_bits(&self) -> Vec<u32> {
        self.interp.cpu.f.iter().map(|f| f.to_bits()).collect()
    }

    pub fn get_pc(&self) -> u32 {
        self.interp.cpu.pc
    }

    pub fn get_instret(&self) -> u64 {
        self.interp.cpu.instret
    }
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    // Use monotonic fallback if performance.now isn't bound directly.
    0.0
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}
