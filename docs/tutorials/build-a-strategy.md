# Chapter 4: Build a Liquidity Strategy

*The Builder's Journey -- Chapter 4 of 6*

Your coin has value. Your name service gives it identity. But value without
liquidity is trapped. This chapter makes your coin tradeable -- not by locking
tokens in a pool, but by proving that every swap obeys a pricing invariant.

The secret being proven: your reserves satisfy x * y = k. The verifier
confirms the math without seeing the reserves.

---

## The Problem with Pools

Every AMM you have used -- Uniswap, Curve, Balancer -- works the same way.
You deposit tokens into a contract. The contract holds them. Your capital sits
there doing one thing: providing liquidity for that single pool.

Want to use the same capital as lending collateral? You cannot. It is locked.
Want it to count for governance votes? It cannot. It is locked. Want it to
back a second trading pair? Deposit more.

The pool model forces a choice: liquidity OR collateral OR governance OR
staking. Pick one. This is not a technical limitation. It is a design flaw
inherited from the EVM's custody model, where "the contract holds your
tokens" is the only way to enforce invariants.

Neptune does not need custody to enforce invariants. It has proofs.

---

## How TIDE Works

TIDE -- Tokens In Direct Exchange -- replaces custodial pools with proof
constraints. Your tokens never leave your account. The AMM is a ZK program
that constrains `pay` operations, not a contract that holds your funds.

Here is a swap. Alice wants to trade 100 TOKEN_A for TOKEN_B. Bob is a maker
who has registered a constant-product strategy.

Two `pay` operations:

```text
TOKEN_A pay: Alice -> Bob, amount = 100
TOKEN_B pay: Bob -> Alice, amount = f(100)
```

The strategy program proves that `f(100)` is the correct output amount -- that
it satisfies the constant-product invariant given Bob's reserves.

That is the entire swap. Two payments. One proof. No tokens leave user
accounts. No approvals. No router contract. No pool address.

Bob's balance simultaneously backs this strategy, a second strategy on a
different pair, lending collateral on a third protocol, and governance votes.
The same tokens. At the same time. Because none of them require custody --
they all require proofs.

---

## The Constant-Product Invariant

The simplest AMM curve. Two reserves, one invariant:

```text
reserve_a * reserve_b = k
```

Before a swap, the reserves satisfy this equation. After the swap, they must
still satisfy it. If Alice sends `amount_in` of TOKEN_A and receives
`amount_out` of TOKEN_B:

```text
Before:  reserve_a * reserve_b = k
After:   (reserve_a + amount_in) * (reserve_b - amount_out) = k
```

Solve for `amount_out`:

```text
amount_out = reserve_b - k / (reserve_a + amount_in)
```

In a traditional AMM, the contract computes this. The reserves are public
storage variables. Anyone can read them. MEV bots read them, simulate your
swap, and front-run you.

In TIDE, the prover computes `amount_out` and proves it satisfies the
invariant. The verifier checks the proof. Nobody sees `reserve_a` or
`reserve_b`. The reserves are private. The math is proven.

---

## The Strategy Program

Create a file called `strategy.tri`:

```trident
program strategy

fn main() {
    // Public: the swap parameters everyone can see
    let amount_in: Field = pub_read()
    let amount_out: Field = pub_read()
    let k_commitment: Digest = pub_read5()

    // Secret: the reserves only the maker knows
    let reserve_a: Field = divine()
    let reserve_b: Field = divine()

    // Verify k commitment -- the maker cannot lie about their invariant
    let computed_k: Field = reserve_a * reserve_b
    let k_digest: Digest = hash(computed_k, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    assert_digest(k_digest, k_commitment)

    // Verify the invariant holds after the swap
    let new_reserve_a: Field = reserve_a + amount_in
    let new_reserve_b: Field = sub(reserve_b, amount_out)
    let new_k: Field = new_reserve_a * new_reserve_b
    assert_eq(new_k, computed_k)

    // Output the new k commitment for the next swap
    let new_k_digest: Digest = hash(new_k, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    pub_write5(new_k_digest)
}
```

Twenty lines of logic. That is a complete constant-product AMM.

---

## What Just Happened

Walk through each section.

#### Public inputs: what the verifier sees

```trident
let amount_in: Field = pub_read()
let amount_out: Field = pub_read()
let k_commitment: Digest = pub_read5()
```

The swap amounts are public -- both parties need to agree on what is being
exchanged. The `k_commitment` is a hash of the invariant constant `k`. It is
public so the verifier can confirm the maker is using a consistent invariant
across swaps, but it reveals nothing about the reserves themselves.

#### Secret inputs: what only the maker knows

```trident
let reserve_a: Field = divine()
let reserve_b: Field = divine()
```

The reserves are `divine()` -- conjured by the prover, invisible to the
verifier. This is the same primitive from Chapter 1. The secret was a
password then. Now the secret is a liquidity position.

#### Commitment verification: binding the invariant

```trident
let computed_k: Field = reserve_a * reserve_b
let k_digest: Digest = hash(computed_k, 0, 0, 0, 0, 0, 0, 0, 0, 0)
assert_digest(k_digest, k_commitment)
```

The maker claims their reserves produce a certain `k`. This section checks
that claim. Compute `k` from the divined reserves, hash it, and assert it
matches the public commitment. If the maker lies about their reserves, the
hash will not match and no proof can be generated.

This is Chapter 1's pattern exactly: `divine`, `hash`, `assert`. The secret
is the reserves. The commitment is `k_commitment`. The constraint is that
they match.

#### Invariant enforcement: the AMM logic

```trident
let new_reserve_a: Field = reserve_a + amount_in
let new_reserve_b: Field = sub(reserve_b, amount_out)
let new_k: Field = new_reserve_a * new_reserve_b
assert_eq(new_k, computed_k)
```

After the swap, the new reserves must still satisfy `x * y = k`. Add
`amount_in` to reserve A. Subtract `amount_out` from reserve B using
`sub()` (Trident has no subtraction operator). Multiply the new reserves.
Assert the product equals the original `k`.

If the taker asks for too many tokens, `new_k` will not equal `computed_k`.
The assertion fails. No proof. No swap.

#### State transition: preparing for the next swap

```trident
let new_k_digest: Digest = hash(new_k, 0, 0, 0, 0, 0, 0, 0, 0, 0)
pub_write5(new_k_digest)
```

The program outputs a new `k` commitment. In a constant-product curve, `k`
stays the same (that is the invariant). But this output is what allows the
strategy to chain: the next swap's `k_commitment` input must match this
swap's `k_commitment` output. The verifier enforces continuity without ever
seeing the reserves.

---

## Composing with Pay

The strategy program proves the pricing is correct. But the actual token
movement happens through `pay` -- the same operation from Chapter 2.

A complete swap is three composed proofs:

```text
Strategy proof:   amount_out = f(amount_in) given reserves
TOKEN_A pay proof: Alice -> Bob, amount = amount_in
TOKEN_B pay proof: Bob -> Alice, amount = amount_out
```

Composed:

```text
Strategy ⊗ Pay_A ⊗ Pay_B -> single verification
```

One proof covers the entire swap. The verifier checks it once. The strategy
confirms the price is fair. The pay proofs confirm the tokens moved. Nobody
trusts anyone. Nobody custodies anything.

This is the same composition pattern Chapter 2 used for coin operations and
Chapter 3 used for name resolution. Every chapter adds a new proof to the
composition. The primitive does not change.

---

## Strategy Registration

A strategy is identified by its commitment:

```trident
strategy = hash(maker, token_a, token_b, program, parameters)
```

`maker` is Bob's identity. `token_a` and `token_b` are the pair. `program`
is the hash of the strategy circuit (our constant-product program). `parameters`
encode curve-specific configuration -- for constant product, this could include
a fee rate.

Once registered, the strategy is immutable. To change parameters, revoke and
re-register. This keeps the on-chain state simple and the proofs clean.

---

## Shared Liquidity

Here is what makes TIDE different from every AMM before it.

Bob has 10,000 TOKEN_A in his account. He registers three strategies:

- Constant-product: TOKEN_A / TOKEN_B
- Stable-swap: TOKEN_A / TOKEN_C
- Oracle-priced: TOKEN_A / TOKEN_D

All three strategies are backed by the same 10,000 TOKEN_A. There is no
allocation, no splitting, no fractional locking. The full balance backs every
strategy.

How? Each swap proof checks Bob's current balance at execution time via the
Merkle root. If two swaps try to spend the same tokens in the same block, the
second proof fails -- the Merkle root already changed. Atomic consistency
without locks.

Bob's 10,000 TOKEN_A simultaneously:

- Backs three AMM strategies
- Serves as lending collateral (via a lending hook)
- Counts for governance votes
- Earns staking rewards

One balance. Five uses. No custody anywhere.

---

## The Power of Privacy

In Uniswap, the reserves are public. They are storage variables in a
Solidity contract. Anyone can call `getReserves()` and see exactly how much
liquidity exists at every price point.

MEV bots exploit this ruthlessly. They see your pending swap in the mempool.
They read the reserves. They compute exactly how much your swap will move the
price. They insert a transaction before yours (front-run) and one after
(back-run). You get a worse price. They pocket the difference.

Sandwich attacks extracted over $1.3 billion from Ethereum users in 2024.

In TIDE, the reserves are `divine()`. They exist only in the maker's memory
during proof generation. The verifier confirms the invariant holds without
seeing the reserves. A MEV bot looking at a TIDE swap sees:

- The amounts being exchanged (public)
- A commitment to `k` (a hash -- reveals nothing)
- A proof that the invariant holds

The bot cannot compute the price impact. It cannot simulate the reserves.
It cannot front-run. The information it needs to extract value simply does
not exist in the public domain.

This is not a mitigation. It is an elimination. The attack surface is gone
because the data the attack requires is never published.

---

## Build It

Compile the strategy to Triton Assembly:

```bash
trident build strategy.tri --target triton -o strategy.tasm
```

Type-check without emitting assembly:

```bash
trident check strategy.tri
```

See the proving cost:

```bash
trident build strategy.tri --costs
```

The cost is modest. Two hash operations, two assertions, a multiplication,
an addition, and a subtraction. This is one of the cheapest useful DeFi
programs you can write -- because the AMM is a constraint, not a computation
over global state.

---

## What You Learned

- **TIDE** replaces custodial pools with proof constraints. Swaps are two
  coordinated `pay` operations. No tokens leave user accounts.
- **Strategy** is a ZK program that enforces a pricing invariant. The maker
  registers it. The verifier checks it. The strategy program is the AMM.
- **Constant product** -- `x * y = k` -- proven without revealing `x` or `y`.
  The reserves are `divine()`. The invariant is `assert_eq()`. Chapter 1's
  pattern, applied to DeFi.
- **Privacy** eliminates MEV. Reserves are secret. Bots cannot compute price
  impact. Front-running and sandwich attacks require data that is never
  published.
- **Composition** -- strategy proof + pay proofs = one verified swap. The same
  composition pattern from Chapters 2 and 3, extended to trading.
- **Shared liquidity** -- one balance backs multiple strategies, lending,
  governance, and staking simultaneously. No custody means no exclusivity.
- **The secret** is your reserve position. `divine`, `hash`, `assert` --
  Chapter 1 again. The primitive never changes. The context does.

---

## Next

[Chapter 5: Auction Names with Hidden Bids](build-an-auction.md) -- Your name
service needs a fair way to sell names. Vickrey auctions let bidders bid their
true value -- and ZK makes the bids genuinely hidden until reveal time.
