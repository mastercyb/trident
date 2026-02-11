use super::{FnSig, TypeChecker};
use crate::types::Ty;

impl TypeChecker {
    pub(super) fn register_builtins(&mut self) {
        let dw = self.target_config.digest_width;
        let hr = self.target_config.hash_rate;
        let fl = self.target_config.field_limbs;
        let xw = self.target_config.xfield_width;
        let digest_ty = Ty::Digest(dw);
        let xfield_ty = Ty::XField(xw);

        let b = &mut self.functions;

        // I/O — parameterized read/write variants up to digest_width
        b.insert(
            "pub_read".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Field,
            },
        );
        for n in 2..dw {
            b.insert(
                format!("pub_read{}", n),
                FnSig {
                    params: vec![],
                    return_ty: Ty::Tuple(vec![Ty::Field; n as usize]),
                },
            );
        }
        b.insert(
            format!("pub_read{}", dw),
            FnSig {
                params: vec![],
                return_ty: digest_ty.clone(),
            },
        );

        b.insert(
            "pub_write".into(),
            FnSig {
                params: vec![("v".into(), Ty::Field)],
                return_ty: Ty::Unit,
            },
        );
        for n in 2..=dw {
            b.insert(
                format!("pub_write{}", n),
                FnSig {
                    params: (0..n).map(|i| (format!("v{}", i), Ty::Field)).collect(),
                    return_ty: Ty::Unit,
                },
            );
        }

        // Non-deterministic input
        b.insert(
            "divine".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Field,
            },
        );
        if xw > 0 {
            b.insert(
                format!("divine{}", xw),
                FnSig {
                    params: vec![],
                    return_ty: Ty::Tuple(vec![Ty::Field; xw as usize]),
                },
            );
        }
        b.insert(
            format!("divine{}", dw),
            FnSig {
                params: vec![],
                return_ty: digest_ty.clone(),
            },
        );

        // Assertions
        b.insert(
            "assert".into(),
            FnSig {
                params: vec![("cond".into(), Ty::Bool)],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "assert_eq".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field), ("b".into(), Ty::Field)],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "assert_digest".into(),
            FnSig {
                params: vec![
                    ("a".into(), digest_ty.clone()),
                    ("b".into(), digest_ty.clone()),
                ],
                return_ty: Ty::Unit,
            },
        );

        // Field operations
        b.insert(
            "field_add".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field), ("b".into(), Ty::Field)],
                return_ty: Ty::Field,
            },
        );
        b.insert(
            "field_mul".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field), ("b".into(), Ty::Field)],
                return_ty: Ty::Field,
            },
        );
        b.insert(
            "inv".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field)],
                return_ty: Ty::Field,
            },
        );
        b.insert(
            "neg".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field)],
                return_ty: Ty::Field,
            },
        );
        b.insert(
            "sub".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field), ("b".into(), Ty::Field)],
                return_ty: Ty::Field,
            },
        );

        // U32 operations — split returns field_limbs U32s
        b.insert(
            "split".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field)],
                return_ty: Ty::Tuple(vec![Ty::U32; fl as usize]),
            },
        );
        b.insert(
            "log2".into(),
            FnSig {
                params: vec![("a".into(), Ty::U32)],
                return_ty: Ty::U32,
            },
        );
        b.insert(
            "pow".into(),
            FnSig {
                params: vec![("base".into(), Ty::U32), ("exp".into(), Ty::U32)],
                return_ty: Ty::U32,
            },
        );
        b.insert(
            "popcount".into(),
            FnSig {
                params: vec![("a".into(), Ty::U32)],
                return_ty: Ty::U32,
            },
        );

        // Hash operations — parameterized by hash_rate
        b.insert(
            "hash".into(),
            FnSig {
                params: (0..hr).map(|i| (format!("x{}", i), Ty::Field)).collect(),
                return_ty: digest_ty.clone(),
            },
        );
        b.insert(
            "sponge_init".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "sponge_absorb".into(),
            FnSig {
                params: (0..hr).map(|i| (format!("x{}", i), Ty::Field)).collect(),
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "sponge_squeeze".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Array(Box::new(Ty::Field), hr as u64),
            },
        );
        b.insert(
            "sponge_absorb_mem".into(),
            FnSig {
                params: vec![("ptr".into(), Ty::Field)],
                return_ty: Ty::Unit,
            },
        );

        // Merkle operations — parameterized by digest_width
        b.insert(
            "merkle_step".into(),
            FnSig {
                params: {
                    let mut p = vec![("idx".into(), Ty::U32)];
                    for i in 0..dw {
                        p.push((format!("d{}", i), Ty::Field));
                    }
                    p
                },
                return_ty: Ty::Tuple(vec![Ty::U32, digest_ty.clone()]),
            },
        );

        // RAM
        b.insert(
            "ram_read".into(),
            FnSig {
                params: vec![("addr".into(), Ty::Field)],
                return_ty: Ty::Field,
            },
        );
        b.insert(
            "ram_write".into(),
            FnSig {
                params: vec![("addr".into(), Ty::Field), ("val".into(), Ty::Field)],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "ram_read_block".into(),
            FnSig {
                params: vec![("addr".into(), Ty::Field)],
                return_ty: digest_ty.clone(),
            },
        );
        b.insert(
            "ram_write_block".into(),
            FnSig {
                params: vec![("addr".into(), Ty::Field), ("d".into(), digest_ty.clone())],
                return_ty: Ty::Unit,
            },
        );

        // Conversion
        b.insert(
            "as_u32".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field)],
                return_ty: Ty::U32,
            },
        );
        b.insert(
            "as_field".into(),
            FnSig {
                params: vec![("a".into(), Ty::U32)],
                return_ty: Ty::Field,
            },
        );

        // XField — only registered if the target has an extension field
        if xw > 0 {
            b.insert(
                "xfield".into(),
                FnSig {
                    params: (0..xw)
                        .map(|i| (format!("{}", (b'a' + i as u8) as char), Ty::Field))
                        .collect(),
                    return_ty: xfield_ty.clone(),
                },
            );
            b.insert(
                "xinvert".into(),
                FnSig {
                    params: vec![("a".into(), xfield_ty.clone())],
                    return_ty: xfield_ty,
                },
            );
        }
    }
}

/// Returns true if a builtin function name performs I/O side effects.
/// Used by the `#[pure]` annotation checker.
pub(super) fn is_io_builtin(name: &str) -> bool {
    matches!(
        name,
        "pub_read"
            | "pub_write"
            | "sec_read"
            | "divine"
            | "sponge_init"
            | "sponge_absorb"
            | "sponge_squeeze"
            | "sponge_absorb_mem"
            | "ram_read"
            | "ram_write"
            | "ram_read_block"
            | "ram_write_block"
            | "merkle_step"
            | "merkle_step_mem"
    ) || name.starts_with("pub_read")
        || name.starts_with("pub_write")
        || name.starts_with("divine")
}
