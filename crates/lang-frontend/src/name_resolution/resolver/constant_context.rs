//! Companion 常量的禁止上下文只影响诊断，不把跳过的 instance scope 注入 lookup。

use super::*;

impl Resolver<'_> {
    pub(super) fn check_constant_this(&mut self, span: Span) -> Result<(), NameResolutionError> {
        if self.constant_owner.is_some() {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                self.constant_context_code,
                "companion constant has no instance receiver",
                span,
            )?);
        }
        Ok(())
    }

    pub(super) fn report_unresolved_reference(
        &mut self,
        span: Span,
        namespace: Namespace,
        name: &str,
    ) -> Result<(), NameResolutionError> {
        let forbidden = self
            .constant_owner
            .and_then(|owner| {
                let state = &self.scopes[owner.0];
                match namespace {
                    Namespace::Type => state.types.get(name),
                    // A type-parameter receiver may reach value lookup after type fallback skipped it.
                    Namespace::Value => state.values.get(name).or_else(|| {
                        state.types.get(name).filter(|binding| matches!(binding,
                            Binding::Single(symbol) if self.symbols[symbol.0].kind() == SymbolKind::TypeParameter))
                    }),
                }
            })
            .is_some_and(|binding| match binding {
                Binding::Functions(_) => namespace == Namespace::Value,
                Binding::Single(symbol) => matches!(
                    self.symbols[symbol.0].kind(),
                    SymbolKind::Field | SymbolKind::TypeParameter
                ),
            });
        let (code, message) = if forbidden {
            (
                self.constant_context_code,
                "companion constant cannot use an instance member or enclosing type parameter",
            )
        } else {
            (self.unresolved_code, "unresolved name")
        };
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            span,
        )?);
        Ok(())
    }
}
