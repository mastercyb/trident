# Trinity Benchmark: Provable Private Neural Inference

## Context

Current benchmarks test each domain (neural, FHE, quantum) in isolation.
No integration case exists. We need a single `.tri` program that uses all
three domains together — something no other system on earth can do:
**a STARK-provable computation that performs FHE polynomial arithmetic,
neural network activation, and quantum circuit verification in one trace.**

This is not a synthetic checkbox exercise. It's the academic reference case
that positions Trident uniquely: TFHE can't prove, Cairo can't encrypt,
Qiskit can't do either.

## The Case: Quantum-Committed Private Neural Inference

Three phases, one program:

1. **Privacy** — linear layer via polynomial pointwise-mul + eval (FHE-style)
2. **Neural** — ReLU activation + bias (standard dense layer post-processing)
3. **Quantum** — Deutsch-style oracle commitment to the classification result

Story: encrypted input (polynomial) goes through a neural layer, the output
class is committed via a quantum circuit. The STARK proof covers everything.

Dimensions kept small for provability: 4-element vectors, degree-4 polynomials,
1-qubit circuit. ~500-1000 instructions total.

## Language Constraints (verified)

- No `!=` operator — use `if class { ... }` (Field as condition: 0=false, nonzero=true)
- No `>` operator — use `convert.split()` + `<` on U32 for ordering
- Cross-module imports from 3 std.* subdomains: works (no limit, verified in resolver)
- Struct types across modules (`gates.Qubit`, `gates.Complex`): works via short aliases
- `if` accepts both `Bool` and `Field` conditions

## Files to Create

### 1. `std/trinity/inference.tri` — the module

```
module std.trinity.inference

use vm.core.field
use vm.core.convert
use vm.io.mem
use std.nn.tensor
use std.private.poly
use std.quantum.gates
```

Public functions:

- `private_neuron(input_addr, weight_addr, out_addr, x, n) -> Field`
  — pointwise_mul + poly eval = one encrypted neuron output
- `private_linear(input_addr, w0..w3_addr, result_addr, tmp_addr, x, n)`
  — 4 neurons = full linear layer
- `activate(result_addr, bias_addr, out_addr)`
  — bias_add + relu_layer on 4 elements
- `quantum_commit(class: Field) -> Bool`
  — init |0>, H, conditional Z (using `if class { pauliz }` not `!=`), H, measure
- `trinity(input_addr, w0..w3_addr, bias_addr, result_addr, activated_addr, tmp_addr, x, n, expected_class) -> Bool`
  — full pipeline: private_linear → activate → quantum_commit

Note: argmax is done in the Rust reference and passed as `expected_class`.
This avoids the Field ordering problem and makes the proof structure cleaner:
the prover must supply the correct class for the quantum commit to verify.

### 2. `benches/std/trinity/inference.baseline.tasm` — hand baseline

Hand-optimized TASM from first principles for each public function.
Labels: `__private_neuron:`, `__private_linear:`, `__activate:`,
`__quantum_commit:`, `__trinity:`.

Target: ~70-90 total instructions.

### 3. `benches/std/trinity/inference.reference.rs` — Rust ground truth

Implements same computation in Rust using `trident::field::{Goldilocks, PrimeField}`.
Computes argmax internally, calls `trinity()` with the result.
Outputs `rust_ns: <N>`.

### 4. `Cargo.toml` — register the example

```toml
[[example]]
name = "ref_std_trinity_inference"
path = "benches/std/trinity/inference.reference.rs"
```

## Implementation Order

1. Create `std/trinity/inference.tri` — the core module
2. Verify it compiles: `trident build std/trinity/inference.tri`
3. Create `benches/std/trinity/inference.reference.rs` + register in Cargo.toml
4. Create `benches/std/trinity/inference.baseline.tasm`
5. Run `trident bench` — Trinity appears in the scoreboard
6. `cargo test` — no regressions
7. Commit

## Verification

- `cargo check` — zero warnings
- `cargo test` — 930+ tests pass
- `trident bench` — Trinity row in table with Tri/Hand/Ratio columns
- `trident bench --full` (if trisha available) — exec/prove/verify timings
- Rust reference produces `rust_ns:` output: `cargo run --example ref_std_trinity_inference`
