//! The closure family in the refined CFG (SPEC-0364, SPEC-0365, SPEC-0370, ADR-0034):
//! lambda thunk bodies, local callable bindings, calls through callable views or local
//! callables, and Borrow String/field loans in closure bodies.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode, UnitDropPoint},
    parser::Expression,
    source::Span,
    type_checking::{BuiltinType, UnitAggregateProjectionKind, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityId, EntityType, LoanKind, SsaTypeId},
    refine::{
        RefineBody, RefineThunk,
        body::{ROp, Slot},
    },
};

pub(super) type LentArgument = (Slot, Vec<Slot>, Option<ExpressionId>);

impl Builder<'_, '_> {
    /// Binds a lambda thunk's optional borrowed environment, capture projections, and
    /// explicit parameters on entry.
    pub(super) fn bind_thunk_entry(
        &mut self,
        thunk: &RefineThunk,
        entities: &[EntityId],
    ) -> Result<(), LoweringError> {
        let env_count = usize::from(thunk.environment.is_some());
        if entities.len() != env_count + self.inputs.parameters.len() {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        let (env_entities, parameter_entities) = entities.split_at(env_count);
        if let (Some(environment), [EntityId::Loan(_)]) = (thunk.environment, env_entities) {
            let env_slot = self.body.slot(EntityType::Loan {
                kind: LoanKind::Shared,
                target: environment,
            });
            self.body.parameters.push(env_slot);
            for (field, capture) in thunk.captures.iter().enumerate() {
                let parent = self.value(
                    ROp::SharedFieldLoan {
                        base: env_slot,
                        field,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: capture.storage,
                    },
                    capture.span,
                )?;
                self.capture_loans.push(parent);
                if capture.mode == ClosureCaptureMode::Shared {
                    let target = self.ssa_type(capture.ty, capture.span)?;
                    let child = self.value(
                        ROp::SharedReferenceFollow { source: parent },
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        },
                        capture.span,
                    )?;
                    self.capture_loans.push(child);
                    self.borrowed.insert(capture.symbol, child);
                } else {
                    self.borrowed.insert(capture.symbol, parent);
                }
            }
        } else if thunk.environment.is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        self.bind_parameters(parameter_entities)?;
        self.drops(UnitDropPoint::LambdaEntry(thunk.expression))
    }

    /// Lowers a lambda thunk's body statement and emits its fallthrough return.
    pub(super) fn lower_thunk_body(
        &mut self,
        thunk: &RefineThunk,
        unit_return: bool,
    ) -> Result<(), LoweringError> {
        let lowered = self.statement(thunk.body)?;
        if lowered == Lowered::Diverged {
            return Ok(());
        }
        if unit_return {
            return self.finish_return(None, true, self.inputs.span);
        }
        let Lowered::Value(value) = lowered else {
            return Err(error(LoweringErrorKind::MissingFact, self.inputs.span));
        };
        let tail = thunk
            .result
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.inputs.span))?;
        let value = self.adapt(value, tail, self.inputs.return_type)?;
        self.deliver(tail);
        self.finish_return(Some(value), false, self.inputs.span)
    }

    /// Calls a view parameter (`CallableInvokeView`) or a local callable (`CallableInvoke`).
    pub(super) fn invoke_callable(
        &mut self,
        expression: ExpressionId,
        arguments: Vec<Slot>,
        unit_return: bool,
        span: Span,
    ) -> Result<(Lowered, Option<ExpressionId>), LoweringError> {
        let Expression::Call { callee, .. } = self.node(expression)?.0 else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let mut root = callee;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (Expression::Name, callee_span) = self.node(root)? else {
            return Err(error(LoweringErrorKind::MissingFact, self.span(callee)));
        };
        let symbol = self.reference(callee_span)?;
        let (op, consumed_callee) = if let Some(&view) = self.borrowed.get(&symbol) {
            (ROp::CallableInvokeView { view, arguments }, None)
        } else if let Some(&callable) = self.bindings.get(&symbol) {
            (
                ROp::CallableInvoke {
                    callable,
                    arguments,
                },
                Some(callee),
            )
        } else {
            return Err(error(LoweringErrorKind::MissingFact, callee_span));
        };
        let result = if unit_return {
            self.push(op, Vec::new(), span)?;
            Lowered::Unit
        } else {
            Lowered::Value(self.typed_value(op, expression)?)
        };
        Ok((result, consumed_callee))
    }

    /// Facts inside a nested lambda body belong to its thunk, not to this body.
    pub(super) fn inside_lambda(&self, origin: Span) -> bool {
        let current = match &self.inputs.body {
            RefineBody::Lambda(thunk) => Some(thunk.expression),
            _ => None,
        };
        let scope = self.inputs.scope_span();
        self.inputs.callables.keys().any(|lambda| {
            Some(*lambda) != current
                && self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(lambda.expression())
                    .is_ok_and(|node| {
                        let span = node.span();
                        scope.is_none_or(|outer| {
                            outer != span
                                && outer.source_id() == span.source_id()
                                && outer.start() <= span.start()
                                && span.end() <= outer.end()
                        }) && origin != span
                            && origin.source_id() == span.source_id()
                            && span.start() <= origin.start()
                            && origin.end() <= span.end()
                    })
        })
    }

    /// The view argument for a Borrow function parameter whose view target is `view`, the
    /// loans to end after the call (in creation order), and any named expression to drop-check.
    pub(super) fn callable_argument(
        &mut self,
        value: ExpressionId,
        view: SsaTypeId,
        span: Span,
    ) -> Result<LentArgument, LoweringError> {
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
            let symbol = self.reference(root_span)?;
            if let Some(&source) = self.borrowed.get(&symbol) {
                // A view parameter is passed on as a call-scoped re-borrow of the view.
                let forwarded = self.value(ROp::SharedReborrow { source }, view_loan, root_span)?;
                return Ok((forwarded, vec![forwarded], Some(value)));
            }
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, root_span))?;
            let EntityType::Value(callable) = self.body.slots[owner.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, root_span));
            };
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
            return Ok((lent, vec![loan, lent], Some(value)));
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
        Ok((lent, vec![loan, lent], None))
    }

    /// A Borrow String parameter or capture read through its shared loan in `==`, `!=`, `+`.
    pub(super) fn borrowed_string_loan(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<Slot>, LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (Expression::Name, span) = self.node(root)? else {
            return Ok(None);
        };
        let id = self.unit(root);
        if self.inputs.typed.non_null_use(id).is_some()
            || self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(id).is_some())
        {
            return Ok(None);
        }
        let Some(&symbol) = self.inputs.references.get(&(span.start(), span.end())) else {
            return Ok(None);
        };
        let is_string = matches!(
            self.symbol_type(symbol)
                .and_then(|ty| self.inputs.typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        );
        Ok(is_string
            .then(|| self.borrowed.get(&symbol).copied())
            .flatten())
    }

    /// A MoveOnly Borrow parameter or capture lent directly to a Borrow call parameter.
    pub(super) fn borrowed_move_only_loan(
        &self,
        expression: ExpressionId,
        parameter: UnitTypeId,
    ) -> Result<Option<Slot>, LoweringError> {
        if !self.move_only(parameter) {
            return Ok(None);
        }
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (Expression::Name, span) = self.node(root)? else {
            return Ok(None);
        };
        let id = self.unit(root);
        if self.inputs.typed.non_null_use(id).is_some()
            || self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(id).is_some())
        {
            return Ok(None);
        }
        let Some(&symbol) = self.inputs.references.get(&(span.start(), span.end())) else {
            return Ok(None);
        };
        Ok((self.symbol_type(symbol) == Some(parameter))
            .then(|| self.borrowed.get(&symbol).copied())
            .flatten())
    }

    /// `owner.field` of a MoveOnly type lent to a Borrow call parameter via `SharedHeapFieldLoan`.
    pub(super) fn borrowed_string_field_loan(
        &mut self,
        expression: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<LentArgument>, LoweringError> {
        if !self.move_only(parameter) {
            return Ok(None);
        }
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        let Some(projection) = self.inputs.typed.aggregate_projection(id) else {
            return Ok(None);
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.c(projection.ty()) != parameter
        {
            return Ok(None);
        }
        if let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
            projection.receiver()
            && let Some(lang_frontend::type_checking::UnitTypeKind::EnumCase { case, root }) = self
                .inputs
                .typed
                .expression_type(receiver)
                .and_then(|ty| self.inputs.typed.types().get(ty))
                .cloned()
        {
            if receiver.source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let root_ty = self.c(root);
            let receiver_expr = receiver.expression();
            let (Expression::Name, receiver_span) = self.node(receiver_expr)? else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let symbol = self.reference(receiver_span)?;
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
            let (_, variant, payload_agg) = self.variant(root_ty, case, span)?;
            let field = self
                .types
                .field_indices
                .get(&(root_ty, projection.field()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let target = self.ssa_type(parameter, span)?;
            let arg_span = self.span(expression);
            let base = self.value(
                ROp::TaggedPayloadPlace { owner, variant },
                EntityType::Place(payload_agg),
                arg_span,
            )?;
            let place = self.value(
                ROp::FieldPlace { base, field },
                EntityType::Place(target),
                arg_span,
            )?;
            let field_loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                arg_span,
            )?;
            return Ok(Some((field_loan, vec![field_loan], Some(receiver_expr))));
        }
        let (owner, owner_type, receiver) = self.projection_owner(projection.receiver(), span)?;
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let owner_loan = self.owner_loan(owner, LoanKind::Shared, span)?;
        let target = self.ssa_type(parameter, span)?;
        let field_loan = self.value(
            ROp::SharedHeapFieldLoan {
                base: owner_loan.loan,
                field,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            self.span(expression),
        )?;
        let mut created = Vec::new();
        if owner_loan.scoped {
            created.push(owner_loan.loan);
        }
        created.push(field_loan);
        Ok(Some((field_loan, created, receiver)))
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
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                self.sync_dead();
                self.dead.insert(symbol);
                self.bindings
                    .get(&symbol)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            }
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }
}
