use super::StackBackend;

pub(crate) struct TritonBackend;

impl StackBackend for TritonBackend {
    fn target_name(&self) -> &str {
        "triton"
    }
    fn output_extension(&self) -> &str {
        ".tasm"
    }

    fn inst_push(&self, value: u64) -> String {
        format!("push {}", value)
    }
    fn inst_pop(&self, count: u32) -> String {
        format!("pop {}", count)
    }
    fn inst_dup(&self, depth: u32) -> String {
        format!("dup {}", depth)
    }
    fn inst_swap(&self, depth: u32) -> String {
        format!("swap {}", depth)
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
        "invert"
    }
    fn inst_split(&self) -> &'static str {
        "split"
    }
    fn inst_lt(&self) -> &'static str {
        "lt"
    }
    fn inst_and(&self) -> &'static str {
        "and"
    }
    fn inst_xor(&self) -> &'static str {
        "xor"
    }
    fn inst_div_mod(&self) -> &'static str {
        "div_mod"
    }
    fn inst_log2(&self) -> &'static str {
        "log_2_floor"
    }
    fn inst_pow(&self) -> &'static str {
        "pow"
    }
    fn inst_pop_count(&self) -> &'static str {
        "pop_count"
    }
    fn inst_xb_mul(&self) -> &'static str {
        "xb_mul"
    }
    fn inst_x_invert(&self) -> &'static str {
        "x_invert"
    }

    fn inst_read_io(&self, count: u32) -> String {
        format!("read_io {}", count)
    }
    fn inst_write_io(&self, count: u32) -> String {
        format!("write_io {}", count)
    }
    fn inst_divine(&self, count: u32) -> String {
        format!("divine {}", count)
    }

    fn inst_read_mem(&self, count: u32) -> String {
        format!("read_mem {}", count)
    }
    fn inst_write_mem(&self, count: u32) -> String {
        format!("write_mem {}", count)
    }

    fn inst_hash(&self) -> &'static str {
        "hash"
    }
    fn inst_sponge_init(&self) -> &'static str {
        "sponge_init"
    }
    fn inst_sponge_absorb(&self) -> &'static str {
        "sponge_absorb"
    }
    fn inst_sponge_squeeze(&self) -> &'static str {
        "sponge_squeeze"
    }
    fn inst_sponge_absorb_mem(&self) -> &'static str {
        "sponge_absorb_mem"
    }

    fn inst_merkle_step(&self) -> &'static str {
        "merkle_step"
    }
    fn inst_merkle_step_mem(&self) -> &'static str {
        "merkle_step_mem"
    }

    fn inst_assert(&self) -> &'static str {
        "assert"
    }
    fn inst_assert_vector(&self) -> &'static str {
        "assert_vector"
    }
    fn inst_skiz(&self) -> &'static str {
        "skiz"
    }
    fn inst_call(&self, label: &str) -> String {
        format!("call {}", label)
    }
    fn inst_return(&self) -> &'static str {
        "return"
    }
    fn inst_recurse(&self) -> &'static str {
        "recurse"
    }
    fn inst_halt(&self) -> &'static str {
        "halt"
    }
}

