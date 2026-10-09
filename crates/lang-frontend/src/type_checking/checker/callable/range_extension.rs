//! 签名首片：词法可见且拥有独立授权的 compiler-bound receiver 候选。
use super::*;
use crate::type_checking::RangeSourceKind;
impl Checker<'_> {
    pub(super) fn range_extension_candidates(
        &mut self,
        receiver: ExpressionId,
        ty: TypeId,
        category: ExpressionCategory,
        name: Span,
    ) -> Result<Vec<CallCandidate>, TypeCheckingError> {
        let (kind, element) = match self.kind(ty) {
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            } if arguments.len() == 1 => (RangeSourceKind::List, arguments[0]),
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            } if arguments.len() == 1 => (RangeSourceKind::View, arguments[0]),
            _ => return Ok(Vec::new()),
        };
        let symbols = match self.value_lookup_hints.get(&(name.start(), name.end())) {
            Some(ReferenceTarget::Symbol(symbol)) => vec![*symbol],
            Some(ReferenceTarget::OverloadSet(symbols)) => symbols.clone(),
            _ => return Ok(Vec::new()),
        };
        let mut candidates = Vec::new();
        for symbol in symbols {
            let Some(binding) = self
                .typed_callables
                .iter()
                .find(|d| d.symbol() == symbol)
                .and_then(|d| d.range_extension())
            else {
                continue;
            };
            if binding.callable() != CallableTarget::Source(symbol)
                || binding.source_kind() != kind
                || !(matches!(
                    self.kind(binding.element_type()),
                    TypeKind::TypeParameter(_)
                ) || binding.element_type() == element)
            {
                continue;
            }
            if let Some(candidate) = self.source_candidate(
                symbol,
                BTreeMap::new(),
                Vec::new(),
                Some((receiver, ty, category)),
            )? {
                candidates.push(candidate);
            }
        }
        Ok(candidates)
    }
    pub(super) fn range_extension_receiver_types(
        &self,
        candidate: &CallCandidate,
    ) -> Option<(TypeId, TypeId)> {
        let CallableTarget::Source(symbol) = candidate.target else {
            return None;
        };
        let binding = self
            .typed_callables
            .iter()
            .find(|d| d.symbol() == symbol)?
            .range_extension()?;
        Some((binding.receiver_type(), candidate.receiver?.ty()))
    }
}
