/// A source location: file ID + byte offset range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub file_id: u16,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file_id: u16, start: u32, end: u32) -> Self {
        Self {
            file_id,
            start,
            end,
        }
    }

    pub fn dummy() -> Self {
        Self {
            file_id: 0,
            start: 0,
            end: 0,
        }
    }

    pub fn merge(self, other: Span) -> Span {
        debug_assert_eq!(self.file_id, other.file_id);
        Span {
            file_id: self.file_id,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A value annotated with its source span.
#[derive(Clone, Debug)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }

    pub fn dummy(node: T) -> Self {
        Self {
            node,
            span: Span::dummy(),
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            node: f(self.node),
            span: self.span,
        }
    }
}
