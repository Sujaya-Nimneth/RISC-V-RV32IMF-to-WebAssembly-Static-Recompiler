//! Structured CFG recovery, tail-call dispatch table, and Wasm emission.

pub mod cfg_struct;

pub use cfg_struct::{StructuredCfg, StructuredNode, recover_structured_cfg};
