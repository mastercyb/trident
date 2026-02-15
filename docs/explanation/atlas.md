# Atlas: The On-Chain Package Registry

Atlas is Trident's on-chain package registry. Every OS (blockchain)
maintains its own Atlas instance where packages are TSP-2 Cards --
published by minting, updated by metadata change, transferred by pay.
The same PLUMB framework that powers all tokens also powers the
package manager.

See [Atlas Reference](../../reference/atlas.md) for the complete
specification.

---

## Why On-Chain Package Management

Centralized package registries are trust bottlenecks. npm, crates.io,
and PyPI each serve as a single point of failure for their respective
ecosystems. The npm left-pad incident demonstrated that one maintainer
removing one package can break thousands of builds worldwide. Crates.io
faces chronic name squatting. PyPI has become a distribution vector for
malware -- typosquatted packages harvesting credentials from developers
who mistype an install command.

These problems share a root cause: the registry is an authority that
everyone must trust but nobody can independently verify.

Content-addressed code solves the identity problem. When a function's
hash IS its identity, there is no ambiguity about what you are
installing. Substitution attacks -- replacing a legitimate artifact with
a malicious one -- become impossible because any modification changes
the hash.

On-chain publication solves the availability problem. A package
published to a blockchain cannot be unpublished by a maintainer having
a bad day, seized by a government order, or lost because a company shut
down its servers. The chain is permissionless, censorship-resistant,
and always queryable.

On-chain publication also solves the provenance problem. The mint
transaction records who published the package, when, and to which
collection -- immutably, forever. No way to backdate a publication,
forge a publisher identity, or alter the historical record.

The final layer is verification certificates. A STARK proof that the
compiler correctly compiled the source to the artifact can accompany
every package. Content hash prevents artifact substitution. Verification
certificate prevents compilation fraud. Together they eliminate the
supply chain as an attack surface.

---

## Why Packages Are Tokens

A package registry has the same operations as a token standard. Consider
the lifecycle:

| Registry operation | Token operation |
|--------------------|-----------------|
| Publish a package  | Mint a Card     |
| Update metadata    | Update          |
| Transfer ownership | Pay             |
| Deprecate          | Burn            |

This is the central insight behind Atlas. TSP-2 Card maps perfectly
onto the package registry domain. Each Card represents a package.
The `asset_id` is `hash(name)` -- deterministic and collision-resistant.
The `metadata_hash` is `content_hash(artifact)` -- the compiled
artifact's identity. The `owner_id` is the publisher's neuron identity.

No separate registry protocol is needed. Atlas reuses PLUMB -- the same
framework, the same auth model, the same hooks, the same proofs that
power every token on the network. The registry is just another
collection of Cards, distinguished only by its purpose.

Package ownership becomes a real on-chain asset. It is transferable,
enabling clean project handoffs when a maintainer moves on. It is
burnable, providing formal deprecation semantics -- a burned Card
signals that the package is officially abandoned, distinct from merely
unmaintained. Creator immutability preserves provenance: even after
ownership transfers, the original publisher's identity remains in the
Card forever.

Hooks enable governance. A collection's `mint_hook` can enforce naming
policies (no profanity, no trademark infringement), quality gates
(must include verification certificate), or community voting before
publication (governance skill approval required). These policies are
ZK programs -- they execute as part of the mint proof, not as off-chain
checks that can be bypassed.

The Card's flags control lifecycle semantics per package. `UPDATABLE`
determines whether versioning is allowed. `BURNABLE` determines whether
deprecation is possible. `TRANSFERABLE` controls whether ownership can
change hands. A package published with `TRANSFERABLE = false` has
permanent, non-transferable ownership -- useful for protocol-critical
infrastructure that should never change hands.

---

## The Three-Tier Resolution Story

Atlas uses an offline-first design with three resolution tiers: local,
cache, and on-chain.

**Local files always win.** A developer can write code, compile, test,
and iterate without internet access, without a running blockchain node,
without any Atlas connectivity. Local `.tri` files and locally stored
definitions take priority over everything else. Development is never
blocked by network conditions.

**Cache preserves fetched artifacts.** Once an artifact is downloaded
from Atlas, it is stored locally in the content-addressed cache. The
content hash verifies integrity -- if the hash matches, the cached
copy is valid regardless of when it was fetched. There is no cache
invalidation problem because there is no cache invalidation. Changed
code produces a different hash, which is a different cache entry. The
old entry remains valid for anyone still referencing the old hash.

**On-chain is the source of truth.** When a package is not available
locally or in cache, the compiler queries the OS's Atlas collection.
The chain survives server outages, company shutdowns, DNS seizures, and
infrastructure migrations. As long as the blockchain operates, every
published package remains available.

Why three tiers instead of querying the chain directly? Latency: local
resolution is instant, cache is a filesystem read, on-chain requires
waiting for a query response. Offline development: researchers on
planes, developers with unreliable connectivity, and CI systems behind
restrictive firewalls all need local resolution. Testing: unpublished
packages need local override without deploying to a test chain.

---

## Per-OS Independence

Each OS (blockchain) maintains its own Atlas collection. There is no
shared global namespace, no cross-chain name authority, no central
coordination.

Different chains can enforce different governance policies. One OS might
allow open publishing -- anyone can mint a Card. Another might restrict
publishing to an allowlist of approved developers. A third might require
governance votes before each publication. The `mint_hook` on the Atlas
collection encodes whatever policy the community chooses.

The same package name on different OSes refers to different packages.
`os.neptune.atlas.my_skill` and `os.ethereum.atlas.my_skill` are
independent Cards in independent collections with potentially different
code, different publishers, and different governance. There is no
conflict because there is no shared namespace.

This independence reflects sovereignty. Each blockchain community
governs its own ecosystem. Neptune's package policies are decided by
Neptune participants. A future Ethereum-targeting OS makes its own
decisions. No single chain controls what gets published elsewhere.

Cross-OS discovery remains possible. A developer searching for a Merkle
proof verifier can query multiple Atlas instances to find implementations
targeting different VMs. Identical source hashes mean identical
computations, regardless of which chain hosts them.

---

## How Atlas Enables the Skill Ecosystem

The 23 standard skills ship with the compiler as `std.skill.*` modules.
They cover the essential DeFi primitives -- liquidity, lending,
governance, compliance, oracle pricing, and more. But the standard
library is a starting point, not a ceiling.

The real power is community skills deployed to Atlas. Any developer can
write a skill, compile it, and publish it as a Card. Other tokens
reference it by name or by content hash. The skill ecosystem grows
without gatekeepers.

Three usage paths define how skills flow through the system:

**Import.** `use std.skill.liquidity` inlines the skill's circuit at
compile time. The skill code becomes part of your program. Fast,
simple, no runtime dependency.

**Fork.** Copy a skill's source from `std/skill/`, modify it to fit
your use case, compile your own version. The fork gets a new content
hash -- it is a new computation, tracked independently. If the original
skill has a verification certificate, your fork needs its own.

**Deploy.** Publish your skill to Atlas. Other tokens reference it by
registry name or content hash in their hook configuration. The verifier
fetches and composes the proof independently -- the token proof and the
skill proof are verified together but generated separately.

A token's hook config can point to an Atlas name (resolved at compile
time via on-chain query) or a content hash (resolved at verification
time). Name references get the latest version. Hash references get the
exact version chosen -- immutable, pinned, immune to upstream changes.

Skills published on Atlas are tradeable assets. A well-audited,
battle-tested liquidity skill has economic value. The publisher (Card
owner) can transfer ownership, and the Card's provenance chain records
every transfer. Versioning works through the Update operation: changing
`metadata_hash` points the Card to a new artifact. Old versions remain
accessible by their content hash -- tokens that referenced a specific
hash continue to work unchanged.

---

## The Content-Addressed Foundation

Every compiled Trident artifact is identified by its content hash.
Poseidon2 over the Goldilocks field provides proof-friendly addressing
-- hashes that are cheap to verify inside ZK circuits. BLAKE3 handles
source-level hashing where proof-friendliness is unnecessary.

Same source code produces the same normalized AST, which produces the
same serialized bytes, which produces the same hash. Reproducible builds
are a property of the system, not a goal that requires careful
engineering. The compiler is deterministic by construction.

The content hash serves three roles simultaneously. It is the
artifact's identity in the content-addressed store. It is the Card's
`metadata_hash` in Atlas. And it is the verifier's `program_digest`
when checking proofs. One hash, three roles, zero ambiguity. When a
verifier checks a proof against a `program_digest`, they are
implicitly confirming that the proof corresponds to the exact artifact
stored under that hash in Atlas.

Verification certificates take this further. A STARK proof that the
compiler correctly compiled the source to the artifact can be stored
as part of the package metadata. The certificate proves a specific
claim: "source with hash X, when compiled, produces artifact with hash
Y." Anyone can verify this claim without re-compiling, without trusting
the publisher's toolchain, without trusting anything except mathematics.

The endgame is provable compilation. The Trident compiler self-hosts on
Triton VM. Every compilation produces a STARK proof. Every Atlas package
comes with a mathematical guarantee that it was compiled correctly --
source to assembly, each transformation proven, chained into a single
certificate. Trust becomes optional because verification is cheap.

---

## Comparison with Existing Registries

| Property | npm / crates.io | Solana Programs | Atlas |
|----------|-----------------|-----------------|-------|
| Hosting | Centralized servers | On-chain bytecode | On-chain Cards |
| Identity | Name + version string | Program address | Content hash |
| Ownership | Account credential | Upgrade authority | TSP-2 Card ownership |
| Versioning | Semver strings | Program replacement | Metadata update |
| Governance | Company policy | Permissionless | Per-OS configurable |
| Provenance | Git history | Deploy transaction | Creator immutability |
| Verification | None | None | STARK compilation proof |
| Offline access | npm cache | No | Three-tier resolution |

Atlas packages are first-class on-chain assets with all the properties
that entails: permissionless access, immutable provenance, programmable
governance, and cryptographic verification. Traditional registries
provide some of these properties through operational practices. Atlas
provides them through mathematical guarantees.

---

## What Ships Today vs. What Is Designed

**Today (Milestone 3, complete).** An HTTP registry with a
content-addressed store. The `trident registry` commands support
publish, pull, and search. Local and remote codebase stores work
end-to-end. Tag-based discovery and type signature search help
developers find packages. Compilation and verification results are
cached by content hash. The foundation is solid and usable.

**Next (Milestone 5, target 0.2).** On-chain TSP-2 Card minting per
OS. Atlas TOML configuration in each OS's `target.toml`. Three-tier
resolution integrated into the compiler's module resolver. Neptune
as the first OS with a live Atlas collection. Publishing a package
mints a Card. Updating a package changes the Card's metadata. The
HTTP registry continues to operate as a convenience layer and
caching tier.

**Future.** Verification certificates stored alongside artifacts --
every package provably compiled. Cross-OS discovery protocol enabling
search across multiple Atlas instances. Governance hooks for
publication policies -- community-defined quality gates enforced at
the proof level.

The path from HTTP registry to on-chain Atlas is incremental. The
content-addressed store, the hashing pipeline, and the publish/pull
workflow already exist. The on-chain layer adds permanence and
programmable governance on top of a working foundation.

---

## See Also

- [Atlas Reference](../../reference/atlas.md) -- complete specification
- [PLUMB Framework](../../reference/plumb.md) -- shared token framework
- [TSP-2 -- Card Standard](../../reference/tsp2-card.md) -- underlying asset standard
- [Content-Addressed Code](content-addressing.md) -- hashing foundation
- [Skill Library](skill-library.md) -- 23 composable token skills
- [Deploying a Program](../guides/deploying-a-program.md) -- deployment pipeline
- [The Gold Standard](gold-standard.md) -- PLUMB, TSP-1, TSP-2 design
