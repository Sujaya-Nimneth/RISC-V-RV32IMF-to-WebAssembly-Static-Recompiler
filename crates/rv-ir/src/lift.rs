use crate::ir::{
    BinOp, BlockId, CmpOp, IrBlock, IrFunction, IrInst, IrModule, IrOp, IrTerm, IrType, MemWidth,
    OptConfig, ValueId,
};
use rv_decode::{BlockTerm, FunctionCfg, Instruction, ProgramCfg, Reg};
use std::collections::BTreeMap;

/// Lift a `ProgramCfg` into an `IrModule` under the given `OptConfig`.
pub fn lift_program(cfg: &ProgramCfg, opt: &OptConfig) -> IrModule {
    let mut functions = BTreeMap::new();
    for (&entry, fcfg) in &cfg.functions {
        functions.insert(entry, lift_function(fcfg, opt));
    }
    let mut module = IrModule {
        entry: cfg.entry,
        functions,
        indirect_targets: cfg.indirect_targets.clone(),
    };
    crate::opt::optimize_module(&mut module, opt);
    module
}

fn lift_function(fcfg: &FunctionCfg, opt: &OptConfig) -> IrFunction {
    let mut pc_to_block = BTreeMap::new();
    for (idx, &pc) in fcfg.blocks.keys().enumerate() {
        pc_to_block.insert(pc, BlockId(idx as u32));
    }

    let mut func = IrFunction {
        entry: fcfg.entry,
        name: fcfg.name.clone(),
        is_indirect_target: fcfg.is_indirect_target,
        blocks: Vec::with_capacity(fcfg.blocks.len()),
        pc_to_block: pc_to_block.clone(),
        next_value: 0,
    };

    for (&start_pc, bb) in &fcfg.blocks {
        let block_id = pc_to_block[&start_pc];
        let mut insts = Vec::new();

        if opt.count_instructions && !bb.insts.is_empty() {
            insts.push(IrInst {
                dst: None,
                ty: None,
                guest_pc: Some(start_pc),
                op: IrOp::AddInstret(bb.insts.len() as u32),
            });
        }

        let mut term_opt: Option<IrTerm> = None;

        for di in &bb.insts {
            let pc = di.pc;
            match di.inst {
                Instruction::Lui { rd, imm } => {
                    let v = emit_const_i32(&mut func, &mut insts, pc, imm);
                    emit_write_reg(&mut insts, pc, rd, v);
                }
                Instruction::Auipc { rd, imm } => {
                    let v = emit_const_i32(&mut func, &mut insts, pc, pc.wrapping_add(imm as u32) as i32);
                    emit_write_reg(&mut insts, pc, rd, v);
                }
                Instruction::Jal { rd, imm } => {
                    let ret = pc.wrapping_add(4) as i32;
                    if rd != Reg::ZERO {
                        let v = emit_const_i32(&mut func, &mut insts, pc, ret);
                        emit_write_reg(&mut insts, pc, rd, v);
                    }
                    let _ = imm;
                }
                Instruction::Jalr { rd, rs1, imm } => {
                    // Compute target before writing rd in case rd == rs1.
                    let base = emit_read_reg(&mut func, &mut insts, pc, rs1);
                    let sum = if imm != 0 {
                        let c = emit_const_i32(&mut func, &mut insts, pc, imm);
                        emit_binop(&mut func, &mut insts, pc, BinOp::Add, base, c)
                    } else {
                        base
                    };
                    let mask = emit_const_i32(&mut func, &mut insts, pc, !1i32);
                    let target = emit_binop(&mut func, &mut insts, pc, BinOp::And, sum, mask);

                    if rd != Reg::ZERO {
                        let ret = emit_const_i32(&mut func, &mut insts, pc, pc.wrapping_add(4) as i32);
                        emit_write_reg(&mut insts, pc, rd, ret);
                    }

                    term_opt = Some(match bb.term {
                        BlockTerm::Return => IrTerm::Return { target_val: target },
                        BlockTerm::IndirectJump => IrTerm::IndirectJump { target_val: target },
                        BlockTerm::IndirectCall { ret_pc } => IrTerm::IndirectCall {
                            target_val: target,
                            ret_pc,
                            fallthrough: pc_to_block[&ret_pc],
                        },
                        _ => IrTerm::Trap,
                    });
                }

                Instruction::Beq { rs1, rs2, .. } => {
                    term_opt = Some(lift_branch(&mut func, &mut insts, pc, CmpOp::Eq, rs1, rs2, &bb.term, &pc_to_block));
                }
                Instruction::Bne { rs1, rs2, .. } => {
                    term_opt = Some(lift_branch(&mut func, &mut insts, pc, CmpOp::Ne, rs1, rs2, &bb.term, &pc_to_block));
                }
                Instruction::Blt { rs1, rs2, .. } => {
                    term_opt = Some(lift_branch(&mut func, &mut insts, pc, CmpOp::LtS, rs1, rs2, &bb.term, &pc_to_block));
                }
                Instruction::Bge { rs1, rs2, .. } => {
                    term_opt = Some(lift_branch(&mut func, &mut insts, pc, CmpOp::GeS, rs1, rs2, &bb.term, &pc_to_block));
                }
                Instruction::Bltu { rs1, rs2, .. } => {
                    term_opt = Some(lift_branch(&mut func, &mut insts, pc, CmpOp::LtU, rs1, rs2, &bb.term, &pc_to_block));
                }
                Instruction::Bgeu { rs1, rs2, .. } => {
                    term_opt = Some(lift_branch(&mut func, &mut insts, pc, CmpOp::GeU, rs1, rs2, &bb.term, &pc_to_block));
                }

                Instruction::Lb { rd, rs1, imm } => lift_load(&mut func, &mut insts, pc, rd, rs1, imm, MemWidth::B1, true),
                Instruction::Lh { rd, rs1, imm } => lift_load(&mut func, &mut insts, pc, rd, rs1, imm, MemWidth::B2, true),
                Instruction::Lw { rd, rs1, imm } => lift_load(&mut func, &mut insts, pc, rd, rs1, imm, MemWidth::B4, true),
                Instruction::Lbu { rd, rs1, imm } => lift_load(&mut func, &mut insts, pc, rd, rs1, imm, MemWidth::B1, false),
                Instruction::Lhu { rd, rs1, imm } => lift_load(&mut func, &mut insts, pc, rd, rs1, imm, MemWidth::B2, false),

                Instruction::Sb { rs1, rs2, imm } => lift_store(&mut func, &mut insts, pc, rs1, rs2, imm, MemWidth::B1),
                Instruction::Sh { rs1, rs2, imm } => lift_store(&mut func, &mut insts, pc, rs1, rs2, imm, MemWidth::B2),
                Instruction::Sw { rs1, rs2, imm } => lift_store(&mut func, &mut insts, pc, rs1, rs2, imm, MemWidth::B4),

                Instruction::Addi { rd, rs1, imm } => lift_alu_imm(&mut func, &mut insts, pc, rd, rs1, imm, BinOp::Add),
                Instruction::Xori { rd, rs1, imm } => lift_alu_imm(&mut func, &mut insts, pc, rd, rs1, imm, BinOp::Xor),
                Instruction::Ori { rd, rs1, imm } => lift_alu_imm(&mut func, &mut insts, pc, rd, rs1, imm, BinOp::Or),
                Instruction::Andi { rd, rs1, imm } => lift_alu_imm(&mut func, &mut insts, pc, rd, rs1, imm, BinOp::And),
                Instruction::Slti { rd, rs1, imm } => lift_cmp_imm(&mut func, &mut insts, pc, rd, rs1, imm, CmpOp::LtS),
                Instruction::Sltiu { rd, rs1, imm } => lift_cmp_imm(&mut func, &mut insts, pc, rd, rs1, imm, CmpOp::LtU),
                Instruction::Slli { rd, rs1, shamt } => lift_alu_imm(&mut func, &mut insts, pc, rd, rs1, shamt as i32, BinOp::Shl),
                Instruction::Srli { rd, rs1, shamt } => lift_alu_imm(&mut func, &mut insts, pc, rd, rs1, shamt as i32, BinOp::ShrU),
                Instruction::Srai { rd, rs1, shamt } => lift_alu_imm(&mut func, &mut insts, pc, rd, rs1, shamt as i32, BinOp::ShrS),

                Instruction::Add { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Add),
                Instruction::Sub { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Sub),
                Instruction::Sll { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Shl),
                Instruction::Srl { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::ShrU),
                Instruction::Sra { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::ShrS),
                Instruction::Xor { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Xor),
                Instruction::Or { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Or),
                Instruction::And { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::And),
                Instruction::Slt { rd, rs1, rs2 } => lift_cmp_rr(&mut func, &mut insts, pc, rd, rs1, rs2, CmpOp::LtS),
                Instruction::Sltu { rd, rs1, rs2 } => lift_cmp_rr(&mut func, &mut insts, pc, rd, rs1, rs2, CmpOp::LtU),

                Instruction::Mul { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Mul),
                Instruction::Mulh { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Mulh),
                Instruction::Mulhsu { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Mulhsu),
                Instruction::Mulhu { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::Mulhu),
                Instruction::Div { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::DivS),
                Instruction::Divu { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::DivU),
                Instruction::Rem { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::RemS),
                Instruction::Remu { rd, rs1, rs2 } => lift_alu_rr(&mut func, &mut insts, pc, rd, rs1, rs2, BinOp::RemU),

                Instruction::Fence => {}
                Instruction::Ecall => {
                    insts.push(IrInst {
                        dst: None,
                        ty: None,
                        guest_pc: Some(pc),
                        op: IrOp::Ecall,
                    });
                }
                Instruction::Ebreak => {
                    term_opt = Some(IrTerm::Trap);
                }
            }
        }

        let term = term_opt.unwrap_or_else(|| match bb.term {
            BlockTerm::Fallthrough { next } => pc_to_block
                .get(&next)
                .map(|&target| IrTerm::Jump { target })
                .unwrap_or(IrTerm::Trap),
            BlockTerm::Jump { target } => pc_to_block
                .get(&target)
                .map(|&target| IrTerm::Jump { target })
                .unwrap_or(IrTerm::Trap),
            BlockTerm::Call { target, ret_pc } => pc_to_block
                .get(&ret_pc)
                .map(|&fallthrough| IrTerm::Call {
                    target_pc: target,
                    ret_pc,
                    fallthrough,
                })
                .unwrap_or(IrTerm::Trap),
            BlockTerm::TailCall { target } => IrTerm::TailCall { target_pc: target },
            _ => IrTerm::Trap,
        });

        func.blocks.push(IrBlock {
            id: block_id,
            guest_pc: start_pc,
            phis: Vec::new(),
            insts,
            term,
        });
    }

    func
}

fn emit_const_i32(func: &mut IrFunction, insts: &mut Vec<IrInst>, pc: u32, val: i32) -> ValueId {
    let dst = func.new_value();
    insts.push(IrInst {
        dst: Some(dst),
        ty: Some(IrType::I32),
        guest_pc: Some(pc),
        op: IrOp::ConstI32(val),
    });
    dst
}

fn emit_read_reg(func: &mut IrFunction, insts: &mut Vec<IrInst>, pc: u32, reg: Reg) -> ValueId {
    let dst = func.new_value();
    insts.push(IrInst {
        dst: Some(dst),
        ty: Some(IrType::I32),
        guest_pc: Some(pc),
        op: IrOp::ReadReg(reg),
    });
    dst
}

fn emit_write_reg(insts: &mut Vec<IrInst>, pc: u32, reg: Reg, val: ValueId) {
    insts.push(IrInst {
        dst: None,
        ty: None,
        guest_pc: Some(pc),
        op: IrOp::WriteReg { reg, val },
    });
}

fn emit_binop(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    op: BinOp,
    lhs: ValueId,
    rhs: ValueId,
) -> ValueId {
    let dst = func.new_value();
    insts.push(IrInst {
        dst: Some(dst),
        ty: Some(IrType::I32),
        guest_pc: Some(pc),
        op: IrOp::BinOp { op, lhs, rhs },
    });
    dst
}

fn emit_cmp(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    op: CmpOp,
    lhs: ValueId,
    rhs: ValueId,
) -> ValueId {
    let dst = func.new_value();
    insts.push(IrInst {
        dst: Some(dst),
        ty: Some(IrType::I32),
        guest_pc: Some(pc),
        op: IrOp::Cmp { op, lhs, rhs },
    });
    dst
}

fn lift_alu_imm(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    rd: Reg,
    rs1: Reg,
    imm: i32,
    op: BinOp,
) {
    let a = emit_read_reg(func, insts, pc, rs1);
    let b = emit_const_i32(func, insts, pc, imm);
    let res = emit_binop(func, insts, pc, op, a, b);
    emit_write_reg(insts, pc, rd, res);
}

fn lift_cmp_imm(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    rd: Reg,
    rs1: Reg,
    imm: i32,
    op: CmpOp,
) {
    let a = emit_read_reg(func, insts, pc, rs1);
    let b = emit_const_i32(func, insts, pc, imm);
    let res = emit_cmp(func, insts, pc, op, a, b);
    emit_write_reg(insts, pc, rd, res);
}

fn lift_alu_rr(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    rd: Reg,
    rs1: Reg,
    rs2: Reg,
    op: BinOp,
) {
    let a = emit_read_reg(func, insts, pc, rs1);
    let b = emit_read_reg(func, insts, pc, rs2);
    let res = emit_binop(func, insts, pc, op, a, b);
    emit_write_reg(insts, pc, rd, res);
}

fn lift_cmp_rr(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    rd: Reg,
    rs1: Reg,
    rs2: Reg,
    op: CmpOp,
) {
    let a = emit_read_reg(func, insts, pc, rs1);
    let b = emit_read_reg(func, insts, pc, rs2);
    let res = emit_cmp(func, insts, pc, op, a, b);
    emit_write_reg(insts, pc, rd, res);
}

fn lift_load(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    rd: Reg,
    rs1: Reg,
    imm: i32,
    width: MemWidth,
    signed: bool,
) {
    let base = emit_read_reg(func, insts, pc, rs1);
    let addr = if imm != 0 {
        let off = emit_const_i32(func, insts, pc, imm);
        emit_binop(func, insts, pc, BinOp::Add, base, off)
    } else {
        base
    };
    let dst = func.new_value();
    insts.push(IrInst {
        dst: Some(dst),
        ty: Some(IrType::I32),
        guest_pc: Some(pc),
        op: IrOp::Load {
            addr,
            offset: 0,
            width,
            signed,
        },
    });
    emit_write_reg(insts, pc, rd, dst);
}

fn lift_store(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    rs1: Reg,
    rs2: Reg,
    imm: i32,
    width: MemWidth,
) {
    let base = emit_read_reg(func, insts, pc, rs1);
    let val = emit_read_reg(func, insts, pc, rs2);
    let addr = if imm != 0 {
        let off = emit_const_i32(func, insts, pc, imm);
        emit_binop(func, insts, pc, BinOp::Add, base, off)
    } else {
        base
    };
    insts.push(IrInst {
        dst: None,
        ty: None,
        guest_pc: Some(pc),
        op: IrOp::Store {
            addr,
            val,
            offset: 0,
            width,
        },
    });
}

fn lift_branch(
    func: &mut IrFunction,
    insts: &mut Vec<IrInst>,
    pc: u32,
    op: CmpOp,
    rs1: Reg,
    rs2: Reg,
    term: &BlockTerm,
    pc_to_block: &BTreeMap<u32, BlockId>,
) -> IrTerm {
    let BlockTerm::Branch {
        target,
        fallthrough,
    } = *term
    else {
        return IrTerm::Trap;
    };
    let (Some(&then_bb), Some(&else_bb)) = (pc_to_block.get(&target), pc_to_block.get(&fallthrough))
    else {
        return IrTerm::Trap;
    };
    let a = emit_read_reg(func, insts, pc, rs1);
    let b = emit_read_reg(func, insts, pc, rs2);
    let cond = emit_cmp(func, insts, pc, op, a, b);
    IrTerm::BrIf {
        cond,
        then_bb,
        else_bb,
    }
}
