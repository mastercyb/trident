use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::span::{Span, Spanned};
use crate::types::{StructTy, Ty};

/// A function signature for type checking.
#[derive(Clone, Debug)]
struct FnSig {
    params: Vec<(String, Ty)>,
    return_ty: Ty,
}

/// A generic (size-parameterized) function definition, stored unresolved.
#[derive(Clone, Debug)]
struct GenericFnDef {
    /// Size parameter names, e.g. `["N"]`.
    type_params: Vec<String>,
    /// Parameter types as AST types (may contain `ArraySize::Param`).
    params: Vec<(String, Type)>,
    /// Return type as AST type (may contain `ArraySize::Param`).
    return_ty: Option<Type>,
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
    /// Mangled label: `sum` with N=3 → `__sum__N3`.
    pub fn mangled_name(&self) -> String {
        let suffix: Vec<String> = self.size_args.iter().map(|n| format!("{}", n)).collect();
        format!("__{}__N{}", self.name, suffix.join("_"))
    }
}

/// Variable info in scope.
#[derive(Clone, Debug)]
struct VarInfo {
    ty: Ty,
    mutable: bool,
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
    functions: HashMap<String, FnSig>,
    /// Variable scopes (stack of scope maps).
    scopes: Vec<HashMap<String, VarInfo>>,
    /// Known constants (name → value).
    constants: HashMap<String, u64>,
    /// Known struct types (name or module.name → StructTy).
    structs: HashMap<String, StructTy>,
    /// Known event types (name → field list).
    events: HashMap<String, Vec<(String, Ty)>>,
    /// Accumulated diagnostics.
    diagnostics: Vec<Diagnostic>,
    /// Variables proven to be in U32 range (via as_u32, split, or U32 type).
    u32_proven: HashSet<String>,
    /// Generic (size-parameterized) function definitions.
    generic_fns: HashMap<String, GenericFnDef>,
    /// Unique monomorphized instances collected during type checking.
    mono_instances: Vec<MonoInstance>,
    /// Per-call-site resolutions in AST walk order.
    call_resolutions: Vec<MonoInstance>,
    /// Active cfg flags for conditional compilation.
    cfg_flags: HashSet<String>,
    /// Target VM configuration (digest width, hash rate, field limbs, etc.).
    target_config: crate::target::TargetConfig,
    /// Whether we are currently inside a `#[pure]` function body.
    in_pure_fn: bool,
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
            functions: HashMap::new(),
            scopes: Vec::new(),
            constants: HashMap::new(),
            structs: HashMap::new(),
            events: HashMap::new(),
            diagnostics: Vec::new(),
            u32_proven: HashSet::new(),
            generic_fns: HashMap::new(),
            mono_instances: Vec::new(),
            call_resolutions: Vec::new(),
            cfg_flags: HashSet::from(["debug".to_string()]),
            target_config: config,
            in_pure_fn: false,
        };
        tc.register_builtins();
        tc
    }

    /// Set active cfg flags for conditional compilation.
    pub(crate) fn with_cfg_flags(mut self, flags: HashSet<String>) -> Self {
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
        let is_std_module =
            file.name.node.starts_with("std.") || file.name.node.starts_with("ext.");

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
                    // #[intrinsic] is only allowed in std.* modules
                    if func.intrinsic.is_some() && !is_std_module {
                        self.error(
                            format!(
                                "#[intrinsic] is only allowed in std.*/ext.* modules, \
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
        let mut used_prefixes: HashSet<String> = HashSet::new();
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

    /// Build a call graph from the file's functions and report any cycles.
    fn detect_recursion(&mut self, file: &File) {
        // Build adjacency list: fn_name → set of called fn_names
        let mut call_graph: HashMap<String, Vec<String>> = HashMap::new();

        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                if let Some(body) = &func.body {
                    let mut callees = Vec::new();
                    Self::collect_calls_block(&body.node, &mut callees);
                    call_graph.insert(func.name.node.clone(), callees);
                }
            }
        }

        // DFS cycle detection
        let fn_names: Vec<String> = call_graph.keys().cloned().collect();
        let mut visited = HashMap::new(); // 0=unvisited, 1=in-stack, 2=done

        for name in &fn_names {
            visited.insert(name.clone(), 0u8);
        }

        for name in &fn_names {
            if visited[name] == 0 {
                let mut path = Vec::new();
                if self.dfs_cycle(name, &call_graph, &mut visited, &mut path) {
                    // Find the span for the function that starts the cycle
                    let cycle_fn = &path[0];
                    let span = file
                        .items
                        .iter()
                        .find_map(|item| {
                            if let Item::Fn(func) = &item.node {
                                if func.name.node == *cycle_fn {
                                    return Some(func.name.span);
                                }
                            }
                            None
                        })
                        .unwrap_or(file.name.span);
                    self.error_with_help(
                        format!("recursive call cycle detected: {}", path.join(" -> ")),
                        span,
                        "stack-machine targets do not support recursion; use loops (`for`) or iterative algorithms instead".to_string(),
                    );
                }
            }
        }
    }

    fn dfs_cycle(
        &self,
        node: &str,
        graph: &HashMap<String, Vec<String>>,
        visited: &mut HashMap<String, u8>,
        path: &mut Vec<String>,
    ) -> bool {
        visited.insert(node.to_string(), 1); // in-stack
        path.push(node.to_string());

        if let Some(callees) = graph.get(node) {
            for callee in callees {
                // Only check local functions (those in our graph)
                let state = visited.get(callee).copied().unwrap_or(2);
                if state == 1 {
                    // Back-edge: cycle found
                    path.push(callee.clone());
                    return true;
                }
                if state == 0 && self.dfs_cycle(callee, graph, visited, path) {
                    return true;
                }
            }
        }

        path.pop();
        visited.insert(node.to_string(), 2); // done
        false
    }

    /// Collect all function call names from a block.
    fn collect_calls_block(block: &Block, calls: &mut Vec<String>) {
        for stmt in &block.stmts {
            Self::collect_calls_stmt(&stmt.node, calls);
        }
        if let Some(tail) = &block.tail_expr {
            Self::collect_calls_expr(&tail.node, calls);
        }
    }

    fn collect_calls_stmt(stmt: &Stmt, calls: &mut Vec<String>) {
        match stmt {
            Stmt::Let { init, .. } => Self::collect_calls_expr(&init.node, calls),
            Stmt::Assign { value, .. } => Self::collect_calls_expr(&value.node, calls),
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                Self::collect_calls_expr(&cond.node, calls);
                Self::collect_calls_block(&then_block.node, calls);
                if let Some(eb) = else_block {
                    Self::collect_calls_block(&eb.node, calls);
                }
            }
            Stmt::For {
                start, end, body, ..
            } => {
                Self::collect_calls_expr(&start.node, calls);
                Self::collect_calls_expr(&end.node, calls);
                Self::collect_calls_block(&body.node, calls);
            }
            Stmt::TupleAssign { value, .. } => Self::collect_calls_expr(&value.node, calls),
            Stmt::Expr(expr) => Self::collect_calls_expr(&expr.node, calls),
            Stmt::Return(Some(val)) => Self::collect_calls_expr(&val.node, calls),
            Stmt::Return(None) => {}
            Stmt::Emit { fields, .. } | Stmt::Seal { fields, .. } => {
                for (_, val) in fields {
                    Self::collect_calls_expr(&val.node, calls);
                }
            }
            Stmt::Asm { .. } => {}
            Stmt::Match { expr, arms } => {
                Self::collect_calls_expr(&expr.node, calls);
                for arm in arms {
                    Self::collect_calls_block(&arm.body.node, calls);
                }
            }
        }
    }

    /// Collect module prefixes used in calls and variable access within a block.
    fn collect_used_modules_block(block: &Block, used: &mut HashSet<String>) {
        for stmt in &block.stmts {
            Self::collect_used_modules_stmt(&stmt.node, used);
        }
        if let Some(tail) = &block.tail_expr {
            Self::collect_used_modules_expr(&tail.node, used);
        }
    }

    fn collect_used_modules_stmt(stmt: &Stmt, used: &mut HashSet<String>) {
        match stmt {
            Stmt::Let { init, .. } => Self::collect_used_modules_expr(&init.node, used),
            Stmt::Assign { value, .. } => Self::collect_used_modules_expr(&value.node, used),
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                Self::collect_used_modules_expr(&cond.node, used);
                Self::collect_used_modules_block(&then_block.node, used);
                if let Some(eb) = else_block {
                    Self::collect_used_modules_block(&eb.node, used);
                }
            }
            Stmt::For {
                start, end, body, ..
            } => {
                Self::collect_used_modules_expr(&start.node, used);
                Self::collect_used_modules_expr(&end.node, used);
                Self::collect_used_modules_block(&body.node, used);
            }
            Stmt::TupleAssign { value, .. } => Self::collect_used_modules_expr(&value.node, used),
            Stmt::Expr(expr) => Self::collect_used_modules_expr(&expr.node, used),
            Stmt::Return(Some(val)) => Self::collect_used_modules_expr(&val.node, used),
            Stmt::Return(None) => {}
            Stmt::Emit { fields, .. } | Stmt::Seal { fields, .. } => {
                for (_, val) in fields {
                    Self::collect_used_modules_expr(&val.node, used);
                }
            }
            Stmt::Asm { .. } => {}
            Stmt::Match { expr, arms } => {
                Self::collect_used_modules_expr(&expr.node, used);
                for arm in arms {
                    Self::collect_used_modules_block(&arm.body.node, used);
                }
            }
        }
    }

    fn collect_used_modules_expr(expr: &Expr, used: &mut HashSet<String>) {
        match expr {
            Expr::Call { path, args, .. } => {
                let dotted = path.node.as_dotted();
                // "module.func" → module is used
                if let Some(dot_pos) = dotted.rfind('.') {
                    let prefix = &dotted[..dot_pos];
                    used.insert(prefix.to_string());
                }
                for arg in args {
                    Self::collect_used_modules_expr(&arg.node, used);
                }
            }
            Expr::Var(name) => {
                // "module.CONST" → module is used
                if let Some(dot_pos) = name.rfind('.') {
                    let prefix = &name[..dot_pos];
                    used.insert(prefix.to_string());
                }
            }
            Expr::BinOp { lhs, rhs, .. } => {
                Self::collect_used_modules_expr(&lhs.node, used);
                Self::collect_used_modules_expr(&rhs.node, used);
            }
            Expr::Tuple(elems) | Expr::ArrayInit(elems) => {
                for e in elems {
                    Self::collect_used_modules_expr(&e.node, used);
                }
            }
            Expr::FieldAccess { expr: inner, .. } | Expr::Index { expr: inner, .. } => {
                Self::collect_used_modules_expr(&inner.node, used);
            }
            Expr::StructInit { path, fields } => {
                let dotted = path.node.as_dotted();
                if let Some(dot_pos) = dotted.rfind('.') {
                    let prefix = &dotted[..dot_pos];
                    used.insert(prefix.to_string());
                }
                for (_, val) in fields {
                    Self::collect_used_modules_expr(&val.node, used);
                }
            }
            Expr::Literal(_) => {}
        }
    }

    fn collect_calls_expr(expr: &Expr, calls: &mut Vec<String>) {
        match expr {
            Expr::Call { path, args, .. } => {
                // Extract the function name (last segment for cross-module calls)
                let dotted = path.node.as_dotted();
                let fn_name = dotted.rsplit('.').next().unwrap_or(&dotted);
                calls.push(fn_name.to_string());
                for arg in args {
                    Self::collect_calls_expr(&arg.node, calls);
                }
            }
            Expr::BinOp { lhs, rhs, .. } => {
                Self::collect_calls_expr(&lhs.node, calls);
                Self::collect_calls_expr(&rhs.node, calls);
            }
            Expr::Tuple(elems) | Expr::ArrayInit(elems) => {
                for e in elems {
                    Self::collect_calls_expr(&e.node, calls);
                }
            }
            Expr::FieldAccess { expr: inner, .. } | Expr::Index { expr: inner, .. } => {
                Self::collect_calls_expr(&inner.node, calls);
            }
            Expr::StructInit { fields, .. } => {
                for (_, val) in fields {
                    Self::collect_calls_expr(&val.node, calls);
                }
            }
            Expr::Literal(_) | Expr::Var(_) => {}
        }
    }

    fn check_fn(&mut self, func: &FnDef) {
        if func.body.is_none() {
            return; // intrinsic, no body to check
        }
        if !func.type_params.is_empty() {
            return; // generic — body checked per monomorphized instance
        }

        // Validate #[test] functions: no parameters, no return type, not generic.
        if func.is_test {
            if !func.params.is_empty() {
                self.error(
                    format!(
                        "#[test] function '{}' must have no parameters",
                        func.name.node
                    ),
                    func.name.span,
                );
            }
            if func.return_ty.is_some() {
                self.error(
                    format!(
                        "#[test] function '{}' must not have a return type",
                        func.name.node
                    ),
                    func.name.span,
                );
            }
        }

        let prev_pure = self.in_pure_fn;
        self.in_pure_fn = func.is_pure;

        self.push_scope();

        // Bind parameters
        for param in &func.params {
            let ty = self.resolve_type(&param.ty.node);
            self.define_var(&param.name.node, ty, false);
        }

        let body = func.body.as_ref().unwrap();
        self.check_block(&body.node);

        self.pop_scope();
        self.in_pure_fn = prev_pure;
    }

    fn check_block(&mut self, block: &Block) -> Ty {
        self.push_scope();
        let mut terminated = false;
        for stmt in &block.stmts {
            if terminated {
                self.error_with_help(
                    "unreachable code after return statement".to_string(),
                    stmt.span,
                    "remove this code or move it before the return".to_string(),
                );
                break;
            }
            self.check_stmt(&stmt.node, stmt.span);
            if self.is_terminating_stmt(&stmt.node) {
                terminated = true;
            }
        }
        if terminated {
            if let Some(tail) = &block.tail_expr {
                self.error_with_help(
                    "unreachable tail expression after return".to_string(),
                    tail.span,
                    "remove this expression or move it before the return".to_string(),
                );
            }
        }
        let ty = if let Some(tail) = &block.tail_expr {
            self.check_expr(&tail.node, tail.span)
        } else {
            Ty::Unit
        };
        self.pop_scope();
        ty
    }

    fn is_terminating_stmt(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Return(_) => true,
            // assert(false) is an unconditional halt
            Stmt::Expr(expr) => {
                if let Expr::Call { path, args, .. } = &expr.node {
                    let name = path.node.as_dotted();
                    if (name == "assert" || name == "assert.is_true") && args.len() == 1 {
                        if let Expr::Literal(Literal::Bool(false)) = &args[0].node {
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt, _span: Span) {
        match stmt {
            Stmt::Let {
                mutable,
                pattern,
                ty,
                init,
            } => {
                let init_ty = self.check_expr(&init.node, init.span);
                let resolved_ty = if let Some(declared_ty) = ty {
                    let expected = self.resolve_type(&declared_ty.node);
                    if expected != init_ty {
                        self.error(
                            format!(
                                "type mismatch: declared {} but expression has type {}",
                                expected.display(),
                                init_ty.display()
                            ),
                            init.span,
                        );
                    }
                    expected
                } else {
                    init_ty
                };

                match pattern {
                    Pattern::Name(name) => {
                        self.define_var(&name.node, resolved_ty.clone(), *mutable);
                        // Track U32-proven variables for H0003:
                        // When as_u32(x) or split(x) is called, the INPUT x
                        // has been range-checked. Mark x as proven so a
                        // subsequent as_u32(x) is flagged as redundant.
                        if let Expr::Call { path, args, .. } = &init.node {
                            let call_name = path.node.as_dotted();
                            let base = call_name.rsplit('.').next().unwrap_or(&call_name);
                            if (base == "as_u32" || base == "split") && !args.is_empty() {
                                if let Expr::Var(arg_name) = &args[0].node {
                                    self.u32_proven.insert(arg_name.clone());
                                }
                            }
                        }
                    }
                    Pattern::Tuple(names) => {
                        // Destructure: type must be a tuple or Digest
                        if let Ty::Tuple(elem_tys) = &resolved_ty {
                            if names.len() != elem_tys.len() {
                                self.error(
                                    format!(
                                        "tuple destructuring: expected {} elements, got {} names",
                                        elem_tys.len(),
                                        names.len()
                                    ),
                                    init.span,
                                );
                            }
                            for (i, name) in names.iter().enumerate() {
                                if name.node != "_" {
                                    let ty = elem_tys.get(i).cloned().unwrap_or(Ty::Field);
                                    self.define_var(&name.node, ty, *mutable);
                                }
                            }
                        } else if matches!(resolved_ty, Ty::Digest(_)) {
                            // Digest decomposition: let (f0, f1, ...) = digest
                            let dw = resolved_ty.width() as usize;
                            if names.len() != dw {
                                self.error(
                                    format!(
                                        "digest destructuring requires exactly {} names, got {}",
                                        dw,
                                        names.len()
                                    ),
                                    init.span,
                                );
                            }
                            for name in names.iter() {
                                if name.node != "_" {
                                    self.define_var(&name.node, Ty::Field, *mutable);
                                }
                            }
                        } else {
                            self.error(
                                format!(
                                    "cannot destructure non-tuple type {}",
                                    resolved_ty.display()
                                ),
                                init.span,
                            );
                        }
                    }
                }
            }
            Stmt::Assign { place, value } => {
                let (place_ty, is_mut) = self.check_place(&place.node, place.span);
                if !is_mut {
                    self.error_with_help(
                        "cannot assign to immutable variable".to_string(),
                        place.span,
                        "declare the variable with `let mut` to make it mutable".to_string(),
                    );
                }
                let val_ty = self.check_expr(&value.node, value.span);
                if place_ty != val_ty {
                    self.error(
                        format!(
                            "type mismatch in assignment: expected {} but got {}",
                            place_ty.display(),
                            val_ty.display()
                        ),
                        value.span,
                    );
                }
                // Invalidate U32-proven status on reassignment
                if let Place::Var(name) = &place.node {
                    self.u32_proven.remove(name);
                }
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_ty = self.check_expr(&cond.node, cond.span);
                if cond_ty != Ty::Bool && cond_ty != Ty::Field {
                    self.error(
                        format!(
                            "if condition must be Bool or Field, got {}",
                            cond_ty.display()
                        ),
                        cond.span,
                    );
                }
                self.check_block(&then_block.node);
                if let Some(else_blk) = else_block {
                    self.check_block(&else_blk.node);
                }
            }
            Stmt::For {
                var,
                start,
                end,
                bound,
                body,
            } => {
                let _start_ty = self.check_expr(&start.node, start.span);
                let _end_ty = self.check_expr(&end.node, end.span);

                // Check that start is a constant 0 or Field/U32
                // end must be a constant or have bounded annotation
                if bound.is_none() {
                    // end must be a compile-time constant
                    if !self.is_constant_expr(&end.node) {
                        self.error_with_help(
                            "loop end must be a compile-time constant, or annotated with a bound".to_string(),
                            end.span,
                            "use a literal like `for i in 0..10 { }` or add a bound: `for i in 0..n bounded 100 { }`".to_string(),
                        );
                    }
                }

                self.push_scope();
                if var.node != "_" {
                    self.define_var(&var.node, Ty::U32, false);
                }
                self.check_block(&body.node);
                self.pop_scope();
            }
            Stmt::TupleAssign { names, value } => {
                let val_ty = self.check_expr(&value.node, value.span);
                let valid = if let Ty::Tuple(elem_tys) = &val_ty {
                    if names.len() != elem_tys.len() {
                        self.error(
                            format!(
                                "tuple assignment: expected {} elements, got {} names",
                                elem_tys.len(),
                                names.len()
                            ),
                            value.span,
                        );
                    }
                    true
                } else if matches!(val_ty, Ty::Digest(_)) {
                    let dw = val_ty.width() as usize;
                    if names.len() != dw {
                        self.error(
                            format!(
                                "Digest destructuring requires exactly {} names, got {}",
                                dw,
                                names.len()
                            ),
                            value.span,
                        );
                    }
                    true
                } else {
                    false
                };
                if valid {
                    for name in names {
                        if let Some(info) = self.lookup_var(&name.node) {
                            if !info.mutable {
                                self.error_with_help(
                                    format!("cannot assign to immutable variable '{}'", name.node),
                                    name.span,
                                    "declare the variable with `let mut` to make it mutable"
                                        .to_string(),
                                );
                            }
                        }
                    }
                } else {
                    self.error(
                        format!(
                            "cannot tuple-assign from non-tuple type {}",
                            val_ty.display()
                        ),
                        value.span,
                    );
                }
            }
            Stmt::Expr(expr) => {
                self.check_expr(&expr.node, expr.span);
            }
            Stmt::Return(value) => {
                if let Some(val) = value {
                    self.check_expr(&val.node, val.span);
                }
            }
            Stmt::Emit { event_name, fields } | Stmt::Seal { event_name, fields } => {
                if self.in_pure_fn {
                    let kind = if matches!(stmt, Stmt::Emit { .. }) {
                        "emit"
                    } else {
                        "seal"
                    };
                    self.error(
                        format!("#[pure] function cannot use '{}' (I/O side effect)", kind),
                        _span,
                    );
                }
                self.check_event_stmt(event_name, fields);
            }
            Stmt::Asm { target, .. } => {
                // Warn if asm block is tagged for a different target
                if let Some(tag) = target {
                    if tag != &self.target_config.name {
                        self.warning(
                            format!(
                                "asm block tagged for '{}' will be skipped (current target: '{}')",
                                tag, self.target_config.name
                            ),
                            _span,
                        );
                    }
                }
            }
            Stmt::Match { expr, arms } => {
                let scrutinee_ty = self.check_expr(&expr.node, expr.span);
                let mut has_wildcard = false;
                let mut has_true = false;
                let mut has_false = false;
                let mut wildcard_seen = false;

                for arm in arms {
                    if wildcard_seen {
                        self.error_with_help(
                            "unreachable pattern after wildcard '_'".to_string(),
                            arm.pattern.span,
                            "the wildcard `_` already matches all values; remove this arm or move it before `_`".to_string(),
                        );
                    }

                    match &arm.pattern.node {
                        MatchPattern::Literal(Literal::Integer(_)) => {
                            if scrutinee_ty != Ty::Field && scrutinee_ty != Ty::U32 {
                                self.error(
                                    format!(
                                        "integer pattern requires Field or U32 scrutinee, got {}",
                                        scrutinee_ty.display()
                                    ),
                                    arm.pattern.span,
                                );
                            }
                        }
                        MatchPattern::Literal(Literal::Bool(b)) => {
                            if scrutinee_ty != Ty::Bool {
                                self.error(
                                    format!(
                                        "boolean pattern requires Bool scrutinee, got {}",
                                        scrutinee_ty.display()
                                    ),
                                    arm.pattern.span,
                                );
                            }
                            if *b {
                                has_true = true;
                            } else {
                                has_false = true;
                            }
                        }
                        MatchPattern::Wildcard => {
                            has_wildcard = true;
                            wildcard_seen = true;
                        }
                        MatchPattern::Struct { name, fields } => {
                            // Look up the struct type
                            if let Some(sty) = self.structs.get(&name.node).cloned() {
                                // Verify scrutinee type matches the struct
                                if scrutinee_ty != Ty::Struct(sty.clone()) {
                                    self.error(
                                        format!(
                                            "struct pattern `{}` does not match scrutinee type `{}`",
                                            name.node,
                                            scrutinee_ty.display()
                                        ),
                                        arm.pattern.span,
                                    );
                                }
                                // Validate each field in the pattern
                                for spf in fields {
                                    if let Some((field_ty, _, _)) =
                                        sty.field_offset(&spf.field_name.node)
                                    {
                                        match &spf.pattern.node {
                                            FieldPattern::Literal(Literal::Integer(_)) => {
                                                if field_ty != Ty::Field && field_ty != Ty::U32 {
                                                    self.error(
                                                        format!(
                                                            "integer pattern on field `{}` requires Field or U32, got {}",
                                                            spf.field_name.node,
                                                            field_ty.display()
                                                        ),
                                                        spf.pattern.span,
                                                    );
                                                }
                                            }
                                            FieldPattern::Literal(Literal::Bool(_)) => {
                                                if field_ty != Ty::Bool {
                                                    self.error(
                                                        format!(
                                                            "boolean pattern on field `{}` requires Bool, got {}",
                                                            spf.field_name.node,
                                                            field_ty.display()
                                                        ),
                                                        spf.pattern.span,
                                                    );
                                                }
                                            }
                                            FieldPattern::Binding(_) | FieldPattern::Wildcard => {}
                                        }
                                    } else {
                                        self.error(
                                            format!(
                                                "struct `{}` has no field `{}`",
                                                name.node, spf.field_name.node
                                            ),
                                            spf.field_name.span,
                                        );
                                    }
                                }
                            } else {
                                self.error(
                                    format!("unknown struct type `{}`", name.node),
                                    name.span,
                                );
                            }
                        }
                    }

                    // For struct patterns, define bound variables in a scope wrapping the arm body
                    if let MatchPattern::Struct { name, fields } = &arm.pattern.node {
                        self.push_scope();
                        if let Some(sty) = self.structs.get(&name.node).cloned() {
                            for spf in fields {
                                if let FieldPattern::Binding(var_name) = &spf.pattern.node {
                                    if let Some((field_ty, _, _)) =
                                        sty.field_offset(&spf.field_name.node)
                                    {
                                        self.define_var(var_name, field_ty, false);
                                    }
                                }
                            }
                        }
                        self.check_block(&arm.body.node);
                        self.pop_scope();
                    } else {
                        self.check_block(&arm.body.node);
                    }
                }

                // Exhaustiveness: require wildcard unless Bool with both true+false,
                // or a struct pattern (structs have exactly one shape)
                let has_struct_pattern = arms
                    .iter()
                    .any(|a| matches!(a.pattern.node, MatchPattern::Struct { .. }));
                let exhaustive = has_wildcard
                    || (scrutinee_ty == Ty::Bool && has_true && has_false)
                    || has_struct_pattern;
                if !exhaustive {
                    self.error_with_help(
                        "non-exhaustive match: not all possible values are covered".to_string(),
                        expr.span,
                        "add a wildcard `_ => { ... }` arm to handle all remaining values"
                            .to_string(),
                    );
                }
            }
        }
    }

    fn check_event_stmt(
        &mut self,
        event_name: &Spanned<String>,
        fields: &[(Spanned<String>, Spanned<Expr>)],
    ) {
        let Some(event_fields) = self.events.get(&event_name.node).cloned() else {
            self.error(
                format!("undefined event '{}'", event_name.node),
                event_name.span,
            );
            return;
        };

        // Check all declared fields are provided
        for (def_name, _def_ty) in &event_fields {
            if !fields.iter().any(|(n, _)| n.node == *def_name) {
                self.error(
                    format!(
                        "missing field '{}' in event '{}'",
                        def_name, event_name.node
                    ),
                    event_name.span,
                );
            }
        }

        // Check provided fields exist and have correct types
        for (name, val) in fields {
            if let Some((_def_name, def_ty)) = event_fields.iter().find(|(n, _)| *n == name.node) {
                let val_ty = self.check_expr(&val.node, val.span);
                if val_ty != *def_ty {
                    self.error(
                        format!(
                            "event field '{}': expected {} but got {}",
                            name.node,
                            def_ty.display(),
                            val_ty.display()
                        ),
                        val.span,
                    );
                }
            } else {
                self.error(
                    format!(
                        "unknown field '{}' in event '{}'",
                        name.node, event_name.node
                    ),
                    name.span,
                );
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr, span: Span) -> Ty {
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
                    let mut subs = HashMap::new();
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

    fn check_binop(&mut self, op: BinOp, lhs: &Ty, rhs: &Ty, span: Span) -> Ty {
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

    fn check_place(&self, place: &Place, _span: Span) -> (Ty, bool) {
        match place {
            Place::Var(name) => {
                if let Some(info) = self.lookup_var(name) {
                    (info.ty.clone(), info.mutable)
                } else {
                    (Ty::Field, false)
                }
            }
            Place::FieldAccess(inner, field) => {
                let (inner_ty, is_mut) = self.check_place(&inner.node, inner.span);
                if let Ty::Struct(sty) = &inner_ty {
                    if let Some((field_ty, _, _)) = sty.field_offset(&field.node) {
                        (field_ty, is_mut)
                    } else {
                        (Ty::Field, false)
                    }
                } else {
                    (Ty::Field, false)
                }
            }
            Place::Index(inner, _) => {
                let (inner_ty, is_mut) = self.check_place(&inner.node, inner.span);
                if let Ty::Array(elem_ty, _) = &inner_ty {
                    (*elem_ty.clone(), is_mut)
                } else {
                    (Ty::Field, false)
                }
            }
        }
    }

    fn is_constant_expr(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Literal(Literal::Integer(_)))
            || matches!(expr, Expr::Var(name) if self.constants.contains_key(name))
    }

    /// Infer size arguments for a generic function from argument types.
    /// E.g. if param is `[Field; N]` and arg type is `[Field; 5]`, infer N=5.
    fn infer_size_args(&mut self, gdef: &GenericFnDef, arg_tys: &[Ty], span: Span) -> Vec<u64> {
        let mut subs: HashMap<String, u64> = HashMap::new();

        for ((_, param_ty), arg_ty) in gdef.params.iter().zip(arg_tys.iter()) {
            Self::unify_sizes(param_ty, arg_ty, &mut subs);
        }

        let mut result = Vec::new();
        for param_name in &gdef.type_params {
            if let Some(&val) = subs.get(param_name) {
                result.push(val);
            } else {
                self.error(
                    format!(
                        "cannot infer size parameter '{}'; provide explicit size argument",
                        param_name
                    ),
                    span,
                );
                result.push(0);
            }
        }
        result
    }

    /// Recursively match an AST type pattern against a concrete Ty to extract
    /// size parameter bindings. E.g. `[Field; N]` vs `[Field; 5]` → N=5.
    fn unify_sizes(pattern: &Type, concrete: &Ty, subs: &mut HashMap<String, u64>) {
        match (pattern, concrete) {
            (Type::Array(inner_pat, ArraySize::Param(name)), Ty::Array(inner_ty, size)) => {
                subs.insert(name.clone(), *size);
                Self::unify_sizes(inner_pat, inner_ty, subs);
            }
            (Type::Array(inner_pat, _), Ty::Array(inner_ty, _)) => {
                Self::unify_sizes(inner_pat, inner_ty, subs);
            }
            (Type::Tuple(pats), Ty::Tuple(tys)) => {
                for (p, t) in pats.iter().zip(tys.iter()) {
                    Self::unify_sizes(p, t, subs);
                }
            }
            _ => {}
        }
    }

    fn resolve_type(&self, ty: &Type) -> Ty {
        self.resolve_type_with_subs(ty, &HashMap::new())
    }

    /// Resolve an AST type to a semantic type, substituting size parameters.
    fn resolve_type_with_subs(&self, ty: &Type, subs: &HashMap<String, u64>) -> Ty {
        match ty {
            Type::Field => Ty::Field,
            Type::XField => Ty::XField(self.target_config.xfield_width),
            Type::Bool => Ty::Bool,
            Type::U32 => Ty::U32,
            Type::Digest => Ty::Digest(self.target_config.digest_width),
            Type::Array(inner, n) => {
                let size = n.eval(subs);
                Ty::Array(Box::new(self.resolve_type_with_subs(inner, subs)), size)
            }
            Type::Tuple(elems) => Ty::Tuple(
                elems
                    .iter()
                    .map(|t| self.resolve_type_with_subs(t, subs))
                    .collect(),
            ),
            Type::Named(path) => {
                let name = path.as_dotted();
                if let Some(sty) = self.structs.get(&name) {
                    Ty::Struct(sty.clone())
                } else {
                    Ty::Field
                }
            }
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_var(&mut self, name: &str, ty: Ty, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), VarInfo { ty, mutable });
        }
    }

    fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    fn error(&mut self, msg: String, span: Span) {
        self.diagnostics.push(Diagnostic::error(msg, span));
    }

    fn error_with_help(&mut self, msg: String, span: Span, help: String) {
        self.diagnostics
            .push(Diagnostic::error(msg, span).with_help(help));
    }

    fn warning(&mut self, msg: String, span: Span) {
        self.diagnostics.push(Diagnostic::warning(msg, span));
    }

    fn register_builtins(&mut self) {
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
fn is_io_builtin(name: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn check(source: &str) -> Result<ModuleExports, Vec<Diagnostic>> {
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        TypeChecker::new().check_file(&file)
    }

    #[test]
    fn test_valid_field_arithmetic() {
        let result = check("program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = a + b\n    pub_write(c)\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_mismatch() {
        let result = check("program test\nfn main() {\n    let a: U32 = pub_read()\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_undefined_variable() {
        let result = check("program test\nfn main() {\n    pub_write(x)\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_with_eq() {
        let result = check("program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = divine()\n    assert(a == b)\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_function_call() {
        let result = check("program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = pub_read()\n    let y: Field = pub_read()\n    let z: Field = add(x, y)\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_struct_init_and_field_access() {
        let result = check("program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let p: Point = Point { x: a, y: b }\n    pub_write(p.x)\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_struct_missing_field() {
        let result = check("program test\nstruct Point {\n    x: Field,\n    y: Field,\n}\nfn main() {\n    let p: Point = Point { x: pub_read() }\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_array_init_and_index() {
        let result = check("program test\nfn main() {\n    let arr: [Field; 3] = [pub_read(), pub_read(), pub_read()]\n    pub_write(arr[0])\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_tuple_destructuring() {
        let result = check("program test\nfn pair() -> (Field, Field) {\n    (pub_read(), pub_read())\n}\nfn main() {\n    let (a, b): (Field, Field) = pair()\n    pub_write(a)\n    pub_write(b)\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_tuple_destructure_arity_mismatch() {
        let result = check("program test\nfn main() {\n    let (a, b, c): (Field, Field) = (pub_read(), pub_read())\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_emit_valid() {
        let result = check("program test\nevent Transfer { from: Field, to: Field, amount: Field }\nfn main() {\n    emit Transfer { from: pub_read(), to: pub_read(), amount: pub_read() }\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_seal_valid() {
        let result = check("program test\nevent Nullifier { id: Field, nonce: Field }\nfn main() {\n    seal Nullifier { id: pub_read(), nonce: pub_read() }\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_emit_undefined_event() {
        let result = check("program test\nfn main() {\n    emit Missing { x: pub_read() }\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_emit_missing_field() {
        let result = check("program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    emit Ev { x: pub_read() }\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_emit_extra_field() {
        let result = check("program test\nevent Ev { x: Field }\nfn main() {\n    emit Ev { x: pub_read(), y: pub_read() }\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_event_max_9_fields() {
        let result = check("program test\nevent Big { f0: Field, f1: Field, f2: Field, f3: Field, f4: Field, f5: Field, f6: Field, f7: Field, f8: Field, f9: Field }\nfn main() {\n}");
        assert!(result.is_err()); // 10 fields > max 9
    }

    #[test]
    fn test_digest_destructuring() {
        let result = check("program test\nfn main() {\n    let d: Digest = divine5()\n    let (f0, f1, f2, f3, f4) = d\n    pub_write(f0)\n    pub_write(f4)\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_digest_destructuring_wrong_arity() {
        let result = check(
            "program test\nfn main() {\n    let d: Digest = divine5()\n    let (a, b, c) = d\n}",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_digest_destructuring_inline() {
        // Destructure directly from hash() call
        let result = check("program test\nfn main() {\n    let (f0, f1, f2, f3, f4) = hash(0, 0, 0, 0, 0, 0, 0, 0, 0, 0)\n    pub_write(f0)\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_intrinsic_rejected_outside_std() {
        let result =
            check("program test\n#[intrinsic(hash)] fn foo() -> Digest {\n}\nfn main() {\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_intrinsic_allowed_in_std_module() {
        let result = check("module std.test\n#[intrinsic(hash)] pub fn foo(x0: Field, x1: Field, x2: Field, x3: Field, x4: Field, x5: Field, x6: Field, x7: Field, x8: Field, x9: Field) -> Digest\n");
        assert!(result.is_ok());
    }

    #[test]
    fn test_direct_recursion_rejected() {
        let result =
            check("program test\nfn loop_forever() {\n    loop_forever()\n}\nfn main() {\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_mutual_recursion_rejected() {
        let result =
            check("program test\nfn a() {\n    b()\n}\nfn b() {\n    a()\n}\nfn main() {\n}");
        assert!(result.is_err());
    }

    #[test]
    fn test_no_false_positive_recursion() {
        // a calls b, b calls c — no cycle
        let result = check("program test\nfn c() {\n    pub_write(1)\n}\nfn b() {\n    c()\n}\nfn a() {\n    b()\n}\nfn main() {\n    a()\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_dead_code_after_return() {
        let result = check(
            "program test\nfn foo() -> Field {\n    return 1\n    pub_write(2)\n}\nfn main() {\n}",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_dead_code_after_assert_false() {
        let result = check(
            "program test\nfn foo() {\n    assert(false)\n    pub_write(1)\n}\nfn main() {\n}",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_no_false_positive_dead_code() {
        let result = check("program test\nfn foo() -> Field {\n    let x: Field = pub_read()\n    pub_write(x)\n    x\n}\nfn main() {\n}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_unused_import_warning() {
        // Unused import should produce a warning but still succeed (it's not an error)
        let result = check("module test_mod\nuse std.hash\npub fn foo() -> Field {\n    42\n}");
        // Should succeed (warnings don't fail compilation)
        assert!(result.is_ok());
        // But should contain a warning
        let exports = result.unwrap();
        assert!(
            !exports.warnings.is_empty(),
            "expected unused import warning"
        );
    }

    #[test]
    fn test_used_import_no_warning() {
        // We can't test cross-module calls in unit tests (no import_module),
        // but we can verify the module prefix collection works by checking
        // that a module with no imports produces no warnings.
        let result = check("module test_mod\npub fn foo() -> Field {\n    42\n}");
        assert!(result.is_ok());
        let exports = result.unwrap();
        assert!(
            exports.warnings.is_empty(),
            "no warning expected for module with no imports, got: {:?}",
            exports.warnings
        );
    }

    #[test]
    fn test_h0003_redundant_as_u32() {
        // First as_u32(a) proves a is in U32 range.
        // Second as_u32(a) is redundant — should warn.
        let result = check(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: U32 = as_u32(a)\n    let c: U32 = as_u32(a)\n}",
        );
        assert!(result.is_ok());
        let exports = result.unwrap();
        let h0003 = exports.warnings.iter().any(|w| w.message.contains("H0003"));
        assert!(
            h0003,
            "expected H0003 warning for redundant as_u32, got: {:?}",
            exports.warnings
        );
    }

    #[test]
    fn test_h0003_no_false_positive() {
        // as_u32 on a fresh Field should NOT warn
        let result = check(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: U32 = as_u32(a)\n}",
        );
        assert!(result.is_ok());
        let exports = result.unwrap();
        let h0003 = exports.warnings.iter().any(|w| w.message.contains("H0003"));
        assert!(!h0003, "should not warn on first as_u32 call");
    }

    #[test]
    fn test_asm_block_type_checks() {
        // asm blocks should pass type checking without errors
        let result = check(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    asm { dup 0\nadd }\n    pub_write(x)\n}",
        );
        assert!(result.is_ok(), "asm block should not cause type errors");
    }

    #[test]
    fn test_asm_block_with_effect() {
        let result =
            check("program test\nfn main() {\n    asm(+1) { push 42 }\n    asm(-1) { pop 1 }\n}");
        assert!(result.is_ok(), "asm with effect should type check");
    }

    // --- Size-generic function tests ---

    #[test]
    fn test_generic_fn_explicit_size_arg() {
        let result = check(
            "program test\nfn sum<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = sum<3>(a)\n    pub_write(s)\n}",
        );
        assert!(
            result.is_ok(),
            "explicit size arg should type check: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_generic_fn_inferred_size() {
        let result = check(
            "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let f: Field = first(a)\n    pub_write(f)\n}",
        );
        assert!(
            result.is_ok(),
            "inferred size arg should type check: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_generic_fn_wrong_size_arg() {
        // Call sum<2> with a [Field; 3] — should fail type check
        let result = check(
            "program test\nfn sum<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = sum<2>(a)\n}",
        );
        assert!(
            result.is_err(),
            "mismatched size arg should fail type check"
        );
    }

    #[test]
    fn test_generic_fn_wrong_param_count() {
        // Function has 1 size param but call provides 2
        let result = check(
            "program test\nfn sum<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let s: Field = sum<3, 5>(a)\n}",
        );
        assert!(result.is_err(), "wrong number of size params should fail");
    }

    #[test]
    fn test_generic_fn_records_mono_instance() {
        let result = check(
            "program test\nfn id<N>(arr: [Field; N]) -> [Field; N] {\n    arr\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let b: [Field; 3] = id<3>(a)\n}",
        );
        assert!(result.is_ok());
        let exports = result.unwrap();
        assert_eq!(exports.mono_instances.len(), 1);
        assert_eq!(exports.mono_instances[0].name, "id");
        assert_eq!(exports.mono_instances[0].size_args, vec![3]);
    }

    #[test]
    fn test_generic_fn_multiple_instantiations() {
        let result = check(
            "program test\nfn first<N>(arr: [Field; N]) -> Field {\n    arr[0]\n}\nfn main() {\n    let a: [Field; 3] = [1, 2, 3]\n    let b: [Field; 5] = [1, 2, 3, 4, 5]\n    let x: Field = first<3>(a)\n    let y: Field = first<5>(b)\n    pub_write(x + y)\n}",
        );
        assert!(result.is_ok());
        let exports = result.unwrap();
        assert_eq!(
            exports.mono_instances.len(),
            2,
            "should have 2 distinct instantiations"
        );
    }

    #[test]
    fn test_generic_fn_non_generic_with_size_args_fails() {
        // Calling a non-generic function with size args should error
        let result = check(
            "program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = add<3>(1, 2)\n}",
        );
        assert!(
            result.is_err(),
            "non-generic fn called with size args should fail"
        );
    }

    // --- conditional compilation ---

    fn check_with_flags(source: &str, flags: &[&str]) -> Result<ModuleExports, Vec<Diagnostic>> {
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        let flag_set: HashSet<String> = flags.iter().map(|s| s.to_string()).collect();
        TypeChecker::new()
            .with_cfg_flags(flag_set)
            .check_file(&file)
    }

    #[test]
    fn test_cfg_debug_includes_debug_fn() {
        let result = check_with_flags(
            "program test\n#[cfg(debug)]\nfn check() {}\nfn main() {\n    check()\n}",
            &["debug"],
        );
        assert!(result.is_ok(), "debug fn should be available in debug mode");
    }

    #[test]
    fn test_cfg_release_excludes_debug_fn() {
        let result = check_with_flags(
            "program test\n#[cfg(debug)]\nfn check() {}\nfn main() {\n    check()\n}",
            &["release"],
        );
        assert!(
            result.is_err(),
            "debug fn should not be available in release mode"
        );
    }

    #[test]
    fn test_cfg_no_attr_always_available() {
        let result = check_with_flags(
            "program test\nfn helper() {}\nfn main() {\n    helper()\n}",
            &["release"],
        );
        assert!(result.is_ok(), "uncfg'd fn always available");
    }

    #[test]
    fn test_cfg_duplicate_names_different_cfg() {
        // Two functions with same name but different cfg — only one active
        let result = check_with_flags(
            "program test\n#[cfg(debug)]\nfn mode() -> Field { 0 }\n#[cfg(release)]\nfn mode() -> Field { 1 }\nfn main() {\n    let x: Field = mode()\n}",
            &["debug"],
        );
        assert!(result.is_ok(), "should pick the debug variant");
    }

    #[test]
    fn test_cfg_const_excluded() {
        let result = check_with_flags(
            "program test\n#[cfg(debug)]\nconst X: Field = 42\nfn main() {\n    let a: Field = X\n}",
            &["release"],
        );
        // X is cfg'd out, so it should be unknown
        assert!(result.is_err(), "const should be excluded in release");
    }

    #[test]
    fn test_cfg_export_filtered() {
        let exports = check_with_flags(
            "module test\n#[cfg(debug)]\npub fn dbg_only() {}\npub fn always() {}",
            &["release"],
        )
        .unwrap();
        assert_eq!(exports.functions.len(), 1, "only always() exported");
        assert_eq!(exports.functions[0].0, "always");
    }

    // --- match statement type checking ---

    #[test]
    fn test_match_field_with_integers() {
        let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n        1 => { pub_write(1) }\n        _ => { pub_write(2) }\n    }\n}");
        assert!(result.is_ok(), "match on Field with integers should pass");
    }

    #[test]
    fn test_match_bool_exhaustive() {
        let result = check("program test\nfn main() {\n    let b: Bool = pub_read() == pub_read()\n    match b {\n        true => { pub_write(1) }\n        false => { pub_write(0) }\n    }\n}");
        assert!(
            result.is_ok(),
            "match on Bool with true+false is exhaustive"
        );
    }

    #[test]
    fn test_match_non_exhaustive_error() {
        let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n        1 => { pub_write(1) }\n    }\n}");
        assert!(
            result.is_err(),
            "match without wildcard on Field should fail"
        );
    }

    #[test]
    fn test_match_bool_pattern_on_field_error() {
        let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        true => { pub_write(1) }\n        _ => { pub_write(0) }\n    }\n}");
        assert!(
            result.is_err(),
            "boolean pattern on Field scrutinee should fail"
        );
    }

    #[test]
    fn test_match_integer_pattern_on_bool_error() {
        let result = check("program test\nfn main() {\n    let b: Bool = pub_read() == pub_read()\n    match b {\n        0 => { pub_write(0) }\n        _ => { pub_write(1) }\n    }\n}");
        assert!(
            result.is_err(),
            "integer pattern on Bool scrutinee should fail"
        );
    }

    #[test]
    fn test_match_unreachable_after_wildcard() {
        let result = check("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        _ => { pub_write(0) }\n        0 => { pub_write(1) }\n    }\n}");
        assert!(
            result.is_err(),
            "pattern after wildcard should be unreachable"
        );
    }

    #[test]
    fn test_match_struct_pattern_valid() {
        let result = check(
            "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, y } => { pub_write(x) }\n    }\n}",
        );
        assert!(
            result.is_ok(),
            "struct pattern match should pass: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_match_struct_pattern_wrong_type() {
        let result = check(
            "program test\nstruct Point { x: Field, y: Field }\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Pair { a, b } => { pub_write(a) }\n    }\n}",
        );
        assert!(
            result.is_err(),
            "struct pattern with wrong type should fail"
        );
    }

    #[test]
    fn test_match_struct_pattern_unknown_field() {
        let result = check(
            "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, z } => { pub_write(x) }\n    }\n}",
        );
        assert!(
            result.is_err(),
            "struct pattern with unknown field should fail"
        );
    }

    #[test]
    fn test_match_struct_pattern_unknown_struct() {
        let result = check(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        Foo { a } => { pub_write(a) }\n    }\n}",
        );
        assert!(
            result.is_err(),
            "struct pattern with unknown struct should fail"
        );
    }

    #[test]
    fn test_match_struct_pattern_with_literal_field() {
        let result = check(
            "program test\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Pair { a: 1, b: 2 }\n    match p {\n        Pair { a: 0, b } => { pub_write(b) }\n        _ => { pub_write(0) }\n    }\n}",
        );
        assert!(
            result.is_ok(),
            "struct pattern with literal field should pass: {:?}",
            result.err()
        );
    }

    // --- #[test] function validation ---

    #[test]
    fn test_test_fn_valid() {
        let result =
            check("program test\n#[test]\nfn check_math() {\n    assert(1 == 1)\n}\nfn main() {}");
        assert!(
            result.is_ok(),
            "valid test fn should pass: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_test_fn_with_params_rejected() {
        let result = check(
            "program test\n#[test]\nfn bad_test(x: Field) {\n    assert(x == x)\n}\nfn main() {}",
        );
        assert!(result.is_err(), "test fn with params should fail");
    }

    #[test]
    fn test_test_fn_with_return_rejected() {
        let result =
            check("program test\n#[test]\nfn bad_test() -> Field {\n    42\n}\nfn main() {}");
        assert!(result.is_err(), "test fn with return type should fail");
    }

    #[test]
    fn test_test_fn_not_emitted_in_normal_build() {
        // Test functions should type-check but not interfere with normal compilation
        let result = check("program test\n#[test]\nfn check() {\n    assert(true)\n}\nfn main() {\n    pub_write(pub_read())\n}");
        assert!(result.is_ok());
    }

    // --- Error path tests: message quality ---

    fn check_err(source: &str) -> Vec<Diagnostic> {
        match check(source) {
            Ok(_) => vec![],
            Err(diags) => diags,
        }
    }

    #[test]
    fn test_error_binary_op_type_mismatch() {
        let diags = check_err(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Bool = a == a\n    let c: Field = a + b\n}",
        );
        assert!(!diags.is_empty(), "should error on Field + Bool");
        let msg = &diags[0].message;
        assert!(
            msg.contains("Field") && msg.contains("Bool"),
            "should show both types in mismatch, got: {}",
            msg
        );
    }

    #[test]
    fn test_error_function_arity_mismatch() {
        let diags = check_err(
            "program test\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {\n    let x: Field = add(1)\n}",
        );
        assert!(!diags.is_empty(), "should error on wrong argument count");
        let msg = &diags[0].message;
        assert!(
            msg.contains("expects 2 arguments") && msg.contains("got 1"),
            "should show expected and actual arity, got: {}",
            msg
        );
    }

    #[test]
    fn test_error_assign_to_immutable() {
        let diags =
            check_err("program test\nfn main() {\n    let x: Field = pub_read()\n    x = 42\n}");
        assert!(!diags.is_empty(), "should error on assigning to immutable");
        let msg = &diags[0].message;
        assert!(
            msg.contains("immutable"),
            "should mention immutability, got: {}",
            msg
        );
        assert!(
            diags[0].help.as_deref().unwrap().contains("let mut"),
            "help should suggest `let mut`"
        );
    }

    #[test]
    fn test_error_return_type_mismatch() {
        // pub_read() returns Field, but let binding declares U32 -- a type mismatch
        let diags = check_err("program test\nfn main() {\n    let x: U32 = pub_read()\n}");
        assert!(!diags.is_empty(), "should error on Field assigned to U32");
        let msg = &diags[0].message;
        assert!(
            msg.contains("U32") && msg.contains("Field"),
            "should show both expected and actual types, got: {}",
            msg
        );
    }

    #[test]
    fn test_error_undefined_event() {
        let diags = check_err("program test\nfn main() {\n    emit NoSuchEvent { x: 1 }\n}");
        assert!(!diags.is_empty(), "should error on undefined event");
        assert!(
            diags[0].message.contains("undefined event 'NoSuchEvent'"),
            "should name the undefined event, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn test_error_struct_unknown_field() {
        let diags = check_err(
            "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p: Point = Point { x: 1, y: 2, z: 3 }\n}",
        );
        assert!(!diags.is_empty(), "should error on unknown struct field");
        let has_unknown = diags
            .iter()
            .any(|d| d.message.contains("unknown field 'z'"));
        assert!(
            has_unknown,
            "should report unknown field 'z', got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_error_recursion_has_help() {
        let diags =
            check_err("program test\nfn loop_forever() {\n    loop_forever()\n}\nfn main() {\n}");
        assert!(!diags.is_empty(), "should detect recursion");
        assert!(
            diags[0].message.contains("recursive call cycle"),
            "should report cycle, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "recursion error should have help text explaining alternative"
        );
    }

    #[test]
    fn test_error_non_exhaustive_match_has_help() {
        let diags = check_err(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n    }\n}",
        );
        assert!(!diags.is_empty(), "should detect non-exhaustive match");
        assert!(
            diags[0].message.contains("non-exhaustive"),
            "should report non-exhaustive match, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.as_deref().unwrap().contains("_ =>"),
            "help should suggest wildcard arm"
        );
    }

    #[test]
    fn test_error_unreachable_code_has_help() {
        let diags = check_err(
            "program test\nfn foo() -> Field {\n    return 1\n    pub_write(2)\n}\nfn main() {\n}",
        );
        assert!(!diags.is_empty(), "should detect unreachable code");
        let unreachable_diag = diags.iter().find(|d| d.message.contains("unreachable"));
        assert!(
            unreachable_diag.is_some(),
            "should report unreachable code, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        assert!(
            unreachable_diag.unwrap().help.is_some(),
            "unreachable code error should have help text"
        );
    }

    #[test]
    fn test_error_undefined_variable_has_help() {
        let diags = check_err("program test\nfn main() {\n    pub_write(xyz)\n}");
        assert!(!diags.is_empty(), "should error on undefined variable");
        assert!(
            diags[0].message.contains("undefined variable 'xyz'"),
            "should name the variable, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "undefined variable error should have help text"
        );
    }

    #[test]
    fn test_error_undefined_function_has_help() {
        let diags = check_err("program test\nfn main() {\n    let x: Field = no_such_fn()\n}");
        assert!(!diags.is_empty(), "should error on undefined function");
        assert!(
            diags[0].message.contains("undefined function 'no_such_fn'"),
            "should name the function, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].help.is_some(),
            "undefined function error should have help text"
        );
    }

    #[test]
    fn test_error_loop_bound_has_help() {
        let diags = check_err(
            "program test\nfn main() {\n    let n: Field = pub_read()\n    for i in 0..n {\n        pub_write(0)\n    }\n}",
        );
        assert!(!diags.is_empty(), "should error on non-constant loop bound");
        let msg = &diags[0].message;
        assert!(
            msg.contains("compile-time constant") || msg.contains("bound"),
            "should explain the loop bound requirement, got: {}",
            msg
        );
        assert!(
            diags[0].help.as_deref().unwrap().contains("bounded"),
            "help should suggest `bounded` keyword"
        );
    }

    #[test]
    fn test_error_lt_requires_u32() {
        let diags = check_err(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    assert(a < b)\n}",
        );
        assert!(!diags.is_empty(), "should error on Field < Field");
        let msg = &diags[0].message;
        assert!(
            msg.contains("U32") && msg.contains("Field"),
            "should show required U32 and actual Field types, got: {}",
            msg
        );
    }

    #[test]
    fn test_error_field_access_on_non_struct() {
        let diags = check_err(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x.y)\n}",
        );
        assert!(
            !diags.is_empty(),
            "should error on field access of non-struct"
        );
        // The parser treats `x.y` as a dotted variable, so the error is
        // "undefined variable 'x.y'" since x is Field, not a struct with field y
        let has_error = diags
            .iter()
            .any(|d| d.message.contains("undefined variable") || d.message.contains("field"));
        assert!(
            has_error,
            "should report variable/field error, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_error_messages_have_spans() {
        // All type checker errors should have non-dummy spans
        let diags = check_err("program test\nfn main() {\n    pub_write(undefined_var)\n}");
        assert!(!diags.is_empty());
        for d in &diags {
            assert!(
                d.span.start != d.span.end || d.span.start > 0,
                "error '{}' should have a meaningful span, got: {:?}",
                d.message,
                d.span
            );
        }
    }

    // --- #[pure] annotation tests ---

    #[test]
    fn test_pure_fn_no_io_compiles() {
        let result = check("program test\n#[pure]\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}\nfn main() {}");
        assert!(
            result.is_ok(),
            "pure fn without I/O should pass: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_pure_fn_rejects_pub_read() {
        let diags =
            check_err("program test\n#[pure]\nfn f() -> Field {\n    pub_read()\n}\nfn main() {}");
        assert!(diags
            .iter()
            .any(|d| d.message.contains("#[pure]") && d.message.contains("pub_read")));
    }

    #[test]
    fn test_pure_fn_rejects_pub_write() {
        let diags =
            check_err("program test\n#[pure]\nfn f(x: Field) {\n    pub_write(x)\n}\nfn main() {}");
        assert!(diags
            .iter()
            .any(|d| d.message.contains("#[pure]") && d.message.contains("pub_write")));
    }

    #[test]
    fn test_pure_fn_rejects_divine() {
        let diags =
            check_err("program test\n#[pure]\nfn f() -> Field {\n    divine()\n}\nfn main() {}");
        assert!(diags
            .iter()
            .any(|d| d.message.contains("#[pure]") && d.message.contains("divine")));
    }

    #[test]
    fn test_pure_fn_allows_assert() {
        // assert is not I/O — it's a control flow operation
        let result =
            check("program test\n#[pure]\nfn f(x: Field) {\n    assert(x == 0)\n}\nfn main() {}");
        assert!(
            result.is_ok(),
            "assert should be allowed in pure fn: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_pure_fn_allows_hash() {
        // hash is a deterministic pure computation (same inputs → same outputs)
        let result = check("program test\n#[pure]\nfn f(a: Field, b: Field, c: Field, d: Field, e: Field, f2: Field, g: Field, h: Field, i: Field, j: Field) -> Digest {\n    hash(a, b, c, d, e, f2, g, h, i, j)\n}\nfn main() {}");
        assert!(
            result.is_ok(),
            "hash should be allowed in pure fn: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_pure_fn_rejects_sponge_init() {
        let diags =
            check_err("program test\n#[pure]\nfn f() {\n    sponge_init()\n}\nfn main() {}");
        assert!(diags
            .iter()
            .any(|d| d.message.contains("#[pure]") && d.message.contains("sponge_init")));
    }
}
