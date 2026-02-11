# Trident Universal Execution

**Trident is a blockchain programming language. One source, every chain.**

---

## The Problem

The blockchain industry forces developers to choose: pick a chain, learn its language, rewrite everything when you need another chain. Solidity locks you to EVM. Rust+Anchor locks you to Solana. Rust+cosmwasm-std locks you to Cosmos. Cairo locks you to StarkNet. Move locks you to Aptos/Sui.

The business logic of blockchain programs — arithmetic, state transitions, access control checks, hash commitments — is the same everywhere. What differs is the execution environment: how you read storage, how you emit events, how you call other contracts, what bytecode the VM runs. Developers rewrite the same logic over and over, introducing bugs each time, paying for separate audits on each chain.

No existing language solves this because every existing blockchain language was designed for one VM and then (sometimes) awkwardly extended to another.

## Why Trident

Trident was designed for provable computation on zero-knowledge virtual machines. This forced a set of language constraints that turned out to be exactly what universal blockchain deployment requires:

- **Bounded loops.** Every loop has a compile-time bound. No infinite execution, no gas-limit surprises. Required by ZK provers, but equally valuable on EVM (predictable gas), SVM (predictable compute units), and CosmWasm (predictable execution).

- **No heap, no dynamic dispatch.** All data has known size at compile time. No malloc, no vtables, no runtime type checks. This makes programs auditable, their cost predictable, and their compilation to any target straightforward.

- **Fixed-width types.** `Field`, `U32`, `Bool`, `Digest`, fixed-size arrays, structs. No dynamically-sized types. Every value's memory footprint is known at compile time.

- **Field-native arithmetic.** The core numeric type is a finite field element (Goldilocks: 2^64 - 2^32 + 1). This fits in a 64-bit integer, making it efficient on every platform — native on ZK VMs, trivial `u64` arithmetic with modular reduction on RISC-V and WASM, `addmod`/`mulmod` on EVM.

- **Compile-time cost analysis.** The compiler tells you exactly what your program costs before you deploy it. Not an estimate — an exact row count per algebraic table (ZK targets) or instruction count (conventional targets).

These properties emerged from ZK requirements. The discovery is that they define a language ideal for safe, portable blockchain execution on any VM.

---

## Architecture: Three Levels

```
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
│            Level 3: Platform Access                      │
│  Thin adapters for target-specific capabilities         │
│  ─────────────────────────────────────────────────────── │
│  EVM: CALLER, CALLVALUE, SSTORE/SLOAD encoding          │
│  SVM: account deserialization, PDAs                      │
│  CosmWasm: Deps, Env, Response                           │
│  Neptune: UTXO model, kernel interface                   │
│  Miden: Miden-specific intrinsics                        │
└─────────────────────────────────────────────────────────┘
```

A `.tri` file that uses only Level 1 constructs compiles to **every** target. Level 2 imports restrict to ZK targets. Level 3 imports lock to a specific platform. The compiler enforces this statically — no runtime check, no silent failure.

### What each level means in practice

**Level 1** is the business logic. The math, the state transitions, the validation rules. This is where developers spend their time and where bugs live. It compiles everywhere.

**Level 2** adds cryptographic provability. Secret witness inputs, public I/O, sealed events, Merkle authentication. The same Level 1 logic now produces STARK proofs. Only available on ZK targets.

**Level 3** is the platform adapter. A thin layer that connects Level 1 logic to a specific chain's calling conventions, state model, and execution environment. This is mechanical, small, and target-locked by design.

The key insight: Level 3 is thin. The adapter between "raw bytes arrive from a transaction" and "call the Level 1 function" is a few dozen lines per target. The business logic — the part that's expensive to write, expensive to audit, and expensive to get wrong — lives entirely in Level 1.

---

## How It Works

### Direct bytecode generation

Trident does not generate Solidity, Vyper, Rust, or any intermediate source language. It generates **target bytecode directly** from its own TIR:

```
Source (.tri)
    │
    ├── Lexer → Parser → AST
    ├── Type checker (+ level check)
    ├── TIRBuilder → Vec<TIROp>
    │
    └── Lowering (per target)
         ├── TritonLowering  → TASM instructions
         ├── MidenLowering   → MASM instructions
         ├── EvmLowering     → EVM bytecode
         ├── WasmLowering    → WASM bytecode (CosmWasm, SVM)
         └── RiscVLowering   → RISC-V ELF (SP1, OpenVM)
```

This is how real compilers work. GCC doesn't generate C to target ARM — it generates ARM machine code. Trident doesn't generate Solidity to target EVM — it generates EVM bytecode. This gives the compiler full control over storage layout, calling conventions, and optimization.

The TIR (Trident Intermediate Representation) is already implemented. It's a list of stack operations with structural control flow — `IfElse`, `IfOnly`, `Loop` contain nested bodies that each backend lowers according to its own conventions. The same `Vec<TIROp>` currently produces Triton's deferred-subroutine pattern and Miden's inline `if.true/else/end` from identical input. Adding EVM, WASM, and RISC-V lowerings follows the same pattern.

### What a Level 1 program looks like

```
program token_vault

use std.core.field
use std.io.storage

struct Vault {
    owner: Field,
    balance: Field,
    locked: Bool,
}

fn deposit(vault_id: Field, amount: Field) {
    let vault: Vault = storage.read_struct(vault_id)
    assert(!vault.locked)
    let new_vault: Vault = Vault {
        owner: vault.owner,
        balance: vault.balance + amount,
        locked: vault.locked,
    }
    storage.write_struct(vault_id, new_vault)
    emit Deposit { vault_id: vault_id, amount: amount }
}

fn withdraw(vault_id: Field, amount: Field, caller: Field) {
    let vault: Vault = storage.read_struct(vault_id)
    assert(vault.owner == caller)
    assert(vault.balance >= amount)
    let new_vault: Vault = Vault {
        owner: vault.owner,
        balance: vault.balance - amount,
        locked: vault.locked,
    }
    storage.write_struct(vault_id, new_vault)
    emit Withdrawal { vault_id: vault_id, amount: amount }
}

event Deposit { vault_id: Field, amount: Field }
event Withdrawal { vault_id: Field, amount: Field }
```

This program is pure Level 1. It compiles to EVM bytecode, WASM for CosmWasm, BPF for Solana, TASM for Triton VM, and MASM for Miden. The developer writes it once. One audit covers all deployments.

### Platform adapters

Each target needs a thin adapter that maps the chain's calling convention to the Level 1 entry points. These adapters are small and mechanical:

**EVM adapter** — parses calldata, dispatches to the Trident function, encodes return data:
```
// Generated by the compiler for --target evm
// Selector dispatch: first 4 bytes of calldata → function
// deposit(uint64,uint64) → selector 0x...
// withdraw(uint64,uint64,uint64) → selector 0x...
// Storage: slot = keccak256(vault_id) for each struct field
// Events: LOG2 with topic = keccak256("Deposit(uint64,uint64)")
```

**CosmWasm adapter** — entry point, Deps wiring, Response construction:
```rust
// Generated by the compiler for --target cosmwasm
#[entry_point]
pub fn execute(deps: DepsMut, _env: Env, info: MessageInfo,
               msg: ExecuteMsg) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Deposit { vault_id, amount } => {
            // Call compiled Trident logic (WASM function)
            // Read/write storage via deps.storage
            // Return Response with events
        }
    }
}
```

**SVM adapter** — account deserialization, PDA derivation:
```rust
// Generated by the compiler for --target svm
pub fn deposit(ctx: Context<DepositAccounts>, vault_id: u64, amount: u64) -> Result<()> {
    // Deserialize vault from account data
    // Call compiled Trident logic
    // Serialize back
}
```

The adapter is not written by the developer. The compiler generates it from the program's function signatures and storage declarations. The developer thinks only in business logic.

---

## Level 1: Execute Anywhere

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

**`hash()`** — cryptographic hash, target-optimal:
| Target    | Implementation      | Cost            |
|-----------|---------------------|-----------------|
| Triton VM | Tip5 permutation    | 1 cycle + 6 co  |
| Miden     | Rescue-Prime        | ~10 cycles      |
| EVM       | KECCAK256 opcode    | 30 gas + 6/word |
| CosmWasm  | SHA-256 (native)    | ~microseconds   |
| SVM       | SHA-256 syscall     | ~100 CUs        |

**`storage.read()` / `storage.write()`** — persistent state:
| Target    | Mapping                                          |
|-----------|--------------------------------------------------|
| Triton VM | RAM addresses + Merkle commitment                |
| Miden     | Memory + state tree                               |
| EVM       | SSTORE/SLOAD with slot derivation from key       |
| CosmWasm  | `deps.storage` with binary key encoding          |
| SVM       | Account data at computed offsets                  |

**`emit`** — observable events:
| Target    | Mapping                                          |
|-----------|--------------------------------------------------|
| Triton VM | Public output                                    |
| EVM       | LOG opcodes with indexed topics                  |
| CosmWasm  | `Response::add_event`                            |
| SVM       | `msg!()` / program logs                          |

The developer writes `hash(data)`. The compiler emits `KECCAK256` on EVM, `hash` on Triton, `SHA-256` on SVM. Same program, target-optimal execution.

### Control flow

All Level 1 control flow compiles to every target:

```
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

## Level 2: Prove Anywhere

Level 2 adds zero-knowledge capabilities. Programs compile only to ZK virtual machines but gain the ability to produce cryptographic proofs of correct execution.

**`divine()`** — secret witness input. The prover supplies data invisible to the verifier. No equivalent in conventional smart contracts.

**`pub_read()` / `pub_write()`** — public I/O for proof circuits. Define the claim the proof attests to.

**`seal`** — privacy-preserving events. Fields are hashed; only the commitment is visible.

**Merkle authentication** — divine-and-authenticate pattern for state proofs.

**Sponge construction** — incremental hashing for variable-length data.

**Recursive proof verification** — verify a STARK inside another STARK.

**Cost annotations** — exact proving cost before you deploy.

A Level 2 program is a Level 1 program with these additions. The business logic is identical — only the I/O and witness handling differ:

```
program private_transfer

use std.crypto.merkle
use std.io.io

fn main() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let amount: Field = pub_read()

    // Same arithmetic as Level 1
    let sender_bal: Field = divine()
    let new_bal: Field = sender_bal - amount
    assert(new_bal >= 0)

    // Merkle proof (Level 2)
    merkle.verify(old_root, sender_leaf, index, DEPTH)

    seal Transfer { amount: amount }
}
```

The `sender_bal - amount` and `assert(new_bal >= 0)` are pure Level 1 logic. The `divine()`, `pub_read5()`, `merkle.verify()`, and `seal` are Level 2. If you remove the ZK parts, the business logic still compiles to every target.

---

## Level 3: Platform Access

Level 3 is the thin adapter layer. It connects Level 1 logic to a specific chain's environment. Importing any Level 3 module locks the program to that target.

Level 3 is **not** a rich framework. It's a minimal bridge between "transaction arrives" and "call the Level 1 function." The less Level 3 code in a program, the more portable it is. Good Trident programs have thick Level 1 and thin Level 3.

### What Level 3 provides per target

**EVM** — `CALLER` (msg.sender), `CALLVALUE` (msg.value), `BALANCE`, `SELFDESTRUCT`, raw `CALL`/`DELEGATECALL`, custom ABI encoding. Used when you need EVM-specific access control or composability.

**CosmWasm** — `Deps`/`Env`/`MessageInfo` access, `Response` builder, IBC packets, bank module, submessages. Used when you need Cosmos-specific interchain communication.

**SVM** — Account declarations, `Pubkey` type, PDA derivation, CPI (cross-program invocation). Used when you need Solana's account model.

**Neptune** — Kernel interface, UTXO authentication, MAST hash, transaction model. Used when you need Neptune-specific consensus features.

**Triton VM** — `XField` type (cubic extension), `xx_dot_step`/`xb_dot_step` for FRI verification, raw `asm(triton)` blocks. Used for cryptographic primitives that exploit Triton's native extension field.

---

## The Economics Argument

The practical value of universal execution is economic, not theoretical.

**One codebase, one audit.** A security audit of Trident Level 1 code covers every deployment target. Today, deploying the same logic on Ethereum, Solana, and Cosmos requires three separate codebases in three languages with three audits. Trident reduces this to one.

**Deploy where the economics are best.** The same program runs on whichever chain offers the best fee structure, liquidity, or user base at any given time. No rewrite required. The operational decision of "which chain" is separated from the engineering decision of "how to build it."

**Prove where it matters.** Level 1 logic can be deployed directly on conventional chains (fast, cheap, transparent execution) or wrapped in Level 2 for ZK targets (private, provable execution). The same business logic, different trust models. A lending protocol can run transparently on EVM while its risk engine runs privately on Triton VM, both from the same source.

**Reduce attack surface.** Every rewrite is a chance to introduce bugs. Every new language is a chance to misunderstand semantics. Trident's constraints (bounded loops, no heap, no dynamic dispatch) eliminate entire classes of vulnerabilities that affect conventional smart contract languages: reentrancy (no callbacks without explicit Level 3 access), integer overflow (field arithmetic is modular by definition), unbounded gas consumption (loops are bounded).

---

## Cross-Chain Proof Verification

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

```
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

## Implementation Status and Roadmap

### What exists today

- **Triton VM backend:** Production-quality. Full type system, bounded loops, modules, cost analysis, 714 tests.
- **Miden VM backend:** Lowering implemented. Inline `if.true/else/end` control flow, correct instruction set. Not validated against Miden runtime.
- **TIR pipeline:** Operational. `TIRBuilder` produces `Vec<TIROp>` from AST. `TritonLowering` and `MidenLowering` produce assembly from TIR. Adding new lowerings is mechanical.
- **5 target configurations:** Triton, Miden, OpenVM, SP1, Cairo. TOML configs with field parameters, stack depth, cost tables.

### Near-term (next to build)

**Level checking.** Compile-time pass that determines minimum level from imports and rejects Level 2/3 constructs when targeting conventional chains. Half-implemented via `#[cfg(target)]`. Formalizing it is weeks of work.

**CosmWasm lowering.** Generate WASM bytecode or Rust with cosmwasm-std. Closest match to Trident's state model (key-value storage, message-based dispatch). Rust output is pragmatic for the first conventional backend.

**EVM lowering.** Generate EVM bytecode directly. The instruction set is small (~140 opcodes). Goldilocks arithmetic is `PUSH8 p; ADDMOD` / `MULMOD`. Storage layout is `KECCAK256(key)` for slot derivation. Event encoding is `LOG` with ABI-encoded topics. This is not a Solidity compiler — it's a small bytecode emitter for a restricted language.

### Medium-term

**SVM lowering.** Generate BPF bytecode or Anchor Rust. Solana's account model is the most foreign to Trident's storage abstraction, but for standard patterns (token vaults, registries, AMMs) the mapping is well-defined.

**Cairo lowering.** SSA register representation. Cairo's `felt252` is a different field (Stark-252), requiring field adaptation or a Goldilocks-over-Stark252 emulation layer.

**Cross-chain proof verification.** Tip5/Poseidon2 library contracts on EVM and CosmWasm. FRI verifier contracts. End-to-end: prove on Triton VM, verify on Ethereum.

### Long-term

**SP1/OpenVM lowering.** RISC-V ELF output. These are general-purpose zkVMs — the lowering is essentially a RISC-V compiler backend, which is substantial.

**Optimization passes on TIR.** Dead code elimination, constant folding, common subexpression elimination. The TIR is well-structured for standard compiler optimizations.

**Formal verification across targets.** Prove that the same Trident source produces semantically equivalent behavior on different targets. The TIR makes this tractable — verify the TIRBuilder once, then verify each lowering independently.

---

## Why Not An Existing Language?

**Solidity** is EVM-only. Solang attempts EVM→SVM compilation but is experimental and not production-grade. Solidity's entire semantic model (storage slots, msg.sender, reentrancy patterns) is EVM-specific.

**Rust** is used by CosmWasm, Solana, and several zkVMs, but the contract interfaces are completely incompatible. You cannot take an Anchor program and deploy it as a CosmWasm contract. The platform-specific code dominates the contract structure.

**Cairo** is StarkNet-only. Its type system and execution model are deeply tied to the STARK prover architecture.

**Move** is restricted to Aptos/Sui. Its resource model is innovative but not portable.

**Fe** (Ethereum Foundation) has the right architectural instincts (Rust-like, uses Yul IR) but is EVM-only and currently mid-rewrite.

No existing language treats field arithmetic, bounded execution, and abstract storage as core primitives. Trident does, because these properties emerged from the requirements of provable computation. The discovery is that they are exactly what universal blockchain deployment requires.

---

## Design Principles

**Think in business logic, not in chains.** The developer writes what the program does. The compiler decides how to do it on each target. Platform-specific code is generated, not written.

**Direct bytecode, no intermediate languages.** Trident generates EVM bytecode, not Solidity. WASM, not Rust. TASM, not some Triton DSL. This gives the compiler full control and eliminates dependency on third-party toolchains.

**Levels are enforced, not suggested.** The compiler rejects Level 2 constructs when targeting EVM. This is a compile error, not a warning. No surprises at deployment.

**Thin Level 3, thick Level 1.** Good programs have most logic in the portable Level 1 core. Level 3 is a thin adapter. The less platform-specific code, the more value from universal deployment.

**Constraints are features.** Bounded loops prevent runaways. No heap prevents memory exploits. No dynamic dispatch prevents reentrancy. These aren't limitations — they're safety guarantees that hold on every chain.

**The proof bridge is a natural extension.** Because Level 1 already requires field arithmetic on every target, the infrastructure for cross-chain proof verification is partially deployed by default. This is not an accident — it's a consequence of field-native design.
