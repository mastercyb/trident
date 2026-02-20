//! Graph neural network operations for burn.
//!
//! Provides scatter-based message passing primitives that burn
//! doesn't have natively. These ops enable GATv2 attention
//! aggregation over graph neighborhoods.

use burn::prelude::*;
use burn::tensor::IndexingUpdateOp;

/// Scatter-add: aggregate source features by destination index.
///
/// For each edge (src, dst), adds `src_features[edge]` into
/// `output[dst_indices[edge]]`. Used for GNN message aggregation.
///
/// - `src_features`: [num_edges, d] — per-edge feature vectors
/// - `dst_indices`: [num_edges] — destination node index per edge
/// - `num_nodes`: total number of nodes (output rows)
///
/// Returns: [num_nodes, d] — aggregated features per node
pub fn scatter_add<B: Backend>(
    src_features: Tensor<B, 2>,
    dst_indices: Tensor<B, 1, Int>,
    num_nodes: usize,
) -> Tensor<B, 2> {
    let device = src_features.device();
    let num_edges = dst_indices.dims()[0];
    let d = src_features.dims()[1];

    // Expand dst_indices [E] → [E, 1] → [E, d] for scatter
    let indices_2d: Tensor<B, 2, Int> = dst_indices.unsqueeze_dim::<2>(1).expand([num_edges, d]);

    let output = Tensor::<B, 2>::zeros([num_nodes, d], &device);
    output.scatter(0, indices_2d, src_features, IndexingUpdateOp::Add)
}

/// Neighborhood softmax: softmax of edge scores grouped by destination node.
///
/// For attention: each destination node's incoming edge scores should
/// sum to 1.0 after softmax. This implements the grouped softmax.
///
/// - `edge_scores`: [num_edges, 1] — raw attention logits
/// - `dst_indices`: [num_edges] — destination node per edge
/// - `num_nodes`: total number of nodes
pub fn neighborhood_softmax<B: Backend>(
    edge_scores: Tensor<B, 2>,
    dst_indices: Tensor<B, 1, Int>,
    num_nodes: usize,
) -> Tensor<B, 2> {
    let device = edge_scores.device();
    let num_edges = edge_scores.dims()[0];

    let indices_2d: Tensor<B, 2, Int> = dst_indices
        .clone()
        .unsqueeze_dim::<2>(1)
        .expand([num_edges, 1]);

    // Clamp scores to prevent overflow in exp (replaces max subtraction)
    let clamped = edge_scores.clamp(-20.0, 20.0);
    let exp_scores = clamped.exp();

    // Sum exp per node via scatter-add
    let zeros = Tensor::<B, 2>::zeros([num_nodes, 1], &device);
    let node_sum = zeros.scatter(0, indices_2d, exp_scores.clone(), IndexingUpdateOp::Add);

    // Gather sum back to edges and normalize
    let edge_sum = node_sum.select(0, dst_indices);
    exp_scores / (edge_sum + 1e-10)
}

/// Batched graph for packing multiple graphs into one large disconnected graph.
pub struct BatchedGraph<B: Backend> {
    pub node_features: Tensor<B, 2>,
    pub src_indices: Tensor<B, 1, Int>,
    pub dst_indices: Tensor<B, 1, Int>,
    pub edge_types: Tensor<B, 1, Int>,
    pub graph_ids: Tensor<B, 1, Int>,
    pub num_nodes: usize,
    pub num_graphs: usize,
}

/// Batch multiple graphs into a single large disconnected graph.
pub fn batch_graphs<B: Backend>(
    node_features_list: &[Tensor<B, 2>],
    src_indices_list: &[Tensor<B, 1, Int>],
    dst_indices_list: &[Tensor<B, 1, Int>],
    edge_types_list: &[Tensor<B, 1, Int>],
    device: &B::Device,
) -> BatchedGraph<B> {
    let num_graphs = node_features_list.len();
    let mut total_nodes = 0usize;

    let mut all_features = Vec::new();
    let mut all_src = Vec::new();
    let mut all_dst = Vec::new();
    let mut all_edge_types = Vec::new();
    let mut all_graph_ids = Vec::new();

    for i in 0..num_graphs {
        let n = node_features_list[i].dims()[0];
        let offset = total_nodes as i64;

        all_features.push(node_features_list[i].clone());

        let offset_tensor =
            Tensor::<B, 1, Int>::full([src_indices_list[i].dims()[0]], offset, device);
        all_src.push(src_indices_list[i].clone() + offset_tensor.clone());
        all_dst.push(dst_indices_list[i].clone() + offset_tensor);
        all_edge_types.push(edge_types_list[i].clone());

        let graph_id = Tensor::<B, 1, Int>::full([n], i as i64, device);
        all_graph_ids.push(graph_id);

        total_nodes += n;
    }

    BatchedGraph {
        node_features: Tensor::cat(all_features, 0),
        src_indices: Tensor::cat(all_src, 0),
        dst_indices: Tensor::cat(all_dst, 0),
        edge_types: Tensor::cat(all_edge_types, 0),
        graph_ids: Tensor::cat(all_graph_ids, 0),
        num_nodes: total_nodes,
        num_graphs,
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;

    type B = NdArray;

    #[test]
    fn scatter_add_basic() {
        let device = Default::default();
        let src = Tensor::<B, 2>::from_floats([[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]], &device);
        let dst = Tensor::<B, 1, Int>::from_ints([0, 0, 1], &device);

        let result = scatter_add(src, dst, 2);
        let data = result.to_data();
        assert_eq!(data.as_slice::<f32>().unwrap(), &[4.0, 6.0, 5.0, 6.0]);
    }

    #[test]
    fn neighborhood_softmax_sums_to_one() {
        let device = Default::default();
        let scores = Tensor::<B, 2>::from_floats([[1.0], [2.0], [3.0], [4.0]], &device);
        let dst = Tensor::<B, 1, Int>::from_ints([0, 0, 1, 1], &device);

        let result = neighborhood_softmax(scores, dst, 2);
        let data = result.to_data();
        let vals = data.as_slice::<f32>().unwrap();

        let sum_node0 = vals[0] + vals[1];
        assert!((sum_node0 - 1.0).abs() < 1e-5, "node 0 sum: {}", sum_node0);

        let sum_node1 = vals[2] + vals[3];
        assert!((sum_node1 - 1.0).abs() < 1e-5, "node 1 sum: {}", sum_node1);
    }
}
