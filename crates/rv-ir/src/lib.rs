//! SSA Intermediate Representation and optimization passes.

pub mod ir;
pub mod lift;
pub mod printer;

pub use ir::{
    BinOp, BlockId, CmpOp, IrBlock, IrFunction, IrInst, IrModule, IrOp, IrTerm, IrType, MemWidth,
    OptConfig, Phi, ValueId,
};
pub use lift::lift_program;
pub use printer::print_function;
