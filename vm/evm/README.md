# EVM (Ethereum Virtual Machine)

[← Target Reference](../../reference/targets.md)

---

## VM Parameters

| Parameter | Value |
|---|---|
| Architecture | Stack |
| Word size | 256-bit native |
| Hash function | Keccak-256 |
| Digest width | 32 bytes |
| Stack depth | 1024 |
| Output format | `.evm` (bytecode) |
| Cost model | Gas (per-opcode: arithmetic 3-8, storage 5K-20K) |

Stack-based virtual machine with 256-bit words. The large word size means a
single stack slot can hold values that would require 4 Goldilocks field
elements. EVM bytecode is a flat sequence of opcodes — no structured
functions, just jump destinations.

Native precompiles: ecRecover, SHA-256, RIPEMD-160, identity, modular
exponentiation, EC addition/multiplication/pairing (BN254), Blake2f. These
map naturally to Trident builtin calls.

Requires dedicated `EvmLowering` due to the unique 256-bit stack
architecture and opcode set.

See [os/ethereum.md](../../os/ethereum/README.md) and other EVM-compatible OS docs for
OS-specific runtime bindings. Same `.evm` bytecode deploys to all
EVM-compatible targets.

---

## Cost Model (Gas)

Per-opcode gas cost. Representative values:

| Operation class | Gas | Examples |
|---|---:|---|
| Arithmetic | 3-5 | ADD, MUL, SUB, DIV |
| Comparison | 3 | LT, GT, EQ |
| Bitwise | 3 | AND, OR, XOR |
| Keccak-256 | 30 + 6/word | KECCAK256 |
| Memory | 3 | MLOAD, MSTORE |
| Storage read | 2,100 | SLOAD (cold) |
| Storage write | 5,000-20,000 | SSTORE (cold, set/change) |
| Call | 2,600 | CALL (cold account) |
| Log | 375 + 375/topic + 8/byte | LOG0-LOG4 |

Storage dominates. Detailed cost model planned.
