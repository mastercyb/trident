# 🖥️ Operating Systems

[← Target Reference](../reference/targets.md)

Designed for 25 OSes. The OS is the runtime — storage, accounts, syscalls, billing.

## Provable

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Neptune | TRITON | `os.neptune.*` | [neptune](neptune/README.md) |
| Polygon Miden | MIDEN | `os.miden.*` | [miden](miden/README.md) |
| Nockchain | NOCK | `os.nockchain.*` | [nockchain](nockchain/README.md) |
| Starknet | CAIRO | `os.starknet.*` | [starknet](starknet/README.md) |
| Boundless | RISCZERO | `os.boundless.*` | [boundless](boundless/README.md) |
| Succinct | SP1 | `os.succinct.*` | [succinct](succinct/README.md) |
| OpenVM Network | OPENVM | `os.openvm.*` | [openvm-network](openvm-network/README.md) |
| Aleo | AVM | `os.aleo.*` | [aleo](aleo/README.md) |
| Aztec | AZTEC | `os.aztec.*` | [aztec](aztec/README.md) |

## Blockchain

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Ethereum | EVM | `os.ethereum.*` | [ethereum](ethereum/README.md) |
| Solana | SBPF | `os.solana.*` | [solana](solana/README.md) |
| Near Protocol | WASM | `os.near.*` | [near](near/README.md) |
| Cosmos (100+ chains) | WASM | `os.cosmwasm.*` | [cosmwasm](cosmwasm/README.md) |
| Arbitrum | WASM + EVM | `os.arbitrum.*` | [arbitrum](arbitrum/README.md) |
| Internet Computer | WASM | `os.icp.*` | [icp](icp/README.md) |
| Sui | MOVEVM | `os.sui.*` | [sui](sui/README.md) |
| Aptos | MOVEVM | `os.aptos.*` | [aptos](aptos/README.md) |
| Ton | TVM | `os.ton.*` | [ton](ton/README.md) |
| Nervos CKB | CKB | `os.nervos.*` | [nervos](nervos/README.md) |
| Polkadot | POLKAVM | `os.polkadot.*` | [polkadot](polkadot/README.md) |

## Traditional

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Linux | X86-64 / ARM64 / RISCV | `os.linux.*` | [linux](linux/README.md) |
| macOS | ARM64 / X86-64 | `os.macos.*` | [macos](macos/README.md) |
| Android | ARM64 / X86-64 | `os.android.*` | [android](android/README.md) |
| WASI | WASM | `os.wasi.*` | [wasi](wasi/README.md) |
| Browser | WASM | `os.browser.*` | [browser](browser/README.md) |

---

See [targets.md](../reference/targets.md) for the full OS model, tier compatibility,
type/builtin availability, and cost model overview.
