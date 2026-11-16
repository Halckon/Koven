//! Sparse, immutable candidates for the existing straight-line borrow-call gate.
//! This is a scan cache, not a new admission proof or a CFG index.
use crate::{
    ast::ExpressionId,
    parser::{Expression, Item, ParsedFile, PrefixOperator, Statement},
    source::Span,
};

#[derive(Default)]
pub(super) struct AdmissionIndex {
    functions: Vec<Span>,
    function_blocks: Vec<Span>,
    call_blocks: Vec<Span>,
    nothing: Vec<Span>,
    #[cfg(test)]
    visits: std::cell::Cell<usize>,
}

impl AdmissionIndex {
    pub(super) fn new(parsed: &ParsedFile, is_nothing: impl Fn(ExpressionId) -> bool) -> Self {
        let mut index = Self::default();
        for (_, node) in parsed.ast().items().iter() {
            index.visit();
            if matches!(node.payload(), Item::Function { .. }) {
                index.functions.push(node.span());
            }
        }
        for (expression, node) in parsed.ast().expressions().iter() {
            index.visit();
            if matches!(
                node.payload(),
                Expression::If { .. }
                    | Expression::When { .. }
                    | Expression::Return { .. }
                    | Expression::Binary { .. }
            ) {
                index.function_blocks.push(node.span());
            }
            if matches!(
                node.payload(),
                Expression::NonNullAssert { .. }
                    | Expression::Prefix {
                        operator: PrefixOperator::Minus,
                        ..
                    }
            ) {
                index.call_blocks.push(node.span());
            }
            if is_nothing(expression) {
                index.nothing.push(node.span());
            }
        }
        for (_, node) in parsed.ast().statements().iter() {
            index.visit();
            if matches!(
                node.payload(),
                Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
            ) {
                index.function_blocks.push(node.span());
            }
        }
        index
    }

    pub(super) fn allows(&self, parsed: &ParsedFile, call: ExpressionId) -> bool {
        let Ok(node) = parsed.ast().expressions().get(call) else {
            return false;
        };
        let call = node.span();
        // Keep original iteration order, including min_by_key's equal-length tie rule.
        let Some(function) = self
            .functions
            .iter()
            .copied()
            .filter(|span| {
                self.visit();
                contains(*span, call)
            })
            .min_by_key(|span| span.end() - span.start())
        else {
            return false;
        };
        !self.function_blocks.iter().any(|span| {
            self.visit();
            contains(function, *span)
        }) && !self.call_blocks.iter().any(|span| {
            self.visit();
            contains(call, *span)
        }) && !self.nothing.iter().any(|span| {
            self.visit();
            // Preserve the old typed-Nothing check's offset-only containment.
            call.start() <= span.start() && span.end() <= call.end()
        })
    }

    #[inline]
    fn visit(&self) {
        #[cfg(test)]
        self.visits.set(self.visits.get() + 1);
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

#[cfg(test)]
mod tests;
