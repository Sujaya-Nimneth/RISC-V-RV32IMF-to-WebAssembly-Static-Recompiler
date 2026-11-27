use rv_ir::{BlockId, IrFunction};
use std::collections::{BTreeMap, BTreeSet};

/// Structured WebAssembly control-flow tree recovered from an `IrFunction` CFG.
#[derive(Debug, Clone)]
pub enum StructuredNode {
    /// A Wasm `block` whose branch target is `target_bb` (immediately following this block).
    Block {
        target_bb: BlockId,
        body: Vec<StructuredNode>,
    },
    /// A Wasm `loop` whose branch target is `header_bb` (at the start of this loop).
    Loop {
        header_bb: BlockId,
        body: Vec<StructuredNode>,
    },
    /// Emit the instructions and terminator of `bb_id`.
    BasicBlock(BlockId),
}

#[derive(Debug, Clone)]
pub enum StructuredCfg {
    /// Reducible CFG lowered via Stackifier.
    Stackified {
        order: Vec<BlockId>,
        nodes: Vec<StructuredNode>,
    },
    /// Irreducible CFG fallback using a `$bb_id` state-machine dispatcher.
    Dispatcher { blocks: Vec<BlockId> },
}

/// Recover structured WebAssembly control flow from an `IrFunction`.
pub fn recover_structured_cfg(func: &IrFunction) -> StructuredCfg {
    let n = func.blocks.len();
    if n == 0 {
        return StructuredCfg::Stackified {
            order: Vec::new(),
            nodes: Vec::new(),
        };
    }

    // 1. Compute reachable blocks in RPO starting from bb0.
    let mut visited = vec![false; n];
    let mut postorder = Vec::with_capacity(n);
    dfs_postorder(func, BlockId(0), &mut visited, &mut postorder);
    postorder.reverse();
    let rpo = postorder;

    let mut rpo_pos = vec![usize::MAX; n];
    for (idx, &bb) in rpo.iter().enumerate() {
        rpo_pos[bb.0 as usize] = idx;
    }

    // Build predecessor lists for reachable blocks.
    let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); n];
    for &bb in &rpo {
        for succ in func.blocks[bb.0 as usize].term.successors() {
            if rpo_pos[succ.0 as usize] != usize::MAX {
                preds[succ.0 as usize].push(bb);
            }
        }
    }

    // 2. Cooper-Harvey-Kennedy dominator tree (in RPO indices).
    let mut idom: Vec<Option<usize>> = vec![None; rpo.len()];
    idom[0] = Some(0);
    let mut changed = true;
    while changed {
        changed = false;
        for b_idx in 1..rpo.len() {
            let bb = rpo[b_idx];
            let mut new_idom: Option<usize> = None;
            for &p in &preds[bb.0 as usize] {
                let p_idx = rpo_pos[p.0 as usize];
                if idom[p_idx].is_some() {
                    new_idom = Some(match new_idom {
                        None => p_idx,
                        Some(cur) => intersect(&idom, p_idx, cur),
                    });
                }
            }
            if new_idom != idom[b_idx] {
                idom[b_idx] = new_idom;
                changed = true;
            }
        }
    }

    let dominates = |a_bb: BlockId, b_bb: BlockId| -> bool {
        let a_idx = rpo_pos[a_bb.0 as usize];
        let mut cur = rpo_pos[b_bb.0 as usize];
        while cur > a_idx {
            match idom[cur] {
                Some(p) if p != cur => cur = p,
                _ => break,
            }
        }
        cur == a_idx
    };

    // 3. Check reducibility and collect natural loops.
    let mut loops: BTreeMap<BlockId, BTreeSet<BlockId>> = BTreeMap::new();
    for &u in &rpo {
        for v in func.blocks[u.0 as usize].term.successors() {
            if rpo_pos[v.0 as usize] <= rpo_pos[u.0 as usize] {
                if !dominates(v, u) {
                    // Irreducible multi-entry loop -> use state-machine dispatcher.
                    return StructuredCfg::Dispatcher { blocks: rpo };
                }
                let body = loops.entry(v).or_default();
                body.insert(v);
                if u != v {
                    let mut stack = vec![u];
                    while let Some(node) = stack.pop() {
                        if body.insert(node) {
                            for &p in &preds[node.0 as usize] {
                                stack.push(p);
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Verify no forward edge jumps into the middle of a loop (safety check).
    let order = rpo;
    let mut pos = vec![usize::MAX; n];
    for (i, &bb) in order.iter().enumerate() {
        pos[bb.0 as usize] = i;
    }

    let mut loop_ranges: Vec<(BlockId, usize, usize)> = Vec::new();
    for (header, body) in &loops {
        let start = pos[header.0 as usize];
        let end = body.iter().map(|b| pos[b.0 as usize]).max().unwrap_or(start);
        // Ensure loop body is contiguous or at least all non-header nodes are > start.
        for &b in body {
            if pos[b.0 as usize] < start {
                return StructuredCfg::Dispatcher { blocks: order };
            }
        }
        loop_ranges.push((*header, start, end));
    }

    // Check forward jumps don't cross into a loop body without targeting its header.
    for &u in &order {
        let u_pos = pos[u.0 as usize];
        for v in func.blocks[u.0 as usize].term.successors() {
            let v_pos = pos[v.0 as usize];
            if v_pos > u_pos {
                for &(_hdr, l_start, l_end) in &loop_ranges {
                    if u_pos < l_start && v_pos > l_start && v_pos <= l_end {
                        return StructuredCfg::Dispatcher { blocks: order };
                    }
                }
            }
        }
    }

    // 5. Build Stackifier tree.
    let nodes = build_stackified_tree(func, &order, &pos, &loop_ranges);
    StructuredCfg::Stackified { order, nodes }
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

fn intersect(idom: &[Option<usize>], mut b1: usize, mut b2: usize) -> usize {
    while b1 != b2 {
        while b1 > b2 {
            b1 = idom[b1].unwrap_or(0);
        }
        while b2 > b1 {
            b2 = idom[b2].unwrap_or(0);
        }
    }
    b1
}

fn build_stackified_tree(
    func: &IrFunction,
    order: &[BlockId],
    pos: &[usize],
    loop_ranges: &[(BlockId, usize, usize)],
) -> Vec<StructuredNode> {
    // Collect forward branch targets needing `block` wrappers:
    // For each forward edge u -> v where pos[v] > pos[u] + 1, we need a Block ending at pos[v].
    let mut forward_targets_at_end: BTreeMap<usize, BTreeSet<BlockId>> = BTreeMap::new();
    for (u_pos, &u) in order.iter().enumerate() {
        for v in func.blocks[u.0 as usize].term.successors() {
            let v_pos = pos[v.0 as usize];
            if v_pos > u_pos + 1 {
                forward_targets_at_end.entry(v_pos).or_default().insert(v);
            }
        }
    }

    // Recursive range builder over order[start..end].
    build_range(order, pos, loop_ranges, &forward_targets_at_end, 0, order.len())
}

fn build_range(
    order: &[BlockId],
    pos: &[usize],
    loop_ranges: &[(BlockId, usize, usize)],
    forward_targets_at_end: &BTreeMap<usize, BTreeSet<BlockId>>,
    start: usize,
    end: usize,
) -> Vec<StructuredNode> {
    if start >= end {
        return Vec::new();
    }

    // If any forward target ends strictly inside (start, end) or at `end`,
    // wrap [start..target_pos] in Block { target_bb } so `br` can exit to target_pos.
    // Find the largest target_pos <= end that starts at `start` (or split).
    if let Some((&t_pos, targets)) = forward_targets_at_end.range((start + 1)..=end).next_back() {
        // Ensure wrapping [start..t_pos] doesn't split a loop that starts < t_pos and ends > t_pos.
        let splits_loop = loop_ranges
            .iter()
            .any(|&(_, l_s, l_e)| l_s >= start && l_s < t_pos && l_e >= t_pos);
        if !splits_loop && t_pos <= end {
            let target_bb = *targets.iter().next().expect("non-empty");
            // Remove this target so we don't infinitely recurse on it.
            let mut next_fwd = forward_targets_at_end.clone();
            if let Some(set) = next_fwd.get_mut(&t_pos) {
                set.remove(&target_bb);
                if set.is_empty() {
                    next_fwd.remove(&t_pos);
                }
            }
            let inner = build_range(order, pos, loop_ranges, &next_fwd, start, t_pos);
            let mut out = vec![StructuredNode::Block {
                target_bb,
                body: inner,
            }];
            out.extend(build_range(order, pos, loop_ranges, &next_fwd, t_pos, end));
            return out;
        }
    }

    // Otherwise emit sequentially, opening Loop scopes at loop headers.
    let mut nodes = Vec::new();
    let mut i = start;
    while i < end {
        if let Some(&(hdr, l_s, l_e)) = loop_ranges
            .iter()
            .find(|&&( _h, l_s, l_e)| l_s == i && l_e < end)
        {
            let inner_ranges: Vec<_> = loop_ranges
                .iter()
                .copied()
                .filter(|&(h2, s2, e2)| h2 != hdr && s2 >= l_s && e2 <= l_e)
                .collect();
            let body = build_range(order, pos, &inner_ranges, forward_targets_at_end, l_s, l_e + 1);
            nodes.push(StructuredNode::Loop {
                header_bb: hdr,
                body,
            });
            i = l_e + 1;
        } else {
            nodes.push(StructuredNode::BasicBlock(order[i]));
            i += 1;
        }
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use rv_ir::{IrBlock, IrTerm};
    use std::collections::BTreeMap;

    fn make_fn(terms: Vec<IrTerm>) -> IrFunction {
        let blocks = terms
            .into_iter()
            .enumerate()
            .map(|(i, term)| IrBlock {
                id: BlockId(i as u32),
                guest_pc: 0x10000 + (i as u32) * 4,
                phis: Vec::new(),
                insts: Vec::new(),
                term,
            })
            .collect();
        IrFunction {
            entry: 0x10000,
            name: "test".into(),
            is_indirect_target: false,
            blocks,
            pc_to_block: BTreeMap::new(),
            next_value: 0,
        }
    }

    #[test]
    fn stackifies_reducible_loop_and_diamond() {
        // bb0 -> br_if bb1, bb2; bb1 -> bb1 (loop) or bb3; bb2 -> bb3; bb3 -> trap
        let func = make_fn(vec![
            IrTerm::BrIf {
                cond: rv_ir::ValueId(0),
                then_bb: BlockId(1),
                else_bb: BlockId(2),
            },
            IrTerm::BrIf {
                cond: rv_ir::ValueId(0),
                then_bb: BlockId(1),
                else_bb: BlockId(3),
            },
            IrTerm::Jump { target: BlockId(3) },
            IrTerm::Trap,
        ]);
        match recover_structured_cfg(&func) {
            StructuredCfg::Stackified { order, .. } => assert_eq!(order.len(), 4),
            StructuredCfg::Dispatcher { .. } => panic!("expected reducible Stackified"),
        }
    }

    #[test]
    fn falls_back_to_dispatcher_for_irreducible_cfg() {
        // bb0 -> br_if bb1, bb2; bb1 -> bb2; bb2 -> br_if bb1, bb3; bb3 -> trap
        // bb1 and bb2 form a 2-entry loop!
        let func = make_fn(vec![
            IrTerm::BrIf {
                cond: rv_ir::ValueId(0),
                then_bb: BlockId(1),
                else_bb: BlockId(2),
            },
            IrTerm::Jump { target: BlockId(2) },
            IrTerm::BrIf {
                cond: rv_ir::ValueId(0),
                then_bb: BlockId(1),
                else_bb: BlockId(3),
            },
            IrTerm::Trap,
        ]);
        match recover_structured_cfg(&func) {
            StructuredCfg::Dispatcher { blocks } => assert_eq!(blocks.len(), 4),
            StructuredCfg::Stackified { .. } => panic!("expected Dispatcher fallback"),
        }
    }
}
