# 💻 Virtual Machines

[← Target Reference](../reference/targets.md)

Designed for 20 VMs. The VM is the CPU — the instruction set architecture.

## Provable

| VM | Arch | Word | Hash | Tier | Doc |
|----|------|------|------|------|-----|
| TRITON | Stack | Goldilocks 64-bit | Tip5 | 0-3 | [triton.md](triton/README.md) |
| MIDEN | Stack | Goldilocks 64-bit | Rescue-Prime | 0-2 | [miden.md](miden/README.md) |
| NOCK | Tree | Goldilocks 64-bit | Tip5 | 0-3 | [nock.md](nock/README.md) |
| SP1 | Register (RISC-V) | Mersenne31 31-bit | Poseidon2 | 0-1 | [sp1.md](sp1/README.md) |
| OPENVM | Register (RISC-V) | Goldilocks 64-bit | Poseidon2 | 0-1 | [openvm.md](openvm/README.md) |
| RISCZERO | Register (RISC-V) | BabyBear 31-bit | SHA-256 | 0-1 | [risczero.md](risczero/README.md) |
| JOLT | Register (RISC-V) | BN254 254-bit | Poseidon2 | 0-1 | [jolt.md](jolt/README.md) |
| CAIRO | Register | STARK-252 251-bit | Pedersen | 0-1 | [cairo.md](cairo/README.md) |
| AVM | Register | Aleo 251-bit | Poseidon | 0-1 | [avm.md](avm/README.md) |
| AZTEC | Circuit (ACIR) | BN254 254-bit | Poseidon2 | 0-1 | [aztec.md](aztec/README.md) |

## Non-provable

| VM | Arch | Word | Hash | Tier | Doc |
|----|------|------|------|------|-----|
| EVM | Stack | u256 | Keccak-256 | 0-1 | [evm.md](evm/README.md) |
| WASM | Stack | u64 | -- | 0-1 | [wasm.md](wasm/README.md) |
| SBPF | Register | u64 | SHA-256 | 0-1 | [sbpf.md](sbpf/README.md) |
| MOVEVM | Register/hybrid | u64 | SHA3-256 | 0-1 | [movevm.md](movevm/README.md) |
| TVM | Stack | u257 | SHA-256 | 0-1 | [tvm.md](tvm/README.md) |
| CKB | Register (RISC-V) | u64 | Blake2b | 0-1 | [ckb.md](ckb/README.md) |
| POLKAVM | Register (RISC-V) | u64 | Blake2b | 0-1 | [polkavm.md](polkavm/README.md) |

## Native

| VM | Arch | Word | Hash | Tier | Doc |
|----|------|------|------|------|-----|
| X86-64 | Register | u64 | Software | 0-1 | [x86-64.md](x86-64/README.md) |
| ARM64 | Register | u64 | Software | 0-1 | [arm64.md](arm64/README.md) |
| RISCV | Register | u64 | Software | 0-1 | [riscv.md](riscv/README.md) |

---

See [targets.md](../reference/targets.md) for the full OS model, tier compatibility,
type/builtin availability, and cost model overview.
