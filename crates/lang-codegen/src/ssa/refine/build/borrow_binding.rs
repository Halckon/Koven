//! Local borrow binding initialization and retirement, preserving original cleanup order.
use super::*;
use crate::ssa::{borrow_result_support::ordinary_local, model::LoanKind};
use lang_frontend::ownership_checking::BorrowBindingStorage;
impl Builder<'_, '_> {
    pub(super) fn local_borrow_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(initializer);
        let fact = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.storage() == BorrowBindingStorage::NewRangeDescriptor {
            let Some((view, loan, created)) = self.lower_range_call(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.range_views.insert(symbol, view);
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(Lowered::Unit);
        }
        if fact.storage() == BorrowBindingStorage::BorrowedCarrierMetadata
            && fact.source_loan().is_none()
            && let Ok(root) = self.ungrouped_expr(initializer)
            && let Ok(parent_sym) = self.reference(self.span(root))
            && let Some(&view) = self.range_views.get(&parent_sym)
        {
            if fact.parent() != Some(parent_sym) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let target = self.ssa_type(self.expression_type(initializer)?, span)?;
            let place = self.value(
                ROp::RootPlace { owner: view },
                EntityType::Place(target),
                span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                span,
            )?;
            self.borrowed.insert(symbol, loan);
            return Ok(Lowered::Unit);
        }
        let Some(source) = fact.source_loan() else {
            let Some((loan, created)) = self.lower_result_place(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(Lowered::Unit);
        };
        self.body.binding_plans.activate(symbol, id, source, span)?;
        let local_policy = ordinary_local::policy(
            self.inputs.parsed,
            self.inputs.typed,
            self.inputs.owned,
            &fact,
        );
        let Some((loan, created)) =
            self.lower_result_call(initializer, source, span, local_policy)?
        else {
            return Ok(Lowered::Diverged);
        };
        self.borrowed.insert(symbol, loan);
        self.result_source_loans.insert(symbol, created);
        self.body
            .binding_plans
            .commit(symbol, &self.borrowed, &self.result_source_loans, span)?;
        Ok(Lowered::Unit)
    }

    pub(super) fn end_result_bindings(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let ends: Vec<_> = self
            .inputs
            .owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|fact| fact.point() == point)
            .copied()
            .collect();
        // A control transfer ends the bindings on its own path only; the fall-through path
        // still holds them (SPEC-0393).
        let transfer = matches!(point, UnitDropPoint::ControlTransfer(_));
        for fact in ends {
            let loan = if transfer {
                self.borrowed.get(&fact.binding()).copied()
            } else {
                self.borrowed.remove(&fact.binding())
            };
            self.body.binding_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            let Some(loan) = loan else {
                continue;
            };
            let span = self
                .inputs
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .find(|b| b.binding() == fact.binding())
                .map_or(self.inputs.span, |b| b.marker_span());
            let view = if transfer {
                self.range_views.get(&fact.binding()).copied()
            } else {
                self.range_views.remove(&fact.binding())
            };
            if let Some(view) = view {
                self.push(ROp::RangeEnd { view, source: loan }, Vec::new(), span)?;
            } else {
                self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            }
            let sources = if transfer {
                self.result_source_loans.get(&fact.binding()).cloned()
            } else {
                self.result_source_loans.remove(&fact.binding())
            };
            if let Some(sources) = sources {
                for source in sources.into_iter().rev() {
                    self.push(ROp::BorrowEnd { loan: source }, Vec::new(), span)?;
                }
            }
        }
        Ok(())
    }
}
