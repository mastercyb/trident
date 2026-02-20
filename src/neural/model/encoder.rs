//! GNN Encoder — GATv2 (Graph Attention Network v2) in burn.
//!
//! Encodes a TirGraph into node embeddings + global context vector.
//! 3-4 GATv2 layers, d=256, ~3M parameters.
//!
//! CPU for single-graph inference, GPU for batched training.

use burn::config::Config;
use burn::module::Module;
use burn::nn::{Embedding, EmbeddingConfig, LayerNorm, LayerNormConfig, Linear, LinearConfig};
use burn::prelude::*;
use burn::tensor::activation::leaky_relu;

use super::gnn_ops::{neighborhood_softmax, scatter_add};
use crate::neural::data::tir_graph::NODE_FEATURE_DIM;

// ─── Configuration ────────────────────────────────────────────────

/// GATv2 layer configuration.
#[derive(Config, Debug)]
pub struct GatV2LayerConfig {
    /// Input feature dimension.
    pub d_in: usize,
    /// Output feature dimension.
    pub d_out: usize,
    /// Edge embedding dimension.
    #[config(default = 32)]
    pub d_edge: usize,
    /// Number of edge types.
    #[config(default = 3)]
    pub num_edge_types: usize,
    /// Negative slope for LeakyReLU.
    #[config(default = 0.2)]
    pub leaky_relu_alpha: f64,
}

/// GNN Encoder configuration.
#[derive(Config, Debug)]
pub struct GnnEncoderConfig {
    /// Model dimension (node embedding size).
    #[config(default = 256)]
    pub d_model: usize,
    /// Number of GATv2 layers.
    #[config(default = 4)]
    pub num_layers: usize,
    /// Edge embedding dimension.
    #[config(default = 32)]
    pub d_edge: usize,
}

// ─── GATv2 Layer ──────────────────────────────────────────────────

/// Single GATv2 attention layer.
///
/// Implements: a^T · LeakyReLU(W_src·h_i + W_dst·h_j + W_edge·e_ij)
/// with softmax per neighborhood, followed by FFN + residual + LayerNorm.
#[derive(Module, Debug)]
pub struct GatV2Layer<B: Backend> {
    /// Source node projection.
    w_src: Linear<B>,
    /// Destination node projection.
    w_dst: Linear<B>,
    /// Edge type projection.
    w_edge: Linear<B>,
    /// Attention scoring vector (projects concatenated features to scalar).
    attn: Linear<B>,
    /// Output FFN.
    ffn: Linear<B>,
    /// Layer normalization.
    norm: LayerNorm<B>,
    /// LeakyReLU negative slope.
    leaky_alpha: f64,
}

impl GatV2LayerConfig {
    /// Initialize a GATv2 layer.
    pub fn init<B: Backend>(&self, device: &B::Device) -> GatV2Layer<B> {
        GatV2Layer {
            w_src: LinearConfig::new(self.d_in, self.d_out).init(device),
            w_dst: LinearConfig::new(self.d_in, self.d_out).init(device),
            w_edge: LinearConfig::new(self.d_edge, self.d_out).init(device),
            attn: LinearConfig::new(self.d_out, 1).init(device),
            ffn: LinearConfig::new(self.d_out, self.d_out).init(device),
            norm: LayerNormConfig::new(self.d_out).init(device),
            leaky_alpha: self.leaky_relu_alpha,
        }
    }
}

impl<B: Backend> GatV2Layer<B> {
    /// Forward pass: GATv2 message passing.
    ///
    /// - `node_features`: [N, d_in] — node feature matrix
    /// - `src_indices`: [E] — source node index per edge
    /// - `dst_indices`: [E] — destination node index per edge
    /// - `edge_embeddings`: [E, d_edge] — edge type embeddings
    /// - `num_nodes`: N
    ///
    /// Returns: [N, d_out] — updated node features
    pub fn forward(
        &self,
        node_features: Tensor<B, 2>,
        src_indices: Tensor<B, 1, Int>,
        dst_indices: Tensor<B, 1, Int>,
        edge_embeddings: Tensor<B, 2>,
        num_nodes: usize,
    ) -> Tensor<B, 2> {
        let num_edges = src_indices.dims()[0];
        let d_out = self.ffn.weight.dims()[0];

        // Project source and destination features
        let h_src = self.w_src.forward(node_features.clone());
        let h_dst = self.w_dst.forward(node_features.clone());

        // Gather per-edge features
        let h_src_edge = h_src.select(0, src_indices.clone()); // [E, d_out]
        let h_dst_edge = h_dst.select(0, dst_indices.clone()); // [E, d_out]
        let e_proj = self.w_edge.forward(edge_embeddings); // [E, d_out]

        // GATv2 attention: a^T · LeakyReLU(h_src + h_dst + e)
        let combined = h_src_edge.clone() + h_dst_edge + e_proj;
        let activated = leaky_relu(combined, self.leaky_alpha);
        let attn_logits = self.attn.forward(activated); // [E, 1]

        // Neighborhood softmax
        let attn_weights = neighborhood_softmax(attn_logits, dst_indices.clone(), num_nodes);

        // Weighted message aggregation: broadcast [E, 1] to [E, d_out]
        let attn_expanded = attn_weights.expand([num_edges, d_out]);
        let messages = h_src_edge * attn_expanded;
        let aggregated = scatter_add(messages, dst_indices, num_nodes);

        // FFN + residual + norm
        let out = self.ffn.forward(aggregated);

        // Residual connection (only if dimensions match)
        let residual = if node_features.dims()[1] == d_out {
            out + node_features
        } else {
            out
        };

        self.norm.forward(residual)
    }
}

// ─── GNN Encoder ──────────────────────────────────────────────────

/// GNN Encoder: stack of GATv2 layers with global pooling.
///
/// Input: TirGraph node features + edge structure
/// Output: (node_embeddings [N, d], global_context [d])
#[derive(Module, Debug)]
pub struct GnnEncoder<B: Backend> {
    /// Initial node feature projection: NODE_FEATURE_DIM → d_model
    node_proj: Linear<B>,
    /// Edge type embedding: 3 types → d_edge
    edge_embed: Embedding<B>,
    /// Stack of GATv2 layers
    layers: Vec<GatV2Layer<B>>,
    /// Global pooling projection: 2*d_model → d_model (mean+max concatenated)
    global_proj: Linear<B>,
}

impl GnnEncoderConfig {
    /// Initialize a GNN encoder.
    pub fn init<B: Backend>(&self, device: &B::Device) -> GnnEncoder<B> {
        let mut layers = Vec::with_capacity(self.num_layers);

        for i in 0..self.num_layers {
            let d_in = if i == 0 { self.d_model } else { self.d_model };
            let config = GatV2LayerConfig {
                d_in,
                d_out: self.d_model,
                d_edge: self.d_edge,
                num_edge_types: 3,
                leaky_relu_alpha: 0.2,
            };
            layers.push(config.init(device));
        }

        GnnEncoder {
            node_proj: LinearConfig::new(NODE_FEATURE_DIM, self.d_model).init(device),
            edge_embed: EmbeddingConfig::new(3, self.d_edge).init(device),
            layers,
            global_proj: LinearConfig::new(self.d_model * 2, self.d_model).init(device),
        }
    }
}

impl<B: Backend> GnnEncoder<B> {
    /// Encode a graph into node embeddings and a global context vector.
    ///
    /// - `node_features`: [N, NODE_FEATURE_DIM] — raw node feature vectors
    /// - `src_indices`: [E] — source node index per edge
    /// - `dst_indices`: [E] — destination node index per edge
    /// - `edge_types`: [E] — edge type (0=DataDep, 1=ControlFlow, 2=MemOrder)
    ///
    /// Returns: (node_embeddings [N, d_model], global_context [d_model])
    pub fn forward(
        &self,
        node_features: Tensor<B, 2>,
        src_indices: Tensor<B, 1, Int>,
        dst_indices: Tensor<B, 1, Int>,
        edge_types: Tensor<B, 1, Int>,
    ) -> (Tensor<B, 2>, Tensor<B, 1>) {
        let num_nodes = node_features.dims()[0];

        // Project node features to model dimension
        let mut h = self.node_proj.forward(node_features);

        // Embed edge types: [E] → [E, 1] → Embedding → [E, 1, d_edge] → [E, d_edge]
        let edge_types_2d: Tensor<B, 2, Int> = edge_types.unsqueeze_dim::<2>(1);
        let edge_emb_3d = self.edge_embed.forward(edge_types_2d); // [E, 1, d_edge]
        let edge_emb: Tensor<B, 2> = edge_emb_3d.squeeze_dim::<2>(1); // [E, d_edge]

        // GATv2 layers
        for layer in &self.layers {
            h = layer.forward(
                h,
                src_indices.clone(),
                dst_indices.clone(),
                edge_emb.clone(),
                num_nodes,
            );
        }

        // Global pooling: mean + max → project to d_model
        let mean_pool: Tensor<B, 1> = h.clone().mean_dim(0).squeeze_dim::<1>(0);
        let max_pool: Tensor<B, 1> = h.clone().max_dim(0).squeeze_dim::<1>(0);
        let global_input = Tensor::cat(vec![mean_pool, max_pool], 0); // [2*d_model]
        let global: Tensor<B, 1> = self
            .global_proj
            .forward(global_input.unsqueeze_dim::<2>(0)) // [2*d] → [1, 2*d] → [1, d]
            .squeeze_dim::<1>(0); // [1, d] → [d]

        (h, global)
    }

    /// Count total parameters.
    pub fn num_params(&self) -> usize {
        // node_proj
        let mut total = NODE_FEATURE_DIM * self.global_proj.weight.dims()[0] / 2; // approximate

        // Each GATv2 layer
        for layer in &self.layers {
            let d = layer.ffn.weight.dims()[0];
            total += d * d * 3; // w_src, w_dst, ffn
            total += d; // attn
            total += d * layer.w_edge.weight.dims()[1]; // w_edge
        }

        total
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;

    type B = NdArray;

    #[test]
    fn gnn_encoder_forward_shape() {
        let device = Default::default();
        let config = GnnEncoderConfig {
            d_model: 32, // Small for test
            num_layers: 2,
            d_edge: 8,
        };
        let encoder = config.init::<B>(&device);

        // 5 nodes, 4 edges
        let features = Tensor::<B, 2>::zeros([5, NODE_FEATURE_DIM], &device);
        let src = Tensor::<B, 1, Int>::from_ints([0, 1, 2, 3], &device);
        let dst = Tensor::<B, 1, Int>::from_ints([1, 2, 3, 4], &device);
        let edge_types = Tensor::<B, 1, Int>::from_ints([0, 1, 1, 2], &device);

        let (node_emb, global): (Tensor<B, 2>, Tensor<B, 1>) =
            encoder.forward(features, src, dst, edge_types);

        assert_eq!(node_emb.dims(), [5, 32]);
        assert_eq!(global.dims(), [32]);
    }

    #[test]
    fn gatv2_layer_preserves_node_count() {
        let device = Default::default();
        let config = GatV2LayerConfig {
            d_in: 16,
            d_out: 16,
            d_edge: 8,
            num_edge_types: 3,
            leaky_relu_alpha: 0.2,
        };
        let layer = config.init::<B>(&device);

        let features = Tensor::<B, 2>::zeros([3, 16], &device);
        let src = Tensor::<B, 1, Int>::from_ints([0, 1], &device);
        let dst = Tensor::<B, 1, Int>::from_ints([1, 2], &device);
        let edge_emb = Tensor::<B, 2>::zeros([2, 8], &device);

        let output = layer.forward(features, src, dst, edge_emb, 3);
        assert_eq!(output.dims(), [3, 16]);
    }
}
