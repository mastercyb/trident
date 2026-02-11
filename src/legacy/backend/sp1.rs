use super::StackBackend;

// ─── SP1 Backend (RISC-V) ─────────────────────────────────────────

/// SP1 backend — emits RISC-V assembly for the Succinct SP1 zkVM.
/// Similar to OpenVM but with different syscall conventions (Plonky3 prover).
pub(crate) struct SP1Backend;

impl StackBackend for SP1Backend {
    fn target_name(&self) -> &str {
        "sp1"
    }
    fn output_extension(&self) -> &str {
        ".S"
    }

    fn inst_push(&self, value: u64) -> String {
        format!("li t0, {}", value)
    }
    fn inst_pop(&self, _count: u32) -> String {
        "# pop (register)".to_string()
    }
    fn inst_dup(&self, _depth: u32) -> String {
        "mv t1, t0".to_string()
    }
    fn inst_swap(&self, _depth: u32) -> String {
        "# swap (register rename)".to_string()
    }

    fn inst_add(&self) -> &'static str {
        "add t0, t0, t1"
    }
    fn inst_mul(&self) -> &'static str {
        "mul t0, t0, t1"
    }
    fn inst_eq(&self) -> &'static str {
        "xor t0, t0, t1\nseqz t0, t0"
    }
    fn inst_invert(&self) -> &'static str {
        "# field_inv (syscall)"
    }
    fn inst_split(&self) -> &'static str {
        "# u32_split"
    }
    fn inst_lt(&self) -> &'static str {
        "sltu t0, t0, t1"
    }
    fn inst_and(&self) -> &'static str {
        "and t0, t0, t1"
    }
    fn inst_xor(&self) -> &'static str {
        "xor t0, t0, t1"
    }
    fn inst_div_mod(&self) -> &'static str {
        "divu t2, t0, t1\nremu t3, t0, t1"
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
        "# xb_mul (N/A)"
    }
    fn inst_x_invert(&self) -> &'static str {
        "# x_invert (N/A)"
    }

    fn inst_read_io(&self, count: u32) -> String {
        format!("# sp1_read_io {} (hint channel)", count)
    }
    fn inst_write_io(&self, count: u32) -> String {
        format!("# sp1_write_io {} (commit channel)", count)
    }
    fn inst_divine(&self, count: u32) -> String {
        format!("# sp1_hint {} (hint precompile)", count)
    }

    fn inst_read_mem(&self, count: u32) -> String {
        format!("lw t0, 0(a0)  # read {}", count)
    }
    fn inst_write_mem(&self, count: u32) -> String {
        format!("sw t0, 0(a0)  # write {}", count)
    }

    fn inst_hash(&self) -> &'static str {
        "# hash (Poseidon2 precompile)"
    }
    fn inst_sponge_init(&self) -> &'static str {
        "# sponge_init"
    }
    fn inst_sponge_absorb(&self) -> &'static str {
        "# sponge_absorb"
    }
    fn inst_sponge_squeeze(&self) -> &'static str {
        "# sponge_squeeze"
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
        "bnez t0, .ok  # assert"
    }
    fn inst_assert_vector(&self) -> &'static str {
        "# assert_vector"
    }
    fn inst_skiz(&self) -> &'static str {
        "beqz t0, .skip"
    }
    fn inst_call(&self, label: &str) -> String {
        format!("jal ra, {}", label)
    }
    fn inst_return(&self) -> &'static str {
        "ret"
    }
    fn inst_recurse(&self) -> &'static str {
        "jal ra, ."
    }
    fn inst_halt(&self) -> &'static str {
        "ecall  # halt"
    }
    fn inst_push_neg_one(&self) -> &'static str {
        "li t0, -1  # field neg one"
    }
}

