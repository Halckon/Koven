//! Final admission of local bindings and recovery from diagnosed patterns.
use super::*;

impl<F: Facts> Checker<'_, F> {
    pub(super) fn binding(
        &mut self,
        marker: NameMarker,
        complete: bool,
        discard_allowed: bool,
    ) -> Result<bool> {
        let NameMarker::Present(span) = marker else {
            return Ok(false);
        };
        let Some((symbol, ty)) = self.facts.declaration(span) else {
            // 只有 for/解构的无 binding discard 可免除值事实；普通 val _ 仍是真 symbol。
            if discard_allowed && self.sources.slice(span).map_err(TypeCheckingError::from)? == "_"
            {
                return Ok(complete);
            }
            return if !complete || self.existing_error(span) {
                Ok(false)
            } else {
                Err(self.missing(span, "declaration type"))
            };
        };
        if !complete {
            self.poisoned.insert(symbol);
            return Ok(false);
        }
        let complete = self.type_root(ty, span, false)?;
        if !complete {
            self.poisoned.insert(symbol);
        }
        Ok(complete)
    }

    pub(super) fn local_destructuring_bindings(
        &mut self,
        bindings: &[NameMarker],
        left: Span,
        right: Option<Span>,
        equals: Option<Span>,
        mut complete: bool,
    ) -> Result<bool> {
        let end = right
            .map(Span::end)
            .or_else(|| equals.map(Span::start))
            .or_else(|| {
                bindings.last().map(|marker| match *marker {
                    NameMarker::Present(span)
                    | NameMarker::Missing(span)
                    | NameMarker::Error(span) => span.end(),
                })
            })
            .unwrap_or(left.end());
        let pattern = self
            .sources
            .span(left.source_id(), left.start(), end)
            .map_err(TypeCheckingError::from)?;
        // A diagnosed pattern cannot initialize any of its bindings.
        complete &= !self.existing_error(pattern);
        for binding in bindings {
            complete &= self.binding(*binding, complete, true)?;
        }
        Ok(complete)
    }
}
