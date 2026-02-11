# Operating Systems

[← Target Reference](../targets.md)

25 OSes. The OS is the runtime — storage, accounts, syscalls, billing.

## Provable

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Neptune | Triton VM | `ext.neptune.*` | [neptune.md](neptune.md) |
| Polygon Miden | Miden VM | `ext.miden.*` | [miden.md](miden.md) |
| Nockchain | Nock | `ext.nockchain.*` | [nockchain.md](nockchain.md) |
| Starknet | Cairo VM | `ext.starknet.*` | [starknet.md](starknet.md) |
| Boundless | RISC Zero | `ext.boundless.*` | [boundless.md](boundless.md) |
| Succinct | SP1 | `ext.succinct.*` | [succinct.md](succinct.md) |
| OpenVM network | OpenVM | `ext.openvm.*` | [openvm-network.md](openvm-network.md) |
| Aleo | AVM (Leo) | `ext.aleo.*` | [aleo.md](aleo.md) |
| Aztec | Aztec (Noir) | `ext.aztec.*` | [aztec.md](aztec.md) |

## Blockchain

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Ethereum | EVM | `ext.ethereum.*` | [ethereum.md](ethereum.md) |
| Solana | eBPF (SVM) | `ext.solana.*` | [solana.md](solana.md) |
| Near Protocol | WASM | `ext.near.*` | [near.md](near.md) |
| Cosmos (100+ chains) | WASM | `ext.cosmwasm.*` | [cosmwasm.md](cosmwasm.md) |
| Arbitrum | WASM + EVM | `ext.arbitrum.*` | [arbitrum.md](arbitrum.md) |
| Internet Computer | WASM | `ext.icp.*` | [icp.md](icp.md) |
| Sui | MoveVM | `ext.sui.*` | [sui.md](sui.md) |
| Aptos | MoveVM | `ext.aptos.*` | [aptos.md](aptos.md) |
| Ton | TVM | `ext.ton.*` | [ton.md](ton.md) |
| Nervos CKB | CKB-VM | `ext.nervos.*` | [nervos.md](nervos.md) |
| Polkadot | PolkaVM | `ext.polkadot.*` | [polkadot.md](polkadot.md) |

## Traditional

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Linux | x86-64 / ARM64 / RISC-V | `ext.linux.*` | [linux.md](linux.md) |
| macOS | ARM64 / x86-64 | `ext.macos.*` | [macos.md](macos.md) |
| Android | ARM64 / x86-64 | `ext.android.*` | [android.md](android.md) |
| WASI | WASM | `ext.wasi.*` | [wasi.md](wasi.md) |
| Browser | WASM | `ext.browser.*` | [browser.md](browser.md) |

---

See [targets.md](../targets.md) for the full OS model, tier compatibility,
type/builtin availability, and cost model overview.
