use crate::ir::{BlockId, IrBlock, IrFunction, IrInst, IrOp, IrTerm, IrType, Phi, ValueId};
use rv_decode::{FReg, Reg};
use std::collections::{BTreeMap, BTreeSet};

/// Promote guest registers (`x1..x31`, `f0..f31`) to SSA values (Wasm locals),
/// flushing dirty/written registers only across calls, ecalls, and function exits.
pub fn run(func: &mut IrFunction) {
    if func.blocks.is_empty() {
        return;
    }

    let mut used_regs = BTreeSet::new();
    let mut written_regs = BTreeSet::new();
    let mut used_fregs = BTreeSet::new();
    let mut written_fregs = BTreeSet::new();

    for bb in &func.blocks {
        for inst in &bb.insts {
            match inst.op {
                IrOp::ReadReg(r) if r != Reg::ZERO => {
                    used_regs.insert(r);
                }
                IrOp::WriteReg { reg, .. } if reg != Reg::ZERO => {
                    used_regs.insert(reg);
                    written_regs.insert(reg);
                }
                IrOp::ReadFReg(r) => {
                    used_fregs.insert(r);
                }
                IrOp::WriteFReg { freg, .. } => {
                    used_fregs.insert(freg);
                    written_fregs.insert(freg);
                }
                IrOp::Ecall => {
                    // Ecall reads a0..a3, a7 and writes a0.
                    for r in [Reg::A0, Reg::A1, Reg::A2, Reg(13), Reg::A7] {
                        used_regs.insert(r);
                    }
                    written_regs.insert(Reg::A0);
                }
                _ => {}
            }
        }
    }

    if used_regs.is_empty() && used_fregs.is_empty() {
        return;
    }

    // 1. Ensure entry_bb has no incoming predecessors (prepend synthetic prologue if needed).
    let mut entry_bb = func.pc_to_block.get(&func.entry).copied().unwrap_or(BlockId(0));
    let entry_has_preds = func
        .blocks
        .iter()
        .any(|bb| bb.term.successors().contains(&entry_bb));
    if entry_has_preds {
        let new_id = BlockId(func.blocks.len() as u32);
        let guest_pc = func.blocks[entry_bb.0 as usize].guest_pc;
        func.blocks.push(IrBlock {
            id: new_id,
            guest_pc,
            phis: Vec::new(),
            insts: Vec::new(),
            term: IrTerm::Jump { target: entry_bb },
        });
        func.pc_to_block.insert(func.entry, new_id);
        entry_bb = new_id;
    }

    // 2. Split Call / IndirectCall fallthrough edges into post-call reload blocks so
    //    register reloads happen cleanly before merging into any multi-pred fallthrough.
    let orig_len = func.blocks.len();
    for i in 0..orig_len {
        let term = func.blocks[i].term.clone();
        match term {
            IrTerm::Call {
                target_pc,
                ret_pc,
                fallthrough,
            } => {
                let post_id = BlockId(func.blocks.len() as u32);
                let insts = build_reload_insts(func, ret_pc, &used_regs, &used_fregs);
                func.blocks.push(IrBlock {
                    id: post_id,
                    guest_pc: ret_pc,
                    phis: Vec::new(),
                    insts,
                    term: IrTerm::Jump { target: fallthrough },
                });
                func.blocks[i].term = IrTerm::Call {
                    target_pc,
                    ret_pc,
                    fallthrough: post_id,
                };
            }
            IrTerm::IndirectCall {
                target_val,
                ret_pc,
                fallthrough,
            } => {
                let post_id = BlockId(func.blocks.len() as u32);
                let insts = build_reload_insts(func, ret_pc, &used_regs, &used_fregs);
                func.blocks.push(IrBlock {
                    id: post_id,
                    guest_pc: ret_pc,
                    phis: Vec::new(),
                    insts,
                    term: IrTerm::Jump { target: fallthrough },
                });
                func.blocks[i].term = IrTerm::IndirectCall {
                    target_val,
                    ret_pc,
                    fallthrough: post_id,
                };
            }
            _ => {}
        }
    }

    // 3. Compute reachable blocks in RPO and predecessor lists.
    let n = func.blocks.len();
    let mut visited = vec![false; n];
    let mut postorder = Vec::with_capacity(n);
    dfs_postorder(func, entry_bb, &mut visited, &mut postorder);
    postorder.reverse();
    let rpo = postorder;

    let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); n];
    for &bb in &rpo {
        for succ in func.blocks[bb.0 as usize].term.successors() {
            preds[succ.0 as usize].push(bb);
        }
    }

    // 4. Pre-allocate Phi nodes at every non-entry reachable block.
    let mut reg_phis: BTreeMap<(BlockId, Reg), ValueId> = BTreeMap::new();
    let mut freg_phis: BTreeMap<(BlockId, FReg), ValueId> = BTreeMap::new();
    for &bb_id in &rpo {
        if bb_id == entry_bb {
            continue;
        }
        for &r in &used_regs {
            let v = func.new_value();
            reg_phis.insert((bb_id, r), v);
            func.blocks[bb_id.0 as usize].phis.push(Phi {
                dst: v,
                ty: IrType::I32,
                incoming: Vec::new(),
            });
        }
        for &fr in &used_fregs {
            let v = func.new_value();
            freg_phis.insert((bb_id, fr), v);
            func.blocks[bb_id.0 as usize].phis.push(Phi {
                dst: v,
                ty: IrType::F32,
                incoming: Vec::new(),
            });
        }
    }

    // 5. Rewrite each reachable block's instructions and record exit environments.
    let mut exit_reg_env: Vec<BTreeMap<Reg, ValueId>> = vec![BTreeMap::new(); n];
    let mut exit_freg_env: Vec<BTreeMap<FReg, ValueId>> = vec![BTreeMap::new(); n];

    for &bb_id in &rpo {
        let mut r_env: BTreeMap<Reg, ValueId> = BTreeMap::new();
        let mut f_env: BTreeMap<FReg, ValueId> = BTreeMap::new();
        let mut new_insts = Vec::new();
        let bb_pc = func.blocks[bb_id.0 as usize].guest_pc;

        if bb_id == entry_bb {
            for &r in &used_regs {
                let v = func.new_value();
                new_insts.push(IrInst {
                    dst: Some(v),
                    ty: Some(IrType::I32),
                    guest_pc: Some(bb_pc),
                    op: IrOp::ReadReg(r),
                });
                r_env.insert(r, v);
            }
            for &fr in &used_fregs {
                let v = func.new_value();
                new_insts.push(IrInst {
                    dst: Some(v),
                    ty: Some(IrType::F32),
                    guest_pc: Some(bb_pc),
                    op: IrOp::ReadFReg(fr),
                });
                f_env.insert(fr, v);
            }
        } else {
            for &r in &used_regs {
                r_env.insert(r, reg_phis[&(bb_id, r)]);
            }
            for &fr in &used_fregs {
                f_env.insert(fr, freg_phis[&(bb_id, fr)]);
            }
        }

        let old_insts = std::mem::take(&mut func.blocks[bb_id.0 as usize].insts);
        for inst in old_insts {
            match inst.op {
                IrOp::ReadReg(r) if r != Reg::ZERO => {
                    let cur = r_env[&r];
                    if let Some(dst) = inst.dst {
                        new_insts.push(IrInst {
                            dst: Some(dst),
                            ty: Some(IrType::I32),
                            guest_pc: inst.guest_pc,
                            op: IrOp::Copy(cur),
                        });
                    }
                    // Note: if this was a post-call reload block, we actually want ReadReg!
                    // Post-call reload blocks were created in step 2; handle them by checking dst?
                    // Even simpler: in step 2, post-call reload blocks don't exist in old_insts?
                    // Wait: step 2 ALREADY added ReadReg to post_id's insts!
                    // So if bb_id >= orig_len, keep ReadReg and update r_env!
                    if (bb_id.0 as usize) >= orig_len {
                        new_insts.pop();
                        new_insts.push(inst.clone());
                        if let Some(dst) = inst.dst {
                            r_env.insert(r, dst);
                        }
                    }
                }
                IrOp::WriteReg { reg, val } if reg != Reg::ZERO => {
                    r_env.insert(reg, val);
                }
                IrOp::ReadFReg(fr) => {
                    if (bb_id.0 as usize) >= orig_len {
                        new_insts.push(inst.clone());
                        if let Some(dst) = inst.dst {
                            f_env.insert(fr, dst);
                        }
                    } else if let Some(dst) = inst.dst {
                        let cur = f_env[&fr];
                        new_insts.push(IrInst {
                            dst: Some(dst),
                            ty: Some(IrType::F32),
                            guest_pc: inst.guest_pc,
                            op: IrOp::Copy(cur),
                        });
                    }
                }
                IrOp::WriteFReg { freg, val } => {
                    f_env.insert(freg, val);
                }
                IrOp::Ecall => {
                    // Flush written registers before ecall.
                    flush_regs(
                        &mut new_insts,
                        inst.guest_pc,
                        &written_regs,
                        &written_fregs,
                        &r_env,
                        &f_env,
                    );
                    new_insts.push(inst.clone());
                    // Reload a0 after ecall.
                    let v_a0 = func.new_value();
                    new_insts.push(IrInst {
                        dst: Some(v_a0),
                        ty: Some(IrType::I32),
                        guest_pc: inst.guest_pc,
                        op: IrOp::ReadReg(Reg::A0),
                    });
                    r_env.insert(Reg::A0, v_a0);
                }
                _ => new_insts.push(inst),
            }
        }

        // Flush written registers before calls / tail calls / returns / traps.
        let needs_flush = matches!(
            func.blocks[bb_id.0 as usize].term,
            IrTerm::Call { .. }
                | IrTerm::TailCall { .. }
                | IrTerm::IndirectCall { .. }
                | IrTerm::IndirectJump { .. }
                | IrTerm::Return { .. }
                | IrTerm::Trap
        );
        if needs_flush {
            flush_regs(
                &mut new_insts,
                Some(bb_pc),
                &written_regs,
                &written_fregs,
                &r_env,
                &f_env,
            );
        }

        func.blocks[bb_id.0 as usize].insts = new_insts;
        exit_reg_env[bb_id.0 as usize] = r_env;
        exit_freg_env[bb_id.0 as usize] = f_env;
    }

    // 6. Populate Phi incoming lists.
    for &bb_id in &rpo {
        if bb_id == entry_bb {
            continue;
        }
        let bb_preds = &preds[bb_id.0 as usize];
        let mut phi_idx = 0usize;
        for &r in &used_regs {
            let incoming = bb_preds
                .iter()
                .map(|&p| (p, exit_reg_env[p.0 as usize][&r]))
                .collect();
            func.blocks[bb_id.0 as usize].phis[phi_idx].incoming = incoming;
            phi_idx += 1;
        }
        for &fr in &used_fregs {
            let incoming = bb_preds
                .iter()
                .map(|&p| (p, exit_freg_env[p.0 as usize][&fr]))
                .collect();
            func.blocks[bb_id.0 as usize].phis[phi_idx].incoming = incoming;
            phi_idx += 1;
        }
    }

    // 7. Iteratively eliminate trivial Phis (phi [v, v, ...] or phi [self, v] -> v).
    eliminate_trivial_phis(func);
}

fn build_reload_insts(
    func: &mut IrFunction,
    pc: u32,
    used_regs: &BTreeSet<Reg>,
    used_fregs: &BTreeSet<FReg>,
) -> Vec<IrInst> {
    let mut insts = Vec::with_capacity(used_regs.len() + used_fregs.len());
    for &r in used_regs {
        let dst = func.new_value();
        insts.push(IrInst {
            dst: Some(dst),
            ty: Some(IrType::I32),
            guest_pc: Some(pc),
            op: IrOp::ReadReg(r),
        });
    }
    for &fr in used_fregs {
        let dst = func.new_value();
        insts.push(IrInst {
            dst: Some(dst),
            ty: Some(IrType::F32),
            guest_pc: Some(pc),
            op: IrOp::ReadFReg(fr),
        });
    }
    insts
}

fn flush_regs(
    insts: &mut Vec<IrInst>,
    guest_pc: Option<u32>,
    written_regs: &BTreeSet<Reg>,
    written_fregs: &BTreeSet<FReg>,
    r_env: &BTreeMap<Reg, ValueId>,
    f_env: &BTreeMap<FReg, ValueId>,
) {
    for &r in written_regs {
        if let Some(&val) = r_env.get(&r) {
            insts.push(IrInst {
                dst: None,
                ty: None,
                guest_pc,
                op: IrOp::WriteReg { reg: r, val },
            });
        }
    }
    for &fr in written_fregs {
        if let Some(&val) = f_env.get(&fr) {
            insts.push(IrInst {
                dst: None,
                ty: None,
                guest_pc,
                op: IrOp::WriteFReg { freg: fr, val },
            });
        }
    }
}

fn dfs_postorder(func: &IrFunction, bb: BlockId, visited: &mut [bool], postorder: &mut Vec<BlockId>) {
    let idx = bb.0 as usize;
    if visited[idx] {
        return;
    }
    visited[idx] = true;
    for succ in func.blocks[idx].term.successors() {
        dfs_postorder(func, succ, visited, postorder);
    }
    postorder.push(bb);
}

fn eliminate_trivial_phis(func: &mut IrFunction) {
    let mut changed = true;
    while changed {
        changed = false;
        let mut replace: BTreeMap<ValueId, ValueId> = BTreeMap::new();

        for bb in &func.blocks {
            for phi in &bb.phis {
                let mut unique: Option<ValueId> = None;
                let mut trivial = true;
                for &(_, v) in &phi.incoming {
                    if v == phi.dst || Some(v) == unique {
                        continue;
                    }
                    if unique.is_some() {
                        trivial = false;
                        break;
                    }
                    unique = Some(v);
                }
                if trivial {
                    if let Some(u) = unique {
                        replace.insert(phi.dst, u);
                        changed = true;
                    }
                }
            }
        }

        if !replace.is_empty() {
            let resolve = |mut v: ValueId| -> ValueId {
                while let Some(&next) = replace.get(&v) {
                    if next == v {
                        break;
                    }
                    v = next;
                }
                v
            };

            for bb in &mut func.blocks {
                bb.phis.retain(|p| !replace.contains_key(&p.dst));
                for phi in &mut bb.phis {
                    for (_, v) in &mut phi.incoming {
                        *v = resolve(*v);
                    }
                }
                for inst in &mut bb.insts {
                    remap_op(&mut inst.op, &resolve);
                }
                remap_term(&mut bb.term, &resolve);
            }
        }
    }
}

pub(crate) fn remap_op(op: &mut IrOp, resolve: &impl Fn(ValueId) -> ValueId) {
    match op {
        IrOp::Copy(v)
        | IrOp::WriteReg { val: v, .. }
        | IrOp::WriteFReg { val: v, .. }
        | IrOp::FUnaryOp { src: v, .. }
        | IrOp::CsrOp { val: v, .. } => {
            *v = resolve(*v);
        }
        IrOp::BinOp { lhs, rhs, .. }
        | IrOp::Cmp { lhs, rhs, .. }
        | IrOp::FBinOp { lhs, rhs, .. }
        | IrOp::FCmp { lhs, rhs, .. } => {
            *lhs = resolve(*lhs);
            *rhs = resolve(*rhs);
        }
        IrOp::FMulAdd { a, b, c, .. } => {
            *a = resolve(*a);
            *b = resolve(*b);
            *c = resolve(*c);
        }
        IrOp::Load { addr, .. } | IrOp::LoadF32 { addr, .. } => {
            *addr = resolve(*addr);
        }
        IrOp::Store { addr, val, .. } | IrOp::StoreF32 { addr, val, .. } => {
            *addr = resolve(*addr);
            *val = resolve(*val);
        }
        _ => {}
    }
}

pub(crate) fn remap_term(term: &mut IrTerm, resolve: &impl Fn(ValueId) -> ValueId) {
    match term {
        IrTerm::BrIf { cond, .. } => *cond = resolve(*cond),
        IrTerm::BrIfCmp { lhs, rhs, .. } => {
            *lhs = resolve(*lhs);
            *rhs = resolve(*rhs);
        }
        IrTerm::IndirectCall { target_val, .. }
        | IrTerm::IndirectJump { target_val }
        | IrTerm::Return { target_val } => {
            *target_val = resolve(*target_val);
        }
        _ => {}
    }
}
