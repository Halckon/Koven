use super::*;

impl Checker<'_> {
    pub(super) fn text_extension_candidates(
        &mut self,
        receiver: ExpressionId,
        ty: TypeId,
        category: ExpressionCategory,
        name: Span,
    ) -> Result<Vec<CallCandidate>, TypeCheckingError> {
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
                .and_then(|d| d.text_extension())
            else {
                continue;
            };
            if binding.callable() != CallableTarget::Source(symbol) || binding.receiver_type() != ty
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
}
