use rv_decode::{FReg, Reg};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptConfig {
    pub zero_reg_elim: bool,
    pub register_promotion: bool,
    pub constant_folding: bool,
    pub memarg_folding: bool,
    pub branch_fusion: bool,
    pub dce: bool,
    pub local_coalescing: bool,
    pub count_instructions: bool,
}

impl OptConfig {
    pub fn o0() -> Self {
        Self {
            zero_reg_elim: false,
            register_promotion: false,
            constant_folding: false,
            memarg_folding: false,
            branch_fusion: false,
            dce: false,
            local_coalescing: false,
            count_instructions: true,
        }
    }

    pub fn o2() -> Self {
        Self {
            zero_reg_elim: true,
            register_promotion: true,
            constant_folding: true,
            memarg_folding: true,
            branch_fusion: true,
            dce: true,
            local_coalescing: true,
            count_instructions: true,
        }
    }
}

impl Default for OptConfig {
    fn default() -> Self {
        Self::o2()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IrType {
    I32,
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemWidth {
    B1,
    B2,
    B4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpOp {
    Eq,
    Ne,
    LtS,
    GeS,
    LtU,
    GeU,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    DivS,
    DivU,
    RemS,
    RemU,
    And,
    Or,
    Xor,
    Shl,
    ShrU,
    ShrS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FBinOp {
    Fadd,
    Fsub,
    Fmul,
    Fdiv,
    Fmin,
    Fmax,
    Fsgnj,
    Fsgnjn,
    Fsgnjx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FUnaryOp {
    Fsqrt,
    FcvtWS,
    FcvtWuS,
    FcvtSW,
    FcvtSWu,
    FmvXW,
    FmvWX,
    Fclass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FCmpOp {
    Feq,
    Flt,
    Fle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FMulAddOp {
    Fmadd,
    Fmsub,
    Fnmsub,
    Fnmadd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsrKind {
    Csrrw,
    Csrrs,
    Csrrc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IrOp {
    ConstI32(i32),
    ConstF32(u32),
    Copy(ValueId),
    ReadReg(Reg),
    WriteReg { reg: Reg, val: ValueId },
    ReadFReg(FReg),
    WriteFReg { freg: FReg, val: ValueId },
    BinOp { op: BinOp, lhs: ValueId, rhs: ValueId },
    Cmp { op: CmpOp, lhs: ValueId, rhs: ValueId },
    Load { addr: ValueId, offset: i32, width: MemWidth, signed: bool },
    Store { addr: ValueId, val: ValueId, offset: i32, width: MemWidth },
    FBinOp { op: FBinOp, lhs: ValueId, rhs: ValueId },
    FUnaryOp { op: FUnaryOp, src: ValueId },
    FMulAdd { op: FMulAddOp, a: ValueId, b: ValueId, c: ValueId },
    FCmp { op: FCmpOp, lhs: ValueId, rhs: ValueId },
    LoadF32 { addr: ValueId, offset: i32 },
    StoreF32 { addr: ValueId, val: ValueId, offset: i32 },
    CsrOp { kind: CsrKind, csr: u16, val: ValueId, write: bool },
    Ecall,
    AddInstret(u32),
}

impl IrOp {
    pub fn has_side_effects(&self) -> bool {
        matches!(
            self,
            IrOp::WriteReg { .. }
                | IrOp::WriteFReg { .. }
                | IrOp::Store { .. }
                | IrOp::StoreF32 { .. }
                | IrOp::CsrOp { .. }
                | IrOp::Ecall
                | IrOp::AddInstret(_)
        )
    }

    pub fn uses(&self) -> Vec<ValueId> {
        match *self {
            IrOp::ConstI32(_)
            | IrOp::ConstF32(_)
            | IrOp::ReadReg(_)
            | IrOp::ReadFReg(_)
            | IrOp::Ecall
            | IrOp::AddInstret(_) => Vec::new(),
            IrOp::Copy(v)
            | IrOp::WriteReg { val: v, .. }
            | IrOp::WriteFReg { val: v, .. }
            | IrOp::FUnaryOp { src: v, .. }
            | IrOp::CsrOp { val: v, .. } => {
                vec![v]
            }
            IrOp::BinOp { lhs, rhs, .. }
            | IrOp::Cmp { lhs, rhs, .. }
            | IrOp::FBinOp { lhs, rhs, .. }
            | IrOp::FCmp { lhs, rhs, .. } => vec![lhs, rhs],
            IrOp::FMulAdd { a, b, c, .. } => vec![a, b, c],
            IrOp::Load { addr, .. } | IrOp::LoadF32 { addr, .. } => vec![addr],
            IrOp::Store { addr, val, .. } | IrOp::StoreF32 { addr, val, .. } => vec![addr, val],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrInst {
    pub dst: Option<ValueId>,
    pub ty: Option<IrType>,
    pub guest_pc: Option<u32>,
    pub op: IrOp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phi {
    pub dst: ValueId,
    pub ty: IrType,
    pub incoming: Vec<(BlockId, ValueId)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrTerm {
    Jump {
        target: BlockId,
    },
    BrIf {
        cond: ValueId,
        then_bb: BlockId,
        else_bb: BlockId,
    },
    BrIfCmp {
        op: CmpOp,
        lhs: ValueId,
        rhs: ValueId,
        then_bb: BlockId,
        else_bb: BlockId,
    },
    Call {
        target_pc: u32,
        ret_pc: u32,
        fallthrough: BlockId,
    },
    TailCall {
        target_pc: u32,
    },
    IndirectCall {
        target_val: ValueId,
        ret_pc: u32,
        fallthrough: BlockId,
    },
    IndirectJump {
        target_val: ValueId,
    },
    Return {
        target_val: ValueId,
    },
    Trap,
}

impl IrTerm {
    pub fn successors(&self) -> Vec<BlockId> {
        match *self {
            IrTerm::Jump { target } => vec![target],
            IrTerm::BrIf { then_bb, else_bb, .. }
            | IrTerm::BrIfCmp {
                then_bb, else_bb, ..
            } => vec![then_bb, else_bb],
            IrTerm::Call { fallthrough, .. } | IrTerm::IndirectCall { fallthrough, .. } => {
                vec![fallthrough]
            }
            IrTerm::TailCall { .. }
            | IrTerm::IndirectJump { .. }
            | IrTerm::Return { .. }
            | IrTerm::Trap => Vec::new(),
        }
    }

    pub fn uses(&self) -> Vec<ValueId> {
        match *self {
            IrTerm::BrIf { cond, .. } => vec![cond],
            IrTerm::BrIfCmp { lhs, rhs, .. } => vec![lhs, rhs],
            IrTerm::IndirectCall { target_val, .. }
            | IrTerm::IndirectJump { target_val }
            | IrTerm::Return { target_val } => vec![target_val],
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrBlock {
    pub id: BlockId,
    pub guest_pc: u32,
    pub phis: Vec<Phi>,
    pub insts: Vec<IrInst>,
    pub term: IrTerm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFunction {
    pub entry: u32,
    pub name: String,
    pub is_indirect_target: bool,
    pub blocks: Vec<IrBlock>,
    pub pc_to_block: BTreeMap<u32, BlockId>,
    pub next_value: u32,
}

impl IrFunction {
    pub fn new_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrModule {
    pub entry: u32,
    pub functions: BTreeMap<u32, IrFunction>,
    pub indirect_targets: BTreeSet<u32>,
}
