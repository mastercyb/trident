use super::StackBackend;

// ─── Miden VM Backend ──────────────────────────────────────────────

/// Miden VM backend — emits Miden Assembly (MASM).
/// Stack-based architecture with Rescue-Prime hash, 64-bit Goldilocks field.
pub(crate) struct MidenBackend;

impl StackBackend for MidenBackend {
    fn target_name(&self) -> &str {
        "miden"
    }
    fn output_extension(&self) -> &str {
        ".masm"
    }

    fn inst_push(&self, value: u64) -> String {
        format!("push.{}", value)
    }
    fn inst_pop(&self, count: u32) -> String {
        if count == 1 {
            "drop".to_string()
        } else {
            format!("dropw  # drop {}", count)
        }
    }
    fn inst_dup(&self, depth: u32) -> String {
        format!("dup.{}", depth)
    }
    fn inst_swap(&self, depth: u32) -> String {
        if depth == 1 {
            "swap".to_string()
        } else {
            format!("movup.{}", depth)
        }
    }

    fn inst_add(&self) -> &'static str {
        "add"
    }
    fn inst_mul(&self) -> &'static str {
        "mul"
    }
    fn inst_eq(&self) -> &'static str {
        "eq"
    }
    fn inst_invert(&self) -> &'static str {
        "inv"
    }
    fn inst_split(&self) -> &'static str {
        "u32split"
    }
    fn inst_lt(&self) -> &'static str {
        "u32lt"
    }
    fn inst_and(&self) -> &'static str {
        "u32and"
    }
    fn inst_xor(&self) -> &'static str {
        "u32xor"
    }
    fn inst_div_mod(&self) -> &'static str {
        "u32divmod"
    }
    fn inst_log2(&self) -> &'static str {
        "ilog2"
    }
    fn inst_pow(&self) -> &'static str {
        "exp"
    }
    fn inst_pop_count(&self) -> &'static str {
        "u32popcnt"
    }
    fn inst_xb_mul(&self) -> &'static str {
        "# xb_mul (unsupported on miden)"
    }
    fn inst_x_invert(&self) -> &'static str {
        "# x_invert (unsupported on miden)"
    }

    fn inst_read_io(&self, count: u32) -> String {
        (0..count)
            .map(|_| "sdepth  # read_io placeholder")
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn inst_write_io(&self, count: u32) -> String {
        (0..count)
            .map(|_| "drop  # write_io placeholder")
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn inst_divine(&self, count: u32) -> String {
        (0..count)
            .map(|_| "adv_push.1")
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn inst_read_mem(&self, count: u32) -> String {
        format!("mem_load  # read {}", count)
    }
    fn inst_write_mem(&self, count: u32) -> String {
        format!("mem_store  # write {}", count)
    }

    fn inst_hash(&self) -> &'static str {
        "hperm"
    }
    fn inst_sponge_init(&self) -> &'static str {
        "# sponge_init (use hperm sequence)"
    }
    fn inst_sponge_absorb(&self) -> &'static str {
        "hperm  # absorb"
    }
    fn inst_sponge_squeeze(&self) -> &'static str {
        "hperm  # squeeze"
    }
    fn inst_sponge_absorb_mem(&self) -> &'static str {
        "# sponge_absorb_mem (Miden: custom)"
    }

    fn inst_merkle_step(&self) -> &'static str {
        "mtree_get  # merkle_step"
    }
    fn inst_merkle_step_mem(&self) -> &'static str {
        "mtree_get  # merkle_step_mem"
    }

    fn inst_assert(&self) -> &'static str {
        "assert"
    }
    fn inst_assert_vector(&self) -> &'static str {
        "assert  # assert_vector (4 words)"
    }
    fn inst_skiz(&self) -> &'static str {
        "if.true"
    }
    fn inst_call(&self, label: &str) -> String {
        format!("exec.{}", label)
    }
    fn inst_return(&self) -> &'static str {
        "end"
    }
    fn inst_recurse(&self) -> &'static str {
        "exec.self  # recurse"
    }
    fn inst_halt(&self) -> &'static str {
        "end  # halt"
    }
    fn inst_push_neg_one(&self) -> &'static str {
        "push.18446744069414584320" // Goldilocks p - 1
    }
}

