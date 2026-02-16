# Trident Benchmarks

Per-function compiler overhead analysis: compiles real library modules
from `std/` and `vm/`, then compares instruction counts against
hand-optimized TASM baselines.

## Directory Structure

```
benches/
  std/
    crypto/
      auth.baseline.tasm       # vs std/crypto/auth.tri
      bigint.baseline.tasm     # vs std/crypto/bigint.tri
      ecdsa.baseline.tasm      # vs std/crypto/ecdsa.tri
      keccak256.baseline.tasm  # vs std/crypto/keccak256.tri
      merkle.baseline.tasm     # vs std/crypto/merkle.tri
      poseidon.baseline.tasm   # vs std/crypto/poseidon.tri
      poseidon2.baseline.tasm  # vs std/crypto/poseidon2.tri
```

The directory tree mirrors the source tree. Each `.baseline.tasm` file
contains hand-optimized TASM for every public function in the
corresponding `.tri` module. No synthetic benchmark programs exist here
-- all compilation targets are real library code.

## How It Works

1. `trident bench` scans `benches/` recursively for `.baseline.tasm` files
2. Each baseline maps to a source module by path:
   `benches/std/crypto/auth.baseline.tasm` -> `std/crypto/auth.tri`
3. The source module is compiled through the full pipeline (resolve, parse,
   typecheck, TIR, optimize, lower) without linking
4. Both compiled output and baseline are parsed into per-function
   instruction maps
5. Functions are matched by label name and instruction counts compared

## Metrics

| Column | Meaning |
|--------|---------|
| Tri    | Compiler-generated instruction count |
| Hand   | Hand-optimized baseline instruction count |
| Ratio  | Tri / Hand (1.00x = compiler matches expert) |

## Running

```nu
trident bench              # from project root
trident bench benches/     # explicit directory
```

Works from any subdirectory -- walks up to find `benches/`.

## Adding a Baseline

1. Write the `.tri` module in `std/` or `vm/` (real library code)
2. Create the matching baseline path in `benches/`:
   `benches/std/crypto/newmod.baseline.tasm`
3. Write hand-optimized TASM with `__funcname:` labels matching the
   module's public functions
4. Run `trident bench` to see the comparison

## Baseline Format

```tasm
// Hand-optimized TASM baseline: std.crypto.example

__function_name:
    instruction1
    instruction2
    return

__another_function:
    instruction1
    return
```

Rules:
- Labels use `__funcname:` format (matching compiler output)
- Comments (`//`) are not counted
- Labels (ending with `:`) are not counted
- `halt` is not counted
- Blank lines are not counted
- Everything else is counted
