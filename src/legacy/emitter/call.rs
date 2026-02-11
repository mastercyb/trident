use super::Emitter;
use crate::ast::*;
use crate::span::Spanned;
use crate::typecheck::MonoInstance;

impl Emitter {
    pub(super) fn emit_call(
        &mut self,
        name: &str,
        generic_args: &[Spanned<ArraySize>],
        args: &[Spanned<Expr>],
    ) {
        // Evaluate arguments — each pushes a temp
        for arg in args {
            self.emit_expr(&arg.node);
        }

        // Pop all arg temps from the model
        let arg_count = args.len();
        for _ in 0..arg_count {
            self.stack.pop();
        }

        // Resolve intrinsic name: check if this function has an #[intrinsic] mapping.
        // For cross-module calls like "std_hash.tip5", extract the short name "tip5".
        let resolved_name = self.intrinsic_map.get(name).cloned().or_else(|| {
            // Cross-module: "module.func" → look up "func"
            name.rsplit('.')
                .next()
                .and_then(|short| self.intrinsic_map.get(short).cloned())
        });
        let effective_name = resolved_name.as_deref().unwrap_or(name);

        // Emit the instruction and push result temp
        match effective_name {
            // I/O
            "pub_read" => {
                let s = self.backend.inst_read_io(1);
                self.emit_and_push(&s, 1);
            }
            "pub_read2" => {
                let s = self.backend.inst_read_io(2);
                self.emit_and_push(&s, 2);
            }
            "pub_read3" => {
                let s = self.backend.inst_read_io(3);
                self.emit_and_push(&s, 3);
            }
            "pub_read4" => {
                let s = self.backend.inst_read_io(4);
                self.emit_and_push(&s, 4);
            }
            "pub_read5" => {
                let s = self.backend.inst_read_io(5);
                self.emit_and_push(&s, 5);
            }
            "pub_write" => {
                self.b_write_io(1);
                self.push_temp(0);
            }
            "pub_write2" => {
                self.b_write_io(2);
                self.push_temp(0);
            }
            "pub_write3" => {
                self.b_write_io(3);
                self.push_temp(0);
            }
            "pub_write4" => {
                self.b_write_io(4);
                self.push_temp(0);
            }
            "pub_write5" => {
                self.b_write_io(5);
                self.push_temp(0);
            }

            // Non-deterministic input
            "divine" => {
                let s = self.backend.inst_divine(1);
                self.emit_and_push(&s, 1);
            }
            "divine3" => {
                let s = self.backend.inst_divine(3);
                self.emit_and_push(&s, 3);
            }
            "divine5" => {
                let s = self.backend.inst_divine(5);
                self.emit_and_push(&s, 5);
            }

            // Assertions — consume arg, produce nothing
            "assert" => {
                self.b_assert();
                self.push_temp(0);
            }
            "assert_eq" => {
                self.b_eq();
                self.b_assert();
                self.push_temp(0);
            }
            "assert_digest" => {
                self.b_assert_vector();
                self.b_pop(5);
                self.push_temp(0);
            }

            // Field operations
            "field_add" => {
                self.b_add();
                self.push_temp(1);
            }
            "field_mul" => {
                self.b_mul();
                self.push_temp(1);
            }
            "inv" => {
                self.b_invert();
                self.push_temp(1);
            }
            "neg" => {
                self.b_push_neg_one();
                self.b_mul();
                self.push_temp(1);
            }
            "sub" => {
                self.b_push_neg_one();
                self.b_mul();
                self.b_add();
                self.push_temp(1);
            }

            // U32 operations
            "split" => {
                self.b_split();
                self.push_temp(2);
            }
            "log2" => {
                self.b_log2();
                self.push_temp(1);
            }
            "pow" => {
                self.b_pow();
                self.push_temp(1);
            }
            "popcount" => {
                self.b_pop_count();
                self.push_temp(1);
            }

            // Hash operations
            "hash" => {
                self.b_hash();
                self.push_temp(5);
            }
            "sponge_init" => {
                self.b_sponge_init();
                self.push_temp(0);
            }
            "sponge_absorb" => {
                self.b_sponge_absorb();
                self.push_temp(0);
            }
            "sponge_squeeze" => {
                let s = self.backend.inst_sponge_squeeze().to_string();
                self.emit_and_push(&s, 10);
            }
            "sponge_absorb_mem" => {
                self.b_sponge_absorb_mem();
                self.push_temp(0);
            }

            // Merkle
            "merkle_step" => {
                let s = self.backend.inst_merkle_step().to_string();
                self.emit_and_push(&s, 6);
            }
            "merkle_step_mem" => {
                let s = self.backend.inst_merkle_step_mem().to_string();
                self.emit_and_push(&s, 7);
            }

            // RAM
            "ram_read" => {
                self.b_read_mem(1);
                self.b_pop(1);
                self.push_temp(1);
            }
            "ram_write" => {
                self.b_write_mem(1);
                self.b_pop(1);
                self.push_temp(0);
            }
            "ram_read_block" => {
                // Read 5 consecutive elements (Digest-sized block)
                self.b_read_mem(5);
                self.b_pop(1);
                self.push_temp(5);
            }
            "ram_write_block" => {
                // Write 5 consecutive elements (Digest-sized block)
                self.b_write_mem(5);
                self.b_pop(1);
                self.push_temp(0);
            }

            // Conversion
            "as_u32" => {
                self.b_split();
                self.b_pop(1);
                self.push_temp(1);
            }
            "as_field" => {
                self.push_temp(1);
            }

            // XField
            "xfield" => {
                self.push_temp(3);
            }
            "xinvert" => {
                self.b_x_invert();
                self.push_temp(3);
            }
            "xx_dot_step" => {
                let s = self.backend.inst_xx_dot_step().to_string();
                self.emit_and_push(&s, 5);
            }
            "xb_dot_step" => {
                let s = self.backend.inst_xb_dot_step().to_string();
                self.emit_and_push(&s, 5);
            }

            // User-defined function
            _ => {
                // Check if this is a generic function call.
                let is_generic = self.generic_fn_defs.contains_key(name);

                let (call_label, base_name) = if is_generic {
                    // Resolve size args: explicit from call site, current_subs
                    // for calls inside generic bodies, or call_resolutions
                    // from the type checker for inferred calls.
                    let size_args: Vec<u64> = if !generic_args.is_empty() {
                        generic_args
                            .iter()
                            .map(|ga| ga.node.eval(&self.current_subs))
                            .collect()
                    } else if !self.current_subs.is_empty() {
                        // Inside a monomorphized body: resolve through current_subs.
                        if let Some(gdef) = self.generic_fn_defs.get(name) {
                            gdef.type_params
                                .iter()
                                .map(|p| self.current_subs.get(&p.node).copied().unwrap_or(0))
                                .collect()
                        } else {
                            vec![]
                        }
                    } else {
                        // Inferred call: consume from call_resolutions.
                        let idx = self.call_resolution_idx;
                        if idx < self.call_resolutions.len()
                            && self.call_resolutions[idx].name == name
                        {
                            self.call_resolution_idx += 1;
                            self.call_resolutions[idx].size_args.clone()
                        } else {
                            // Fallback: search for a matching resolution.
                            let mut found = vec![];
                            for (i, res) in self.call_resolutions.iter().enumerate() {
                                if i >= self.call_resolution_idx && res.name == name {
                                    self.call_resolution_idx = i + 1;
                                    found = res.size_args.clone();
                                    break;
                                }
                            }
                            found
                        }
                    };
                    let inst = MonoInstance {
                        name: name.to_string(),
                        size_args,
                    };
                    let base = inst.mangled_name();
                    let label = self.backend.format_label(&base);
                    (label, base)
                } else if name.contains('.') {
                    // Cross-module call: "merkle.verify" → "call __merkle__verify"
                    let parts: Vec<&str> = name.rsplitn(2, '.').collect();
                    let fn_name = parts[0];
                    let short_module = parts[1];
                    let full_module = self
                        .module_aliases
                        .get(short_module)
                        .map(|s| s.as_str())
                        .unwrap_or(short_module);
                    let mangled = full_module.replace('.', "_");
                    let base = format!("{}__{}", mangled, fn_name);
                    let label = self.backend.format_label(&base);
                    (label, fn_name.to_string())
                } else {
                    let label = self.backend.format_label(name);
                    (label, name.to_string())
                };
                let ret_width = self.fn_return_widths.get(&base_name).copied().unwrap_or(0);
                let call_inst = self.backend.inst_call(&call_label);
                if ret_width > 0 {
                    self.emit_and_push(&call_inst, ret_width);
                } else {
                    // Void function — emit call but don't push a stack entry
                    self.b_call(&call_label);
                    self.push_temp(0);
                }
            }
        }
    }
}
