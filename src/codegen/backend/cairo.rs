use super::StackBackend;

// ─── Cairo Backend (Sierra) ───────────────────────────────────────

/// Cairo backend — emits Sierra intermediate language for StarkNet/StarkWare.
/// Register-based. 251-bit prime field (different from Goldilocks).
pub(crate) struct CairoBackend;

impl StackBackend for CairoBackend {
    fn target_name(&self) -> &str {
        "cairo"
    }
    fn output_extension(&self) -> &str {
        ".sierra"
    }

    fn inst_push(&self, value: u64) -> String {
        format!("felt252_const<{}>() -> ([0])", value)
    }
    fn inst_pop(&self, _count: u32) -> String {
        "drop([0])".to_string()
    }
    fn inst_dup(&self, _depth: u32) -> String {
        "dup<felt252>([0]) -> ([0], [1])".to_string()
    }
    fn inst_swap(&self, _depth: u32) -> String {
        "# swap (Sierra: rename)".to_string()
    }

    fn inst_add(&self) -> &'static str {
        "felt252_add([0], [1]) -> ([2])"
    }
    fn inst_mul(&self) -> &'static str {
        "felt252_mul([0], [1]) -> ([2])"
    }
    fn inst_eq(&self) -> &'static str {
        "felt252_is_zero([0]) -> { zero([1]) fallthrough([2]) }"
    }
    fn inst_invert(&self) -> &'static str {
        "# felt252_inv (division)"
    }
    fn inst_split(&self) -> &'static str {
        "# u32_split (bitwise)"
    }
    fn inst_lt(&self) -> &'static str {
        "felt252_lt([0], [1]) -> ([2])"
    }
    fn inst_and(&self) -> &'static str {
        "bitwise([0], [1]) -> ([2], [3], [4])"
    }
    fn inst_xor(&self) -> &'static str {
        "# xor (via bitwise)"
    }
    fn inst_div_mod(&self) -> &'static str {
        "# divmod (Sierra: felt252_div)"
    }
    fn inst_log2(&self) -> &'static str {
        "# log2 (software)"
    }
    fn inst_pow(&self) -> &'static str {
        "# pow (software)"
    }
    fn inst_pop_count(&self) -> &'static str {
        "# popcount (software)"
    }
    fn inst_xb_mul(&self) -> &'static str {
        "# xb_mul (N/A on Cairo)"
    }
    fn inst_x_invert(&self) -> &'static str {
        "# x_invert (N/A on Cairo)"
    }

    fn inst_read_io(&self, count: u32) -> String {
        format!("# cairo_read_io {} (program input)", count)
    }
    fn inst_write_io(&self, count: u32) -> String {
        format!("# cairo_write_io {} (program output)", count)
    }
    fn inst_divine(&self, count: u32) -> String {
        format!("# cairo_hint {} (hint mechanism)", count)
    }

    fn inst_read_mem(&self, count: u32) -> String {
        format!("# mem_read {} (alloc + store)", count)
    }
    fn inst_write_mem(&self, count: u32) -> String {
        format!("# mem_write {}", count)
    }

    fn inst_hash(&self) -> &'static str {
        "pedersen([0], [1]) -> ([2])  # hash"
    }
    fn inst_sponge_init(&self) -> &'static str {
        "# sponge_init (Poseidon)"
    }
    fn inst_sponge_absorb(&self) -> &'static str {
        "# sponge_absorb (Poseidon)"
    }
    fn inst_sponge_squeeze(&self) -> &'static str {
        "# sponge_squeeze (Poseidon)"
    }
    fn inst_sponge_absorb_mem(&self) -> &'static str {
        "# sponge_absorb_mem"
    }

    fn inst_merkle_step(&self) -> &'static str {
        "# merkle_step (software)"
    }
    fn inst_merkle_step_mem(&self) -> &'static str {
        "# merkle_step_mem (software)"
    }

    fn inst_assert(&self) -> &'static str {
        "felt252_is_zero([0]) { fallthrough() zero([1]) }"
    }
    fn inst_assert_vector(&self) -> &'static str {
        "# assert_vector"
    }
    fn inst_skiz(&self) -> &'static str {
        "branch_align()"
    }
    fn inst_call(&self, label: &str) -> String {
        format!("function_call<{}>([0]) -> ([1])", label)
    }
    fn inst_return(&self) -> &'static str {
        "return([0])"
    }
    fn inst_recurse(&self) -> &'static str {
        "# recurse (not supported in Sierra)"
    }
    fn inst_halt(&self) -> &'static str {
        "return([0])  // halt"
    }
    fn inst_push_neg_one(&self) -> &'static str {
        "felt252_const<-1>()  // neg one"
    }
}
