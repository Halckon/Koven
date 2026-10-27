//! `Inout` in the refined CFG (SPEC-0357): `&local` arguments lend the binding's root place
//! exclusively and read the replaced value back after the call; an `Inout` parameter's
//! target is replaced through the caller's loan.
use lang_frontend::{
    ast::ExpressionId, name_resolution::UnitSymbolId, parser::Expression, source::Span,
};

use super::Builder;
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};

/// A binding lent to an `Inout` parameter; its new value is read back from `place`.
pub(super) struct Writeback {
    symbol: UnitSymbolId,
    place: Slot,
    move_only: bool,
    span: Span,
}

impl Builder<'_, '_> {
    /// `&local`: an exclusive loan of the binding's root place, live until the call returns;
    /// `&p` for an `Inout` parameter passes its loan on (a reborrow, nothing to write back).
    pub(super) fn inout_argument(
        &mut self,
        expression: ExpressionId,
    ) -> Result<(Slot, Option<Writeback>), LoweringError> {
        let (payload, span) = self.node(expression)?;
        let Expression::Name = payload else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let symbol = self.reference(span)?;
        if let Some(loan) = self.inout_loan(symbol) {
            return Ok((loan, None));
        }
        let owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let EntityType::Value(target) = self.body.slots[owner.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            },
            span,
        )?;
        let move_only = self.move_only(self.expression_type(expression)?);
        Ok((
            loan,
            Some(Writeback {
                symbol,
                place,
                move_only,
                span,
            }),
        ))
    }

    /// After the loan ended: the binding now names the value the callee left in its place;
    /// a MoveOnly owner is taken back from the place.
    pub(super) fn write_back(&mut self, writeback: Writeback) -> Result<(), LoweringError> {
        let binding = *self
            .bindings
            .get(&writeback.symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, writeback.span))?;
        let EntityType::Place(target) = self.body.slots[writeback.place.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, writeback.span));
        };
        let op = if writeback.move_only {
            ROp::RootPlaceTake {
                owner: binding,
                place: writeback.place,
            }
        } else {
            ROp::ReadPlace {
                place: writeback.place,
            }
        };
        let value = self.value(op, EntityType::Value(target), writeback.span)?;
        self.push(ROp::Alias { source: value }, vec![binding], writeback.span)?;
        self.narrowed.remove(&writeback.symbol);
        Ok(())
    }

    /// The exclusive loan of an `Inout` parameter, if `symbol` is one.
    pub(super) fn inout_loan(&self, symbol: UnitSymbolId) -> Option<Slot> {
        self.borrowed.get(&symbol).copied().filter(|loan| {
            matches!(
                self.body.slots[loan.0],
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    ..
                }
            )
        })
    }
}
