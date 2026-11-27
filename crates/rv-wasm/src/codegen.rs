use crate::cfg_struct::{StructuredCfg, StructuredNode, recover_structured_cfg};
use crate::dispatch::DispatchTable;
use rv_abi::{LINEAR_MEM_PAGES, STACK_TOP};
use rv_decode::{FReg, LoadedElf, Reg};
use rv_ir::{
    BinOp, BlockId, CmpOp, CsrKind, FBinOp, FCmpOp, FMulAddOp, FUnaryOp, IrBlock, IrFunction,
    IrModule, IrOp, IrTerm, IrType, MemWidth, ValueId,
};
use std::collections::BTreeMap;
use thiserror::Error;
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, ElementSection, Elements, EntityType,
    ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType,
    ImportSection, Instruction as WInst, MemArg, MemArgTarget, MemorySection, MemoryType, Module,
    RefType, TableSection, TableType, TypeSection, ValType,
};
use wasmparser::{Validator, WasmFeatures};

#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("missing function entry 0x{0:08x} in IrModule")]
    MissingEntry(u32),
    #[error("generated Wasm module failed wasmparser validation: {0}")]
    Validation(String),
}

#[derive(Debug, Clone)]
pub struct CompiledWasm {
    pub wasm_bytes: Vec<u8>,
    pub valid: bool,
    pub validation_error: Option<String>,
    pub function_count: usize,
    pub dispatch_table_size: usize,
}

/// Browser-compatible WebAssembly feature set (universal Chrome/Firefox/Safari, no multi-memory).
pub fn browser_wasm_features() -> WasmFeatures {
    WasmFeatures::MVP
        | WasmFeatures::MUTABLE_GLOBAL
        | WasmFeatures::SATURATING_FLOAT_TO_INT
        | WasmFeatures::SIGN_EXTENSION
        | WasmFeatures::TAIL_CALL
        | WasmFeatures::MULTI_VALUE
        | WasmFeatures::BULK_MEMORY
        | WasmFeatures::REFERENCE_TYPES
}

pub fn validate_wasm(bytes: &[u8]) -> Result<(), String> {
    let mut validator = Validator::new_with_features(browser_wasm_features());
    validator
        .validate_all(bytes)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Global indices
const G_X_BASE: u32 = 0; // 0..31 -> x1..x31
const G_F_BASE: u32 = 31; // 31..63 -> f0..f31
const G_PC: u32 = 63;
const G_FCSR: u32 = 64;
const G_INSTRET: u32 = 65;
const G_INITIAL_BRK: u32 = 66;

// Type indices
const TY_ECALL: u32 = 0;
const TY_TRAP: u32 = 1;
const TY_GUEST_FN: u32 = 2;
const TY_DIV: u32 = 3;
const TY_RESOLVE: u32 = 4;
const TY_START: u32 = 5;
const TY_GET_REG: u32 = 6;
const TY_GET_INSTRET: u32 = 7;
const TY_F1: u32 = 8;   // (f32) -> f32
const TY_F2: u32 = 9;   // (f32, f32) -> f32
const TY_F3: u32 = 10;  // (f32, f32, f32) -> f32
const TY_FI: u32 = 11;  // (f32) -> i32

// Fixed function indices
const FN_IMP_ECALL: u32 = 0;
const FN_IMP_TRAP_PC: u32 = 1;
const FN_DIV_S: u32 = 2;
const FN_DIV_U: u32 = 3;
const FN_REM_S: u32 = 4;
const FN_REM_U: u32 = 5;
const FN_RESOLVE_PC: u32 = 6;
const FN_TRAP_STUB: u32 = 7;
const FN_CANON_F32: u32 = 8;
const FN_FMIN_S: u32 = 9;
const FN_FMAX_S: u32 = 10;
const FN_FCVT_W_S: u32 = 11;
const FN_FCVT_WU_S: u32 = 12;
const FN_FCLASS_S: u32 = 13;
const FN_FMA_S: u32 = 14;
const FN_GUEST_BASE: u32 = 15;

/// Compile an `IrModule` and `LoadedElf` into a validated standalone WebAssembly module.
pub fn compile_to_wasm(module: &IrModule, elf: &LoadedElf) -> Result<CompiledWasm, CodegenError> {
    compile_to_wasm_with_opts(module, elf, true)
}

/// Compile an `IrModule` and `LoadedElf` with configurable local coalescing.
pub fn compile_to_wasm_with_opts(
    module: &IrModule,
    elf: &LoadedElf,
    local_coalescing: bool,
) -> Result<CompiledWasm, CodegenError> {
    let mut func_index_map: BTreeMap<u32, u32> = BTreeMap::new();
    let guest_pcs: Vec<u32> = module.functions.keys().copied().collect();
    for (idx, &pc) in guest_pcs.iter().enumerate() {
        func_index_map.insert(pc, FN_GUEST_BASE + idx as u32);
    }
    let entry_wasm_fn = *func_index_map
        .get(&module.entry)
        .ok_or(CodegenError::MissingEntry(module.entry))?;

    let num_guest = guest_pcs.len() as u32;
    let fn_start = FN_GUEST_BASE + num_guest;
    let fn_get_reg = fn_start + 1;
    let fn_get_freg_bits = fn_start + 2;
    let fn_get_pc = fn_start + 3;
    let fn_get_instret = fn_start + 4;
    let fn_get_initial_brk = fn_start + 5;

    let dispatch = DispatchTable::build(module, &func_index_map);

    let mut wasm = Module::new();

    // 1. Types
    let mut types = TypeSection::new();
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    ); // 0: ecall
    types.ty().function([ValType::I32], []); // 1: trap_invalid_pc
    types.ty().function([ValType::I32], []); // 2: guest fn (expected_ra)
    types.ty().function([ValType::I32, ValType::I32], [ValType::I32]); // 3: div/rem
    types.ty().function([ValType::I32], [ValType::I32]); // 4: resolve_pc
    types.ty().function([], [ValType::I32]); // 5: _start / get_pc / get_initial_brk
    types.ty().function([ValType::I32], [ValType::I32]); // 6: get_reg / get_freg_bits
    types.ty().function([], [ValType::I64]); // 7: get_instret
    types.ty().function([ValType::F32], [ValType::F32]); // 8: canon_f32
    types.ty().function([ValType::F32, ValType::F32], [ValType::F32]); // 9: fmin/fmax
    types.ty().function([ValType::F32, ValType::F32, ValType::F32], [ValType::F32]); // 10: fma
    types.ty().function([ValType::F32], [ValType::I32]); // 11: fcvt_w/wu/fclass
    wasm.section(&types);

    // 2. Imports
    let mut imports = ImportSection::new();
    imports.import("env", "ecall", EntityType::Function(TY_ECALL));
    imports.import("env", "trap_invalid_pc", EntityType::Function(TY_TRAP));
    wasm.section(&imports);

    // 3. Function declarations
    let mut funcs = FunctionSection::new();
    funcs.function(TY_DIV); // FN_DIV_S
    funcs.function(TY_DIV); // FN_DIV_U
    funcs.function(TY_DIV); // FN_REM_S
    funcs.function(TY_DIV); // FN_REM_U
    funcs.function(TY_RESOLVE); // FN_RESOLVE_PC
    funcs.function(TY_GUEST_FN); // FN_TRAP_STUB
    funcs.function(TY_F1); // FN_CANON_F32
    funcs.function(TY_F2); // FN_FMIN_S
    funcs.function(TY_F2); // FN_FMAX_S
    funcs.function(TY_FI); // FN_FCVT_W_S
    funcs.function(TY_FI); // FN_FCVT_WU_S
    funcs.function(TY_FI); // FN_FCLASS_S
    funcs.function(TY_F3); // FN_FMA_S
    for _ in 0..num_guest {
        funcs.function(TY_GUEST_FN);
    }
    funcs.function(TY_START); // _start
    funcs.function(TY_GET_REG); // get_reg
    funcs.function(TY_GET_REG); // get_freg_bits
    funcs.function(TY_START); // get_pc
    funcs.function(TY_GET_INSTRET); // get_instret
    funcs.function(TY_START); // get_initial_brk
    wasm.section(&funcs);

    // 4. Table (funcref dispatch table)
    let table_len = (dispatch.entries.len() + 1) as u64;
    let mut tables = TableSection::new();
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        minimum: table_len,
        maximum: Some(table_len),
        table64: false,
        shared: false,
    });
    wasm.section(&tables);

    // 5. Single Linear Memory
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: LINEAR_MEM_PAGES as u64,
        maximum: Some(LINEAR_MEM_PAGES as u64),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    wasm.section(&memories);

    // 6. Globals
    let mut globals = GlobalSection::new();
    for _ in 1..=31 {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );
    }
    for _ in 0..32 {
        globals.global(
            GlobalType {
                val_type: ValType::F32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::f32_const(0.0f32.into()),
        );
    }
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(module.entry as i32),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(0),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: false,
            shared: false,
        },
        &ConstExpr::i32_const(elf.initial_brk as i32),
    );
    wasm.section(&globals);

    // 7. Exports
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("_start", ExportKind::Func, fn_start);
    exports.export("get_reg", ExportKind::Func, fn_get_reg);
    exports.export("get_freg_bits", ExportKind::Func, fn_get_freg_bits);
    exports.export("get_pc", ExportKind::Func, fn_get_pc);
    exports.export("get_instret", ExportKind::Func, fn_get_instret);
    exports.export("get_initial_brk", ExportKind::Func, fn_get_initial_brk);
    wasm.section(&exports);

    // 8. Elements (populate dispatch table)
    let mut elem_funcs: Vec<u32> = dispatch.entries.iter().map(|&(_, f)| f).collect();
    elem_funcs.push(FN_TRAP_STUB);
    let mut elements = ElementSection::new();
    elements.active(
        Some(0),
        &ConstExpr::i32_const(0),
        Elements::Functions(elem_funcs.into()),
    );
    wasm.section(&elements);

    // 9. Code Section
    let mut code = CodeSection::new();
    code.function(&emit_div_s());
    code.function(&emit_div_u());
    code.function(&emit_rem_s());
    code.function(&emit_rem_u());
    code.function(&emit_resolve_pc(&dispatch));
    code.function(&emit_trap_stub());
    code.function(&crate::fp_helpers::emit_canon_f32());
    code.function(&crate::fp_helpers::emit_fmin_s());
    code.function(&crate::fp_helpers::emit_fmax_s());
    code.function(&crate::fp_helpers::emit_fcvt_w_s());
    code.function(&crate::fp_helpers::emit_fcvt_wu_s());
    code.function(&crate::fp_helpers::emit_fclass_s());
    code.function(&crate::fp_helpers::emit_fma_s(FN_CANON_F32));

    for &pc in &guest_pcs {
        let ir_fn = &module.functions[&pc];
        code.function(&emit_guest_function(ir_fn, &func_index_map, local_coalescing));
    }

    code.function(&emit_start_fn(entry_wasm_fn));
    code.function(&emit_get_reg_fn());
    code.function(&emit_get_freg_bits_fn());
    code.function(&emit_get_global_i32(G_PC));
    code.function(&emit_get_instret_fn());
    code.function(&emit_get_global_i32(G_INITIAL_BRK));
    wasm.section(&code);

    // 10. Active Data Segments from ELF PT_LOAD
    let mut data = DataSection::new();
    for seg in &elf.segments {
        if !seg.data.is_empty() {
            data.active(0, &ConstExpr::i32_const(seg.vaddr as i32), seg.data.iter().copied());
        }
    }
    wasm.section(&data);

    let wasm_bytes = wasm.finish();
    let (valid, validation_error) = match validate_wasm(&wasm_bytes) {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e)),
    };

    if let Some(ref err) = validation_error {
        return Err(CodegenError::Validation(err.clone()));
    }

    Ok(CompiledWasm {
        wasm_bytes,
        valid,
        validation_error,
        function_count: guest_pcs.len(),
        dispatch_table_size: dispatch.entries.len(),
    })
}

#[inline]
fn reg_global(r: Reg) -> u32 {
    debug_assert!(r.0 != 0);
    G_X_BASE + (r.0 as u32 - 1)
}

#[inline]
fn freg_global(r: FReg) -> u32 {
    G_F_BASE + (r.0 as u32)
}

fn emit_div_s() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::I32Const(-1));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::I32Const(i32::MIN));
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Const(-1));
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::I32Const(i32::MIN));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32DivS);
    f.instruction(&WInst::End);
    f
}

fn emit_div_u() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::I32Const(-1));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32DivU);
    f.instruction(&WInst::End);
    f
}

fn emit_rem_s() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::I32Const(i32::MIN));
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Const(-1));
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::I32Const(0));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32RemS);
    f.instruction(&WInst::End);
    f
}

fn emit_rem_u() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32RemU);
    f.instruction(&WInst::End);
    f
}

fn emit_resolve_pc(dispatch: &DispatchTable) -> Function {
    // param 0: pc (i32) -> returns table slot index
    let mut f = Function::new([]);
    emit_binary_search(&mut f, &dispatch.entries, 0, dispatch.trap_slot);
    f.instruction(&WInst::End);
    f
}

fn emit_binary_search(f: &mut Function, slice: &[(u32, u32)], base_slot: u32, trap_slot: u32) {
    if slice.is_empty() {
        f.instruction(&WInst::I32Const(trap_slot as i32));
        return;
    }
    if slice.len() <= 4 {
        for (idx, &(pc, _)) in slice.iter().enumerate() {
            f.instruction(&WInst::LocalGet(0));
            f.instruction(&WInst::I32Const(pc as i32));
            f.instruction(&WInst::I32Eq);
            f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
            f.instruction(&WInst::I32Const((base_slot + idx as u32) as i32));
            f.instruction(&WInst::Else);
        }
        f.instruction(&WInst::I32Const(trap_slot as i32));
        for _ in slice {
            f.instruction(&WInst::End);
        }
        return;
    }
    let mid = slice.len() / 2;
    let mid_pc = slice[mid].0;
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::I32Const(mid_pc as i32));
    f.instruction(&WInst::I32LtU);
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    emit_binary_search(f, &slice[..mid], base_slot, trap_slot);
    f.instruction(&WInst::Else);
    emit_binary_search(f, &slice[mid..], base_slot + mid as u32, trap_slot);
    f.instruction(&WInst::End);
}

fn emit_trap_stub() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::GlobalGet(G_PC));
    f.instruction(&WInst::Call(FN_IMP_TRAP_PC));
    f.instruction(&WInst::Unreachable);
    f.instruction(&WInst::End);
    f
}

fn emit_start_fn(entry_wasm_fn: u32) -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::I32Const(STACK_TOP as i32));
    f.instruction(&WInst::GlobalSet(reg_global(Reg::SP)));
    // Sentinel expected_ra = 0xFFFF_FFFC
    f.instruction(&WInst::I32Const(-4));
    f.instruction(&WInst::Call(entry_wasm_fn));
    f.instruction(&WInst::GlobalGet(reg_global(Reg::A0)));
    f.instruction(&WInst::End);
    f
}

fn emit_get_reg_fn() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(0));
    f.instruction(&WInst::Else);
    for r in 1u8..=31 {
        f.instruction(&WInst::LocalGet(0));
        f.instruction(&WInst::I32Const(r as i32));
        f.instruction(&WInst::I32Eq);
        f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
        f.instruction(&WInst::GlobalGet(G_X_BASE + (r as u32 - 1)));
        f.instruction(&WInst::Return);
        f.instruction(&WInst::Else);
    }
    f.instruction(&WInst::I32Const(0));
    for _ in 1..=31 {
        f.instruction(&WInst::End);
    }
    f.instruction(&WInst::End);
    f.instruction(&WInst::End);
    f
}

fn emit_get_freg_bits_fn() -> Function {
    let mut f = Function::new([]);
    for r in 0u8..32 {
        f.instruction(&WInst::LocalGet(0));
        f.instruction(&WInst::I32Const(r as i32));
        f.instruction(&WInst::I32Eq);
        f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
        f.instruction(&WInst::GlobalGet(G_F_BASE + r as u32));
        f.instruction(&WInst::I32ReinterpretF32);
        f.instruction(&WInst::Return);
        f.instruction(&WInst::Else);
    }
    f.instruction(&WInst::I32Const(0));
    for _ in 0..32 {
        f.instruction(&WInst::End);
    }
    f.instruction(&WInst::End);
    f
}

fn emit_get_global_i32(global_idx: u32) -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::GlobalGet(global_idx));
    f.instruction(&WInst::End);
    f
}

fn emit_get_instret_fn() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::GlobalGet(G_INSTRET));
    f.instruction(&WInst::End);
    f
}

struct EmitCtx<'a> {
    ir_fn: &'a IrFunction,
    func_index_map: &'a BTreeMap<u32, u32>,
    /// Local index for each `ValueId` (param 0 is `$expected_ra`, locals start at 1).
    val_locals: Vec<u32>,
    /// Label stack: `(target_bb, is_loop)` pushed from outer to inner.
    label_stack: Vec<(BlockId, bool)>,
    /// Optional `$bb_id` local when using Dispatcher fallback.
    bb_id_local: Option<u32>,
    /// Depth of the outer dispatcher `loop` when using Dispatcher fallback.
    dispatcher_loop_depth: Option<u32>,
    /// Map from `BlockId` to `br_table` position when using Dispatcher fallback.
    dispatcher_block_to_pos: Option<BTreeMap<BlockId, u32>>,
}

impl<'a> EmitCtx<'a> {
    fn val_local(&self, v: ValueId) -> u32 {
        self.val_locals[v.0 as usize]
    }

    fn label_depth(&self, bb: BlockId) -> Option<u32> {
        for (rev_idx, &(target, _)) in self.label_stack.iter().rev().enumerate() {
            if target == bb {
                return Some(rev_idx as u32);
            }
        }
        None
    }
}

fn emit_guest_function(
    ir_fn: &IrFunction,
    func_index_map: &BTreeMap<u32, u32>,
    local_coalescing: bool,
) -> Function {
    // Determine ValueId types (default I32).
    let mut val_types = vec![ValType::I32; ir_fn.next_value as usize];
    for bb in &ir_fn.blocks {
        for phi in &bb.phis {
            val_types[phi.dst.0 as usize] = match phi.ty {
                IrType::I32 => ValType::I32,
                IrType::F32 => ValType::F32,
            };
        }
        for inst in &bb.insts {
            if let (Some(dst), Some(ty)) = (inst.dst, inst.ty) {
                val_types[dst.0 as usize] = match ty {
                    IrType::I32 => ValType::I32,
                    IrType::F32 => ValType::F32,
                };
            }
        }
    }

    let structured = recover_structured_cfg(ir_fn);
    let is_dispatcher = matches!(structured, StructuredCfg::Dispatcher { .. });

    let (val_locals, bb_id_local, runs) =
        crate::local_alloc::allocate_locals(ir_fn, &val_types, is_dispatcher, local_coalescing);

    let mut f = Function::new(runs);
    let mut ctx = EmitCtx {
        ir_fn,
        func_index_map,
        val_locals,
        label_stack: Vec::new(),
        bb_id_local,
        dispatcher_loop_depth: None,
        dispatcher_block_to_pos: None,
    };

    match structured {
        StructuredCfg::Stackified { nodes, .. } => {
            emit_structured_nodes(&mut f, &mut ctx, &nodes, None);
        }
        StructuredCfg::Dispatcher { blocks } => {
            let bb_var = bb_id_local.expect("bb_id_local");
            let mut block_to_pos = BTreeMap::new();
            for (i, &bb) in blocks.iter().enumerate() {
                block_to_pos.insert(bb, i as u32);
            }
            let entry_bb = ir_fn.pc_to_block.get(&ir_fn.entry).copied().unwrap_or(BlockId(0));
            let entry_pos = block_to_pos.get(&entry_bb).copied().unwrap_or(0);
            ctx.dispatcher_block_to_pos = Some(block_to_pos);

            f.instruction(&WInst::I32Const(entry_pos as i32));
            f.instruction(&WInst::LocalSet(bb_var));
            f.instruction(&WInst::Loop(BlockType::Empty)); // outer loop

            // Build block ladder for br_table
            for _ in &blocks {
                f.instruction(&WInst::Block(BlockType::Empty));
            }
            f.instruction(&WInst::LocalGet(bb_var));
            let targets: Vec<u32> = (0..blocks.len() as u32).collect();
            let default_target = (blocks.len().saturating_sub(1)) as u32;
            f.instruction(&WInst::BrTable(targets.into(), default_target));

            for (i, &bb_id) in blocks.iter().enumerate() {
                f.instruction(&WInst::End);
                // Depth to outer loop: (blocks.len() - 1 - i) blocks? No, we already closed i+1 blocks!
                // Remaining open blocks = blocks.len() - 1 - i, plus the outer loop itself.
                ctx.dispatcher_loop_depth = Some((blocks.len() - 1 - i) as u32);
                let bb = &ir_fn.blocks[bb_id.0 as usize];
                emit_basic_block(&mut f, &mut ctx, bb, None);
            }
            f.instruction(&WInst::End); // end outer loop
        }
    }

    f.instruction(&WInst::Unreachable);
    f.instruction(&WInst::End);
    f
}

fn emit_structured_nodes(
    f: &mut Function,
    ctx: &mut EmitCtx<'_>,
    nodes: &[StructuredNode],
    fallthrough_after: Option<BlockId>,
) {
    for (idx, node) in nodes.iter().enumerate() {
        let next_ft = if idx + 1 < nodes.len() {
            first_block(nodes[idx + 1].clone())
        } else {
            fallthrough_after
        };
        match node {
            StructuredNode::Block { target_bb, body } => {
                f.instruction(&WInst::Block(BlockType::Empty));
                ctx.label_stack.push((*target_bb, false));
                emit_structured_nodes(f, ctx, body, Some(*target_bb));
                ctx.label_stack.pop();
                f.instruction(&WInst::End);
            }
            StructuredNode::Loop { header_bb, body } => {
                f.instruction(&WInst::Loop(BlockType::Empty));
                ctx.label_stack.push((*header_bb, true));
                emit_structured_nodes(f, ctx, body, next_ft);
                ctx.label_stack.pop();
                f.instruction(&WInst::End);
            }
            StructuredNode::BasicBlock(bb_id) => {
                let bb = &ctx.ir_fn.blocks[bb_id.0 as usize];
                emit_basic_block(f, ctx, bb, next_ft);
            }
        }
    }
}

fn first_block(node: StructuredNode) -> Option<BlockId> {
    match node {
        StructuredNode::BasicBlock(bb) => Some(bb),
        StructuredNode::Block { body, .. } | StructuredNode::Loop { body, .. } => {
            body.into_iter().next().and_then(first_block)
        }
    }
}

fn emit_basic_block(
    f: &mut Function,
    ctx: &mut EmitCtx<'_>,
    bb: &IrBlock,
    fallthrough: Option<BlockId>,
) {
    for inst in &bb.insts {
        match &inst.op {
            IrOp::ConstI32(c) => {
                f.instruction(&WInst::I32Const(*c));
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::ConstF32(bits) => {
                f.instruction(&WInst::F32Const(f32::from_bits(*bits).into()));
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::Copy(src) => {
                f.instruction(&WInst::LocalGet(ctx.val_local(*src)));
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::ReadReg(r) => {
                if r.0 == 0 {
                    f.instruction(&WInst::I32Const(0));
                } else {
                    f.instruction(&WInst::GlobalGet(reg_global(*r)));
                }
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::WriteReg { reg, val } => {
                if reg.0 != 0 {
                    f.instruction(&WInst::LocalGet(ctx.val_local(*val)));
                    f.instruction(&WInst::GlobalSet(reg_global(*reg)));
                }
            }
            IrOp::ReadFReg(r) => {
                f.instruction(&WInst::GlobalGet(freg_global(*r)));
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::WriteFReg { freg, val } => {
                f.instruction(&WInst::LocalGet(ctx.val_local(*val)));
                f.instruction(&WInst::GlobalSet(freg_global(*freg)));
            }
            IrOp::BinOp { op, lhs, rhs } => {
                emit_binop(f, ctx, *op, *lhs, *rhs);
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::Cmp { op, lhs, rhs } => {
                emit_cmp(f, ctx, *op, *lhs, *rhs);
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::Load {
                addr,
                offset,
                width,
                signed,
            } => {
                f.instruction(&WInst::LocalGet(ctx.val_local(*addr)));
                let memarg = MemArg {
                    offset: (*offset as u32) as u64,
                    align: 0,
                    target: MemArgTarget::Memory(0),
                    ordering: None,
                };
                match (width, signed) {
                    (MemWidth::B1, true) => f.instruction(&WInst::I32Load8S(memarg)),
                    (MemWidth::B1, false) => f.instruction(&WInst::I32Load8U(memarg)),
                    (MemWidth::B2, true) => f.instruction(&WInst::I32Load16S(memarg)),
                    (MemWidth::B2, false) => f.instruction(&WInst::I32Load16U(memarg)),
                    (MemWidth::B4, _) => f.instruction(&WInst::I32Load(memarg)),
                };
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::Store {
                addr,
                val,
                offset,
                width,
            } => {
                f.instruction(&WInst::LocalGet(ctx.val_local(*addr)));
                f.instruction(&WInst::LocalGet(ctx.val_local(*val)));
                let memarg = MemArg {
                    offset: (*offset as u32) as u64,
                    align: 0,
                    target: MemArgTarget::Memory(0),
                    ordering: None,
                };
                match width {
                    MemWidth::B1 => f.instruction(&WInst::I32Store8(memarg)),
                    MemWidth::B2 => f.instruction(&WInst::I32Store16(memarg)),
                    MemWidth::B4 => f.instruction(&WInst::I32Store(memarg)),
                };
            }
            IrOp::FBinOp { op, lhs, rhs } => {
                emit_fbinop(f, ctx, *op, *lhs, *rhs);
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::FUnaryOp { op, src } => {
                emit_funaryop(f, ctx, *op, *src);
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::FMulAdd { op, a, b, c } => {
                emit_fmuladd(f, ctx, *op, *a, *b, *c);
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::FCmp { op, lhs, rhs } => {
                f.instruction(&WInst::LocalGet(ctx.val_local(*lhs)));
                f.instruction(&WInst::LocalGet(ctx.val_local(*rhs)));
                match op {
                    FCmpOp::Feq => f.instruction(&WInst::F32Eq),
                    FCmpOp::Flt => f.instruction(&WInst::F32Lt),
                    FCmpOp::Fle => f.instruction(&WInst::F32Le),
                };
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::LoadF32 { addr, offset } => {
                f.instruction(&WInst::LocalGet(ctx.val_local(*addr)));
                f.instruction(&WInst::F32Load(MemArg {
                    offset: (*offset as u32) as u64,
                    align: 0,
                    target: MemArgTarget::Memory(0),
                    ordering: None,
                }));
                f.instruction(&WInst::LocalSet(ctx.val_local(inst.dst.unwrap())));
            }
            IrOp::StoreF32 { addr, val, offset } => {
                f.instruction(&WInst::LocalGet(ctx.val_local(*addr)));
                f.instruction(&WInst::LocalGet(ctx.val_local(*val)));
                f.instruction(&WInst::F32Store(MemArg {
                    offset: (*offset as u32) as u64,
                    align: 0,
                    target: MemArgTarget::Memory(0),
                    ordering: None,
                }));
            }
            IrOp::CsrOp { kind, csr, val, write } => {
                let dst_loc = ctx.val_local(inst.dst.unwrap());
                emit_csr_op(f, ctx, *kind, *csr, *val, *write, dst_loc);
            }
            IrOp::Ecall => {
                f.instruction(&WInst::GlobalGet(reg_global(Reg::A7)));
                f.instruction(&WInst::GlobalGet(reg_global(Reg::A0)));
                f.instruction(&WInst::GlobalGet(reg_global(Reg::A1)));
                f.instruction(&WInst::GlobalGet(reg_global(Reg::A2)));
                f.instruction(&WInst::GlobalGet(reg_global(Reg(13))));
                f.instruction(&WInst::Call(FN_IMP_ECALL));
                f.instruction(&WInst::GlobalSet(reg_global(Reg::A0)));
            }
            IrOp::AddInstret(n) => {
                f.instruction(&WInst::GlobalGet(G_INSTRET));
                f.instruction(&WInst::I64Const(*n as i64));
                f.instruction(&WInst::I64Add);
                f.instruction(&WInst::GlobalSet(G_INSTRET));
            }
        }
    }

    // Terminator
    match &bb.term {
        IrTerm::Jump { target } => {
            emit_branch_to(f, ctx, bb.id, *target, fallthrough);
        }
        IrTerm::BrIf {
            cond,
            then_bb,
            else_bb,
        } => {
            f.instruction(&WInst::LocalGet(ctx.val_local(*cond)));
            f.instruction(&WInst::If(BlockType::Empty));
            ctx.label_stack.push((BlockId(u32::MAX), false));
            emit_branch_to(f, ctx, bb.id, *then_bb, None);
            ctx.label_stack.pop();
            f.instruction(&WInst::End);
            emit_branch_to(f, ctx, bb.id, *else_bb, fallthrough);
        }
        IrTerm::BrIfCmp {
            op,
            lhs,
            rhs,
            then_bb,
            else_bb,
        } => {
            emit_cmp(f, ctx, *op, *lhs, *rhs);
            f.instruction(&WInst::If(BlockType::Empty));
            ctx.label_stack.push((BlockId(u32::MAX), false));
            emit_branch_to(f, ctx, bb.id, *then_bb, None);
            ctx.label_stack.pop();
            f.instruction(&WInst::End);
            emit_branch_to(f, ctx, bb.id, *else_bb, fallthrough);
        }
        IrTerm::Call {
            target_pc,
            ret_pc,
            fallthrough: ft,
        } => {
            if let Some(&wasm_fn) = ctx.func_index_map.get(target_pc) {
                f.instruction(&WInst::I32Const(*ret_pc as i32));
                f.instruction(&WInst::Call(wasm_fn));
            } else {
                f.instruction(&WInst::I32Const(*target_pc as i32));
                f.instruction(&WInst::GlobalSet(G_PC));
                f.instruction(&WInst::I32Const(*ret_pc as i32));
                f.instruction(&WInst::I32Const(*target_pc as i32));
                f.instruction(&WInst::Call(FN_RESOLVE_PC));
                f.instruction(&WInst::CallIndirect {
                    type_index: TY_GUEST_FN,
                    table_index: 0,
                });
            }
            emit_branch_to(f, ctx, bb.id, *ft, fallthrough);
        }
        IrTerm::TailCall { target_pc } => {
            if let Some(&wasm_fn) = ctx.func_index_map.get(target_pc) {
                f.instruction(&WInst::LocalGet(0));
                f.instruction(&WInst::ReturnCall(wasm_fn));
            } else {
                f.instruction(&WInst::I32Const(*target_pc as i32));
                f.instruction(&WInst::GlobalSet(G_PC));
                f.instruction(&WInst::LocalGet(0));
                f.instruction(&WInst::I32Const(*target_pc as i32));
                f.instruction(&WInst::Call(FN_RESOLVE_PC));
                f.instruction(&WInst::ReturnCallIndirect {
                    type_index: TY_GUEST_FN,
                    table_index: 0,
                });
            }
        }
        IrTerm::IndirectCall {
            target_val,
            ret_pc,
            fallthrough: ft,
        } => {
            f.instruction(&WInst::LocalGet(ctx.val_local(*target_val)));
            f.instruction(&WInst::GlobalSet(G_PC));
            f.instruction(&WInst::I32Const(*ret_pc as i32));
            f.instruction(&WInst::LocalGet(ctx.val_local(*target_val)));
            f.instruction(&WInst::Call(FN_RESOLVE_PC));
            f.instruction(&WInst::CallIndirect {
                type_index: TY_GUEST_FN,
                table_index: 0,
            });
            emit_branch_to(f, ctx, bb.id, *ft, fallthrough);
        }
        IrTerm::IndirectJump { target_val } => {
            f.instruction(&WInst::LocalGet(ctx.val_local(*target_val)));
            f.instruction(&WInst::GlobalSet(G_PC));
            f.instruction(&WInst::LocalGet(0));
            f.instruction(&WInst::LocalGet(ctx.val_local(*target_val)));
            f.instruction(&WInst::Call(FN_RESOLVE_PC));
            f.instruction(&WInst::ReturnCallIndirect {
                type_index: TY_GUEST_FN,
                table_index: 0,
            });
        }
        IrTerm::Return { target_val } => {
            f.instruction(&WInst::LocalGet(ctx.val_local(*target_val)));
            f.instruction(&WInst::LocalGet(0));
            f.instruction(&WInst::I32Eq);
            f.instruction(&WInst::If(BlockType::Empty));
            f.instruction(&WInst::Return);
            f.instruction(&WInst::End);

            f.instruction(&WInst::LocalGet(ctx.val_local(*target_val)));
            f.instruction(&WInst::GlobalSet(G_PC));
            f.instruction(&WInst::LocalGet(0));
            f.instruction(&WInst::LocalGet(ctx.val_local(*target_val)));
            f.instruction(&WInst::Call(FN_RESOLVE_PC));
            f.instruction(&WInst::ReturnCallIndirect {
                type_index: TY_GUEST_FN,
                table_index: 0,
            });
        }
        IrTerm::Trap => {
            f.instruction(&WInst::Unreachable);
        }
    }
}

fn emit_branch_to(
    f: &mut Function,
    ctx: &mut EmitCtx<'_>,
    from_bb: BlockId,
    target_bb: BlockId,
    fallthrough: Option<BlockId>,
) {
    // Emit parallel phi copies for target_bb (push all sources before setting any dst).
    let target_phis = &ctx.ir_fn.blocks[target_bb.0 as usize].phis;
    if !target_phis.is_empty() {
        let mut copies = Vec::with_capacity(target_phis.len());
        for phi in target_phis {
            if let Some(&(_, src_val)) = phi.incoming.iter().find(|&&(pred, _)| pred == from_bb) {
                if src_val != phi.dst {
                    copies.push((phi.dst, src_val));
                }
            }
        }
        for &(_, src_val) in &copies {
            f.instruction(&WInst::LocalGet(ctx.val_local(src_val)));
        }
        for &(dst_val, _) in copies.iter().rev() {
            f.instruction(&WInst::LocalSet(ctx.val_local(dst_val)));
        }
    }

    if let Some(loop_depth) = ctx.dispatcher_loop_depth {
        // Count any extra inner labels (e.g. WInst::If) pushed inside this basic block.
        let extra_depth = ctx
            .label_stack
            .iter()
            .rev()
            .take_while(|(id, _)| *id == BlockId(u32::MAX))
            .count() as u32;
        let pos = ctx
            .dispatcher_block_to_pos
            .as_ref()
            .and_then(|m| m.get(&target_bb).copied())
            .unwrap_or(0);
        f.instruction(&WInst::I32Const(pos as i32));
        f.instruction(&WInst::LocalSet(ctx.bb_id_local.unwrap()));
        f.instruction(&WInst::Br(loop_depth + extra_depth));
        return;
    }

    if fallthrough == Some(target_bb) {
        return;
    }

    if let Some(depth) = ctx.label_depth(target_bb) {
        f.instruction(&WInst::Br(depth));
    } else {
        f.instruction(&WInst::Unreachable);
    }
}

fn emit_binop(f: &mut Function, ctx: &EmitCtx<'_>, op: BinOp, lhs: ValueId, rhs: ValueId) {
    let l = ctx.val_local(lhs);
    let r = ctx.val_local(rhs);
    match op {
        BinOp::Add => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32Add);
        }
        BinOp::Sub => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32Sub);
        }
        BinOp::Mul => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32Mul);
        }
        BinOp::Mulh => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::I64ExtendI32S);
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I64ExtendI32S);
            f.instruction(&WInst::I64Mul);
            f.instruction(&WInst::I64Const(32));
            f.instruction(&WInst::I64ShrU);
            f.instruction(&WInst::I32WrapI64);
        }
        BinOp::Mulhsu => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::I64ExtendI32S);
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I64ExtendI32U);
            f.instruction(&WInst::I64Mul);
            f.instruction(&WInst::I64Const(32));
            f.instruction(&WInst::I64ShrU);
            f.instruction(&WInst::I32WrapI64);
        }
        BinOp::Mulhu => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::I64ExtendI32U);
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I64ExtendI32U);
            f.instruction(&WInst::I64Mul);
            f.instruction(&WInst::I64Const(32));
            f.instruction(&WInst::I64ShrU);
            f.instruction(&WInst::I32WrapI64);
        }
        BinOp::DivS => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::Call(FN_DIV_S));
        }
        BinOp::DivU => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::Call(FN_DIV_U));
        }
        BinOp::RemS => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::Call(FN_REM_S));
        }
        BinOp::RemU => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::Call(FN_REM_U));
        }
        BinOp::And => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32And);
        }
        BinOp::Or => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32Or);
        }
        BinOp::Xor => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32Xor);
        }
        BinOp::Shl => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32Shl);
        }
        BinOp::ShrU => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32ShrU);
        }
        BinOp::ShrS => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32ShrS);
        }
    }
}

fn emit_cmp(f: &mut Function, ctx: &EmitCtx<'_>, op: CmpOp, lhs: ValueId, rhs: ValueId) {
    f.instruction(&WInst::LocalGet(ctx.val_local(lhs)));
    f.instruction(&WInst::LocalGet(ctx.val_local(rhs)));
    match op {
        CmpOp::Eq => f.instruction(&WInst::I32Eq),
        CmpOp::Ne => f.instruction(&WInst::I32Ne),
        CmpOp::LtS => f.instruction(&WInst::I32LtS),
        CmpOp::GeS => f.instruction(&WInst::I32GeS),
        CmpOp::LtU => f.instruction(&WInst::I32LtU),
        CmpOp::GeU => f.instruction(&WInst::I32GeU),
    };
}

fn emit_fbinop(f: &mut Function, ctx: &EmitCtx<'_>, op: FBinOp, lhs: ValueId, rhs: ValueId) {
    let l = ctx.val_local(lhs);
    let r = ctx.val_local(rhs);
    match op {
        FBinOp::Fadd => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::F32Add);
            f.instruction(&WInst::Call(FN_CANON_F32));
        }
        FBinOp::Fsub => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::F32Sub);
            f.instruction(&WInst::Call(FN_CANON_F32));
        }
        FBinOp::Fmul => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::F32Mul);
            f.instruction(&WInst::Call(FN_CANON_F32));
        }
        FBinOp::Fdiv => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::F32Div);
            f.instruction(&WInst::Call(FN_CANON_F32));
        }
        FBinOp::Fmin => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::Call(FN_FMIN_S));
        }
        FBinOp::Fmax => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::Call(FN_FMAX_S));
        }
        FBinOp::Fsgnj => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::I32ReinterpretF32);
            f.instruction(&WInst::I32Const(0x7FFF_FFFF));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32ReinterpretF32);
            f.instruction(&WInst::I32Const(i32::MIN));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::I32Or);
            f.instruction(&WInst::F32ReinterpretI32);
        }
        FBinOp::Fsgnjn => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::I32ReinterpretF32);
            f.instruction(&WInst::I32Const(0x7FFF_FFFF));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32ReinterpretF32);
            f.instruction(&WInst::I32Const(i32::MIN));
            f.instruction(&WInst::I32Xor);
            f.instruction(&WInst::I32Const(i32::MIN));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::I32Or);
            f.instruction(&WInst::F32ReinterpretI32);
        }
        FBinOp::Fsgnjx => {
            f.instruction(&WInst::LocalGet(l));
            f.instruction(&WInst::I32ReinterpretF32);
            f.instruction(&WInst::LocalGet(r));
            f.instruction(&WInst::I32ReinterpretF32);
            f.instruction(&WInst::I32Const(i32::MIN));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::I32Xor);
            f.instruction(&WInst::F32ReinterpretI32);
        }
    }
}

fn emit_funaryop(f: &mut Function, ctx: &EmitCtx<'_>, op: FUnaryOp, src: ValueId) {
    let s = ctx.val_local(src);
    f.instruction(&WInst::LocalGet(s));
    match op {
        FUnaryOp::Fsqrt => {
            f.instruction(&WInst::F32Sqrt);
            f.instruction(&WInst::Call(FN_CANON_F32));
        }
        FUnaryOp::FcvtWS => {
            f.instruction(&WInst::Call(FN_FCVT_W_S));
        }
        FUnaryOp::FcvtWuS => {
            f.instruction(&WInst::Call(FN_FCVT_WU_S));
        }
        FUnaryOp::FcvtSW => {
            f.instruction(&WInst::F32ConvertI32S);
        }
        FUnaryOp::FcvtSWu => {
            f.instruction(&WInst::F32ConvertI32U);
        }
        FUnaryOp::FmvXW => {
            f.instruction(&WInst::I32ReinterpretF32);
        }
        FUnaryOp::FmvWX => {
            f.instruction(&WInst::F32ReinterpretI32);
        }
        FUnaryOp::Fclass => {
            f.instruction(&WInst::Call(FN_FCLASS_S));
        }
    }
}

fn emit_fmuladd(
    f: &mut Function,
    ctx: &EmitCtx<'_>,
    op: FMulAddOp,
    a: ValueId,
    b: ValueId,
    c: ValueId,
) {
    f.instruction(&WInst::LocalGet(ctx.val_local(a)));
    if matches!(op, FMulAddOp::Fnmsub | FMulAddOp::Fnmadd) {
        f.instruction(&WInst::F32Neg);
    }
    f.instruction(&WInst::LocalGet(ctx.val_local(b)));
    f.instruction(&WInst::LocalGet(ctx.val_local(c)));
    if matches!(op, FMulAddOp::Fmsub | FMulAddOp::Fnmadd) {
        f.instruction(&WInst::F32Neg);
    }
    f.instruction(&WInst::Call(FN_FMA_S));
}

fn emit_csr_op(
    f: &mut Function,
    ctx: &EmitCtx<'_>,
    kind: CsrKind,
    csr: u16,
    val: ValueId,
    write: bool,
    dst_loc: u32,
) {
    // Read old value and store into dst_loc.
    match csr {
        0x001 => {
            f.instruction(&WInst::GlobalGet(G_FCSR));
            f.instruction(&WInst::I32Const(0x1F));
            f.instruction(&WInst::I32And);
        }
        0x002 => {
            f.instruction(&WInst::GlobalGet(G_FCSR));
            f.instruction(&WInst::I32Const(5));
            f.instruction(&WInst::I32ShrU);
            f.instruction(&WInst::I32Const(0x7));
            f.instruction(&WInst::I32And);
        }
        0x003 => {
            f.instruction(&WInst::GlobalGet(G_FCSR));
            f.instruction(&WInst::I32Const(0xFF));
            f.instruction(&WInst::I32And);
        }
        _ => {
            f.instruction(&WInst::I32Const(0));
            f.instruction(&WInst::LocalSet(dst_loc));
            return;
        }
    }
    f.instruction(&WInst::LocalSet(dst_loc));

    if !write {
        return;
    }

    // Compute new field value in stack from old (dst_loc) and val.
    match kind {
        CsrKind::Csrrw => {
            f.instruction(&WInst::LocalGet(ctx.val_local(val)));
        }
        CsrKind::Csrrs => {
            f.instruction(&WInst::LocalGet(dst_loc));
            f.instruction(&WInst::LocalGet(ctx.val_local(val)));
            f.instruction(&WInst::I32Or);
        }
        CsrKind::Csrrc => {
            f.instruction(&WInst::LocalGet(dst_loc));
            f.instruction(&WInst::LocalGet(ctx.val_local(val)));
            f.instruction(&WInst::I32Const(-1));
            f.instruction(&WInst::I32Xor);
            f.instruction(&WInst::I32And);
        }
    }

    match csr {
        0x001 => {
            f.instruction(&WInst::I32Const(0x1F));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::GlobalGet(G_FCSR));
            f.instruction(&WInst::I32Const(!0x1F));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::I32Or);
            f.instruction(&WInst::GlobalSet(G_FCSR));
        }
        0x002 => {
            f.instruction(&WInst::I32Const(0x7));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::I32Const(5));
            f.instruction(&WInst::I32Shl);
            f.instruction(&WInst::GlobalGet(G_FCSR));
            f.instruction(&WInst::I32Const(!(0x7 << 5)));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::I32Or);
            f.instruction(&WInst::GlobalSet(G_FCSR));
        }
        0x003 => {
            f.instruction(&WInst::I32Const(0xFF));
            f.instruction(&WInst::I32And);
            f.instruction(&WInst::GlobalSet(G_FCSR));
        }
        _ => {
            f.instruction(&WInst::Drop);
        }
    }
}
