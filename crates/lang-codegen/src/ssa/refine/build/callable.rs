//! The closure family in the refined CFG (SPEC-0364, ADR-0034): calls through a callable
//! view, views forwarded to Borrow function parameters, and lambda literals formed as
//! function pointers or closures and lent to the callee as views.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode},
    parser::Expression,
    source::Span,
};

use super::Builder;
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, SsaTypeId},
    refine::body::{ROp, Slot},
};

impl Builder<'_, '_> {
    /// The callable view loan of `callee`, a view parameter.
    pub(super) fn view_callee(&self, callee: ExpressionId) -> Result<Slot, LoweringError> {
        let mut callee = callee;
        while let (Expression::Group { expression: inner }, _) = self.node(callee)? {
            callee = inner;
        }
        let (Expression::Name, span) = self.node(callee)? else {
            return Err(error(LoweringErrorKind::MissingFact, self.span(callee)));
        };
        let symbol = self.reference(span)?;
        self.borrowed
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    /// Facts inside a lambda body belong to its thunk, not to this function's body.
    pub(super) fn inside_lambda(&self, origin: Span) -> bool {
        self.inputs.callables.keys().any(|lambda| {
            self.inputs
                .parsed
                .ast()
                .expressions()
                .get(lambda.expression())
                .is_ok_and(|node| {
                    let span = node.span();
                    origin != span
                        && origin.source_id() == span.source_id()
                        && span.start() <= origin.start()
                        && origin.end() <= span.end()
                })
        })
    }

    /// The view argument for a Borrow function parameter whose view target is `view`, and
    /// the loans to end after the call (in creation order).
    pub(super) fn callable_argument(
        &mut self,
        value: ExpressionId,
        view: SsaTypeId,
        span: Span,
    ) -> Result<(Slot, Vec<Slot>), LoweringError> {
        let mut root = value;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let view_loan = EntityType::Loan {
            kind: LoanKind::Shared,
            target: view,
        };
        let (payload, root_span) = self.node(root)?;
        if let Expression::Name = payload {
            // A view parameter is passed on as a call-scoped re-borrow of the view.
            let source = self.view_callee(root)?;
            let forwarded = self.value(ROp::SharedReborrow { source }, view_loan, root_span)?;
            return Ok((forwarded, vec![forwarded]));
        }
        if !matches!(payload, Expression::Lambda { .. }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, root_span));
        }
        let owner = self.form_callable(root, root_span)?;
        let callable = self
            .inputs
            .callables
            .get(&self.unit(root))
            .map(|callable| callable.callable)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, root_span))?;
        let place = self.value(
            ROp::RootPlace { owner },
            EntityType::Place(callable),
            root_span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: callable,
            },
            root_span,
        )?;
        let lent = self.value(ROp::CallableViewOf { source: loan }, view_loan, span)?;
        Ok((lent, vec![loan, lent]))
    }

    /// Forms a lambda literal of this instance; the closure temporary's published drop
    /// releases it (and, in SSA, its shared capture loans).
    pub(super) fn form_callable(
        &mut self,
        lambda: ExpressionId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let callable = self
            .inputs
            .callables
            .get(&self.unit(lambda))
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ty = EntityType::Value(callable.callable);
        let slot = if callable.captures.is_empty() {
            self.value(
                ROp::FunctionAddress {
                    target: callable.thunk,
                },
                ty,
                span,
            )?
        } else {
            let mut captures = Vec::with_capacity(callable.captures.len());
            for capture in &callable.captures {
                captures.push(self.capture_operand(capture, span)?);
            }
            self.value(
                ROp::ClosureConstruct {
                    closure: callable.callable,
                    thunk: callable.thunk,
                    captures,
                },
                ty,
                span,
            )?
        };
        self.temporaries.insert(self.unit(lambda), slot);
        Ok(slot)
    }

    fn capture_operand(
        &mut self,
        capture: &crate::ssa::refine::RefineCapture,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let symbol = capture.symbol;
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: capture.ty,
        };
        match (capture.mode, capture.effect) {
            (ClosureCaptureMode::Shared, _) => {
                if let Some(source) = self.borrowed.get(&symbol).copied() {
                    return self.value(ROp::SharedReborrow { source }, shared, span);
                }
                let owner = *self
                    .bindings
                    .get(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let place = self.value(
                    ROp::RootPlace { owner },
                    EntityType::Place(capture.ty),
                    span,
                )?;
                self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    shared,
                    span,
                )
            }
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Copy) => {
                if let Some(loan) = self.borrowed.get(&symbol).copied() {
                    return self.value(ROp::ReadLoan { loan }, EntityType::Value(capture.ty), span);
                }
                self.bindings
                    .get(&symbol)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            }
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => self
                .bindings
                .get(&symbol)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }
}
