//! Preserve resolved associated constant identity; full evaluation/capability follows separately.
use super::{BodyChecker, CompilationUnitTypeError};
use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::{DeclarationVisibility, Namespace, SourceUnitId, UnitReferenceTarget},
    source::Span,
    type_checking::{UnitExpressionId, UnitTypeId},
};

impl BodyChecker<'_> {
    /// Consume the unit resolver's static target; do not derive a target from its printed path.
    pub(super) fn check_selected_constant(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        name_span: Span,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let Some(UnitReferenceTarget::Symbol(target)) =
            self.reference(source, name_span, Namespace::Value)
        else {
            return Ok(None);
        };
        let target = *target;
        let Some((owner, visibility)) = self.signatures.constant_declaration(target) else {
            return Ok(None);
        };
        if visibility == DeclarationVisibility::Private && self.current_owner != Some(owner) {
            self.emit(
                codes::INVISIBLE_ASSOCIATED_CONSTANT,
                "associated constant is private to its declaring classifier",
                name_span,
            )?;
            return Ok(Some(self.error_type()));
        }
        let ty = self
            .parts
            .symbol_types
            .get(&target)
            .copied()
            .or_else(|| self.signatures.symbol_type(target))
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        self.parts
            .constant_selections
            .insert(UnitExpressionId::new(source, expression), target);
        Ok(Some(ty))
    }
}
