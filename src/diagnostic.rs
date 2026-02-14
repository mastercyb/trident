use crate::span::Span;

/// A compiler diagnostic (error, warning, or hint).
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Diagnostic {
    pub fn error(message: String, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message,
            span,
            notes: Vec::new(),
            help: None,
        }
    }

    pub fn warning(message: String, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            message,
            span,
            notes: Vec::new(),
            help: None,
        }
    }

    pub fn with_note(mut self, note: String) -> Self {
        self.notes.push(note);
        self
    }

    pub fn with_help(mut self, help: String) -> Self {
        self.help = Some(help);
        self
    }

    /// Render the diagnostic to stderr using ariadne.
    pub fn render(&self, filename: &str, source: &str) {
        use ariadne::{Color, Label, Report, ReportKind, Source};

        let kind = match self.severity {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
        };

        let color = match self.severity {
            Severity::Error => Color::Red,
            Severity::Warning => Color::Yellow,
        };

        let mut report = Report::build(kind, filename, self.span.start as usize)
            .with_message(&self.message)
            .with_label(
                Label::new((filename, self.span.start as usize..self.span.end as usize))
                    .with_message(&self.message)
                    .with_color(color),
            );

        for note in &self.notes {
            report = report.with_note(note);
        }

        if let Some(help) = &self.help {
            report = report.with_help(help);
        }

        report
            .finish()
            .eprint((filename, Source::from(source)))
            .unwrap();
    }
}

/// Render a list of diagnostics.
pub fn render_diagnostics(diagnostics: &[Diagnostic], filename: &str, source: &str) {
    for diag in diagnostics {
        diag.render(filename, source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_construction() {
        let span = Span::new(0, 10, 15);
        let d = Diagnostic::error("type mismatch".to_string(), span);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "type mismatch");
        assert_eq!(d.span.start, 10);
        assert_eq!(d.span.end, 15);
        assert!(d.notes.is_empty());
        assert!(d.help.is_none());
    }

    #[test]
    fn test_warning_construction() {
        let span = Span::dummy();
        let d = Diagnostic::warning("unused variable".to_string(), span);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.message, "unused variable");
    }

    #[test]
    fn test_with_note() {
        let d = Diagnostic::error("error".to_string(), Span::dummy())
            .with_note("expected Field".to_string())
            .with_note("found U32".to_string());
        assert_eq!(d.notes.len(), 2);
        assert_eq!(d.notes[0], "expected Field");
        assert_eq!(d.notes[1], "found U32");
    }

    #[test]
    fn test_with_help() {
        let d = Diagnostic::error("error".to_string(), Span::dummy())
            .with_help("try as_field()".to_string());
        assert_eq!(d.help.as_deref(), Some("try as_field()"));
    }

    #[test]
    fn test_chained_builders() {
        let d = Diagnostic::warning("hint".to_string(), Span::new(0, 0, 5))
            .with_note("note 1".to_string())
            .with_help("help text".to_string())
            .with_note("note 2".to_string());
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.notes.len(), 2);
        assert!(d.help.is_some());
    }

    #[test]
    fn test_render_does_not_panic() {
        let source = "let x: Field = 1\nlet y: U32 = x\n";
        let d = Diagnostic::error("type mismatch".to_string(), Span::new(0, 18, 32))
            .with_note("expected U32, found Field".to_string());
        // Render to stderr — just verify it doesn't panic
        d.render("test.tri", source);
    }

    #[test]
    fn test_render_diagnostics_multiple() {
        let source = "let x = 1\nlet y = 2\n";
        let diagnostics = vec![
            Diagnostic::warning("unused x".to_string(), Span::new(0, 4, 5)),
            Diagnostic::warning("unused y".to_string(), Span::new(0, 14, 15)),
        ];
        // Just verify it doesn't panic
        render_diagnostics(&diagnostics, "test.tri", source);
    }

    #[test]
    fn test_render_warning_does_not_panic() {
        let source = "fn main() {\n    as_u32(x)\n}\n";
        let d = Diagnostic::warning("redundant range check".to_string(), Span::new(0, 16, 25))
            .with_help("x is already proven U32".to_string());
        d.render("test.tri", source);
    }
}
