//! Codegen IR — re-exports from the standalone `crate::ir` module.
//!
//! The canonical IR definitions live in `src/ir/`. This module provides
//! backward-compatible paths for existing code.

pub mod builder;

// Re-export everything from the canonical ir module.
pub use crate::ir::lower;
pub use crate::ir::lower::{create_lowering, Lowering};
pub use crate::ir::IROp;
