//! Function call dispatch: intrinsic resolution and user-defined calls.

use crate::ast::*;
use crate::span::Spanned;
use crate::tir::TIROp;
use crate::typecheck::MonoInstance;

use super::TIRBuilder;

impl TIRBuilder {
    /// Emit a function call (intrinsic or user-defined).
    pub(crate) fn build_call(
        &mut self,
        name: &str,
        generic_args: &[Spanned<ArraySize>],
        args: &[Spanned<Expr>],
    ) {
        // Evaluate arguments — each pushes a temp.
        for arg in args {
            self.build_expr(&arg.node);
        }

        // Pop all arg temps from the model.
        let arg_count = args.len();
        for _ in 0..arg_count {
            self.stack.pop();
        }

        // Resolve intrinsic name.
        let resolved_name = self.intrinsic_map.get(name).cloned().or_else(|| {
            name.rsplit('.')
                .next()
                .and_then(|short| self.intrinsic_map.get(short).cloned())
        });
        let effective_name = resolved_name.as_deref().unwrap_or(name);

        match effective_name {
            // ── I/O ──
            "pub_read" => {
                self.emit_and_push(TIROp::ReadIo(1), 1);
            }
            "pub_read2" => {
                self.emit_and_push(TIROp::ReadIo(2), 2);
            }
            "pub_read3" => {
                self.emit_and_push(TIROp::ReadIo(3), 3);
            }
            "pub_read4" => {
                self.emit_and_push(TIROp::ReadIo(4), 4);
            }
            "pub_read5" => {
                self.emit_and_push(TIROp::ReadIo(5), 5);
            }
            "pub_write" => {
                self.ops.push(TIROp::WriteIo(1));
                self.push_temp(0);
            }
            "pub_write2" => {
                self.ops.push(TIROp::WriteIo(2));
                self.push_temp(0);
            }
            "pub_write3" => {
                self.ops.push(TIROp::WriteIo(3));
                self.push_temp(0);
            }
            "pub_write4" => {
                self.ops.push(TIROp::WriteIo(4));
                self.push_temp(0);
            }
            "pub_write5" => {
                self.ops.push(TIROp::WriteIo(5));
                self.push_temp(0);
            }

            // ── Non-deterministic input ──
            "divine" => {
                self.emit_and_push(TIROp::Hint(1), 1);
            }
            "divine3" => {
                self.emit_and_push(TIROp::Hint(3), 3);
            }
            "divine5" => {
                self.emit_and_push(TIROp::Hint(5), 5);
            }

            // ── Assertions ──
            "assert" => {
                self.ops.push(TIROp::Assert(1));
                self.push_temp(0);
            }
            "assert_eq" => {
                self.ops.push(TIROp::Eq);
                self.ops.push(TIROp::Assert(1));
                self.push_temp(0);
            }
            "assert_digest" => {
                self.ops.push(TIROp::Assert(5));
                self.ops.push(TIROp::Pop(self.target_config.digest_width));
                self.push_temp(0);
            }

            // ── Field operations ──
            "field_add" => {
                self.ops.push(TIROp::Add);
                self.push_temp(1);
            }
            "field_mul" => {
                self.ops.push(TIROp::Mul);
                self.push_temp(1);
            }
            "inv" => {
                self.ops.push(TIROp::Invert);
                self.push_temp(1);
            }
            "neg" => {
                self.ops.push(TIROp::Neg);
                self.push_temp(1);
            }
            "sub" => {
                self.ops.push(TIROp::Sub);
                self.push_temp(1);
            }

            // ── U32 operations ──
            "split" => {
                self.ops.push(TIROp::Split);
                self.push_temp(2);
            }
            "log2" => {
                self.ops.push(TIROp::Log2);
                self.push_temp(1);
            }
            "pow" => {
                self.ops.push(TIROp::Pow);
                self.push_temp(1);
            }
            "popcount" => {
                self.ops.push(TIROp::PopCount);
                self.push_temp(1);
            }

            // ── Hash operations ──
            "hash" => {
                self.ops.push(TIROp::Hash {
                    width: self.target_config.digest_width,
                });
                self.push_temp(self.target_config.digest_width);
            }
            "sponge_init" => {
                self.ops.push(TIROp::SpongeInit);
                self.push_temp(0);
            }
            "sponge_absorb" => {
                self.ops.push(TIROp::SpongeAbsorb);
                self.push_temp(0);
            }
            "sponge_squeeze" => {
                self.emit_and_push(TIROp::SpongeSqueeze, self.target_config.hash_rate);
            }
            "sponge_absorb_mem" => {
                self.ops.push(TIROp::SpongeLoad);
                self.push_temp(0);
            }

            // ── Merkle ──
            "merkle_step" => {
                self.emit_and_push(TIROp::MerkleStep, 6);
            }
            "merkle_step_mem" => {
                self.emit_and_push(TIROp::MerkleLoad, 7);
            }

            // ── RAM ──
            "ram_read" => {
                self.ops.push(TIROp::ReadStorage { width: 1 });
                self.push_temp(1);
            }
            "ram_write" => {
                self.ops.push(TIROp::WriteStorage { width: 1 });
                self.push_temp(0);
            }
            "ram_read_block" => {
                self.ops.push(TIROp::ReadStorage { width: 5 });
                self.push_temp(5);
            }
            "ram_write_block" => {
                self.ops.push(TIROp::WriteStorage { width: 5 });
                self.push_temp(0);
            }

            // ── Conversion ──
            "as_u32" => {
                self.ops.push(TIROp::Split);
                self.ops.push(TIROp::Pop(1));
                self.push_temp(1);
            }
            "as_field" => {
                self.push_temp(1);
            }

            // ── XField ──
            "xfield" => {
                self.push_temp(3);
            }
            "xinvert" => {
                self.ops.push(TIROp::ExtInvert);
                self.push_temp(3);
            }
            "xx_dot_step" => {
                self.emit_and_push(TIROp::FoldExt, 5);
            }
            "xb_dot_step" => {
                self.emit_and_push(TIROp::FoldBase, 5);
            }

            // ── User-defined function ──
            _ => {
                self.build_user_call(name, generic_args);
            }
        }
    }

    /// Emit a call to a user-defined (non-intrinsic) function.
    fn build_user_call(&mut self, name: &str, generic_args: &[Spanned<ArraySize>]) {
        let is_generic = self.generic_fn_defs.contains_key(name);

        let (call_label, base_name) = if is_generic {
            let size_args: Vec<u64> = if !generic_args.is_empty() {
                generic_args
                    .iter()
                    .map(|ga| ga.node.eval(&self.current_subs))
                    .collect()
            } else if !self.current_subs.is_empty() {
                if let Some(gdef) = self.generic_fn_defs.get(name) {
                    gdef.type_params
                        .iter()
                        .map(|p| self.current_subs.get(&p.node).copied().unwrap_or(0))
                        .collect()
                } else {
                    vec![]
                }
            } else {
                let idx = self.call_resolution_idx;
                if idx < self.call_resolutions.len() && self.call_resolutions[idx].name == name {
                    self.call_resolution_idx += 1;
                    self.call_resolutions[idx].size_args.clone()
                } else {
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
            (base.clone(), base)
        } else if name.contains('.') {
            // Cross-module call.
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
            (base, fn_name.to_string())
        } else {
            (name.to_string(), name.to_string())
        };

        let ret_width = self.fn_return_widths.get(&base_name).copied().unwrap_or(0);
        if ret_width > 0 {
            self.emit_and_push(TIROp::Call(call_label), ret_width);
        } else {
            self.ops.push(TIROp::Call(call_label));
            self.push_temp(0);
        }
    }
}
