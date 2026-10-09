//! unit receiver 首片复用已解析的 package/import 可见性和 canonical 声明。
use super::*;
use crate::{
    source::Span,
    type_checking::{IntrinsicTypeConstructor, RangeSourceKind},
};
impl BodyChecker<'_> {
    pub(super) fn range_extension_candidates(
        &self,
        source: SourceUnitId,
        ty: UnitTypeId,
        name: Span,
    ) -> Result<Vec<CallCandidate>, CompilationUnitTypeError> {
        let (kind, element) = match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            }) if arguments.len() == 1 => (RangeSourceKind::List, arguments[0]),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            }) if arguments.len() == 1 => (RangeSourceKind::View, arguments[0]),
            _ => return Ok(Vec::new()),
        };
        let declarations = match self
            .names
            .names()
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
            let Some(binding) = callable.range_extension() else {
                continue;
            };
            if binding.callable()
                != crate::type_checking::UnitCallableTarget::Declaration(declaration)
                || binding.source_kind() != kind
                || !(matches!(
                    self.signatures.types().get(binding.element_type()),
                    Some(UnitTypeKind::TypeParameter(_))
                ) || binding.element_type() == element)
            {
                continue;
            }
            let mut candidate = CallCandidate::from_signature(declaration, callable);
            candidate.receiver = Some((binding.receiver_mode(), ty));
            candidates.push(candidate);
        }
        Ok(candidates)
    }
    pub(super) fn range_extension_receiver_types(
        &self,
        candidate: &CallCandidate,
    ) -> Option<(UnitTypeId, UnitTypeId)> {
        let UnitCallTarget::Declaration(declaration) = candidate.target else {
            return None;
        };
        let binding = self
            .signatures
            .declaration(declaration)?
            .callable()?
            .range_extension()?;
        Some((binding.receiver_type(), candidate.receiver?.1))
    }
}
