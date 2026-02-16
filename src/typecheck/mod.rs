mod analysis;
mod block;
mod builtins;
mod expr;
mod resolve;
mod stmt;
#[cfg(test)]
mod tests;
pub mod types;

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::span::{Span, Spanned};
use crate::types::{StructTy, Ty};

/// A function signature for type checking.
#[derive(Clone, Debug)]
pub(super) struct FnSig {
    pub(super) params: Vec<(String, Ty)>,
    pub(super) return_ty: Ty,
}

/// A generic (size-parameterized) function definition, stored unresolved.
#[derive(Clone, Debug)]
pub(super) struct GenericFnDef {
    /// Size parameter names, e.g. `["N"]`.
    pub(super) type_params: Vec<String>,
    /// Parameter types as AST types (may contain `ArraySize::Param`).
    pub(super) params: Vec<(String, Type)>,
    /// Return type as AST type (may contain `ArraySize::Param`).
    pub(super) return_ty: Option<Type>,
}

/// A monomorphized instance of a generic function.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MonoInstance {
    /// Original function name.
    pub name: String,
    /// Concrete size values for each type parameter.
    pub size_args: Vec<u64>,
}

impl MonoInstance {
    /// Mangled label: `sum` with N=3 -> `__sum__N3`.
    pub fn mangled_name(&self) -> String {
        let suffix: Vec<String> = self.size_args.iter().map(|n| format!("{}", n)).collect();
        format!("{}__N{}", self.name, suffix.join("_"))
    }
}

/// Variable info in scope.
#[derive(Clone, Debug)]
pub(super) struct VarInfo {
    pub(super) ty: Ty,
    pub(super) mutable: bool,
}

/// A function's exported signature: (name, params, return_type).
pub type FnExport = (String, Vec<(String, Ty)>, Ty);

/// Exported signatures from a type-checked module.
#[derive(Clone, Debug)]
pub struct ModuleExports {
    pub module_name: String,
    pub functions: Vec<FnExport>,
    pub constants: Vec<(String, Ty, u64)>, // (name, ty, value)
    pub structs: Vec<StructTy>,            // exported struct types
    pub warnings: Vec<Diagnostic>,         // non-fatal diagnostics
    /// Unique monomorphized instances of generic functions to emit.
    pub mono_instances: Vec<MonoInstance>,
    /// Per-call-site resolution: each generic call in AST order maps to a MonoInstance.
    /// The emitter consumes these in order to know which mangled name to call.
    pub call_resolutions: Vec<MonoInstance>,
}

pub(crate) struct TypeChecker {
    /// Known function signatures (user-defined + builtins).
    pub(super) functions: BTreeMap<String, FnSig>,
    /// Variable scopes (stack of scope maps).
    pub(super) scopes: Vec<BTreeMap<String, VarInfo>>,
    /// Known constants (name -> value).
    pub(super) constants: BTreeMap<String, u64>,
    /// Known struct types (name or module.name -> StructTy).
    pub(super) structs: BTreeMap<String, StructTy>,
    /// Known event types (name -> field list).
    pub(super) events: BTreeMap<String, Vec<(String, Ty)>>,
    /// Accumulated diagnostics.
    pub(super) diagnostics: Vec<Diagnostic>,
    /// Variables proven to be in U32 range (via as_u32, split, or U32 type).
    pub(super) u32_proven: BTreeSet<String>,
    /// Generic (size-parameterized) function definitions.
    pub(super) generic_fns: BTreeMap<String, GenericFnDef>,
    /// Unique monomorphized instances collected during type checking.
    pub(super) mono_instances: Vec<MonoInstance>,
    /// Per-call-site resolutions in AST walk order.
    pub(super) call_resolutions: Vec<MonoInstance>,
    /// Active cfg flags for conditional compilation.
    pub(super) cfg_flags: BTreeSet<String>,
    /// Target VM configuration (digest width, hash rate, field limbs, etc.).
    pub(super) target_config: crate::target::TargetConfig,
    /// Whether we are currently inside a `#[pure]` function body.
    pub(super) in_pure_fn: bool,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    pub(crate) fn new() -> Self {
        Self::with_target(crate::target::TargetConfig::triton())
    }

    pub(crate) fn with_target(config: crate::target::TargetConfig) -> Self {
        let mut tc = Self {
            functions: BTreeMap::new(),
            scopes: Vec::new(),
            constants: BTreeMap::new(),
            structs: BTreeMap::new(),
            events: BTreeMap::new(),
            diagnostics: Vec::new(),
            u32_proven: BTreeSet::new(),
            generic_fns: BTreeMap::new(),
            mono_instances: Vec::new(),
            call_resolutions: Vec::new(),
            cfg_flags: BTreeSet::from(["debug".to_string()]),
            target_config: config,
            in_pure_fn: false,
        };
        tc.register_builtins();
        tc
    }

    /// Set active cfg flags for conditional compilation.
    pub(crate) fn with_cfg_flags(mut self, flags: BTreeSet<String>) -> Self {
        self.cfg_flags = flags;
        self
    }

    /// Check if an item's cfg attribute is active.
    fn is_cfg_active(&self, cfg: &Option<Spanned<String>>) -> bool {
        match cfg {
            None => true,
            Some(flag) => self.cfg_flags.contains(&flag.node),
        }
    }

    /// Check if a top-level item's cfg is active.
    fn is_item_cfg_active(&self, item: &Item) -> bool {
        match item {
            Item::Fn(f) => self.is_cfg_active(&f.cfg),
            Item::Const(c) => self.is_cfg_active(&c.cfg),
            Item::Struct(s) => self.is_cfg_active(&s.cfg),
            Item::Event(e) => self.is_cfg_active(&e.cfg),
        }
    }

    /// Import exported signatures from another module.
    /// Makes them available as `module_name.fn_name`.
    /// For dotted modules like `std.hash`, also registers under
    /// the short alias `hash.fn_name` so `hash.tip5()` works.
    pub(crate) fn import_module(&mut self, exports: &ModuleExports) {
        // Short alias: last segment of dotted module name
        let short_prefix = exports
            .module_name
            .rsplit('.')
            .next()
            .unwrap_or(&exports.module_name);
        let has_short = short_prefix != exports.module_name;

        for (fn_name, params, return_ty) in &exports.functions {
            let qualified = format!("{}.{}", exports.module_name, fn_name);
            let sig = FnSig {
                params: params.clone(),
                return_ty: return_ty.clone(),
            };
            self.functions.insert(qualified, sig.clone());
            if has_short {
                let short = format!("{}.{}", short_prefix, fn_name);
                self.functions.insert(short, sig);
            }
        }
        for (const_name, _ty, value) in &exports.constants {
            let qualified = format!("{}.{}", exports.module_name, const_name);
            self.constants.insert(qualified, *value);
            if has_short {
                let short = format!("{}.{}", short_prefix, const_name);
                self.constants.insert(short, *value);
            }
        }
        for sty in &exports.structs {
            let qualified = format!("{}.{}", exports.module_name, sty.name);
            self.structs.insert(qualified, sty.clone());
            if has_short {
                let short = format!("{}.{}", short_prefix, sty.name);
                self.structs.insert(short, sty.clone());
            }
        }
    }

    pub(crate) fn check_file(mut self, file: &File) -> Result<ModuleExports, Vec<Diagnostic>> {
        let is_std_module = file.name.node.starts_with("std.")
            || file.name.node.starts_with("vm.")
            || file.name.node.starts_with("os.")
            || file.name.node.starts_with("ext.")
            || file.name.node.contains(".ext.");

        // First pass: register all structs, function signatures, and constants
        for item in &file.items {
            // Skip items excluded by conditional compilation
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            match &item.node {
                Item::Struct(sdef) => {
                    let fields: Vec<(String, Ty, bool)> = sdef
                        .fields
                        .iter()
                        .map(|f| (f.name.node.clone(), self.resolve_type(&f.ty.node), f.is_pub))
                        .collect();
                    let sty = StructTy {
                        name: sdef.name.node.clone(),
                        fields,
                    };
                    self.structs.insert(sdef.name.node.clone(), sty);
                }
                Item::Fn(func) => {
                    // #[intrinsic] is only allowed in vm.*/std.*/os.*/ext.* modules
                    if func.intrinsic.is_some() && !is_std_module {
                        self.error(
                            format!(
                                "#[intrinsic] is only allowed in vm.*/std.*/os.* modules, \
                                 not in '{}'",
                                file.name.node
                            ),
                            func.name.span,
                        );
                    }
                    if func.type_params.is_empty() {
                        // Non-generic function: resolve immediately.
                        let params: Vec<(String, Ty)> = func
                            .params
                            .iter()
                            .map(|p| (p.name.node.clone(), self.resolve_type(&p.ty.node)))
                            .collect();
                        let return_ty = func
                            .return_ty
                            .as_ref()
                            .map(|t| self.resolve_type(&t.node))
                            .unwrap_or(Ty::Unit);
                        self.functions
                            .insert(func.name.node.clone(), FnSig { params, return_ty });
                    } else {
                        // Generic function: store unresolved for monomorphization.
                        let gdef = GenericFnDef {
                            type_params: func.type_params.iter().map(|p| p.node.clone()).collect(),
                            params: func
                                .params
                                .iter()
                                .map(|p| (p.name.node.clone(), p.ty.node.clone()))
                                .collect(),
                            return_ty: func.return_ty.as_ref().map(|t| t.node.clone()),
                        };
                        self.generic_fns.insert(func.name.node.clone(), gdef);
                    }
                }
                Item::Const(cdef) => {
                    if let Expr::Literal(Literal::Integer(v)) = &cdef.value.node {
                        self.constants.insert(cdef.name.node.clone(), *v);
                    }
                }
                Item::Event(edef) => {
                    if edef.fields.len() > 9 {
                        self.error(
                            format!(
                                "event '{}' has {} fields, max is 9",
                                edef.name.node,
                                edef.fields.len()
                            ),
                            edef.name.span,
                        );
                    }
                    let fields: Vec<(String, Ty)> = edef
                        .fields
                        .iter()
                        .map(|f| {
                            let ty = self.resolve_type(&f.ty.node);
                            if ty != Ty::Field {
                                self.error(
                                    format!(
                                        "event field '{}' must be Field type, got {}",
                                        f.name.node,
                                        ty.display()
                                    ),
                                    f.ty.span,
                                );
                            }
                            (f.name.node.clone(), ty)
                        })
                        .collect();
                    self.events.insert(edef.name.node.clone(), fields);
                }
            }
        }

        // Recursion detection: build call graph and reject cycles
        self.detect_recursion(file);

        // Second pass: type check function bodies
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                self.check_fn(func);
            }
        }

        // Unused import detection: collect used module prefixes from all calls
        let mut used_prefixes: BTreeSet<String> = BTreeSet::new();
        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            if let Item::Fn(func) = &item.node {
                if let Some(body) = &func.body {
                    Self::collect_used_modules_block(&body.node, &mut used_prefixes);
                }
            }
        }
        for use_stmt in &file.uses {
            let module_path = use_stmt.node.as_dotted();
            // Short alias: last segment
            let short = module_path
                .rsplit('.')
                .next()
                .unwrap_or(&module_path)
                .to_string();
            if !used_prefixes.contains(&short) && !used_prefixes.contains(&module_path) {
                self.warning(format!("unused import '{}'", module_path), use_stmt.span);
            }
        }

        // Collect exports (pub items only)
        let module_name = file.name.node.clone();
        let mut exported_fns = Vec::new();
        let mut exported_consts = Vec::new();
        let mut exported_structs = Vec::new();

        for item in &file.items {
            if !self.is_item_cfg_active(&item.node) {
                continue;
            }
            match &item.node {
                Item::Fn(func) if func.is_pub => {
                    let params: Vec<(String, Ty)> = func
                        .params
                        .iter()
                        .map(|p| (p.name.node.clone(), self.resolve_type(&p.ty.node)))
                        .collect();
                    let return_ty = func
                        .return_ty
                        .as_ref()
                        .map(|t| self.resolve_type(&t.node))
                        .unwrap_or(Ty::Unit);
                    exported_fns.push((func.name.node.clone(), params, return_ty));
                }
                Item::Const(cdef) if cdef.is_pub => {
                    let ty = self.resolve_type(&cdef.ty.node);
                    if let Expr::Literal(Literal::Integer(v)) = &cdef.value.node {
                        exported_consts.push((cdef.name.node.clone(), ty, *v));
                    }
                }
                Item::Struct(sdef) if sdef.is_pub => {
                    if let Some(sty) = self.structs.get(&sdef.name.node) {
                        exported_structs.push(sty.clone());
                    }
                }
                _ => {}
            }
        }

        let has_errors = self
            .diagnostics
            .iter()
            .any(|d| d.severity == crate::diagnostic::Severity::Error);
        if has_errors {
            Err(self.diagnostics)
        } else {
            Ok(ModuleExports {
                module_name,
                functions: exported_fns,
                constants: exported_consts,
                structs: exported_structs,
                warnings: self.diagnostics,
                mono_instances: self.mono_instances,
                call_resolutions: self.call_resolutions,
            })
        }
    }

    // --- Scope management ---

    pub(super) fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn define_var(&mut self, name: &str, ty: Ty, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), VarInfo { ty, mutable });
        }
    }

    pub(super) fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    // --- Diagnostics ---

    pub(super) fn error(&mut self, msg: String, span: Span) {
        self.diagnostics.push(Diagnostic::error(msg, span));
    }

    pub(super) fn error_with_help(&mut self, msg: String, span: Span, help: String) {
        self.diagnostics
            .push(Diagnostic::error(msg, span).with_help(help));
    }

    pub(super) fn warning(&mut self, msg: String, span: Span) {
        self.diagnostics.push(Diagnostic::warning(msg, span));
    }
}
