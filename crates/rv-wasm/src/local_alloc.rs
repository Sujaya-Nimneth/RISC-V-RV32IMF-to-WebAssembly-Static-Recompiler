use rv_ir::{IrFunction, IrType};
use std::collections::{BTreeMap, BTreeSet};
use wasm_encoder::ValType;

/// Allocate Wasm local indices for SSA values, optionally coalescing block-local temporaries.
pub fn allocate_locals(
    ir_fn: &IrFunction,
    val_types: &[ValType],
    is_dispatcher: bool,
    coalesce: bool,
) -> (Vec<u32>, Option<u32>, Vec<(u32, ValType)>) {
    let n = val_types.len();
    let mut val_locals = vec![0u32; n];
    let mut locals_decl: Vec<ValType> = Vec::new();

    if !coalesce {
        for (idx, &ty) in val_types.iter().enumerate() {
            val_locals[idx] = 1 + locals_decl.len() as u32;
            locals_decl.push(ty);
        }
    } else {
        // Identify values used across multiple blocks or in Phis/terminators (global values).
        let mut def_block: Vec<Option<usize>> = vec![None; n];
        let mut cross_block: Vec<bool> = vec![false; n];

        for (b_idx, bb) in ir_fn.blocks.iter().enumerate() {
            for phi in &bb.phis {
                def_block[phi.dst.0 as usize] = Some(b_idx);
                cross_block[phi.dst.0 as usize] = true;
                for &(_, v) in &phi.incoming {
                    cross_block[v.0 as usize] = true;
                }
            }
            for inst in &bb.insts {
                if let Some(dst) = inst.dst {
                    def_block[dst.0 as usize] = Some(b_idx);
                }
                for u in inst.op.uses() {
                    if def_block[u.0 as usize] != Some(b_idx) {
                        cross_block[u.0 as usize] = true;
                    }
                }
            }
            for u in bb.term.uses() {
                if def_block[u.0 as usize] != Some(b_idx) {
                    cross_block[u.0 as usize] = true;
                }
            }
        }

        // Allocate dedicated locals for cross-block values.
        for (idx, &ty) in val_types.iter().enumerate() {
            if cross_block[idx] || def_block[idx].is_none() {
                val_locals[idx] = 1 + locals_decl.len() as u32;
                locals_decl.push(ty);
            }
        }

        // Pool per-type block-local temporaries across blocks using last-use liveness.
        let mut pool_i32: Vec<u32> = Vec::new();
        let mut pool_f32: Vec<u32> = Vec::new();

        for (b_idx, bb) in ir_fn.blocks.iter().enumerate() {
            let mut last_use: BTreeMap<u32, usize> = BTreeMap::new();
            for (i_idx, inst) in bb.insts.iter().enumerate() {
                for u in inst.op.uses() {
                    if !cross_block[u.0 as usize] && def_block[u.0 as usize] == Some(b_idx) {
                        last_use.insert(u.0, i_idx);
                    }
                }
            }
            for u in bb.term.uses() {
                if !cross_block[u.0 as usize] && def_block[u.0 as usize] == Some(b_idx) {
                    last_use.insert(u.0, bb.insts.len());
                }
            }

            let mut free_i32 = pool_i32.clone();
            let mut free_f32 = pool_f32.clone();
            let mut active: BTreeSet<(usize, u32, bool)> = BTreeSet::new();

            for (i_idx, inst) in bb.insts.iter().enumerate() {
                // Free any temporaries whose last use was strictly before i_idx.
                while let Some(&(end_i, v_id, is_f32)) = active.first() {
                    if end_i < i_idx {
                        active.remove(&(end_i, v_id, is_f32));
                        let loc = val_locals[v_id as usize];
                        if is_f32 {
                            free_f32.push(loc);
                        } else {
                            free_i32.push(loc);
                        }
                    } else {
                        break;
                    }
                }

                if let Some(dst) = inst.dst {
                    if !cross_block[dst.0 as usize] {
                        let is_f32 = matches!(inst.ty, Some(IrType::F32));
                        let loc = if is_f32 {
                            free_f32.pop().unwrap_or_else(|| {
                                let l = 1 + locals_decl.len() as u32;
                                locals_decl.push(ValType::F32);
                                pool_f32.push(l);
                                l
                            })
                        } else {
                            free_i32.pop().unwrap_or_else(|| {
                                let l = 1 + locals_decl.len() as u32;
                                locals_decl.push(ValType::I32);
                                pool_i32.push(l);
                                l
                            })
                        };
                        val_locals[dst.0 as usize] = loc;
                        let end_i = last_use.get(&dst.0).copied().unwrap_or(i_idx);
                        active.insert((end_i, dst.0, is_f32));
                    }
                }
            }
        }
    }

    let bb_id_local = if is_dispatcher {
        let idx = 1 + locals_decl.len() as u32;
        locals_decl.push(ValType::I32);
        Some(idx)
    } else {
        None
    };

    let mut runs: Vec<(u32, ValType)> = Vec::new();
    for ty in locals_decl {
        if let Some(last) = runs.last_mut() {
            if last.1 == ty {
                last.0 += 1;
                continue;
            }
        }
        runs.push((1, ty));
    }

    (val_locals, bb_id_local, runs)
}
