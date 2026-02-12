# Solana

[← Target Reference](../targets.md) | VM: [SBPF](../vm/sbpf.md)

Solana is the high-performance blockchain powered by SBPF (Solana Berkeley
Packet Filter). Trident compiles to SBPF bytecode (`.so`) and links against
`solana.ext.*` for Solana-specific runtime bindings. Programs are stateless -- all state
lives in accounts passed into each transaction.

---

## Runtime Parameters

| Parameter | Value |
|---|---|
| VM | SBPF |
| Runtime binding | `solana.ext.*` |
| Account model | Stateless programs (state in passed accounts) |
| Storage model | Account-based |
| Transaction model | Signed (Ed25519) |
| Cost model | Compute units (200K default, 1.4M max) |
| Cross-chain | -- |

---

## Programming Model

### Entry Points

Solana programs have a single entry point: **process instruction**. Every
transaction specifies which accounts to pass and what instruction data to
send. The program receives accounts by index.

```
program my_token

use solana.ext.account
use solana.ext.log

// Single entry point -- dispatch on instruction data
fn main() {
    let instruction: Field = pub_read()

    match instruction {
        0 => { initialize() }
        1 => { transfer() }
        2 => { balance_of() }
        _ => { assert(false) }
    }
}
```

Unlike Ethereum where functions are exported by selector, Solana programs
receive raw instruction data and dispatch manually.

### State Access

Programs are **stateless** -- they own no storage. All state lives in
**accounts** that are passed into the transaction by the caller. Each
account is a byte buffer with an owner, lamport balance, and data.

```
use solana.ext.account

// Accounts are accessed by index (order in the transaction)
let owner: Field = solana.ext.account.owner(0)        // account 0's owner program
let lamports: Field = solana.ext.account.lamports(0)   // account 0's balance
let data: Field = solana.ext.account.data(0, offset)   // read from account 0's data

// Write to account data (program must own the account)
solana.ext.account.write_data(0, offset, value)

// Account properties
let key: Digest = solana.ext.account.key(0)            // account 0's public key
let is_writable: Bool = solana.ext.account.is_writable(0)
let is_signer: Bool = solana.ext.account.is_signer(0)
```

The account model is fundamentally different from Ethereum:
- **Ethereum**: contract owns storage, callers invoke functions
- **Solana**: program is stateless, callers supply accounts containing data

### Identity and Authorization

Signers are accounts that signed the transaction. The runtime verifies
Ed25519 signatures before the program runs.

```
use solana.ext.account

// Check if an account signed the transaction
assert(solana.ext.account.is_signer(0))

// The signer's public key is their identity
let authority: Digest = solana.ext.account.key(0)
```

**Program Derived Addresses (PDAs)** are deterministic addresses derived
from a program ID and seeds. They allow programs to "sign" for accounts
they control.

```
use solana.ext.pda

// Find a PDA for this program
let (pda, bump): (Digest, Field) = solana.ext.pda.find(
    program_id,
    [seed1, seed2]     // seeds array
)

// Verify an account matches a PDA
let expected: Digest = solana.ext.pda.create_address(
    program_id,
    [seed1, seed2, bump_seed]
)
assert_digest(solana.ext.account.key(2), expected)
```

### Value Transfer

SOL transfers move lamports between accounts:

```
use solana.ext.transfer

// Transfer lamports from account 0 to account 1
// Account 0 must be a signer
solana.ext.transfer.lamports(0, 1, amount)
```

SPL token transfers use CPI to the Token Program (see below).

### Cross-Contract Interaction

**Cross-Program Invocation (CPI)** lets programs call other programs:

```
use solana.ext.cpi

// Invoke another program
solana.ext.cpi.invoke(
    program_id,        // target program
    accounts,          // account metas (index, is_signer, is_writable)
    instruction_data   // raw instruction bytes
)

// Invoke with PDA signer (program signs for its PDA)
solana.ext.cpi.invoke_signed(
    program_id,
    accounts,
    instruction_data,
    signer_seeds       // PDA derivation seeds
)
```

Common CPI targets:
- **System Program** -- create accounts, transfer SOL
- **Token Program** -- SPL token operations
- **Associated Token Account** -- canonical token account derivation

### Events

Solana uses program logs and structured event data:

```
event Transfer { from: Digest, to: Digest, amount: Field }

// reveal compiles to sol_log_data (structured event)
reveal Transfer { from: sender, to: receiver, amount: value }
```

`reveal` maps to `sol_log_data` for indexed event consumption.
`seal` emits only the commitment hash as log data.

```
use solana.ext.log

// Raw log message
solana.ext.log.msg("transfer complete")

// Raw structured data
solana.ext.log.data(bytes)
```

---

## Portable Alternative (`os.*`)

Programs that don't need Solana-specific features can use `os.*`
instead of `solana.ext.*` for cross-chain portability:

| `solana.ext.*` (this OS only) | `os.*` (any OS) |
|-------------------------------|---------------------|
| `solana.ext.account.data(idx, off)` | `os.state.read(key)` → account data read |
| `solana.ext.account.key(0)` + `is_signer` | `os.neuron.id()` → first signer key |
| `solana.ext.transfer.lamports(from, to, amt)` | `os.signal.send(from, to, amt)` → system transfer |
| `solana.ext.clock.unix_timestamp()` | `os.time.now()` → Clock sysvar |

Use `solana.ext.*` when you need: PDAs, CPI, specific account indices,
rent exemption checks, or other Solana-specific features. See
[os.md](../os.md) for the full `os.*` API.

---

## Ecosystem Mapping

| Solana/Anchor concept | Trident equivalent |
|---|---|
| `declare_id!("...")` | Program identified by compiled bytecode hash |
| `#[program] mod my_program` | `program my_token` with `use solana.ext.*` |
| `pub fn initialize(ctx: Context<Init>)` | `fn initialize()` with account index access |
| `ctx.accounts.authority` | `solana.ext.account.key(N)` (account by index) |
| `#[account] struct MyAccount` | Account data at `solana.ext.account.data(N, offset)` |
| `Account<'info, Mint>` | `solana.ext.account.data(N, offset)` (manual deserialization) |
| `Signer<'info>` | `assert(solana.ext.account.is_signer(N))` |
| `has_one = authority` | `assert(solana.ext.account.key(N) == expected)` |
| `#[account(mut)]` | `assert(solana.ext.account.is_writable(N))` |
| `system_program::transfer` | `solana.ext.transfer.lamports(from, to, amount)` |
| `CpiContext::new(...)` + `invoke` | `solana.ext.cpi.invoke(program, accounts, data)` |
| `invoke_signed` | `solana.ext.cpi.invoke_signed(program, accounts, data, seeds)` |
| `Pubkey::find_program_address(seeds)` | `solana.ext.pda.find(program_id, seeds)` |
| `msg!("log message")` | `solana.ext.log.msg("log message")` |
| `emit!(MyEvent { ... })` | `reveal MyEvent { ... }` |
| `Clock::get()?.unix_timestamp` | `solana.ext.clock.unix_timestamp()` |
| `Clock::get()?.slot` | `solana.ext.clock.slot()` |
| `Clock::get()?.epoch` | `solana.ext.clock.epoch()` |
| `Rent::get()?.minimum_balance(size)` | `solana.ext.rent.minimum_balance(size)` |
| `require!(condition, ErrorCode)` | `assert(condition)` |

---

## `solana.ext.*` API Reference

| Module | Function | Signature | Description |
|--------|----------|-----------|-------------|
| **account** | `key(index)` | `U32 -> Digest` | Account public key |
| | `owner(index)` | `U32 -> Field` | Account owner program |
| | `lamports(index)` | `U32 -> Field` | Account lamport balance |
| | `data(index, offset)` | `(U32, U32) -> Field` | Read from account data |
| | `write_data(index, offset, val)` | `(U32, U32, Field) -> ()` | Write to account data |
| | `data_len(index)` | `U32 -> U32` | Account data length |
| | `is_signer(index)` | `U32 -> Bool` | Was this account a signer? |
| | `is_writable(index)` | `U32 -> Bool` | Is this account writable? |
| **pda** | `find(program, seeds)` | `(Field, [Field]) -> (Digest, Field)` | Find PDA + bump |
| | `create_address(program, seeds)` | `(Field, [Field]) -> Digest` | Derive PDA address |
| **cpi** | `invoke(program, accounts, data)` | `(Field, [...], [Field]) -> ()` | Cross-program invocation |
| | `invoke_signed(program, accounts, data, seeds)` | `(Field, [...], [Field], [[Field]]) -> ()` | CPI with PDA signer |
| **transfer** | `lamports(from, to, amount)` | `(U32, U32, Field) -> ()` | SOL transfer |
| **system** | `create_account(from, new, lamports, space, owner)` | `(...) -> ()` | Create new account |
| | `allocate(account, space)` | `(U32, U32) -> ()` | Allocate account space |
| | `assign(account, owner)` | `(U32, Field) -> ()` | Assign account owner |
| **log** | `msg(text)` | `[Field] -> ()` | Log message |
| | `data(bytes)` | `[Field] -> ()` | Log structured data |
| **clock** | `slot()` | `-> Field` | Current slot |
| | `epoch()` | `-> Field` | Current epoch |
| | `unix_timestamp()` | `-> Field` | Unix timestamp |
| **rent** | `minimum_balance(size)` | `U32 -> Field` | Rent-exempt minimum |

---

## Notes

Solana's stateless account model is fundamentally different from Ethereum's
contract storage. Programs don't own state -- accounts do. The caller must
supply every account the program will read or write, and the runtime
validates account ownership and signer status before execution begins.

Compute unit budget: 200K default per instruction, 1.4M max per transaction.
The compiler reports cost in compute units via `--costs`.

For VM details, see [sbpf.md](../vm/sbpf.md).
For mental model migration from Anchor/Rust, see
[For Blockchain Devs](../../tutorials/for-blockchain-devs.md).
