# Chapter 1: Prove a Secret

*The Builder's Journey -- Chapter 1 of 6*

You are about to learn the most powerful primitive in cryptography: proving
you know something without revealing what it is.

By the end of this chapter you will write a program, compile it, and understand
how a verifier can confirm you know a password -- without ever seeing it.

---

## The Program

Create a file called `secret.tri`:

```trident
program secret

fn main() {
    let lock_hash: Digest = pub_read5()
    let secret: Field = divine()
    let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    assert_digest(computed, lock_hash)
}
```

Four lines of logic. That is the entire program. It proves: "I know a value
that hashes to this digest." The verifier becomes convinced of that fact
without ever learning what the value is.

---

## What Just Happened

Let us walk through each line.

**`program secret`** -- Every Trident file starts with a declaration. A
`program` has a `main` function and compiles to an executable. The name
`secret` is the program identifier.

**`fn main() {`** -- The entry point. All I/O happens inside `main` through
builtins: `pub_read5`, `divine`, `hash`, and `assert_digest`.

**`let lock_hash: Digest = pub_read5()`** -- Read five field elements from
public input and pack them into a `Digest`. This is the hash the prover claims
to know the preimage of. It is *public* -- the verifier sees it. Think of it as
the lock on a door: everyone can see the lock, but only the person with the key
can open it.

**`let secret: Field = divine()`** -- Read one field element from secret input.
This is the preimage -- the key. The word "divine" means the prover conjures
the value. The verifier never sees it. It does not appear in the proof, it is
not transmitted, it is not encrypted. It simply never leaves the prover's
machine.

**`let computed: Digest = hash(secret, 0, 0, 0, 0, 0, 0, 0, 0, 0)`** -- Hash
the secret. The `hash` builtin takes exactly 10 field elements (the Tip5 hash
rate) and returns a 5-element `Digest`. We only have one field of real data, so
the remaining nine slots are zero-padded. The result is a one-way commitment:
given `computed`, nobody can recover `secret`.

**`assert_digest(computed, lock_hash)`** -- Assert that the two digests are
equal, element by element. If they match, execution continues and a proof can
be generated. If they do not match, execution fails and no proof is produced.
This is the core constraint: the prover must supply a secret whose hash matches
the public lock hash. There is no other way to satisfy this assertion.

The program does not output anything. It does not need to. The mere existence
of a valid proof is the output -- it means the prover knew the secret.

---

## Build It

Compile to Triton Assembly:

```bash
trident build secret.tri --target triton -o secret.tasm
```

Type-check without emitting assembly:

```bash
trident check secret.tri
```

See the proving cost:

```bash
trident build secret.tri --costs
```

The cost will be tiny. One hash, one assertion, a few I/O reads. This is among
the cheapest useful programs you can write.

---

## The Mental Model

There are two roles: the **prover** and the **verifier**. They have radically
different views of the same program.

**The prover** runs the program with all inputs -- both public and secret. The
prover knows `lock_hash` and `secret`. The prover executes every instruction,
produces a full execution trace, and compresses that trace into a proof using
the STARK protocol.

**The verifier** never runs the program. The verifier receives three things:
the program hash (which program was proved), the public input (`lock_hash`),
and the proof. The verifier runs a fast mathematical check -- milliseconds,
constant time, regardless of how complex the original program was -- and either
accepts or rejects.

If the verifier accepts: the prover knew a value whose hash equals `lock_hash`.
That is all the proof says. It does not say what the value was.

If the verifier rejects: something was wrong. The prover either did not know
the secret or tampered with the computation.

This is not encryption. The secret is never transmitted in any form -- not
plaintext, not ciphertext, not obfuscated. It exists only in the prover's
memory during execution and then it is gone. What crosses the wire is a proof:
a few hundred kilobytes of polynomial commitments and hash queries that
convince the verifier without revealing anything.

The asymmetry is the point. The prover does the heavy work once. Anyone can
verify instantly.

---

## Why This Matters

This four-line program is the atom of zero-knowledge programming. Every
interesting application is a variation of the same pattern.

A **payment** is proving you know the secret that unlocks a coin -- without
revealing the secret. The lock hash lives in the coin. Your secret is the key.
Chapter 2.

A **name registration** is proving you own the secret behind a unique asset --
without revealing it. The lock hash lives in the name record. Chapter 3.

A **trade** is proving your position obeys a mathematical invariant -- without
showing the position itself. The invariant is public, the position is divine.
Chapter 4.

A **sealed bid** is proving your bid price is committed and hidden until
reveal time -- without anyone seeing it early. The commitment is a hash. The
price is the preimage. Chapter 5.

A **vote** is proving you hold tokens that entitle you to vote -- without
revealing which tokens. The token set is divine. The eligibility proof is a
hash. Chapter 6.

Every chapter is this chapter with more context. The primitive does not change.
`divine`, `hash`, `assert` -- secret in, commitment out, constraint satisfied.

---

## What You Learned

- `divine()` loads secret data that only the prover knows. The verifier never
  sees it.
- `pub_read5()` loads public data visible to both prover and verifier.
- `hash()` creates a one-way commitment from field elements. Given the hash,
  nobody can recover the input.
- `assert_digest()` proves two digests are equal. If the assertion holds, the
  prover knew the preimage. If it fails, no proof is generated.
- The verifier confirms the proof without seeing the secret, without re-running
  the program, and in constant time.
- This pattern -- divine, hash, assert -- is the foundation of all
  zero-knowledge programming. Everything that follows builds on it.

---

## Next

[Chapter 2: Build a Coin](build-a-coin.md) -- You will use this same pattern
to build a private token with pay, lock, mint, and burn operations. The
"secret" becomes your account's auth key, and the "proof" becomes a
transaction.
