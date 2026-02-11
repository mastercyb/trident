use super::StackBackend;

// ─── OpenVM Backend (RISC-V) ──────────────────────────────────────

/// OpenVM backend — emits RISC-V assembly for the OpenVM zkVM.
/// Register-based architecture. Field operations via custom instructions.
pub(crate) struct OpenVMBackend;

impl StackBackend for OpenVMBackend {
    fn target_name(&self) -> &str {
        "openvm"
    }
    fn output_extension(&self) -> &str {
        ".S"
    }

    fn inst_push(&self, value: u64) -> String {
        format!("li t0, {}", value)
    }
    fn inst_pop(&self, _count: u32) -> String {
        "# pop (register machine: noop)".to_string()
    }
    fn inst_dup(&self, _depth: u32) -> String {
        "mv t1, t0  # dup".to_string()
    }
    fn inst_swap(&self, _depth: u32) -> String {
        "# swap (register machine: register rename)".to_string()
    }

    fn inst_add(&self) -> &'static str {
        "add t0, t0, t1  # field_add"
    }
    fn inst_mul(&self) -> &'static str {
        "mul t0, t0, t1  # field_mul"
    }
    fn inst_eq(&self) -> &'static str {
        "beq t0, t1, .eq_true  # field_eq"
    }
    fn inst_invert(&self) -> &'static str {
        "# field_inv (custom instruction)"
    }
    fn inst_split(&self) -> &'static str {
        "# u32_split (shift+mask)"
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
        "divu t0, t0, t1\nremu t1, t0, t1"
    }
    fn inst_log2(&self) -> &'static str {
        "# log2 (software)"
    }
    fn inst_pow(&self) -> &'static str {
        "# pow (software loop)"
    }
    fn inst_pop_count(&self) -> &'static str {
        "# popcount (software)"
    }
    fn inst_xb_mul(&self) -> &'static str {
        "# xb_mul (software)"
    }
    fn inst_x_invert(&self) -> &'static str {
        "# x_invert (software)"
    }

    fn inst_read_io(&self, count: u32) -> String {
        format!("# read_io {} (ecall)", count)
    }
    fn inst_write_io(&self, count: u32) -> String {
        format!("# write_io {} (ecall)", count)
    }
    fn inst_divine(&self, count: u32) -> String {
        format!("# divine {} (hint read)", count)
    }

    fn inst_read_mem(&self, count: u32) -> String {
        format!("lw t0, 0(a0)  # read_mem {}", count)
    }
    fn inst_write_mem(&self, count: u32) -> String {
        format!("sw t0, 0(a0)  # write_mem {}", count)
    }

    fn inst_hash(&self) -> &'static str {
        "# hash (Poseidon2 syscall)"
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
        "bnez t0, .assert_ok  # assert"
    }
    fn inst_assert_vector(&self) -> &'static str {
        "# assert_vector"
    }
    fn inst_skiz(&self) -> &'static str {
        "beqz t0, .skip  # skiz"
    }
    fn inst_call(&self, label: &str) -> String {
        format!("jal ra, {}", label)
    }
    fn inst_return(&self) -> &'static str {
        "jalr zero, ra, 0  # ret"
    }
    fn inst_recurse(&self) -> &'static str {
        "jal ra, .  # recurse"
    }
    fn inst_halt(&self) -> &'static str {
        "ebreak  # halt"
    }
    fn inst_push_neg_one(&self) -> &'static str {
        "li t0, -1  # field neg one"
    }
}

