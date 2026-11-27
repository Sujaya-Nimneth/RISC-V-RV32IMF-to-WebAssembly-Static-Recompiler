use wasm_encoder::{BlockType, Function, Instruction as WInst, ValType};

const CANON_NAN: i32 = 0x7FC0_0000u32 as i32;

/// Canonicalize NaN to `0x7FC0_0000`: `(f32) -> f32`.
pub fn emit_canon_f32() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F32Ne);
    f.instruction(&WInst::If(BlockType::Result(ValType::F32)));
    f.instruction(&WInst::F32Const(f32::from_bits(CANON_NAN as u32).into()));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::End);
    f.instruction(&WInst::End);
    f
}

/// RISC-V `FMIN.S(a, b)`: non-NaN wins, both NaN -> canonical NaN, `-0.0 < +0.0`.
pub fn emit_fmin_s() -> Function {
    emit_fminmax_s(true)
}

/// RISC-V `FMAX.S(a, b)`: non-NaN wins, both NaN -> canonical NaN, `-0.0 < +0.0`.
pub fn emit_fmax_s() -> Function {
    emit_fminmax_s(false)
}

fn emit_fminmax_s(is_min: bool) -> Function {
    let mut f = Function::new([(2, ValType::I32)]); // local 2: ia, local 3: ib
    // if a != a && b != b -> canon NaN
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F32Ne);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::F32Ne);
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::F32Const(f32::from_bits(CANON_NAN as u32).into()));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    // if a != a -> return b
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F32Ne);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    // if b != b -> return a
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::F32Ne);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    // Check -0.0 vs +0.0
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::I32ReinterpretF32);
    f.instruction(&WInst::LocalSet(2));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32ReinterpretF32);
    f.instruction(&WInst::LocalSet(3));

    f.instruction(&WInst::LocalGet(2));
    f.instruction(&WInst::I32Const(i32::MIN));
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::LocalGet(3));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(if is_min { 0 } else { 1 }));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    f.instruction(&WInst::LocalGet(3));
    f.instruction(&WInst::I32Const(i32::MIN));
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::LocalGet(2));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(if is_min { 1 } else { 0 }));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&if is_min {
        WInst::F32Min
    } else {
        WInst::F32Max
    });
    f.instruction(&WInst::End);
    f
}

/// RISC-V `FCVT.W.S`: NaN -> `i32::MAX`, finite saturates with RNE rounding.
pub fn emit_fcvt_w_s() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F32Ne);
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(i32::MAX));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F32Nearest);
    f.instruction(&WInst::I32TruncSatF32S);
    f.instruction(&WInst::End);
    f.instruction(&WInst::End);
    f
}

/// RISC-V `FCVT.WU.S`: NaN -> `u32::MAX`, finite saturates with RNE rounding.
pub fn emit_fcvt_wu_s() -> Function {
    let mut f = Function::new([]);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F32Ne);
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(-1));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F32Nearest);
    f.instruction(&WInst::I32TruncSatF32U);
    f.instruction(&WInst::End);
    f.instruction(&WInst::End);
    f
}

/// RISC-V `FCLASS.S`: 10-bit classification mask.
pub fn emit_fclass_s() -> Function {
    // param 0: f32; locals: 1=bits, 2=sign, 3=exp, 4=mant
    let mut f = Function::new([(4, ValType::I32)]);
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::I32ReinterpretF32);
    f.instruction(&WInst::LocalSet(1));

    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Const(31));
    f.instruction(&WInst::I32ShrU);
    f.instruction(&WInst::LocalSet(2));

    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Const(23));
    f.instruction(&WInst::I32ShrU);
    f.instruction(&WInst::I32Const(0xFF));
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::LocalSet(3));

    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::I32Const(0x7F_FFFF));
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::LocalSet(4));

    // exp == 0xFF
    f.instruction(&WInst::LocalGet(3));
    f.instruction(&WInst::I32Const(0xFF));
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(4));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(2));
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(1 << 0));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::I32Const(1 << 7));
    f.instruction(&WInst::End);
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);
    f.instruction(&WInst::LocalGet(4));
    f.instruction(&WInst::I32Const(0x40_0000));
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(1 << 8));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::I32Const(1 << 9));
    f.instruction(&WInst::End);
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    // exp == 0
    f.instruction(&WInst::LocalGet(3));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(4));
    f.instruction(&WInst::I32Eqz);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(2));
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(1 << 3));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::I32Const(1 << 4));
    f.instruction(&WInst::End);
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);
    f.instruction(&WInst::LocalGet(2));
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(1 << 2));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::I32Const(1 << 5));
    f.instruction(&WInst::End);
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    // normal
    f.instruction(&WInst::LocalGet(2));
    f.instruction(&WInst::If(BlockType::Result(ValType::I32)));
    f.instruction(&WInst::I32Const(1 << 1));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::I32Const(1 << 6));
    f.instruction(&WInst::End);
    f.instruction(&WInst::End);
    f
}

/// Exact single-rounding IEEE-754 binary32 FMA: `(a: f32, b: f32, c: f32) -> f32`.
/// Uses exact `f64` product + Knuth TwoSum + Boldo-Melquiond round-to-odd adjustment.
pub fn emit_fma_s(canon_fn_idx: u32) -> Function {
    // params: 0=a, 1=b, 2=c (f32)
    // locals: 3=p(f64), 4=cd(f64), 5=s(f64), 6=v(f64), 7=e(f64), 8=s_bits(i64)
    let mut f = Function::new([(5, ValType::F64), (1, ValType::I64)]);

    // p = f64(a) * f64(b)
    f.instruction(&WInst::LocalGet(0));
    f.instruction(&WInst::F64PromoteF32);
    f.instruction(&WInst::LocalGet(1));
    f.instruction(&WInst::F64PromoteF32);
    f.instruction(&WInst::F64Mul);
    f.instruction(&WInst::LocalSet(3));

    // cd = f64(c)
    f.instruction(&WInst::LocalGet(2));
    f.instruction(&WInst::F64PromoteF32);
    f.instruction(&WInst::LocalSet(4));

    // s = p + cd
    f.instruction(&WInst::LocalGet(3));
    f.instruction(&WInst::LocalGet(4));
    f.instruction(&WInst::F64Add);
    f.instruction(&WInst::LocalSet(5));

    // If s is NaN or Inf (|s| == Inf, i.e. (s - s) != 0.0), demote s and canonicalize NaN.
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::F64Sub);
    f.instruction(&WInst::F64Const(0.0f64.into()));
    f.instruction(&WInst::F64Ne);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::F32DemoteF64);
    f.instruction(&WInst::Call(canon_fn_idx));
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    // Knuth TwoSum: v = s - p; e = (p - (s - v)) + (cd - v)
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::LocalGet(3));
    f.instruction(&WInst::F64Sub);
    f.instruction(&WInst::LocalSet(6));

    f.instruction(&WInst::LocalGet(3));
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::LocalGet(6));
    f.instruction(&WInst::F64Sub);
    f.instruction(&WInst::F64Sub);
    f.instruction(&WInst::LocalGet(4));
    f.instruction(&WInst::LocalGet(6));
    f.instruction(&WInst::F64Sub);
    f.instruction(&WInst::F64Add);
    f.instruction(&WInst::LocalSet(7));

    // If s == 0.0 (exact cancellation): if e != 0.0 return f32(e), else preserve s's sign (-0.0).
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::F64Const(0.0f64.into()));
    f.instruction(&WInst::F64Eq);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(7));
    f.instruction(&WInst::F64Const(0.0f64.into()));
    f.instruction(&WInst::F64Ne);
    f.instruction(&WInst::If(BlockType::Result(ValType::F32)));
    f.instruction(&WInst::LocalGet(7));
    f.instruction(&WInst::F32DemoteF64);
    f.instruction(&WInst::Else);
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::F32DemoteF64);
    f.instruction(&WInst::End);
    f.instruction(&WInst::Return);
    f.instruction(&WInst::End);

    // Round-to-odd adjustment when e != 0.0 and (s_bits & 1) == 0
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::I64ReinterpretF64);
    f.instruction(&WInst::LocalSet(8));

    f.instruction(&WInst::LocalGet(7));
    f.instruction(&WInst::F64Const(0.0f64.into()));
    f.instruction(&WInst::F64Ne);
    f.instruction(&WInst::LocalGet(8));
    f.instruction(&WInst::I64Const(1));
    f.instruction(&WInst::I64And);
    f.instruction(&WInst::I64Eqz);
    f.instruction(&WInst::I32And);
    f.instruction(&WInst::If(BlockType::Empty));
    // If (e > 0.0) == (s > 0.0): s_bits += 1 else s_bits -= 1
    f.instruction(&WInst::LocalGet(7));
    f.instruction(&WInst::F64Const(0.0f64.into()));
    f.instruction(&WInst::F64Gt);
    f.instruction(&WInst::LocalGet(5));
    f.instruction(&WInst::F64Const(0.0f64.into()));
    f.instruction(&WInst::F64Gt);
    f.instruction(&WInst::I32Eq);
    f.instruction(&WInst::If(BlockType::Empty));
    f.instruction(&WInst::LocalGet(8));
    f.instruction(&WInst::I64Const(1));
    f.instruction(&WInst::I64Add);
    f.instruction(&WInst::LocalSet(8));
    f.instruction(&WInst::Else);
    f.instruction(&WInst::LocalGet(8));
    f.instruction(&WInst::I64Const(1));
    f.instruction(&WInst::I64Sub);
    f.instruction(&WInst::LocalSet(8));
    f.instruction(&WInst::End);
    f.instruction(&WInst::End);

    f.instruction(&WInst::LocalGet(8));
    f.instruction(&WInst::F64ReinterpretI64);
    f.instruction(&WInst::F32DemoteF64);
    f.instruction(&WInst::End);
    f
}
