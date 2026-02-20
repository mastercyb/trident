//! TirGraph — graph representation of TIR for GNN encoding.
//!
//! Converts a flat `Vec<TIROp>` into a graph with typed edges:
//! - DataDep: producer→consumer via abstract stack simulation
//! - ControlFlow: sequential and branch edges
//! - MemOrder: conservative ordering between memory operations

use crate::ir::tir::TIROp;

// ─── Types ────────────────────────────────────────────────────────

/// Edge types in the TIR graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Data dependency: op A produces a value consumed by op B.
    DataDep,
    /// Control flow: sequential or branch edge.
    ControlFlow,
    /// Memory ordering: conservative ordering between memory ops.
    MemOrder,
}

/// Field type annotation for a TIR node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Base field element (Goldilocks).
    BFE,
    /// Extension field element (cubic extension).
    XFE,
    /// Unknown or not applicable.
    Unknown,
}

/// Opcode kind — mirrors TIROp variants without payloads.
/// Used for one-hot encoding in the GNN feature vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum OpKind {
    // Tier 0 — Structure (11)
    Call = 0,
    Return = 1,
    Halt = 2,
    IfElse = 3,
    IfOnly = 4,
    Loop = 5,
    FnStart = 6,
    FnEnd = 7,
    Entry = 8,
    Comment = 9,
    Asm = 10,
    // Tier 1 — Universal (31)
    Push = 11,
    Pop = 12,
    Dup = 13,
    Swap = 14,
    Add = 15,
    Sub = 16,
    Mul = 17,
    Neg = 18,
    Invert = 19,
    Eq = 20,
    Lt = 21,
    And = 22,
    Or = 23,
    Xor = 24,
    PopCount = 25,
    Split = 26,
    DivMod = 27,
    Shl = 28,
    Shr = 29,
    Log2 = 30,
    Pow = 31,
    ReadIo = 32,
    WriteIo = 33,
    ReadMem = 34,
    WriteMem = 35,
    Assert = 36,
    Hash = 37,
    Reveal = 38,
    Seal = 39,
    RamRead = 40,
    RamWrite = 41,
    // Tier 2 — Provable (7)
    Hint = 42,
    SpongeInit = 43,
    SpongeAbsorb = 44,
    SpongeSqueeze = 45,
    SpongeLoad = 46,
    MerkleStep = 47,
    MerkleLoad = 48,
    // Tier 3 — Recursion (5)
    ExtMul = 49,
    ExtInvert = 50,
    FoldExt = 51,
    FoldBase = 52,
    ProofBlock = 53,
}

pub const NUM_OP_KINDS: usize = 54;

impl OpKind {
    pub fn from_tir_op(op: &TIROp) -> Self {
        match op {
            TIROp::Call(_) => OpKind::Call,
            TIROp::Return => OpKind::Return,
            TIROp::Halt => OpKind::Halt,
            TIROp::IfElse { .. } => OpKind::IfElse,
            TIROp::IfOnly { .. } => OpKind::IfOnly,
            TIROp::Loop { .. } => OpKind::Loop,
            TIROp::FnStart(_) => OpKind::FnStart,
            TIROp::FnEnd => OpKind::FnEnd,
            TIROp::Entry(_) => OpKind::Entry,
            TIROp::Comment(_) => OpKind::Comment,
            TIROp::Asm { .. } => OpKind::Asm,
            TIROp::Push(_) => OpKind::Push,
            TIROp::Pop(_) => OpKind::Pop,
            TIROp::Dup(_) => OpKind::Dup,
            TIROp::Swap(_) => OpKind::Swap,
            TIROp::Add => OpKind::Add,
            TIROp::Sub => OpKind::Sub,
            TIROp::Mul => OpKind::Mul,
            TIROp::Neg => OpKind::Neg,
            TIROp::Invert => OpKind::Invert,
            TIROp::Eq => OpKind::Eq,
            TIROp::Lt => OpKind::Lt,
            TIROp::And => OpKind::And,
            TIROp::Or => OpKind::Or,
            TIROp::Xor => OpKind::Xor,
            TIROp::PopCount => OpKind::PopCount,
            TIROp::Split => OpKind::Split,
            TIROp::DivMod => OpKind::DivMod,
            TIROp::Shl => OpKind::Shl,
            TIROp::Shr => OpKind::Shr,
            TIROp::Log2 => OpKind::Log2,
            TIROp::Pow => OpKind::Pow,
            TIROp::ReadIo(_) => OpKind::ReadIo,
            TIROp::WriteIo(_) => OpKind::WriteIo,
            TIROp::ReadMem(_) => OpKind::ReadMem,
            TIROp::WriteMem(_) => OpKind::WriteMem,
            TIROp::Assert(_) => OpKind::Assert,
            TIROp::Hash { .. } => OpKind::Hash,
            TIROp::Reveal { .. } => OpKind::Reveal,
            TIROp::Seal { .. } => OpKind::Seal,
            TIROp::RamRead { .. } => OpKind::RamRead,
            TIROp::RamWrite { .. } => OpKind::RamWrite,
            TIROp::Hint(_) => OpKind::Hint,
            TIROp::SpongeInit => OpKind::SpongeInit,
            TIROp::SpongeAbsorb => OpKind::SpongeAbsorb,
            TIROp::SpongeSqueeze => OpKind::SpongeSqueeze,
            TIROp::SpongeLoad => OpKind::SpongeLoad,
            TIROp::MerkleStep => OpKind::MerkleStep,
            TIROp::MerkleLoad => OpKind::MerkleLoad,
            TIROp::ExtMul => OpKind::ExtMul,
            TIROp::ExtInvert => OpKind::ExtInvert,
            TIROp::FoldExt => OpKind::FoldExt,
            TIROp::FoldBase => OpKind::FoldBase,
            TIROp::ProofBlock { .. } => OpKind::ProofBlock,
        }
    }
}

/// A node in the TIR graph.
#[derive(Debug, Clone)]
pub struct TirNode {
    pub op: OpKind,
    pub field_type: FieldType,
    pub immediate: Option<u64>,
}

/// Graph representation of TIR operations.
#[derive(Debug, Clone)]
pub struct TirGraph {
    pub nodes: Vec<TirNode>,
    pub edges: Vec<(usize, usize, EdgeKind)>,
}

// ─── Feature Vector ───────────────────────────────────────────────

/// Node feature vector dimensions:
/// - op_onehot: 54 (NUM_OP_KINDS)
/// - field_type_onehot: 3 (BFE, XFE, Unknown)
/// - has_immediate: 1
/// - immediate_normalized: 1
/// Total: 59
pub const NODE_FEATURE_DIM: usize = NUM_OP_KINDS + 3 + 1 + 1;

impl TirNode {
    /// Encode this node as a 59-dimensional feature vector.
    pub fn feature_vector(&self) -> [f32; NODE_FEATURE_DIM] {
        let mut v = [0.0f32; NODE_FEATURE_DIM];

        // One-hot op kind (54 dims)
        v[self.op as usize] = 1.0;

        // One-hot field type (3 dims, offset 54)
        let ft_offset = NUM_OP_KINDS;
        match self.field_type {
            FieldType::BFE => v[ft_offset] = 1.0,
            FieldType::XFE => v[ft_offset + 1] = 1.0,
            FieldType::Unknown => v[ft_offset + 2] = 1.0,
        }

        // Has immediate (1 dim, offset 57)
        if self.immediate.is_some() {
            v[ft_offset + 3] = 1.0;
        }

        // Normalized immediate (1 dim, offset 58)
        // Normalize to [0, 1] using log1p for large values
        if let Some(imm) = self.immediate {
            v[ft_offset + 4] = (imm as f64 + 1.0).ln() as f32 / 44.4; // ln(2^64) ≈ 44.4
        }

        v
    }
}

// ─── Stack Effects ────────────────────────────────────────────────

/// Determine the field type of a TIROp's output.
fn output_field_type(op: &TIROp) -> FieldType {
    match op {
        TIROp::ExtMul | TIROp::ExtInvert => FieldType::XFE,
        TIROp::FoldExt => FieldType::XFE,
        TIROp::SpongeSqueeze => FieldType::BFE,
        TIROp::Hash { .. } => FieldType::BFE,
        TIROp::Add
        | TIROp::Sub
        | TIROp::Mul
        | TIROp::Neg
        | TIROp::Invert
        | TIROp::Eq
        | TIROp::Lt
        | TIROp::And
        | TIROp::Or
        | TIROp::Xor
        | TIROp::DivMod
        | TIROp::Split
        | TIROp::Shl
        | TIROp::Shr
        | TIROp::Log2
        | TIROp::Pow
        | TIROp::PopCount
        | TIROp::Push(_) => FieldType::BFE,
        _ => FieldType::Unknown,
    }
}

// ─── Graph Construction ───────────────────────────────────────────

impl TirGraph {
    /// Build a TirGraph from a flat sequence of TIR operations.
    ///
    /// Flattens structural ops (IfElse bodies, Loop bodies) into
    /// a single node list, adding appropriate control flow edges.
    pub fn from_tir_ops(ops: &[TIROp]) -> Self {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Flatten ops into nodes, recursing into structural bodies
        flatten_ops(ops, &mut nodes, &mut edges);

        // Extract DataDep edges via abstract stack simulation
        extract_data_deps(&nodes, &mut edges);

        // Extract MemOrder edges (conservative pairwise ordering)
        extract_mem_order(&nodes, &mut edges);

        TirGraph { nodes, edges }
    }

    /// Number of nodes.
    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges.
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Count edges of a specific kind.
    pub fn count_edges(&self, kind: EdgeKind) -> usize {
        self.edges.iter().filter(|(_, _, k)| *k == kind).count()
    }
}

/// Flatten TIR ops into graph nodes, handling structural ops recursively.
/// Adds ControlFlow edges between sequential ops and into/out-of bodies.
fn flatten_ops(ops: &[TIROp], nodes: &mut Vec<TirNode>, edges: &mut Vec<(usize, usize, EdgeKind)>) {
    let mut prev_idx: Option<usize> = None;

    for op in ops {
        let idx = nodes.len();

        // Determine immediate value (Q5: BFE only, XFE ops get None)
        let immediate = match op {
            TIROp::Push(v) => Some(*v),
            TIROp::Pop(n) | TIROp::Dup(n) | TIROp::Swap(n) => Some(*n as u64),
            TIROp::ReadIo(n)
            | TIROp::WriteIo(n)
            | TIROp::ReadMem(n)
            | TIROp::WriteMem(n)
            | TIROp::Assert(n)
            | TIROp::Hint(n) => Some(*n as u64),
            TIROp::Hash { width } | TIROp::RamRead { width } | TIROp::RamWrite { width } => {
                Some(*width as u64)
            }
            TIROp::Reveal { field_count, .. } | TIROp::Seal { field_count, .. } => {
                Some(*field_count as u64)
            }
            TIROp::Asm { effect, .. } => Some(*effect as u64),
            // XFE ops: has_immediate=0 per Q5 resolution
            TIROp::ExtMul | TIROp::ExtInvert => None,
            _ => None,
        };

        let node = TirNode {
            op: OpKind::from_tir_op(op),
            field_type: output_field_type(op),
            immediate,
        };
        nodes.push(node);

        // Sequential ControlFlow edge
        if let Some(p) = prev_idx {
            edges.push((p, idx, EdgeKind::ControlFlow));
        }

        // Recurse into structural bodies
        match op {
            TIROp::IfElse {
                then_body,
                else_body,
            } => {
                if !then_body.is_empty() {
                    let then_start = nodes.len();
                    flatten_ops(then_body, nodes, edges);
                    edges.push((idx, then_start, EdgeKind::ControlFlow));
                }
                if !else_body.is_empty() {
                    let else_start = nodes.len();
                    flatten_ops(else_body, nodes, edges);
                    edges.push((idx, else_start, EdgeKind::ControlFlow));
                }
            }
            TIROp::IfOnly { then_body } => {
                if !then_body.is_empty() {
                    let then_start = nodes.len();
                    flatten_ops(then_body, nodes, edges);
                    edges.push((idx, then_start, EdgeKind::ControlFlow));
                }
            }
            TIROp::Loop { body, .. } => {
                if !body.is_empty() {
                    let body_start = nodes.len();
                    flatten_ops(body, nodes, edges);
                    let body_end = nodes.len() - 1;
                    edges.push((idx, body_start, EdgeKind::ControlFlow));
                    // Back edge: loop body end → loop header
                    edges.push((body_end, idx, EdgeKind::ControlFlow));
                }
            }
            TIROp::ProofBlock { body, .. } => {
                if !body.is_empty() {
                    let body_start = nodes.len();
                    flatten_ops(body, nodes, edges);
                    edges.push((idx, body_start, EdgeKind::ControlFlow));
                }
            }
            _ => {}
        }

        prev_idx = Some(idx);
    }
}

/// Abstract stack entry: tracks which node produced this value.
#[derive(Clone, Copy)]
struct StackEntry {
    producer: usize,
}

/// Extract DataDep edges by simulating an abstract stack.
/// When op B pops a value produced by op A → edge (A→B, DataDep).
fn extract_data_deps(nodes: &[TirNode], edges: &mut Vec<(usize, usize, EdgeKind)>) {
    let mut stack: Vec<StackEntry> = Vec::new();

    for (idx, node) in nodes.iter().enumerate() {
        let (pops, pushes) = stack_effect_from_kind(node);

        // Pop: create DataDep edges from producers to this consumer
        let actual_pops = pops.min(stack.len());
        for _ in 0..actual_pops {
            if let Some(entry) = stack.pop() {
                edges.push((entry.producer, idx, EdgeKind::DataDep));
            }
        }

        // Handle Dup specially: reads from depth without consuming
        if node.op == OpKind::Dup {
            let depth = node.immediate.unwrap_or(0) as usize;
            if depth < stack.len() {
                let producer = stack[stack.len() - 1 - depth].producer;
                edges.push((producer, idx, EdgeKind::DataDep));
            }
        }

        // Handle Swap: creates read-dependencies on both swapped positions
        if node.op == OpKind::Swap {
            let depth = node.immediate.unwrap_or(1) as usize;
            if depth < stack.len() && !stack.is_empty() {
                let top = stack.len() - 1;
                let other = stack.len() - 1 - depth;
                stack.swap(top, other);
            }
        }

        // Push: record this node as producer
        for _ in 0..pushes {
            stack.push(StackEntry { producer: idx });
        }
    }
}

/// Get stack effect from a TirNode (using OpKind + immediate).
fn stack_effect_from_kind(node: &TirNode) -> (usize, usize) {
    let n = node.immediate.unwrap_or(0) as usize;
    match node.op {
        OpKind::Push => (0, 1),
        OpKind::Pop => (n, 0),
        OpKind::Dup => (0, 1),
        OpKind::Swap => (0, 0),
        OpKind::Add | OpKind::Sub | OpKind::Mul => (2, 1),
        OpKind::Neg | OpKind::Invert => (1, 1),
        OpKind::Eq | OpKind::Lt => (2, 1),
        OpKind::And | OpKind::Or | OpKind::Xor => (2, 1),
        OpKind::PopCount | OpKind::Log2 => (1, 1),
        OpKind::Split => (1, 2),
        OpKind::DivMod => (2, 2),
        OpKind::Shl | OpKind::Shr | OpKind::Pow => (2, 1),
        OpKind::ReadIo => (0, n),
        OpKind::WriteIo => (n, 0),
        OpKind::ReadMem => (1, n + 1),
        OpKind::WriteMem => (n + 1, 1),
        OpKind::Assert => (n, 0),
        OpKind::Hash => (10, 5),
        OpKind::Reveal | OpKind::Seal => (n, 0),
        OpKind::RamRead => (1, n),
        OpKind::RamWrite => (n + 1, 0),
        OpKind::Hint => (0, n),
        OpKind::SpongeInit => (0, 0),
        OpKind::SpongeAbsorb => (10, 0),
        OpKind::SpongeSqueeze => (0, 10),
        OpKind::SpongeLoad => (1, 1),
        OpKind::MerkleStep | OpKind::MerkleLoad => (0, 0),
        OpKind::ExtMul => (6, 3),
        OpKind::ExtInvert => (3, 3),
        OpKind::FoldExt | OpKind::FoldBase => (0, 0),
        OpKind::IfElse | OpKind::IfOnly | OpKind::Loop => (1, 0),
        _ => (0, 0),
    }
}

/// Extract MemOrder edges: pairwise between all memory operations.
/// Conservative — preserves all possible orderings.
fn extract_mem_order(nodes: &[TirNode], edges: &mut Vec<(usize, usize, EdgeKind)>) {
    let mem_indices: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            matches!(
                n.op,
                OpKind::ReadMem
                    | OpKind::WriteMem
                    | OpKind::RamRead
                    | OpKind::RamWrite
                    | OpKind::SpongeLoad
                    | OpKind::MerkleLoad
            )
        })
        .map(|(i, _)| i)
        .collect();

    // Pairwise edges between consecutive memory ops (not O(n²) — sequential ordering)
    for window in mem_indices.windows(2) {
        edges.push((window[0], window[1], EdgeKind::MemOrder));
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_arithmetic_graph() {
        // push 3; push 4; add → 3 nodes
        let ops = vec![TIROp::Push(3), TIROp::Push(4), TIROp::Add];
        let graph = TirGraph::from_tir_ops(&ops);

        assert_eq!(graph.num_nodes(), 3);
        // ControlFlow: push3→push4, push4→add = 2
        assert_eq!(graph.count_edges(EdgeKind::ControlFlow), 2);
        // DataDep: push3→add, push4→add = 2
        assert_eq!(graph.count_edges(EdgeKind::DataDep), 2);
        assert_eq!(graph.count_edges(EdgeKind::MemOrder), 0);
    }

    #[test]
    fn memory_ops_get_mem_order_edges() {
        let ops = vec![
            TIROp::Push(100),
            TIROp::ReadMem(1),
            TIROp::Push(200),
            TIROp::WriteMem(1),
        ];
        let graph = TirGraph::from_tir_ops(&ops);

        assert_eq!(graph.num_nodes(), 4);
        assert!(graph.count_edges(EdgeKind::MemOrder) >= 1);
    }

    #[test]
    fn if_else_creates_branch_edges() {
        let ops = vec![
            TIROp::Push(1), // condition
            TIROp::IfElse {
                then_body: vec![TIROp::Push(10)],
                else_body: vec![TIROp::Push(20)],
            },
        ];
        let graph = TirGraph::from_tir_ops(&ops);

        // 4 nodes: Push(1), IfElse, Push(10), Push(20)
        assert_eq!(graph.num_nodes(), 4);
        // ControlFlow edges include branch edges to both bodies
        let cf = graph.count_edges(EdgeKind::ControlFlow);
        assert!(cf >= 3, "expected ≥3 CF edges, got {}", cf);
    }

    #[test]
    fn loop_creates_back_edge() {
        let ops = vec![
            TIROp::Push(5),
            TIROp::Loop {
                label: "l".into(),
                body: vec![TIROp::Push(1), TIROp::Sub],
            },
        ];
        let graph = TirGraph::from_tir_ops(&ops);

        // 4 nodes: Push(5), Loop, Push(1), Sub
        assert_eq!(graph.num_nodes(), 4);
        // Should have a back edge from Sub → Loop
        let has_back_edge = graph
            .edges
            .iter()
            .any(|(from, to, kind)| *kind == EdgeKind::ControlFlow && *from == 3 && *to == 1);
        assert!(has_back_edge, "missing loop back edge");
    }

    #[test]
    fn feature_vector_dimensions() {
        let node = TirNode {
            op: OpKind::Add,
            field_type: FieldType::BFE,
            immediate: None,
        };
        let fv = node.feature_vector();
        assert_eq!(fv.len(), NODE_FEATURE_DIM);
        assert_eq!(fv.len(), 59);
        // Add is index 15
        assert_eq!(fv[15], 1.0);
        // BFE is index 54
        assert_eq!(fv[54], 1.0);
        // No immediate
        assert_eq!(fv[57], 0.0);
    }

    #[test]
    fn feature_vector_with_immediate() {
        let node = TirNode {
            op: OpKind::Push,
            field_type: FieldType::BFE,
            immediate: Some(42),
        };
        let fv = node.feature_vector();
        assert_eq!(fv[11], 1.0); // Push is index 11
        assert_eq!(fv[57], 1.0); // has_immediate = 1
        assert!(fv[58] > 0.0); // normalized immediate > 0
    }

    #[test]
    fn empty_ops_produces_empty_graph() {
        let graph = TirGraph::from_tir_ops(&[]);
        assert_eq!(graph.num_nodes(), 0);
        assert_eq!(graph.num_edges(), 0);
    }

    #[test]
    fn all_54_op_kinds_are_numbered() {
        assert_eq!(OpKind::Call as u8, 0);
        assert_eq!(OpKind::ProofBlock as u8, 53);
        assert_eq!(NUM_OP_KINDS, 54);
    }

    #[test]
    fn dup_creates_data_dep_without_consuming() {
        // push 7; dup 0 → dup reads from push without consuming
        let ops = vec![TIROp::Push(7), TIROp::Dup(0)];
        let graph = TirGraph::from_tir_ops(&ops);

        assert_eq!(graph.num_nodes(), 2);
        // DataDep: push→dup (read dependency)
        assert_eq!(graph.count_edges(EdgeKind::DataDep), 1);
    }
}
