# MoveVM

[← Target Reference](../targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Register/hybrid (Move bytecode) |
| Word size | 64-bit (u64, u128, u256 native) |
| Hash function | SHA3-256 |
| Digest width | 32 bytes |
| Stack depth | Register-addressed (locals + operand stack) |
| Output format | `.mv` (Move bytecode modules) |
| Cost model | Gas (per-bytecode-instruction + storage) |

Resource-oriented bytecode VM. Move's type system enforces linear resource
semantics — assets cannot be copied or implicitly dropped, only moved. The
compiler produces `.mv` modules via dedicated `MoveLowering`.

Move bytecode uses a hybrid architecture: local variables are register-addressed,
but execution uses an operand stack for intermediate values. The bytecode
verifier enforces type safety, resource linearity, and reference safety
before execution.

Native precompiles: SHA3-256, ed25519 verification, BLS12-381 operations.

See [os/sui.md](../os/sui.md) and [os/aptos.md](../os/aptos.md) for
OS-specific runtime bindings. Same `.mv` bytecode output, different
`ext.*` bindings.
