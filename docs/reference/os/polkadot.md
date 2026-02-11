# Polkadot — Operating System

[← Target Reference](../targets.md) | VM: [PolkaVM](../targets/polkavm.md)

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | PolkaVM |
| Runtime binding | `ext.polkadot.*` |
| Account model | Account |
| Storage model | Key-value |
| Cost model | Weight (ref_time + proof_size) |
| Cross-chain | XCM (Cross-Consensus Messaging) |

## Runtime Binding (`ext.polkadot.*`)

- **Storage access** — key-value read/write to on-chain storage
- **Cross-contract calls** — invoke other contracts within the same parachain
- **XCM dispatch** — send cross-consensus messages to other parachains and relay chain

## Notes

Polkadot uses a two-dimensional weight system: `ref_time` (computation time)
and `proof_size` (state proof size for validators). This provides more
accurate resource metering than single-dimensional gas models.

XCM (Cross-Consensus Messaging) enables native cross-chain communication
between parachains, the relay chain, and external bridges — without
third-party intermediaries.

Language-agnostic — any code compiling to RISC-V can target PolkaVM. This
opens Polkadot smart contract development beyond Rust/ink! to any language
with a RISC-V backend.

For PolkaVM details (instruction set, lowering path, bytecode format),
see [polkavm.md](../targets/polkavm.md).
