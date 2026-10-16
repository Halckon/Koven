//! Ordinary root-variable assignment keeps the existing immutable-place obligation.
use crate::name_resolution::ScopeKind;

use super::*;

impl Checker<'_> {
    pub(super) fn check_assignment_variable_mutability(
        &mut self,
        target: ExpressionId,
        place: &OwnershipPlace,
        primary: Span,
    ) -> Result<bool, OwnershipCheckingError> {
        if !place.is_root() || !self.is_assignment_variable(target)? {
            return Ok(true);
        }
        if self.is_mutable_place(target)? {
            return Ok(true);
        }
        let binding = &self.names.symbols()[place.root().index()];
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.immutable_inout_code,
            "assignment target is not a mutable place",
            primary,
        )?;
        diagnostic.add_label(
            self.sources,
            binding.span(),
            "immutable binding declared here",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(false)
    }

    fn is_assignment_variable(&self, target: ExpressionId) -> Result<bool, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(target)?;
        match node.payload() {
            Expression::Group { expression } => self.is_assignment_variable(*expression),
            Expression::Name => {
                let Some(symbol) = self.reference_symbol(node.span()) else {
                    return Ok(false);
                };
                let binding = &self.names.symbols()[symbol.index()];
                Ok(match binding.kind() {
                    SymbolKind::Variable => !matches!(
                        self.names.scopes()[binding.scope().index()].kind(),
                        ScopeKind::Classifier | ScopeKind::Companion
                    ),
                    SymbolKind::ValueParameter
                    | SymbolKind::LambdaParameter
                    | SymbolKind::ForBinding
                    | SymbolKind::DestructuringBinding => true,
                    _ => false,
                })
            }
            _ => Ok(false),
        }
    }
}
