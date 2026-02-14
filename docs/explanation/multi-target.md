# 🌐 Multi-Target Compilation

*Write once. Prove anywhere.*

---

## 🔭 The Problem

The blockchain industry forces developers to choose: pick a chain, learn its language, rewrite everything when you need another chain. Solidity locks you to EVM. Rust+Anchor locks you to Solana. Rust+cosmwasm-std locks you to Cosmos. Cairo locks you to StarkNet. Move locks you to Aptos/Sui.

The business logic of blockchain programs — arithmetic, state transitions, access control checks, hash commitments — is the same everywhere. What differs is the execution environment: how you read storage, how you emit events, how you call other contracts, what bytecode the VM runs. Developers rewrite the same logic over and over, introducing bugs each time, paying for separate audits on each chain.

No existing language solves this because every existing blockchain language was designed for one VM and then (sometimes) awkwardly extended to another.

## 🌐 Why Trident

Trident was designed for provable computation on zero-knowledge virtual machines. This forced a set of language constraints that turned out to be exactly what universal blockchain deployment requires:

- Bounded loops. Every loop has a compile-time bound. No infinite execution, no gas-limit surprises. Required by ZK provers, but equally valuable on EVM (predictable gas), SVM (predictable compute units), and CosmWasm (predictable execution).

- No heap, no dynamic dispatch. All data has known size at compile time. No malloc, no vtables, no runtime type checks. This makes programs auditable, their cost predictable, and their compilation to any target straightforward.

- Fixed-width types. `Field`, `U32`, `Bool`, `Digest`, fixed-size arrays, structs. No dynamically-sized types. Every value's memory footprint is known at compile time.

- Field-native arithmetic. The core numeric type is a finite field element (Goldilocks: 2^64 - 2^32 + 1). This fits in a 64-bit integer, making it efficient on every platform — native on ZK VMs, trivial `u64` arithmetic with modular reduction on RISC-V and WASM, `addmod`/`mulmod` on EVM. This prime field structure also makes Trident programs [quantum-native](quantum.md) — the same algebraic foundation required for provability is the optimal substrate for prime-dimensional quantum computing.

- Compile-time cost analysis. The compiler tells you exactly what your program costs before you deploy it. Not an estimate — an exact row count per algebraic table (ZK targets) or instruction count (conventional targets).

These properties emerged from ZK requirements. The discovery is that they define a language ideal for safe, portable blockchain execution on any VM.

---

## 🏗️ Architecture: Three Levels

```text
┌─────────────────────────────────────────────────────────┐
│               Level 1: Execute Anywhere                  │
│  Field, U32, Bool, Digest, structs, bounded loops,      │
│  match, modules, hash(), storage, events                │
│  ─────────────────────────────────────────────────────── │
│  Targets: EVM, SVM, CosmWasm, Triton VM, Miden, Cairo   │
├─────────────────────────────────────────────────────────┤
│              Level 2: Prove Anywhere                     │
│  divine(), pub_read/pub_write, seal events,             │
│  Merkle authentication, sponge construction,            │
│  recursive proof verification, cost annotations         │
│  ─────────────────────────────────────────────────────── │
│  Targets: Triton VM, Miden, Cairo, SP1/RISC-V zkVMs     │
├─────────────────────────────────────────────────────────┤
│            Level 3: OS Access                            │
│  os.* (portable) + os.<os>.* (OS-specific)              │
│  ─────────────────────────────────────────────────────── │
│  os.neuron: identity, authorization                      │
│  os.signal: value transfer between neurons               │
│  os.token: pay, lock, update, mint, burn (PLUMB)         │
│  os.state: persistent storage                            │
│  os.<os>.*: OS-specific extensions                       │
└─────────────────────────────────────────────────────────┘
```

A `.tri` file that uses only Level 1 constructs is designed to compile to every target. Level 2 imports restrict to ZK targets. Level 3 imports lock to a specific platform. The compiler enforces this statically — no runtime check, no silent failure.

### What each level means in practice

Level 1 is the business logic. The math, the state transitions, the validation rules. This is where developers spend their time and where bugs live. It is designed to compile everywhere.

Level 2 adds cryptographic provability. Secret witness inputs, public I/O, sealed events, Merkle authentication. The same Level 1 logic now produces STARK proofs. Only available on ZK targets.

Level 3 is the OS layer. Two tiers: `os.*` is the portable runtime
(neuron identity, signals, tokens, state, time — designed for all 25
OSes), and `os.<os>.*` provides OS-specific extensions (PDAs on Solana,
UTXO authentication on Neptune, CPI on Sui). The compiler is designed to
lower `os.*` calls to OS-native mechanisms based on `--target`. Importing `os.<os>.*`
locks the program to that OS.

The key insight: `os.*` is thin. The entire blockchain design space reduces
to three primitives — neurons (actors), signals (transactions), tokens
(assets). The business logic — the part that's expensive to write, expensive
to audit, and expensive to get wrong — lives entirely in Level 1.

---

## 🔍 How It Works

### Direct bytecode generation

Trident does not generate Solidity, Vyper, Rust, or any intermediate source language. It generates target bytecode directly from its own TIR:

```text
Source (.tri)
    │
    ├── Lexer → Parser → AST
    ├── Type checker (+ level check)
    ├── TIRBuilder → Vec<TIROp>
    │
    └── StackLowering (per target)
         ├── TritonLowering  → TASM instructions
         ├── MidenLowering   → MASM instructions
         ├── EvmLowering     → EVM bytecode
         ├── WasmLowering    → WASM bytecode (CosmWasm, SVM)
         └── RiscVLowering   → RISC-V ELF (SP1, OpenVM)
```

This is how real compilers work. GCC doesn't generate C to target ARM — it generates ARM machine code. Trident doesn't generate Solidity to target EVM — it generates EVM bytecode. This gives the compiler full control over storage layout, calling conventions, and optimization.

The TIR (Trident Intermediate Representation) is already implemented. It's a list of stack operations with structural control flow — `IfElse`, `IfOnly`, `Loop` contain nested bodies that each backend lowers according to its own conventions. The same `Vec<TIROp>` currently produces Triton's deferred-subroutine pattern and Miden's inline `if.true/else/end` from identical input. Adding EVM, WASM, and RISC-V lowerings follows the same pattern.

### What a Level 1 program looks like

```trident
program token_vault

use os.state
use os.neuron

struct Vault {
    owner: Field,
    balance: Field,
    locked: Bool,
}

fn deposit(vault_id: Field, amount: Field) {
    let owner: Field = state.read(vault_id)
    let balance: Field = state.read(vault_id + 1)
    let locked: Field = state.read(vault_id + 2)
    assert_eq(locked, 0)
    state.write(vault_id + 1, balance + amount)
    reveal Deposit { vault_id: vault_id, amount: amount }
}

fn withdraw(vault_id: Field, amount: Field) {
    let caller: Digest = neuron.id()
    let owner: Field = state.read(vault_id)
    let balance: Field = state.read(vault_id + 1)
    // Subtraction wraps modulo p; the prover must supply valid witness
    let new_balance: Field = sub(balance, amount)
    state.write(vault_id + 1, new_balance)
    reveal Withdrawal { vault_id: vault_id, amount: amount }
}

event Deposit { vault_id: Field, amount: Field }
event Withdrawal { vault_id: Field, amount: Field }
```

This program uses `os.state` and `os.neuron` — the portable OS API. It
is designed to compile to EVM bytecode, WASM for CosmWasm, BPF for Solana,
TASM for Triton VM, and MASM for Miden. The developer writes it once. One
audit covers all deployments.

The compiler is designed to lower `os.state.read()` to `SLOAD` on EVM,
`deps.storage` on CosmWasm, account data on Solana, RAM with Merkle
authentication on Triton VM. `os.neuron.id()` becomes `msg.sender` on EVM,
`predecessor_account_id` on Near, `tx_context::sender` on Sui. Same source,
target-optimal execution. No adapters to write.

---

## 🌐 Level 1: Execute Anywhere

### Core types

| Type       | Width    | EVM             | CosmWasm/SVM    | ZK VMs          |
|------------|----------|-----------------|-----------------|-----------------|
| `Field`    | 1 word   | `uint64` + mod  | `u64` + mod     | native element  |
| `U32`      | 1 word   | `uint32`        | `u32`           | range-checked   |
| `Bool`     | 1 word   | `uint8` (0/1)   | `bool`          | 0 or 1          |
| `Digest`   | 5 words  | `uint64[5]`     | `[u64; 5]`      | 5 elements      |
| `[T; N]`   | N*w(T)   | packed storage  | `[T; N]`        | N*w(T) elements |
| struct     | sum(fi)  | packed slots    | Rust struct     | flattened        |

`Field` is the universal numeric type. Goldilocks (p = 2^64 - 2^32 + 1) fits in 64 bits with fast modular reduction. On EVM this means `addmod(a, b, p)` where `p` fits in a single `uint256` word — cheaper than native 256-bit arithmetic for many workloads. On WASM and RISC-V it's native 64-bit math with a single conditional subtraction for reduction.

### Abstract primitives

Level 1 provides abstract interfaces. The compiler maps them to target-native implementations:

`hash()` — cryptographic hash, target-optimal:
| Target    | Implementation      | Cost            |
|-----------|---------------------|-----------------|
| Triton VM | Tip5 permutation    | 1 cycle + 6 co  |
| Miden     | Rescue-Prime        | ~10 cycles      |
| EVM       | KECCAK256 opcode    | 30 gas + 6/word |
| CosmWasm  | SHA-256 (native)    | ~microseconds   |
| SVM       | SHA-256 syscall     | ~100 CUs        |

`os.state.read()` / `os.state.write()` — persistent state:
| Target    | Mapping                                          |
|-----------|--------------------------------------------------|
| Triton VM | RAM addresses + Merkle commitment                |
| Miden     | Memory + state tree                               |
| EVM       | SSTORE/SLOAD with slot derivation from key       |
| CosmWasm  | `deps.storage` with binary key encoding          |
| SVM       | Account data at computed offsets                  |

`reveal` — observable events:
| Target    | Mapping                                          |
|-----------|--------------------------------------------------|
| Triton VM | Public output                                    |
| EVM       | LOG opcodes with indexed topics                  |
| CosmWasm  | `Response::add_event`                            |
| SVM       | `msg!()` / program logs                          |

The developer writes `hash(data)`. The compiler is designed to emit `KECCAK256` on EVM, `hash` on Triton, `SHA-256` on SVM. Same program, target-optimal execution.

### Control flow

All Level 1 control flow is designed to compile to every target:

```trident
if condition { ... } else { ... }     // Structural: if.true/else/end on Miden,
                                      // JUMPI on EVM, skiz+call on Triton

for i in 0..n bounded 100 { ... }    // Counter loop with compile-time bound

match value {                         // Exhaustive pattern match
    0 => { ... }
    1 => { ... }
    _ => { ... }
}
```

Bounded loops guarantee termination on every target. No gas-limit runaways, no stuck transactions, no halting-problem surprises. The bound is enforced by the compiler, not the runtime.

---

## ⚡ Level 2: Prove Anywhere

Level 2 adds zero-knowledge capabilities. Programs compile only to ZK virtual machines but gain the ability to produce cryptographic proofs of correct execution.

`divine()` — secret witness input. The prover supplies data invisible to the verifier. No equivalent in conventional smart contracts.

`pub_read()` / `pub_write()` — public I/O for proof circuits. Define the claim the proof attests to.

`seal` — privacy-preserving events. Fields are hashed; only the commitment is visible.

Merkle authentication — divine-and-authenticate pattern for state proofs.

Sponge construction — incremental hashing for variable-length data.

Recursive proof verification — verify a STARK inside another STARK.

Cost annotations — exact proving cost before you deploy.

A Level 2 program is a Level 1 program with these additions. The business logic is identical — only the I/O and witness handling differ:

```trident
program private_transfer

use std.crypto.merkle

fn main() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let amount: Field = pub_read()

    // Same arithmetic as Level 1
    let sender_bal: Field = divine()
    let new_bal: Field = sub(sender_bal, amount)

    // Merkle proof (Level 2)
    let sender_leaf: Digest = divine5()
    let index: Field = divine()
    std.crypto.merkle.verify(old_root, sender_leaf, index, 20)

    seal Transfer { amount: amount }
}
```

The `sender_bal - amount` and `assert(new_bal >= 0)` are pure Level 1 logic. The `divine()`, `pub_read5()`, `merkle.verify()`, and `seal` are Level 2. If you remove the ZK parts, the business logic is designed to compile to every target.

---

## ⚙️ TargetConfig

Targets are defined as TOML files in `vm/<name>/target.toml`. Each declares architecture, field parameters, stack depth, hash function, and cost table names. The compiler loads them via `--target <name>`. See [Target Reference](../reference/targets.md) for the schema, shipped configurations, and how to add new targets.

---

## 🔧 Backend Traits

Each target implements two traits: `StackLowering` (mapping TIR operations to target assembly) and `CostModel` (providing per-instruction cost in target-native dimensions). Stack targets (Triton, Miden) share control-flow and stack-management logic. Register targets use `RegisterLowering` via LIR. Tree targets (Nock) use `TreeLowering`. See [IR Reference](../reference/ir.md) for the trait interfaces and [Target Reference](../reference/targets.md) for implemented backends.

---

## 📚 Standard Library Layers

Three layers enable portability:

- `std.core` -- Pure Trident, no VM dependencies. Compiles everywhere.
- `std.io` / `std.crypto` -- Same API on every target. The compiler dispatches to target-native instructions.
- `os.<os>.*` -- OS-specific extensions that lock to one target.

Programs using only `std.*` compile to any backend. `std/target.tri` exposes compile-time constants (`DIGEST_WIDTH`, `FIELD_LIMBS`, `HASH_RATE`) derived from the active target, enabling polymorphic code without `#[cfg]` guards. See [Standard Library Reference](../reference/stdlib.md) for the full module inventory.

---

## 🏷️ Target-Tagged Assembly

Inline assembly blocks are tagged with the target they belong to:

```trident
asm(triton) {
    dup 0
    add
    swap 5 pop 1
}
```

The parser recognizes the `asm(<target>) { ... }` syntax. When emitting code,
the compiler compares the tag against the active target name. Assembly blocks
tagged for a different target are silently skipped.

Bare `asm { ... }` blocks (no target tag) are also supported. They use the
declared stack effect annotation and emit for whatever target is active, passing
the body through as raw instructions.

### Multi-Target Programs

A single source file can contain assembly blocks for multiple targets. Only the
blocks matching the active `--target` are emitted:

```trident
fn fast_double(a: Field) -> Field {
    asm(triton) { dup 0 add }         // Emitted when --target triton
    asm(miden)  { dup.0 add }         // Emitted when --target miden
}
```

The `#[cfg(target)]` conditional compilation attribute works for larger blocks:

```trident
#[cfg(triton)]
use os.neptune.xfield

fn compute() -> Field {
    #[cfg(triton)]
    {
        // Use native extension field dot products
    }
    #[cfg(not(triton))]
    {
        // Portable fallback
    }
}
```

---

## ➕ Adding a New Target

To add support for a new stack-based zkVM:

### 1. Create the target TOML

Add `vm/<name>.toml` with the target's parameters:

```toml
[target]
name = "newvm"
display_name = "New VM"
architecture = "stack"
output_extension = ".nasm"

[field]
prime = "..."
limbs = 2

[stack]
depth = 16
spill_ram_base = 1073741824

[hash]
function = "..."
digest_width = 4
rate = 8

[extension_field]
degree = 0

[cost]
tables = ["cycles"]
```

### 2. Implement the Lowering Trait

For stack targets, add a new struct in `src/tir/lower/` that implements the
`StackLowering` trait. Every method maps a semantic operation (push, add, hash,
etc.) to the target's assembly syntax. For register targets, implement
`RegisterLowering` via the LIR path. See [targets.md](../reference/targets.md)
for which lowering path to use per VM family.

### 3. Implement CostModel

Add a cost model struct in `src/cost.rs` that implements the `CostModel` trait.
Provide per-instruction costs in the target's native cost dimensions. Register
it in `create_cost_model()`.

### 4. Add extension modules

If the target has unique capabilities (special types, native instructions, VM-
specific APIs), add Trident library files under `os/<name>/`.

### 5. Verify

Run the existing test suite with `--target <name>` to validate that universal
core programs compile correctly. Add target-specific tests for extension modules
and instruction encoding.

---

## 🛡️ The Economics Argument

The practical value of multi-target compilation is economic, not theoretical.

One codebase, one audit. A security audit of Trident Level 1 code covers every deployment target. Today, deploying the same logic on Ethereum, Solana, and Cosmos requires three separate codebases in three languages with three audits. Trident reduces this to one.

Deploy where the economics are best. The same program runs on whichever chain offers the best fee structure, liquidity, or user base at any given time. No rewrite required. The operational decision of "which chain" is separated from the engineering decision of "how to build it."

Prove where it matters. Level 1 logic can be deployed directly on conventional chains (fast, cheap, transparent execution) or wrapped in Level 2 for ZK targets (private, provable execution). The same business logic, different trust models. A lending protocol can run transparently on EVM while its risk engine runs privately on Triton VM, both from the same source.

Reduce attack surface. Every rewrite is a chance to introduce bugs. Every new language is a chance to misunderstand semantics. Trident's constraints (bounded loops, no heap, no dynamic dispatch) eliminate entire classes of vulnerabilities that affect conventional smart contract languages: reentrancy (no callbacks without explicit `os.<os>.*` access), integer overflow (field arithmetic is modular by definition), unbounded gas consumption (loops are bounded).

---

## 🌐 Cross-Chain Proof Verification

The three-level architecture enables a natural bridge pattern:

1. Write business logic in Level 1.
2. Deploy it directly on Chain A (Level 1 → EVM bytecode).
3. Deploy a provable version on Triton VM (Level 1 + Level 2 → TASM).
4. Deploy a verifier contract on Chain A that checks Triton VM proofs.

Now Chain A can trust off-chain computation without re-executing it. The verifier contract needs:
- Goldilocks field arithmetic (already present — it's part of Level 1 infrastructure)
- Algebraic hash (Tip5/Poseidon2) for proof verification
- FRI verifier logic

Because Level 1 already requires Goldilocks support on every target, the infrastructure for proof verification is partially deployed by default. The field arithmetic library that makes `Field` work on EVM is the same library the FRI verifier uses.

```text
┌──────────────────┐    STARK proof    ┌──────────────────┐
│ Triton VM        │ ────────────────→ │ EVM              │
│ Level 1+2        │                   │ Verifier contract│
│ (proves result)  │                   │ (checks proof)   │
└──────────────────┘                   └──────────────────┘
         │                                      │
    Same Level 1 logic                    Same Level 1 logic
    executed with proof                   executed directly
         │                                      │
    Result: cryptographic                 Result: on-chain
    proof of correctness                  execution
```

This creates a spectrum of trust: deploy the same logic directly (transparent, auditable, on-chain) or deploy it with proofs (private, off-chain, verified on-chain). The developer chooses per deployment, not per codebase.

---

## 🔮 Implementation Status and Roadmap

### What exists today

- Triton VM backend: Production-quality. Full type system, bounded loops, modules, cost analysis, 756 tests.
- Miden VM backend: Lowering implemented. Inline `if.true/else/end` control flow, correct instruction set. Not validated against Miden runtime.
- TIR pipeline: Operational. `TIRBuilder` produces `Vec<TIROp>` from AST. `TritonLowering` and `MidenLowering` produce assembly from TIR. Adding new lowerings is mechanical.
- 20 VM + 25 OS configurations: TOML configs with field parameters, stack depth, cost tables. Each lives in `vm/{name}/target.toml` and `os/{name}/target.toml`.

---

## 🗺️ Why Not An Existing Language?

Existing blockchain languages are designed for one VM: Solidity for EVM, Cairo for StarkNet, Move for Aptos/Sui. Rust is used across CosmWasm, Solana, and zkVMs, but the contract interfaces are incompatible — you cannot take an Anchor program and deploy it as a CosmWasm contract. No existing language treats field arithmetic, bounded execution, and abstract storage as core primitives. See [Comparative Analysis](provable-computing.md) for the full comparison.

---

## 📐 Design Principles

Think in business logic, not in chains. The developer writes what the program does. The compiler decides how to do it on each target. Platform-specific code is generated, not written.

Direct bytecode, no intermediate languages. Trident generates EVM bytecode, not Solidity. WASM, not Rust. TASM, not some Triton DSL. This gives the compiler full control and eliminates dependency on third-party toolchains.

Levels are enforced, not suggested. The compiler rejects Level 2 constructs when targeting EVM. This is a compile error, not a warning. No surprises at deployment.

Thin `os.<os>.*`, thick Level 1. Good programs have most logic in the portable Level 1 core. `os.*` is the portable runtime. `os.<os>.*` is OS-specific. The less OS-specific code, the more value from universal deployment.

Constraints are features. Bounded loops prevent runaways. No heap prevents memory exploits. No dynamic dispatch prevents reentrancy. These aren't limitations — they're safety guarantees that hold on every chain.

The proof bridge is a natural extension. Because Level 1 already requires field arithmetic on every target, the infrastructure for cross-chain proof verification is partially deployed by default. This is not an accident — it's a consequence of field-native design.

---

## 🎯 Current Targets

### Triton VM (Production)

- Status: Fully implemented. All compiler features, standard library, cost
  analysis, and tooling work with Triton VM.
- Architecture: 16-element operand stack, Goldilocks field, Tip5 hash.
- Output: `.tasm` files (Triton Assembly).
- Extensions: `os.neptune.xfield`, `os.neptune.kernel`, `os.neptune.utxo`,
  `os.neptune.proof`, `os.neptune.recursive`.
- Cost model: 6-table model (processor, hash, u32, op_stack, ram,
  jump_stack) with padded-height estimation, boundary warnings, and hotspot
  analysis.

### Other Targets (Architecture Ready)

Backend implementations and target configurations exist for:

- Miden VM -- Stack machine, Goldilocks field, Rescue-Prime hash, 4-element
  digests. `StackBackend` and `CostModel` implemented. TOML shipped.
- OpenVM -- RISC-V register machine, Goldilocks field, Poseidon2 hash.
  `StackBackend` and cycle-based `CostModel` implemented. TOML shipped.
- SP1 -- RISC-V register machine, Mersenne-31 field, Poseidon2 hash.
  `StackBackend` and cycle-based `CostModel` implemented. TOML shipped.
- Cairo -- Register machine, Stark-252 field, Pedersen hash.
  `StackBackend` and steps-based `CostModel` implemented. TOML shipped.

These backends have structural implementations -- trait methods are filled in
with correct instruction mnemonics and cost tables. They have not been validated
against their respective VM runtimes. Triton VM remains the only target with
end-to-end proving and verification.

---

## 🔗 See Also

- [IR Reference](../reference/ir.md) — Lowering architecture
- [Target Reference](../reference/targets.md) — OS model, target profiles
- [Programming Model](programming-model.md) — Execution model, OS abstraction
- [Language Reference](../reference/language.md) — Syntax and semantics
- [Compiling a Program](../guides/compiling-a-program.md) — `--target` flag
