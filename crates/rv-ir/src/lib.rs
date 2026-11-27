//! SSA Intermediate Representation and optimization passes.

pub mod ir;
pub mod lift;
pub mod opt;
pub mod printer;

pub use ir::{
    BinOp, BlockId, CmpOp, CsrKind, FBinOp, FCmpOp, FMulAddOp, FUnaryOp, IrBlock, IrFunction,
    IrInst, IrModule, IrOp, IrTerm, IrType, MemWidth, OptConfig, Phi, ValueId,
};
pub use lift::lift_program;
pub use opt::{optimize_function, optimize_module};
pub use printer::print_function;
