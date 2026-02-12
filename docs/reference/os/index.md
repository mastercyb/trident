# Operating Systems

[← Target Reference](../targets.md)

25 OSes. The OS is the runtime — storage, accounts, syscalls, billing.

## Provable

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Neptune | TRITON | `ext.neptune.*` | [neptune.md](neptune.md) |
| Polygon Miden | MIDEN | `ext.miden.*` | [miden.md](miden.md) |
| Nockchain | NOCK | `ext.nockchain.*` | [nockchain.md](nockchain.md) |
| Starknet | CAIRO | `ext.starknet.*` | [starknet.md](starknet.md) |
| Boundless | RISCZERO | `ext.boundless.*` | [boundless.md](boundless.md) |
| Succinct | SP1 | `ext.succinct.*` | [succinct.md](succinct.md) |
| OpenVM Network | OPENVM | `ext.openvm.*` | [openvm-network.md](openvm-network.md) |
| Aleo | AVM | `ext.aleo.*` | [aleo.md](aleo.md) |
| Aztec | AZTEC | `ext.aztec.*` | [aztec.md](aztec.md) |

## Blockchain

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Ethereum | EVM | `ext.ethereum.*` | [ethereum.md](ethereum.md) |
| Solana | SBPF | `ext.solana.*` | [solana.md](solana.md) |
| Near Protocol | WASM | `ext.near.*` | [near.md](near.md) |
| Cosmos (100+ chains) | WASM | `ext.cosmwasm.*` | [cosmwasm.md](cosmwasm.md) |
| Arbitrum | WASM + EVM | `ext.arbitrum.*` | [arbitrum.md](arbitrum.md) |
| Internet Computer | WASM | `ext.icp.*` | [icp.md](icp.md) |
| Sui | MOVEVM | `ext.sui.*` | [sui.md](sui.md) |
| Aptos | MOVEVM | `ext.aptos.*` | [aptos.md](aptos.md) |
| Ton | TVM | `ext.ton.*` | [ton.md](ton.md) |
| Nervos CKB | CKB | `ext.nervos.*` | [nervos.md](nervos.md) |
| Polkadot | POLKAVM | `ext.polkadot.*` | [polkadot.md](polkadot.md) |

## Traditional

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Linux | X86-64 / ARM64 / RISCV | `ext.linux.*` | [linux.md](linux.md) |
| macOS | ARM64 / X86-64 | `ext.macos.*` | [macos.md](macos.md) |
| Android | ARM64 / X86-64 | `ext.android.*` | [android.md](android.md) |
| WASI | WASM | `ext.wasi.*` | [wasi.md](wasi.md) |
| Browser | WASM | `ext.browser.*` | [browser.md](browser.md) |

---

See [targets.md](../targets.md) for the full OS model, tier compatibility,
type/builtin availability, and cost model overview.
