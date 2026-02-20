//! Neural compiler v2: GNN encoder + Transformer decoder.
//!
//! Replaces the v1 MLP evolutionary model with a ~13M parameter
//! architecture trained via supervised learning + GFlowNets.

pub mod data;
pub mod inference;
pub mod model;
