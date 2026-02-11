use super::Emitter;

impl Emitter {
    // ── Backend-delegating instruction helpers ──────────────────────
    pub(super) fn b_push(&mut self, value: u64) {
        let s = self.backend.inst_push(value);
        self.inst(&s);
    }
    pub(super) fn b_pop(&mut self, count: u32) {
        let s = self.backend.inst_pop(count);
        self.inst(&s);
    }
    pub(super) fn b_dup(&mut self, depth: u32) {
        let s = self.backend.inst_dup(depth);
        self.inst(&s);
    }
    pub(super) fn b_swap(&mut self, depth: u32) {
        let s = self.backend.inst_swap(depth);
        self.inst(&s);
    }
    pub(super) fn b_push_neg_one(&mut self) {
        self.inst(self.backend.inst_push_neg_one());
    }
    pub(super) fn b_add(&mut self) {
        self.inst(self.backend.inst_add());
    }
    pub(super) fn b_mul(&mut self) {
        self.inst(self.backend.inst_mul());
    }
    pub(super) fn b_eq(&mut self) {
        self.inst(self.backend.inst_eq());
    }
    pub(super) fn b_lt(&mut self) {
        self.inst(self.backend.inst_lt());
    }
    pub(super) fn b_and(&mut self) {
        self.inst(self.backend.inst_and());
    }
    pub(super) fn b_xor(&mut self) {
        self.inst(self.backend.inst_xor());
    }
    pub(super) fn b_div_mod(&mut self) {
        self.inst(self.backend.inst_div_mod());
    }
    pub(super) fn b_xb_mul(&mut self) {
        self.inst(self.backend.inst_xb_mul());
    }
    pub(super) fn b_invert(&mut self) {
        self.inst(self.backend.inst_invert());
    }
    pub(super) fn b_x_invert(&mut self) {
        self.inst(self.backend.inst_x_invert());
    }
    pub(super) fn b_split(&mut self) {
        self.inst(self.backend.inst_split());
    }
    pub(super) fn b_log2(&mut self) {
        self.inst(self.backend.inst_log2());
    }
    pub(super) fn b_pow(&mut self) {
        self.inst(self.backend.inst_pow());
    }
    pub(super) fn b_pop_count(&mut self) {
        self.inst(self.backend.inst_pop_count());
    }
    pub(super) fn b_assert(&mut self) {
        self.inst(self.backend.inst_assert());
    }
    pub(super) fn b_assert_vector(&mut self) {
        self.inst(self.backend.inst_assert_vector());
    }
    pub(super) fn b_hash(&mut self) {
        self.inst(self.backend.inst_hash());
    }
    pub(super) fn b_sponge_init(&mut self) {
        self.inst(self.backend.inst_sponge_init());
    }
    pub(super) fn b_sponge_absorb(&mut self) {
        self.inst(self.backend.inst_sponge_absorb());
    }
    #[allow(dead_code)]
    pub(super) fn b_sponge_squeeze(&mut self) {
        self.inst(self.backend.inst_sponge_squeeze());
    }
    pub(super) fn b_sponge_absorb_mem(&mut self) {
        self.inst(self.backend.inst_sponge_absorb_mem());
    }
    #[allow(dead_code)]
    pub(super) fn b_merkle_step(&mut self) {
        self.inst(self.backend.inst_merkle_step());
    }
    #[allow(dead_code)]
    pub(super) fn b_merkle_step_mem(&mut self) {
        self.inst(self.backend.inst_merkle_step_mem());
    }
    pub(super) fn b_call(&mut self, label: &str) {
        let s = self.backend.inst_call(label);
        self.inst(&s);
    }
    #[allow(dead_code)]
    pub(super) fn b_read_io(&mut self, count: u32) {
        let s = self.backend.inst_read_io(count);
        self.inst(&s);
    }
    pub(super) fn b_write_io(&mut self, count: u32) {
        let s = self.backend.inst_write_io(count);
        self.inst(&s);
    }
    #[allow(dead_code)]
    pub(super) fn b_divine(&mut self, count: u32) {
        let s = self.backend.inst_divine(count);
        self.inst(&s);
    }
    pub(super) fn b_read_mem(&mut self, count: u32) {
        let s = self.backend.inst_read_mem(count);
        self.inst(&s);
    }
    pub(super) fn b_write_mem(&mut self, count: u32) {
        let s = self.backend.inst_write_mem(count);
        self.inst(&s);
    }
    #[allow(dead_code)]
    pub(super) fn b_xx_dot_step(&mut self) {
        self.inst(self.backend.inst_xx_dot_step());
    }
    #[allow(dead_code)]
    pub(super) fn b_xb_dot_step(&mut self) {
        self.inst(self.backend.inst_xb_dot_step());
    }
}
