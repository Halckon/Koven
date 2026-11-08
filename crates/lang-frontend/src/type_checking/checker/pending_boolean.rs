//! Boolean outcomes keep the reaching iteration bindings until a consumer selects an edge.
use super::*;
use crate::parser::{Expression, LiteralKind, PrefixOperator};

#[derive(Clone, Default)]
pub(super) struct PendingBooleanEdges {
    pub(super) yes: Option<BTreeSet<SymbolId>>,
    pub(super) no: Option<BTreeSet<SymbolId>>,
}

impl PendingBooleanEdges {
    pub(super) fn bindings(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.yes.iter().chain(&self.no).flatten().copied()
    }

    pub(super) fn unknown(bindings: &BTreeSet<SymbolId>) -> Self {
        Self {
            yes: Some(bindings.clone()),
            no: Some(bindings.clone()),
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        if let Some(yes) = other.yes {
            self.yes.get_or_insert_with(BTreeSet::new).extend(yes);
        }
        if let Some(no) = other.no {
            self.no.get_or_insert_with(BTreeSet::new).extend(no);
        }
    }
}

impl Checker<'_> {
    pub(super) fn pending_boolean_edges(
        &self,
        mut expression: ExpressionId,
        falls_through: bool,
    ) -> Result<PendingBooleanEdges, TypeCheckingError> {
        if !self.pending_expression_exit(expression, falls_through) {
            return Ok(PendingBooleanEdges::default());
        }
        let mut negated = false;
        let mut edges = loop {
            match self.ast().expressions().get(expression)?.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    expression = *operand;
                    negated = !negated;
                }
                Expression::Literal(LiteralKind::Boolean(value)) => {
                    let bindings = &self.iteration_recovery.bindings;
                    break PendingBooleanEdges {
                        yes: value.then(|| bindings.clone()),
                        no: (!value).then(|| bindings.clone()),
                    };
                }
                _ => {
                    break self
                        .iteration_recovery
                        .boolean_edges
                        .get(&expression.index())
                        .cloned()
                        .unwrap_or_else(|| {
                            PendingBooleanEdges::unknown(&self.iteration_recovery.bindings)
                        });
                }
            }
        };
        if negated {
            std::mem::swap(&mut edges.yes, &mut edges.no);
        }
        Ok(edges)
    }

    pub(super) fn pending_branch_edges(
        &self,
        statement: StatementId,
    ) -> Result<PendingBooleanEdges, TypeCheckingError> {
        if let Some(tail) =
            crate::type_checking::control_body::tail_expression(self.parsed, statement)?
        {
            self.pending_boolean_edges(tail, true)
        } else {
            Ok(PendingBooleanEdges::unknown(
                &self.iteration_recovery.bindings,
            ))
        }
    }

    pub(super) fn finish_pending_boolean(
        &mut self,
        expression: ExpressionId,
        edges: PendingBooleanEdges,
    ) {
        let recovery = &mut self.iteration_recovery;
        recovery.bindings.clear();
        recovery.bindings.extend(edges.bindings());
        if edges.yes.is_none() && edges.no.is_none() {
            recovery.abrupt_expressions.insert(expression.index());
        } else if edges.yes != edges.no {
            recovery.boolean_edges.insert(expression.index(), edges);
        }
    }
}
