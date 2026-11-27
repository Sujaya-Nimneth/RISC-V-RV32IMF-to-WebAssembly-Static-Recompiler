use crate::elf::LoadedElf;
use crate::inst::{DecodeError, Instruction, Reg, decode_instruction};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CfgError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("missing instruction word at 0x{0:08x}")]
    MissingInstruction(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedInst {
    pub pc: u32,
    pub word: u32,
    pub inst: Instruction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockTerm {
    Fallthrough { next: u32 },
    Jump { target: u32 },
    Branch { target: u32, fallthrough: u32 },
    Call { target: u32, ret_pc: u32 },
    TailCall { target: u32 },
    Return,
    IndirectCall { ret_pc: u32 },
    IndirectJump,
    Trap,
}

impl BlockTerm {
    /// Intra-function successor block PCs.
    pub fn intra_successors(&self) -> Vec<u32> {
        match *self {
            BlockTerm::Fallthrough { next } => vec![next],
            BlockTerm::Jump { target } => vec![target],
            BlockTerm::Branch {
                target,
                fallthrough,
            } => vec![target, fallthrough],
            BlockTerm::Call { ret_pc, .. } | BlockTerm::IndirectCall { ret_pc } => vec![ret_pc],
            BlockTerm::TailCall { .. }
            | BlockTerm::Return
            | BlockTerm::IndirectJump
            | BlockTerm::Trap => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicBlock {
    pub start_pc: u32,
    pub end_pc: u32,
    pub insts: Vec<DecodedInst>,
    pub term: BlockTerm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCfg {
    pub entry: u32,
    pub name: String,
    pub blocks: BTreeMap<u32, BasicBlock>,
    pub is_indirect_target: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramCfg {
    pub entry: u32,
    pub functions: BTreeMap<u32, FunctionCfg>,
    pub indirect_targets: BTreeSet<u32>,
}

/// Discover functions and build basic-block CFGs via recursive-descent traversal.
pub fn discover_cfg(elf: &LoadedElf) -> Result<ProgramCfg, CfgError> {
    let mut fn_seeds: BTreeSet<u32> = BTreeSet::new();
    let mut indirect_targets: BTreeSet<u32> = BTreeSet::new();

    fn_seeds.insert(elf.entry);
    indirect_targets.insert(elf.entry);

    for sym in &elf.symbols {
        if elf.is_exec_addr(sym.addr) {
            fn_seeds.insert(sym.addr);
            indirect_targets.insert(sym.addr);
        }
    }

    // Scan .rodata / .data segments for aligned 32-bit code pointers (vtables, jump tables, fn ptrs).
    for seg in &elf.segments {
        if seg.is_exec {
            continue;
        }
        let mut off = 0usize;
        while off + 4 <= seg.data.len() {
            let word = u32::from_le_bytes([
                seg.data[off],
                seg.data[off + 1],
                seg.data[off + 2],
                seg.data[off + 3],
            ]);
            if elf.is_exec_addr(word) {
                indirect_targets.insert(word);
                fn_seeds.insert(word);
            }
            off += 4;
        }
    }

    let known_sym_addrs: BTreeSet<u32> = elf
        .symbols
        .iter()
        .filter(|s| elf.is_exec_addr(s.addr))
        .map(|s| s.addr)
        .collect();

    let mut functions = BTreeMap::new();
    let mut queue: VecDeque<u32> = fn_seeds.iter().copied().collect();
    let mut visited_fns: BTreeSet<u32> = BTreeSet::new();

    while let Some(fn_entry) = queue.pop_front() {
        if !visited_fns.insert(fn_entry) || !elf.is_exec_addr(fn_entry) {
            continue;
        }

        let name = elf
            .symbol_name(fn_entry)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("fn_{fn_entry:08x}"));

        let (blocks, new_fn_calls) = discover_function_blocks(
            elf,
            fn_entry,
            &known_sym_addrs,
            &mut indirect_targets,
        )?;

        for callee in new_fn_calls {
            if !visited_fns.contains(&callee) {
                queue.push_back(callee);
            }
        }

        functions.insert(
            fn_entry,
            FunctionCfg {
                entry: fn_entry,
                name,
                blocks,
                is_indirect_target: indirect_targets.contains(&fn_entry),
            },
        );
    }

    // Refresh indirect_target flags after all discoveries.
    for (addr, func) in &mut functions {
        func.is_indirect_target = indirect_targets.contains(addr);
    }

    Ok(ProgramCfg {
        entry: elf.entry,
        functions,
        indirect_targets,
    })
}

fn discover_function_blocks(
    elf: &LoadedElf,
    fn_entry: u32,
    known_sym_addrs: &BTreeSet<u32>,
    indirect_targets: &mut BTreeSet<u32>,
) -> Result<(BTreeMap<u32, BasicBlock>, Vec<u32>), CfgError> {
    let mut leaders: BTreeSet<u32> = BTreeSet::new();
    let mut worklist: VecDeque<u32> = VecDeque::new();
    let mut decoded: BTreeMap<u32, DecodedInst> = BTreeMap::new();
    let mut new_calls: Vec<u32> = Vec::new();
    // Track simple LUI/AUIPC + ADDI code address materializations for fn pointers.
    let mut upper_imm: [Option<u32>; 32];

    leaders.insert(fn_entry);
    worklist.push_back(fn_entry);

    while let Some(start) = worklist.pop_front() {
        let mut pc = start;
        upper_imm = [None; 32];

        loop {
            if decoded.contains_key(&pc) {
                break;
            }
            let Some(word) = elf.read_u32(pc) else {
                break;
            };
            if !elf.is_exec_addr(pc) {
                break;
            }
            let inst = decode_instruction(pc, word)?;
            decoded.insert(pc, DecodedInst { pc, word, inst });

            // Heuristic: detect LUI/AUIPC + ADDI materializing a code address.
            match inst {
                Instruction::Lui { rd, imm } if rd != Reg::ZERO => {
                    upper_imm[rd.index()] = Some(imm as u32);
                }
                Instruction::Auipc { rd, imm } if rd != Reg::ZERO => {
                    upper_imm[rd.index()] = Some(pc.wrapping_add(imm as u32));
                }
                Instruction::Addi { rd, rs1, imm } if rd != Reg::ZERO => {
                    if let Some(base) = upper_imm[rs1.index()] {
                        let addr = base.wrapping_add(imm as u32);
                        if elf.is_exec_addr(addr) {
                            indirect_targets.insert(addr);
                            new_calls.push(addr);
                        }
                    }
                    upper_imm[rd.index()] = None;
                }
                _ => {}
            }

            let next_pc = pc.wrapping_add(4);
            match inst {
                Instruction::Jal { rd, imm } => {
                    let target = pc.wrapping_add(imm as u32);
                    if rd == Reg::ZERO {
                        if target != fn_entry && known_sym_addrs.contains(&target) {
                            // Tail call to another function.
                            new_calls.push(target);
                        } else if elf.is_exec_addr(target) {
                            leaders.insert(target);
                            worklist.push_back(target);
                        }
                        break;
                    } else {
                        // Direct function call.
                        if elf.is_exec_addr(target) {
                            new_calls.push(target);
                        }
                        if elf.is_exec_addr(next_pc) {
                            leaders.insert(next_pc);
                            worklist.push_back(next_pc);
                        }
                        break;
                    }
                }
                Instruction::Jalr { rd, rs1, imm } => {
                    if rd == Reg::ZERO && rs1 == Reg::RA && imm == 0 {
                        // Normal return.
                        break;
                    } else if rd == Reg::ZERO {
                        // Indirect jump / tail call.
                        break;
                    } else {
                        // Indirect call.
                        if elf.is_exec_addr(next_pc) {
                            leaders.insert(next_pc);
                            worklist.push_back(next_pc);
                        }
                        break;
                    }
                }
                Instruction::Beq { imm, .. }
                | Instruction::Bne { imm, .. }
                | Instruction::Blt { imm, .. }
                | Instruction::Bge { imm, .. }
                | Instruction::Bltu { imm, .. }
                | Instruction::Bgeu { imm, .. } => {
                    let target = pc.wrapping_add(imm as u32);
                    if elf.is_exec_addr(target) {
                        leaders.insert(target);
                        worklist.push_back(target);
                    }
                    if elf.is_exec_addr(next_pc) {
                        leaders.insert(next_pc);
                        worklist.push_back(next_pc);
                    }
                    break;
                }
                Instruction::Ebreak => {
                    break;
                }
                _ => {
                    pc = next_pc;
                }
            }
        }
    }

    // Partition decoded instructions into basic blocks at leader boundaries.
    let leader_list: Vec<u32> = leaders.iter().copied().collect();
    let mut blocks = BTreeMap::new();

    for (idx, &b_start) in leader_list.iter().enumerate() {
        let next_leader = leader_list.get(idx + 1).copied().unwrap_or(u32::MAX);
        let mut insts = Vec::new();
        let mut cur = b_start;
        while cur < next_leader {
            let Some(di) = decoded.get(&cur) else {
                break;
            };
            insts.push(di.clone());
            let is_term = matches!(
                di.inst,
                Instruction::Jal { .. }
                    | Instruction::Jalr { .. }
                    | Instruction::Beq { .. }
                    | Instruction::Bne { .. }
                    | Instruction::Blt { .. }
                    | Instruction::Bge { .. }
                    | Instruction::Bltu { .. }
                    | Instruction::Bgeu { .. }
                    | Instruction::Ebreak
            );
            cur = cur.wrapping_add(4);
            if is_term {
                break;
            }
        }
        if insts.is_empty() {
            continue;
        }
        let end_pc = cur;
        let last = insts.last().expect("non-empty block");
        let term = match last.inst {
            Instruction::Jal { rd, imm } => {
                let target = last.pc.wrapping_add(imm as u32);
                if rd == Reg::ZERO {
                    if target != fn_entry && known_sym_addrs.contains(&target) {
                        BlockTerm::TailCall { target }
                    } else if blocks_or_leaders_has(&leaders, target) {
                        BlockTerm::Jump { target }
                    } else {
                        BlockTerm::Trap
                    }
                } else {
                    BlockTerm::Call {
                        target,
                        ret_pc: end_pc,
                    }
                }
            }
            Instruction::Jalr { rd, rs1, imm } => {
                if rd == Reg::ZERO && rs1 == Reg::RA && imm == 0 {
                    BlockTerm::Return
                } else if rd == Reg::ZERO {
                    BlockTerm::IndirectJump
                } else {
                    BlockTerm::IndirectCall { ret_pc: end_pc }
                }
            }
            Instruction::Beq { imm, .. }
            | Instruction::Bne { imm, .. }
            | Instruction::Blt { imm, .. }
            | Instruction::Bge { imm, .. }
            | Instruction::Bltu { imm, .. }
            | Instruction::Bgeu { imm, .. } => BlockTerm::Branch {
                target: last.pc.wrapping_add(imm as u32),
                fallthrough: end_pc,
            },
            Instruction::Ebreak => BlockTerm::Trap,
            _ => {
                if leaders.contains(&end_pc) {
                    BlockTerm::Fallthrough { next: end_pc }
                } else {
                    BlockTerm::Trap
                }
            }
        };

        blocks.insert(
            b_start,
            BasicBlock {
                start_pc: b_start,
                end_pc,
                insts,
                term,
            },
        );
    }

    Ok((blocks, new_calls))
}

#[inline]
fn blocks_or_leaders_has(leaders: &BTreeSet<u32>, addr: u32) -> bool {
    leaders.contains(&addr)
}
