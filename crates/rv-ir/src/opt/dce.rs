use crate::ir::{IrFunction, ValueId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Mark-and-sweep Dead Code Elimination removing unused side-effect-free instructions and Phis.
pub fn run(func: &mut IrFunction) {
    let mut def_inst: BTreeMap<ValueId, (usize, usize)> = BTreeMap::new();
    let mut def_phi: BTreeMap<ValueId, (usize, usize)> = BTreeMap::new();
    let mut live: BTreeSet<ValueId> = BTreeSet::new();
    let mut worklist: VecDeque<ValueId> = VecDeque::new();

    for (b_idx, bb) in func.blocks.iter().enumerate() {
        for (p_idx, phi) in bb.phis.iter().enumerate() {
            def_phi.insert(phi.dst, (b_idx, p_idx));
        }
        for (i_idx, inst) in bb.insts.iter().enumerate() {
            if let Some(dst) = inst.dst {
                def_inst.insert(dst, (b_idx, i_idx));
            }
            if inst.op.has_side_effects() {
                for u in inst.op.uses() {
                    if live.insert(u) {
                        worklist.push_back(u);
                    }
                }
            }
        }
        for u in bb.term.uses() {
            if live.insert(u) {
                worklist.push_back(u);
            }
        }
    }

    while let Some(v) = worklist.pop_front() {
        if let Some(&(b_idx, i_idx)) = def_inst.get(&v) {
            for u in func.blocks[b_idx].insts[i_idx].op.uses() {
                if live.insert(u) {
                    worklist.push_back(u);
                }
            }
        } else if let Some(&(b_idx, p_idx)) = def_phi.get(&v) {
            for &(_, u) in &func.blocks[b_idx].phis[p_idx].incoming {
                if live.insert(u) {
                    worklist.push_back(u);
                }
            }
        }
    }

    for bb in &mut func.blocks {
        bb.phis.retain(|p| live.contains(&p.dst));
        bb.insts.retain(|inst| {
            inst.op.has_side_effects() || inst.dst.map(|d| live.contains(&d)).unwrap_or(false)
        });
    }
}
