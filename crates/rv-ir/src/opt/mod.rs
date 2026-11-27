pub mod branch_fuse;
pub mod const_fold;
pub mod dce;
pub mod memarg_fold;
pub mod reg_promote;
pub mod zero_reg;

use crate::ir::{IrFunction, IrModule, OptConfig};

/// Run enabled optimization passes on an entire `IrModule`.
pub fn optimize_module(module: &mut IrModule, opt: &OptConfig) {
    for func in module.functions.values_mut() {
        optimize_function(func, opt);
    }
}

/// Run enabled optimization passes on a single `IrFunction`.
pub fn optimize_function(func: &mut IrFunction, opt: &OptConfig) {
    if opt.zero_reg_elim {
        zero_reg::run(func);
    }
    if opt.register_promotion {
        reg_promote::run(func);
    }
    if opt.constant_folding {
        const_fold::run(func);
    }
    if opt.memarg_folding {
        memarg_fold::run(func);
    }
    if opt.branch_fusion {
        branch_fuse::run(func);
    }
    if opt.dce {
        dce::run(func);
    }
}
