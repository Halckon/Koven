//! `RangeView` reads in the refined CFG: the loan a `for` iterates and `View<T>.size`.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{RangeUseSite, UnitDropPoint},
    source::Span,
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, SsaTypeId},
    refine::body::{ROp, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn range_iteration_loan(
        &mut self,
        source: ExpressionId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let actual = self.ungrouped_expr(source)?;
        let id = self.unit(actual);
        if self
            .inputs
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .any(|fact| fact.expression() == id && fact.site() == RangeUseSite::Iteration)
        {
            let Some((view, protecting, continued)) = self.lower_range_call(actual, span)? else {
                return Ok(None);
            };
            self.short_ranges.insert(id, (view, protecting, continued));
            let place = self.value(ROp::RootPlace { owner: view }, EntityType::Place(ty), span)?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                span,
            )?;
            return Ok(Some(loan));
        }
        let symbol = self.named_container_symbol(source, span)?;
        if let Some(&view) = self.range_views.get(&symbol) {
            let place = self.value(ROp::RootPlace { owner: view }, EntityType::Place(ty), span)?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                span,
            )?;
            return Ok(Some(loan));
        }
        let caller_loan = *self
            .borrowed
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let loan = self.value(
            ROp::SharedReborrow {
                source: caller_loan,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            },
            span,
        )?;
        Ok(Some(loan))
    }

    pub(super) fn lower_range_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .range_size(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let view = if let Some(loan) = self.this_receiver_loan(receiver)? {
            loan
        } else {
            let symbol = self.named_container_symbol(receiver, span)?;
            self.range_views
                .get(&symbol)
                .or_else(|| self.borrowed.get(&symbol))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
        };
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::RangeLength { view },
            EntityType::Value(result_ty),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }
}
