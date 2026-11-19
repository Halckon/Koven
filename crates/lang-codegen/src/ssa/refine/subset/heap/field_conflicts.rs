//! Admission checks during an owned direct-field exchange.
use super::*;

impl Checker<'_, '_> {
    pub(super) fn field_replace_rhs_conflicts(
        &self,
        rhs: ExpressionId,
        owner_symbol: lang_frontend::name_resolution::UnitSymbolId,
        field: lang_frontend::name_resolution::UnitSymbolId,
    ) -> bool {
        let Ok(rhs_node) = self.inputs.parsed.ast().expressions().get(rhs) else {
            return true;
        };
        let rhs_span = rhs_node.span();
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expr_id, node)| {
                let span = node.span();
                rhs_span.source_id() == span.source_id()
                    && rhs_span.start() <= span.start()
                    && span.end() <= rhs_span.end()
                    && matches!(node.payload(), Expression::Name)
                    && self.reference(expr_id) == Some(owner_symbol)
                    && !self.is_disjoint_nested_field_replace_receiver(
                        rhs_span,
                        expr_id,
                        owner_symbol,
                        field,
                    )
                    && !self.is_disjoint_sibling_field_borrow(rhs_span, expr_id, field)
            })
    }
}
