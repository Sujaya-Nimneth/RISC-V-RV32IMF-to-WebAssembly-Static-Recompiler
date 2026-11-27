//! Structured CFG recovery, tail-call dispatch table, and Wasm emission.

pub mod cfg_struct;
pub mod codegen;
pub mod dispatch;
pub mod fp_helpers;
pub mod local_alloc;
#[cfg(feature = "native-runtime")]
pub mod runtime;

pub use cfg_struct::{StructuredCfg, StructuredNode, recover_structured_cfg};
pub use codegen::{
    CodegenError, CompiledWasm, browser_wasm_features, compile_to_wasm, compile_to_wasm_with_opts,
    validate_wasm,
};
pub use dispatch::DispatchTable;
