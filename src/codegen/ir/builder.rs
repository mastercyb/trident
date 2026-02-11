//! IRBuilder: lowers a type-checked AST into `Vec<IROp>`.
//!
//! This module will be implemented in Phase 2.

#![allow(dead_code)]

use super::IROp;

/// Builds IR from a type-checked AST.
pub struct IRBuilder {
    ops: Vec<IROp>,
    label_counter: u32,
}

impl IRBuilder {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            label_counter: 0,
        }
    }
}
