use super::*;
use crate::source::Span;

impl BodyChecker<'_> {
    pub(super) fn text_extension_candidates(
        &self,
        source: SourceUnitId,
        ty: UnitTypeId,
        name: Span,
    ) -> Result<Vec<CallCandidate>, CompilationUnitTypeError> {
        let declarations = match self
            .names
            .value_lookup_hints()
            .iter()
            .find(|hint| hint.source_unit() == source && hint.span() == name)
            .map(|hint| hint.target())
        {
            Some(UnitReferenceTarget::Declaration(id)) => vec![*id],
            Some(UnitReferenceTarget::OverloadSet(ids)) => ids.clone(),
            _ => return Ok(Vec::new()),
        };
        let mut candidates = Vec::new();
        for declaration in declarations {
            let Some(callable) = self
                .signatures
                .declaration(declaration)
                .and_then(|d| d.callable())
            else {
                continue;
            };
            let Some(binding) = callable.text_extension() else {
                continue;
            };
            if binding.callable()
                != crate::type_checking::UnitCallableTarget::Declaration(declaration)
                || binding.receiver_type() != ty
            {
                continue;
            }
            let mut candidate = CallCandidate::from_signature(declaration, callable);
            candidate.receiver = Some((ParameterMode::Borrow, ty));
            candidates.push(candidate);
        }
        Ok(candidates)
    }
}
