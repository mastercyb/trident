//! Type width helpers and struct layout registration/lookup.

use std::collections::BTreeMap;

use crate::ast::*;
use crate::span::Spanned;
use crate::target::TargetConfig;

use super::TIRBuilder;

// ─── Free functions: type helpers ─────────────────────────────────

pub(crate) fn format_type_name(ty: &Type) -> String {
    match ty {
        Type::Field => "Field".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::U32 => "U32".to_string(),
        Type::XField => "XField".to_string(),
        Type::Digest => "Digest".to_string(),
        Type::Array(inner, n) => format!("[{}; {}]", format_type_name(inner), n),
        Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(format_type_name).collect();
            format!("({})", parts.join(", "))
        }
        Type::Named(path) => path.0.join("."),
    }
}

pub(crate) fn resolve_type_width(ty: &Type, tc: &TargetConfig) -> u32 {
    match ty {
        Type::Field | Type::Bool | Type::U32 => 1,
        Type::XField => tc.xfield_width,
        Type::Digest => tc.digest_width,
        Type::Array(inner, n) => {
            let size = n.as_literal().unwrap_or(0);
            resolve_type_width(inner, tc) * (size as u32)
        }
        Type::Tuple(elems) => elems.iter().map(|t| resolve_type_width(t, tc)).sum(),
        Type::Named(_) => 1,
    }
}

pub(crate) fn resolve_type_width_with_subs(
    ty: &Type,
    subs: &BTreeMap<String, u64>,
    tc: &TargetConfig,
) -> u32 {
    match ty {
        Type::Field | Type::Bool | Type::U32 => 1,
        Type::XField => tc.xfield_width,
        Type::Digest => tc.digest_width,
        Type::Array(inner, n) => {
            let size = n.eval(subs);
            resolve_type_width_with_subs(inner, subs, tc) * (size as u32)
        }
        Type::Tuple(elems) => elems
            .iter()
            .map(|t| resolve_type_width_with_subs(t, subs, tc))
            .sum(),
        Type::Named(_) => 1,
    }
}

// ─── TIRBuilder struct layout methods ──────────────────────────────

impl TIRBuilder {
    /// Register struct field layout from a type annotation.
    pub(crate) fn register_struct_layout_from_type(&mut self, var_name: &str, ty: &Type) {
        if let Type::Named(path) = ty {
            let struct_name = path.0.last().map(|s| s.as_str()).unwrap_or("");
            if let Some(sdef) = self.struct_types.get(struct_name).cloned() {
                let mut field_map = BTreeMap::new();
                let total: u32 = sdef
                    .fields
                    .iter()
                    .map(|f| resolve_type_width(&f.ty.node, &self.target_config))
                    .sum();
                let mut offset = 0u32;
                for sf in &sdef.fields {
                    let fw = resolve_type_width(&sf.ty.node, &self.target_config);
                    let from_top = total - offset - fw;
                    field_map.insert(sf.name.node.clone(), (from_top, fw));
                    offset += fw;
                }
                self.struct_layouts.insert(var_name.to_string(), field_map);
            }
        }
    }

    /// Look up field offset within a struct variable.
    pub(crate) fn find_field_offset_in_var(
        &self,
        var_name: &str,
        field_name: &str,
    ) -> Option<(u32, u32)> {
        if let Some(offsets) = self.struct_layouts.get(var_name) {
            return offsets.get(field_name).copied();
        }
        None
    }

    /// Resolve field offset for Expr::FieldAccess.
    pub(crate) fn resolve_field_offset(&self, inner: &Expr, field: &str) -> Option<(u32, u32)> {
        if let Expr::Var(name) = inner {
            return self.find_field_offset_in_var(name, field);
        }
        None
    }

    /// Compute field widths for a struct init.
    pub(crate) fn compute_struct_field_widths(
        &self,
        ty: &Option<Spanned<Type>>,
        fields: &[(Spanned<String>, Spanned<Expr>)],
    ) -> Vec<u32> {
        if let Some(sp_ty) = ty {
            if let Type::Named(path) = &sp_ty.node {
                if let Some(name) = path.0.last() {
                    if let Some(sdef) = self.struct_types.get(name) {
                        return sdef
                            .fields
                            .iter()
                            .map(|f| resolve_type_width(&f.ty.node, &self.target_config))
                            .collect();
                    }
                }
            }
        }
        vec![1u32; fields.len()]
    }
}
