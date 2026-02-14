/// Semantic types used by the type checker (distinct from AST syntactic types).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Field,
    /// Extension field element — width in base field elements (e.g. 3 for Triton's cubic extension).
    XField(u32),
    Bool,
    U32,
    /// Hash digest — width in field elements (e.g. 5 for Tip5, 4 for RPO).
    Digest(u32),
    Array(Box<Ty>, u64),
    Tuple(Vec<Ty>),
    Struct(StructTy),
    Unit,
}

/// A resolved struct type with field layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructTy {
    pub name: String,
    pub fields: Vec<(String, Ty, bool)>, // (name, type, is_pub)
}

impl StructTy {
    pub fn width(&self) -> u32 {
        self.fields.iter().map(|(_, ty, _)| ty.width()).sum()
    }

    /// Get a field's type and its offset from the "top" of the struct on the stack.
    /// Fields are pushed in order, so first field is deepest.
    /// Returns (type, offset_from_top, is_pub).
    pub fn field_offset(&self, field_name: &str) -> Option<(Ty, u32, bool)> {
        let total = self.width();
        let mut offset = 0u32;
        for (name, ty, is_pub) in &self.fields {
            if name == field_name {
                // Offset from top = total - offset - field_width
                let from_top = total - offset - ty.width();
                return Some((ty.clone(), from_top, *is_pub));
            }
            offset += ty.width();
        }
        None
    }
}

impl Ty {
    /// Width in field elements (compile-time known for all types).
    pub fn width(&self) -> u32 {
        match self {
            Ty::Field | Ty::Bool | Ty::U32 => 1,
            Ty::XField(w) => *w,
            Ty::Digest(w) => *w,
            Ty::Array(inner, n) => inner.width() * (*n as u32),
            Ty::Tuple(elems) => elems.iter().map(|t| t.width()).sum(),
            Ty::Struct(s) => s.width(),
            Ty::Unit => 0,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Ty::Field => "Field".to_string(),
            Ty::XField(_) => "XField".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::U32 => "U32".to_string(),
            Ty::Digest(_) => "Digest".to_string(),
            Ty::Array(inner, n) => format!("[{}; {}]", inner.display(), n),
            Ty::Tuple(elems) => {
                let parts: Vec<_> = elems.iter().map(|t| t.display()).collect();
                format!("({})", parts.join(", "))
            }
            Ty::Struct(s) => s.name.clone(),
            Ty::Unit => "()".to_string(),
        }
    }
}
