use rv_ir::IrModule;
use std::collections::BTreeMap;

/// Dense indirect-call dispatch table mapping valid guest PCs to Wasm table indices.
#[derive(Debug, Clone)]
pub struct DispatchTable {
    /// Sorted list of `(guest_pc, wasm_func_index)` entries.
    pub entries: Vec<(u32, u32)>,
    /// Map from `guest_pc` to table slot index `0..entries.len()`.
    pub pc_to_slot: BTreeMap<u32, u32>,
    /// Table slot index for `$trap_invalid_pc` fallback (`entries.len()`).
    pub trap_slot: u32,
}

impl DispatchTable {
    /// Build a dispatch table for all functions in `module` that are valid indirect targets
    /// (plus the entry point). `func_index_map` maps guest function entry PC -> Wasm function index.
    pub fn build(module: &IrModule, func_index_map: &BTreeMap<u32, u32>) -> Self {
        let mut entries = Vec::new();
        let mut pc_to_slot = BTreeMap::new();

        for (&pc, &wasm_fn_idx) in func_index_map {
            let is_target = pc == module.entry
                || module.indirect_targets.contains(&pc)
                || module
                    .functions
                    .get(&pc)
                    .map(|f| f.is_indirect_target)
                    .unwrap_or(false);
            if is_target {
                let slot = entries.len() as u32;
                entries.push((pc, wasm_fn_idx));
                pc_to_slot.insert(pc, slot);
            }
        }

        let trap_slot = entries.len() as u32;
        Self {
            entries,
            pc_to_slot,
            trap_slot,
        }
    }
}
