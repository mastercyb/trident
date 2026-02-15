//! Expression type checking: check_expr, check_binop.

use std::collections::BTreeMap;

use crate::ast::*;
use crate::span::Span;
use crate::types::Ty;

use super::builtins::is_io_builtin;
use super::{MonoInstance, TypeChecker};

impl TypeChecker {
    pub(super) fn check_expr(&mut self, expr: &Expr, span: Span) -> Ty {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::Integer(_) => Ty::Field,
                Literal::Bool(_) => Ty::Bool,
            },
            Expr::Var(name) => {
                // Direct variable lookup
                if let Some(info) = self.lookup_var(name) {
                    return info.ty.clone();
                }
                // Known constant
                if self.constants.contains_key(name) {
                    return Ty::Field;
                }
                // Dotted name: could be field access (var.field) or module constant
                if let Some(dot_pos) = name.rfind('.') {
                    let prefix = &name[..dot_pos];
                    let suffix = &name[dot_pos + 1..];
                    // Check if prefix is a variable with struct type
                    if let Some(info) = self.lookup_var(prefix) {
                        if let Ty::Struct(sty) = &info.ty {
                            if let Some((field_ty, _, _)) = sty.field_offset(suffix) {
                                return field_ty;
                            }
                            self.error(
                                format!("struct '{}' has no field '{}'", sty.name, suffix),
                                span,
                            );
                            return Ty::Field;
                        }
                    }
                }
                self.error_with_help(
                    format!("undefined variable '{}'", name),
                    span,
                    "check that the variable is declared with `let` before use".to_string(),
                );
                Ty::Field
            }
            Expr::BinOp { op, lhs, rhs } => {
                let lhs_ty = self.check_expr(&lhs.node, lhs.span);
                let rhs_ty = self.check_expr(&rhs.node, rhs.span);
                self.check_binop(*op, &lhs_ty, &rhs_ty, span)
            }
            Expr::Call {
                path,
                generic_args,
                args,
            } => {
                let fn_name = path.node.as_dotted();
                let arg_tys: Vec<Ty> = args
                    .iter()
                    .map(|a| self.check_expr(&a.node, a.span))
                    .collect();

                // Reject I/O builtins inside #[pure] functions.
                if self.in_pure_fn {
                    let base = fn_name.rsplit('.').next().unwrap_or(&fn_name);
                    if is_io_builtin(base) {
                        self.error(
                            format!(
                                "#[pure] function cannot call '{}' (I/O side effect)",
                                fn_name
                            ),
                            span,
                        );
                    }
                }

                // Check if this is a generic function call.
                if let Some(gdef) = self.generic_fns.get(&fn_name).cloned() {
                    // Resolve size arguments: explicit or inferred.
                    let size_args = if !generic_args.is_empty() {
                        // Explicit: sum<3>(...)
                        if generic_args.len() != gdef.type_params.len() {
                            self.error(
                                format!(
                                    "function '{}' expects {} size parameters, got {}",
                                    fn_name,
                                    gdef.type_params.len(),
                                    generic_args.len()
                                ),
                                span,
                            );
                            return Ty::Field;
                        }
                        let mut sizes = Vec::new();
                        for ga in generic_args {
                            if let Some(n) = ga.node.as_literal() {
                                sizes.push(n);
                            } else {
                                self.error(
                                    format!("expected concrete size, got '{}'", ga.node),
                                    ga.span,
                                );
                                sizes.push(0);
                            }
                        }
                        sizes
                    } else {
                        // Infer from argument types.
                        self.infer_size_args(&gdef, &arg_tys, span)
                    };

                    // Build substitution map.
                    let mut subs = BTreeMap::new();
                    for (param_name, size_val) in gdef.type_params.iter().zip(size_args.iter()) {
                        subs.insert(param_name.clone(), *size_val);
                    }

                    // Monomorphize the signature.
                    let params: Vec<(String, Ty)> = gdef
                        .params
                        .iter()
                        .map(|(name, ty)| (name.clone(), self.resolve_type_with_subs(ty, &subs)))
                        .collect();
                    let return_ty = gdef
                        .return_ty
                        .as_ref()
                        .map(|t| self.resolve_type_with_subs(t, &subs))
                        .unwrap_or(Ty::Unit);

                    // Type-check arguments against the monomorphized signature.
                    if arg_tys.len() != params.len() {
                        self.error(
                            format!(
                                "function '{}' expects {} arguments, got {}",
                                fn_name,
                                params.len(),
                                arg_tys.len()
                            ),
                            span,
                        );
                    } else {
                        for (i, ((_, expected), actual)) in
                            params.iter().zip(arg_tys.iter()).enumerate()
                        {
                            if expected != actual {
                                self.error(
                                    format!(
                                        "argument {} of '{}': expected {} but got {}",
                                        i + 1,
                                        fn_name,
                                        expected.display(),
                                        actual.display()
                                    ),
                                    args[i].span,
                                );
                            }
                        }
                    }

                    // Record this monomorphized instance.
                    let instance = MonoInstance {
                        name: fn_name.clone(),
                        size_args: size_args.clone(),
                    };
                    if !self.mono_instances.contains(&instance) {
                        self.mono_instances.push(instance.clone());
                    }
                    // Record per-call-site resolution for the emitter.
                    self.call_resolutions.push(instance);

                    return_ty
                } else if let Some(sig) = self.functions.get(&fn_name).cloned() {
                    // Non-generic function call — existing logic.
                    if !generic_args.is_empty() {
                        self.error(
                            format!(
                                "function '{}' is not generic but called with size arguments",
                                fn_name
                            ),
                            span,
                        );
                    }
                    if arg_tys.len() != sig.params.len() {
                        self.error(
                            format!(
                                "function '{}' expects {} arguments, got {}",
                                fn_name,
                                sig.params.len(),
                                arg_tys.len()
                            ),
                            span,
                        );
                    } else {
                        for (i, ((_, expected), actual)) in
                            sig.params.iter().zip(arg_tys.iter()).enumerate()
                        {
                            if expected != actual {
                                self.error(
                                    format!(
                                        "argument {} of '{}': expected {} but got {}",
                                        i + 1,
                                        fn_name,
                                        expected.display(),
                                        actual.display()
                                    ),
                                    args[i].span,
                                );
                            }
                        }
                    }
                    // H0003: detect redundant as_u32 range checks
                    let base_name = fn_name.rsplit('.').next().unwrap_or(&fn_name);
                    if base_name == "as_u32" && args.len() == 1 {
                        if let Expr::Var(var_name) = &args[0].node {
                            if self.u32_proven.contains(var_name) {
                                self.warning(
                                    format!(
                                        "hint[H0003]: as_u32({}) is redundant — value is already proven U32",
                                        var_name
                                    ),
                                    span,
                                );
                            }
                        }
                    }

                    sig.return_ty
                } else {
                    self.error_with_help(
                        format!("undefined function '{}'", fn_name),
                        span,
                        "check the function name and ensure the module is imported with `use`"
                            .to_string(),
                    );
                    Ty::Field
                }
            }
            Expr::FieldAccess { expr: inner, field } => {
                let inner_ty = self.check_expr(&inner.node, inner.span);
                if let Ty::Struct(sty) = &inner_ty {
                    if let Some((field_ty, _, _)) = sty.field_offset(&field.node) {
                        field_ty
                    } else {
                        self.error(
                            format!("struct '{}' has no field '{}'", sty.name, field.node),
                            span,
                        );
                        Ty::Field
                    }
                } else {
                    self.error(
                        format!("field access on non-struct type {}", inner_ty.display()),
                        span,
                    );
                    Ty::Field
                }
            }
            Expr::Index { expr: inner, index } => {
                let inner_ty = self.check_expr(&inner.node, inner.span);
                let _idx_ty = self.check_expr(&index.node, index.span);
                match &inner_ty {
                    Ty::Array(elem_ty, _) => *elem_ty.clone(),
                    _ => {
                        self.error(
                            format!("index access on non-array type {}", inner_ty.display()),
                            span,
                        );
                        Ty::Field
                    }
                }
            }
            Expr::StructInit {
                path,
                fields: init_fields,
            } => {
                let struct_name = path.node.as_dotted();
                if let Some(sty) = self.structs.get(&struct_name).cloned() {
                    // Check all required fields are provided
                    for (def_name, def_ty, _) in &sty.fields {
                        if let Some((_name, val)) =
                            init_fields.iter().find(|(n, _)| n.node == *def_name)
                        {
                            let val_ty = self.check_expr(&val.node, val.span);
                            if val_ty != *def_ty {
                                self.error(
                                    format!(
                                        "field '{}': expected {} but got {}",
                                        def_name,
                                        def_ty.display(),
                                        val_ty.display()
                                    ),
                                    val.span,
                                );
                            }
                        } else {
                            self.error(
                                format!("missing field '{}' in struct init", def_name),
                                span,
                            );
                        }
                    }
                    // Check for extra fields
                    for (name, _) in init_fields {
                        if !sty.fields.iter().any(|(n, _, _)| *n == name.node) {
                            self.error(
                                format!(
                                    "unknown field '{}' in struct '{}'",
                                    name.node, struct_name
                                ),
                                name.span,
                            );
                        }
                    }
                    Ty::Struct(sty)
                } else {
                    self.error_with_help(
                        format!("undefined struct '{}'", struct_name),
                        span,
                        "check the struct name spelling, or import the module that defines it"
                            .to_string(),
                    );
                    Ty::Field
                }
            }
            Expr::ArrayInit(elements) => {
                if elements.is_empty() {
                    Ty::Array(Box::new(Ty::Field), 0)
                } else {
                    let first_ty = self.check_expr(&elements[0].node, elements[0].span);
                    for elem in &elements[1..] {
                        let ty = self.check_expr(&elem.node, elem.span);
                        if ty != first_ty {
                            self.error(
                                format!(
                                    "array element type mismatch: expected {} got {}",
                                    first_ty.display(),
                                    ty.display()
                                ),
                                elem.span,
                            );
                        }
                    }
                    Ty::Array(Box::new(first_ty), elements.len() as u64)
                }
            }
            Expr::Tuple(elements) => {
                let tys: Vec<Ty> = elements
                    .iter()
                    .map(|e| self.check_expr(&e.node, e.span))
                    .collect();
                Ty::Tuple(tys)
            }
        }
    }

    pub(super) fn check_binop(&mut self, op: BinOp, lhs: &Ty, rhs: &Ty, span: Span) -> Ty {
        match op {
            BinOp::Add | BinOp::Mul => {
                if lhs == &Ty::Field && rhs == &Ty::Field {
                    Ty::Field
                } else if matches!(lhs, Ty::XField(_)) && lhs == rhs {
                    lhs.clone()
                } else {
                    self.error(
                        format!(
                            "operator '{}' requires both operands to be Field (or both XField), got {} and {}",
                            op.as_str(), lhs.display(), rhs.display()
                        ),
                        span,
                    );
                    Ty::Field
                }
            }
            BinOp::Eq => {
                if lhs != rhs {
                    self.error(
                        format!(
                            "operator '==' requires same types, got {} and {}",
                            lhs.display(),
                            rhs.display()
                        ),
                        span,
                    );
                }
                Ty::Bool
            }
            BinOp::Lt => {
                if lhs != &Ty::U32 || rhs != &Ty::U32 {
                    self.error(
                        format!(
                            "operator '<' requires U32 operands, got {} and {}",
                            lhs.display(),
                            rhs.display()
                        ),
                        span,
                    );
                }
                Ty::Bool
            }
            BinOp::BitAnd | BinOp::BitXor => {
                if lhs != &Ty::U32 || rhs != &Ty::U32 {
                    self.error(
                        format!(
                            "operator '{}' requires U32 operands, got {} and {}",
                            op.as_str(),
                            lhs.display(),
                            rhs.display()
                        ),
                        span,
                    );
                }
                Ty::U32
            }
            BinOp::DivMod => {
                if lhs != &Ty::U32 || rhs != &Ty::U32 {
                    self.error(
                        format!(
                            "operator '/%' requires U32 operands, got {} and {}",
                            lhs.display(),
                            rhs.display()
                        ),
                        span,
                    );
                }
                Ty::Tuple(vec![Ty::U32, Ty::U32])
            }
            BinOp::XFieldMul => {
                if !matches!(lhs, Ty::XField(_)) || rhs != &Ty::Field {
                    self.error(
                        format!(
                            "operator '*.' requires XField and Field, got {} and {}",
                            lhs.display(),
                            rhs.display()
                        ),
                        span,
                    );
                }
                lhs.clone()
            }
        }
    }
}
