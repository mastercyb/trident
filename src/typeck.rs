use std::collections::HashMap;

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

/// Variable info in scope.
#[derive(Clone, Debug)]
struct VarInfo {
    ty: Ty,
    mutable: bool,
}

/// Exported signatures from a type-checked module.
#[derive(Clone, Debug)]
pub struct ModuleExports {
    pub module_name: String,
    pub functions: Vec<(String, Vec<(String, Ty)>, Ty)>, // (name, params, return_ty)
    pub constants: Vec<(String, Ty, u64)>,               // (name, ty, value)
    pub structs: Vec<StructTy>,                          // exported struct types
}

pub struct TypeChecker {
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
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self {
            functions: HashMap::new(),
            scopes: Vec::new(),
            constants: HashMap::new(),
            structs: HashMap::new(),
            events: HashMap::new(),
            diagnostics: Vec::new(),
        };
        tc.register_builtins();
        tc
    }

    /// Import exported signatures from another module.
    /// Makes them available as `module_name.fn_name`.
    /// For dotted modules like `std.hash`, also registers under
    /// the short alias `hash.fn_name` so `hash.tip5()` works.
    pub fn import_module(&mut self, exports: &ModuleExports) {
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

    pub fn check_file(mut self, file: &File) -> Result<ModuleExports, Vec<Diagnostic>> {
        // First pass: register all structs, function signatures, and constants
        for item in &file.items {
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

        // Second pass: type check function bodies
        for item in &file.items {
            if let Item::Fn(func) = &item.node {
                self.check_fn(func);
            }
        }

        // Collect exports (pub items only)
        let module_name = file.name.node.clone();
        let mut exported_fns = Vec::new();
        let mut exported_consts = Vec::new();
        let mut exported_structs = Vec::new();

        for item in &file.items {
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

        if self.diagnostics.is_empty() {
            Ok(ModuleExports {
                module_name,
                functions: exported_fns,
                constants: exported_consts,
                structs: exported_structs,
            })
        } else {
            Err(self.diagnostics)
        }
    }

    fn check_fn(&mut self, func: &FnDef) {
        if func.body.is_none() {
            return; // intrinsic, no body to check
        }

        self.push_scope();

        // Bind parameters
        for param in &func.params {
            let ty = self.resolve_type(&param.ty.node);
            self.define_var(&param.name.node, ty, false);
        }

        let body = func.body.as_ref().unwrap();
        self.check_block(&body.node);

        self.pop_scope();
    }

    fn check_block(&mut self, block: &Block) -> Ty {
        self.push_scope();
        for stmt in &block.stmts {
            self.check_stmt(&stmt.node, stmt.span);
        }
        let ty = if let Some(tail) = &block.tail_expr {
            self.check_expr(&tail.node, tail.span)
        } else {
            Ty::Unit
        };
        self.pop_scope();
        ty
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
                        self.define_var(&name.node, resolved_ty, *mutable);
                    }
                    Pattern::Tuple(names) => {
                        // Destructure: type must be a tuple with matching arity
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
                    self.error(
                        "cannot assign to immutable variable".to_string(),
                        place.span,
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
                        self.error(
                            "loop end must be a compile-time constant, or use 'bounded N'"
                                .to_string(),
                            end.span,
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
                if let Ty::Tuple(elem_tys) = &val_ty {
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
                    for name in names {
                        if let Some(info) = self.lookup_var(&name.node) {
                            if !info.mutable {
                                self.error(
                                    format!("cannot assign to immutable variable '{}'", name.node),
                                    name.span,
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
                self.check_event_stmt(event_name, fields);
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
                self.error(format!("undefined variable '{}'", name), span);
                Ty::Field
            }
            Expr::BinOp { op, lhs, rhs } => {
                let lhs_ty = self.check_expr(&lhs.node, lhs.span);
                let rhs_ty = self.check_expr(&rhs.node, rhs.span);
                self.check_binop(*op, &lhs_ty, &rhs_ty, span)
            }
            Expr::Call { path, args } => {
                let fn_name = path.node.as_dotted();
                let arg_tys: Vec<Ty> = args
                    .iter()
                    .map(|a| self.check_expr(&a.node, a.span))
                    .collect();

                if let Some(sig) = self.functions.get(&fn_name).cloned() {
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
                    sig.return_ty
                } else {
                    self.error(format!("undefined function '{}'", fn_name), span);
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
                    self.error(format!("undefined struct '{}'", struct_name), span);
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
                } else if lhs == &Ty::XField && rhs == &Ty::XField {
                    Ty::XField
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
                if lhs != &Ty::XField || rhs != &Ty::Field {
                    self.error(
                        format!(
                            "operator '*.' requires XField and Field, got {} and {}",
                            lhs.display(),
                            rhs.display()
                        ),
                        span,
                    );
                }
                Ty::XField
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

    fn resolve_type(&self, ty: &Type) -> Ty {
        match ty {
            Type::Field => Ty::Field,
            Type::XField => Ty::XField,
            Type::Bool => Ty::Bool,
            Type::U32 => Ty::U32,
            Type::Digest => Ty::Digest,
            Type::Array(inner, n) => Ty::Array(Box::new(self.resolve_type(inner)), *n),
            Type::Tuple(elems) => Ty::Tuple(elems.iter().map(|t| self.resolve_type(t)).collect()),
            Type::Named(path) => {
                let name = path.as_dotted();
                if let Some(sty) = self.structs.get(&name) {
                    Ty::Struct(sty.clone())
                } else {
                    Ty::Field // fallback for unknown types
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

    fn register_builtins(&mut self) {
        let b = &mut self.functions;

        // I/O
        b.insert(
            "pub_read".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Field,
            },
        );
        b.insert(
            "pub_read2".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Tuple(vec![Ty::Field; 2]),
            },
        );
        b.insert(
            "pub_read3".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Tuple(vec![Ty::Field; 3]),
            },
        );
        b.insert(
            "pub_read4".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Tuple(vec![Ty::Field; 4]),
            },
        );
        b.insert(
            "pub_read5".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Digest,
            },
        );

        b.insert(
            "pub_write".into(),
            FnSig {
                params: vec![("v".into(), Ty::Field)],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "pub_write2".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field), ("b".into(), Ty::Field)],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "pub_write3".into(),
            FnSig {
                params: vec![
                    ("a".into(), Ty::Field),
                    ("b".into(), Ty::Field),
                    ("c".into(), Ty::Field),
                ],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "pub_write4".into(),
            FnSig {
                params: vec![
                    ("a".into(), Ty::Field),
                    ("b".into(), Ty::Field),
                    ("c".into(), Ty::Field),
                    ("d".into(), Ty::Field),
                ],
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "pub_write5".into(),
            FnSig {
                params: vec![
                    ("a".into(), Ty::Field),
                    ("b".into(), Ty::Field),
                    ("c".into(), Ty::Field),
                    ("d".into(), Ty::Field),
                    ("e".into(), Ty::Field),
                ],
                return_ty: Ty::Unit,
            },
        );

        // Non-deterministic input
        b.insert(
            "divine".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Field,
            },
        );
        b.insert(
            "divine3".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Tuple(vec![Ty::Field; 3]),
            },
        );
        b.insert(
            "divine5".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Digest,
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
                params: vec![("a".into(), Ty::Digest), ("b".into(), Ty::Digest)],
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

        // U32 operations
        b.insert(
            "split".into(),
            FnSig {
                params: vec![("a".into(), Ty::Field)],
                return_ty: Ty::Tuple(vec![Ty::U32, Ty::U32]),
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

        // Hash operations
        b.insert(
            "hash".into(),
            FnSig {
                params: (0..10).map(|i| (format!("x{}", i), Ty::Field)).collect(),
                return_ty: Ty::Digest,
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
                params: (0..10).map(|i| (format!("x{}", i), Ty::Field)).collect(),
                return_ty: Ty::Unit,
            },
        );
        b.insert(
            "sponge_squeeze".into(),
            FnSig {
                params: vec![],
                return_ty: Ty::Array(Box::new(Ty::Field), 10),
            },
        );
        b.insert(
            "sponge_absorb_mem".into(),
            FnSig {
                params: vec![("ptr".into(), Ty::Field)],
                return_ty: Ty::Unit,
            },
        );

        // Merkle operations
        b.insert(
            "merkle_step".into(),
            FnSig {
                params: vec![
                    ("idx".into(), Ty::U32),
                    ("d0".into(), Ty::Field),
                    ("d1".into(), Ty::Field),
                    ("d2".into(), Ty::Field),
                    ("d3".into(), Ty::Field),
                    ("d4".into(), Ty::Field),
                ],
                return_ty: Ty::Tuple(vec![Ty::U32, Ty::Digest]),
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
                return_ty: Ty::Digest, // returns [Field; 5] by default; actual size from context
            },
        );
        b.insert(
            "ram_write_block".into(),
            FnSig {
                params: vec![("addr".into(), Ty::Field), ("d".into(), Ty::Digest)],
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

        // XField
        b.insert(
            "xfield".into(),
            FnSig {
                params: vec![
                    ("a".into(), Ty::Field),
                    ("b".into(), Ty::Field),
                    ("c".into(), Ty::Field),
                ],
                return_ty: Ty::XField,
            },
        );
        b.insert(
            "xinvert".into(),
            FnSig {
                params: vec![("a".into(), Ty::XField)],
                return_ty: Ty::XField,
            },
        );
    }
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
}
