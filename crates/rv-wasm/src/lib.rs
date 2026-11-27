//! Structured CFG recovery, tail-call dispatch table, and Wasm emission.

pub mod cfg_struct;
pub mod codegen;
pub mod dispatch;
#[cfg(feature = "native-runtime")]
pub mod runtime;

pub use cfg_struct::{StructuredCfg, StructuredNode, recover_structured_cfg};
pub use codegen::{CodegenError, CompiledWasm, browser_wasm_features, compile_to_wasm, validate_wasm};
pub use dispatch::DispatchTable;
