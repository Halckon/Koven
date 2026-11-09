//! The closure family in the refined CFG (SPEC-0364, SPEC-0365, SPEC-0370, ADR-0034):
//! lambda thunk bodies, local callable bindings, calls through callable views or local
//! callables, and Borrow String/field loans in closure bodies.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, LoanKind as FrontendLoanKind, UnitDropPoint,
        UnitLoanTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, Copyability, NominalKind, UnitAggregateProjectionKind, UnitExpressionId,
        UnitTypeId, UnitTypeKind,
    },
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
    pub(super) fn lower_nested_borrow_argument(
        &mut self,
        call: lang_frontend::type_checking::UnitExpressionId,
        argument: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<(Slot, Vec<Slot>)>, LoweringError> {
        let argument_id = self.unit(argument);
        let Some(fact) = self
            .inputs
            .owned
            .borrow_results()
            .call_uses()
            .iter()
            .find(|fact| fact.call() == call && fact.argument() == argument_id)
        else {
            return Ok(None);
        };
        let actual = self.ungrouped_expr(argument)?;
        let source = fact.source_loan();
        let copies = fact.end_point() == UnitDropPoint::AfterExpression(argument_id);
        if fact.expression() != self.unit(actual)
            || (!copies && fact.end_point() != UnitDropPoint::CallReturn(call))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if !copies {
            return self.lower_result_call(actual, source, span, None);
        }

        let payload = self.expression_type(argument)?;
        let parameter = self.c(parameter);
        if self.copyability_of(payload) != Copyability::Copyable
            || !matches!(self.inputs.typed.types().get(parameter), Some(UnitTypeKind::Nullable(inner)) if *inner == payload)
            || !self.inputs.owned.loans().iter().any(|loan| {
                loan.call() == call
                    && loan.argument() == argument_id
                    && loan.kind() == FrontendLoanKind::Shared
                    && loan.target() == &UnitLoanTarget::Temporary(argument_id)
            })
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let Some((loan, source_loans)) = self.lower_result_call(actual, source, span, None)? else {
            return Ok(None);
        };
        let value = self.value(
            ROp::ReadLoan { loan },
            EntityType::Value(self.ssa_type(payload, span)?),
            span,
        )?;
        self.end_short_call_loan(loan, span)?;
        for loan in source_loans.into_iter().rev() {
            self.end_short_call_loan(loan, span)?;
        }
        self.drops(UnitDropPoint::AfterExpression(argument_id))?;

        let owner = self.adapt(value, argument, parameter)?;
        let target = self.ssa_type(parameter, span)?;
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
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
        Ok(Some((loan, Vec::new())))
    }

    pub(super) fn lower_temporary_field_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<(Slot, Vec<Slot>)>, LoweringError> {
        let parameter = self.c(parameter);
        let Some((_, copied)) = self
            .inputs
            .temporary_field_argument(call, argument, parameter)
        else {
            return Ok(None);
        };
        let mut loans = Vec::new();
        let field = self
            .clone_field_loan(argument, &mut loans, span)?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !copied {
            return Ok(Some((field, loans)));
        }
        let payload = self.ssa_type(self.expression_type(argument)?, span)?;
        let value = self.value(
            ROp::ReadLoan { loan: field },
            EntityType::Value(payload),
            span,
        )?;
        for loan in loans.into_iter().rev() {
            self.end_short_call_loan(loan, span)?;
        }
        let owner = self.adapt_borrowed(value, argument, parameter)?;
        let loan = self.lend(owner, LoanKind::Shared, span)?.loan;
        Ok(Some((loan, vec![loan])))
    }

    /// A borrowed binding can be reborrowed directly only when its storage matches the ABI.
    pub(super) fn reborrow_call_argument(
        &mut self,
        call: ExpressionId,
        operand: ExpressionId,
        parameter: UnitTypeId,
    ) -> Result<Option<Slot>, LoweringError> {
        let root = self.ungrouped_expr(operand)?;
        let (payload, root_span) = self.node(root)?;
        let source = match payload {
            Expression::Name => self
                .reference(root_span)
                .ok()
                .and_then(|symbol| self.borrowed.get(&symbol).copied()),
            Expression::This => match self.reference(root_span) {
                Ok(symbol) => self.borrowed.get(&symbol).copied(),
                Err(_) => self.receiver,
            },
            _ => None,
        };
        let Some(source) = source else {
            return Ok(None);
        };
        let span = self.span(operand);
        let EntityType::Loan { target, .. } = self.body.slots[source.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if target == self.ssa_type(self.c(parameter), span)? {
            return self
                .value(
                    ROp::SharedReborrow { source },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    },
                    span,
                )
                .map(Some);
        }
        let operand_id = self.unit(operand);
        if self.copyability_of(self.expression_type(operand)?) == Copyability::Copyable
            && self.inputs.owned.loans().iter().any(|fact| {
                fact.call() == self.unit(call)
                    && fact.argument() == operand_id
                    && fact.kind() == FrontendLoanKind::Shared
                    && fact.target() == &UnitLoanTarget::Temporary(operand_id)
            })
        {
            // The ordinary argument path reads through the source loan before wrapping the copy.
            Ok(None)
        } else {
            Err(error(LoweringErrorKind::MissingFact, span))
        }
    }

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
        return_type: UnitTypeId,
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
        let result = if self.is_nothing(return_type) {
            self.push(op, Vec::new(), span)?;
            self.terminate(super::RTerm::Unreachable, span)?;
            Lowered::Diverged
        } else if self.is_unit(return_type) {
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
            if let Some((owner, callable)) = self.form_known_function(root, root_span)? {
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
                return Ok((lent, vec![loan, lent], None));
            }
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
        if value != root {
            // Exit drops refer to the original argument, including any parentheses.
            self.register_temporary(value, owner)?;
        }
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

    pub(super) fn form_known_function(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<(Slot, SsaTypeId)>, LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (Expression::Name, root_span) = self.node(root)? else {
            return Ok(None);
        };
        let id = self.unit(root);
        let Some(fact) = self.inputs.owned.callable_origin(id) else {
            return Ok(None);
        };
        let lang_frontend::ownership_checking::UnitCallableOrigin::KnownFunction(target) =
            fact.origin()
        else {
            return Ok(None);
        };
        if let Ok(symbol) = self.reference(root_span)
            && (self.borrowed.contains_key(&symbol) || self.bindings.contains_key(&symbol))
        {
            return Ok(None);
        }
        let &(function, callable) = self
            .inputs
            .known_functions
            .get(&target)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, root_span))?;
        let slot = self.value(
            ROp::FunctionAddress { target: function },
            EntityType::Value(callable),
            span,
        )?;
        self.temporaries.insert(id, slot);
        Ok(Some((slot, callable)))
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
        let (Expression::Name | Expression::This, span) = self.node(root)? else {
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
        let (payload, span) = self.node(root)?;
        let id = self.unit(root);
        if self.inputs.typed.non_null_use(id).is_some()
            || self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(id).is_some())
        {
            return Ok(None);
        }
        match payload {
            Expression::Name => {
                let Some(&symbol) = self.inputs.references.get(&(span.start(), span.end())) else {
                    return Ok(None);
                };
                Ok((self.symbol_type(symbol) == Some(parameter))
                    .then(|| self.borrowed.get(&symbol).copied())
                    .flatten())
            }
            Expression::This => Ok(self
                .inputs
                .receiver
                .filter(|receiver| {
                    receiver.mode == lang_frontend::type_checking::ParameterMode::Borrow
                        && receiver.ty == parameter
                })
                .and(self.receiver)
                .or_else(|| {
                    let symbol = self
                        .inputs
                        .references
                        .get(&(span.start(), span.end()))
                        .copied()?;
                    (self.symbol_type(symbol) == Some(parameter))
                        .then(|| self.borrowed.get(&symbol).copied())
                        .flatten()
                })),
            _ => Ok(None),
        }
    }

    /// `owner.field` of a MoveOnly type lent to a Borrow call parameter via `SharedHeapFieldLoan`.
    pub(super) fn borrowed_string_field_loan(
        &mut self,
        expression: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<LentArgument>, LoweringError> {
        let is_deinit = self.inputs.function_item.is_some_and(|item| {
            matches!(
                self.inputs
                    .parsed
                    .ast()
                    .items()
                    .get(item)
                    .map(|node| node.payload()),
                Ok(lang_frontend::parser::Item::Deinit { .. })
            )
        });
        if !self.move_only(parameter) && !is_deinit {
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
        let (field_loan, created, drops) =
            self.field_projection_loan(root, self.span(expression))?;
        for intermediate in drops.iter().skip(1) {
            self.drops(UnitDropPoint::AfterExpression(self.unit(*intermediate)))?;
        }
        Ok(Some((field_loan, created, drops.first().copied())))
    }

    /// Resolves a single- or multi-hop class field projection `r.f1.f2...` into a leaf
    /// `SharedHeapFieldLoan`, all created loans in creation order, and the receiver
    /// expressions encountered from root to immediate parent.
    pub(super) fn field_projection_loan(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(Slot, Vec<Slot>, Vec<ExpressionId>), LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        let projection = self
            .inputs
            .typed
            .aggregate_projection(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (base_loan, mut created, drops, owner_type) = match projection.receiver() {
            lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver)
                if receiver.source_unit() == self.inputs.source_unit =>
            {
                let mut ungrouped = receiver.expression();
                while let (Expression::Group { expression: inner }, _) = self.node(ungrouped)? {
                    ungrouped = inner;
                }
                if self
                    .inputs
                    .typed
                    .aggregate_projection(self.unit(ungrouped))
                    .is_some()
                {
                    let (parent_loan, created, mut drops) =
                        self.field_projection_loan(receiver.expression(), span)?;
                    let owner_type = self.expression_type(ungrouped)?;
                    drops.push(receiver.expression());
                    (parent_loan, created, drops, owner_type)
                } else if let (Expression::Name, receiver_span) = self.node(ungrouped)?
                    && let Ok(symbol) = self.reference(receiver_span)
                    && !self.borrowed.contains_key(&symbol)
                    && self.narrowed_view(ungrouped)?.is_none()
                {
                    let owner_type = match self.inputs.typed.non_null_use(self.unit(ungrouped)) {
                        Some(fact) => self.c(fact.narrowed_type()),
                        None => self.expression_type(ungrouped)?,
                    };
                    if self.is_class(owner_type) {
                        let owner = *self
                            .bindings
                            .get(&symbol)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
                        let owner_ssa = self.ssa_type(owner_type, span)?;
                        let payload_ssa = *self
                            .types
                            .heap_payloads
                            .get(&owner_ssa)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let base = self.value(
                            ROp::HeapPayloadPlace { owner },
                            EntityType::Place(payload_ssa),
                            receiver_span,
                        )?;
                        let field = self
                            .types
                            .field_indices
                            .get(&(owner_type, projection.field()))
                            .copied()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let target = self.ssa_type(self.c(projection.ty()), span)?;
                        let place = self.value(
                            ROp::FieldPlace { base, field },
                            EntityType::Place(target),
                            span,
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
                            span,
                        )?;
                        return Ok((field_loan, vec![field_loan], vec![ungrouped]));
                    }
                    let (owner, owner_type, receiver_expr) =
                        self.projection_owner(projection.receiver(), span)?;
                    let owner_loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                    let mut created = Vec::new();
                    if owner_loan.scoped {
                        created.push(owner_loan.loan);
                    }
                    let drops = receiver_expr.into_iter().collect();
                    (owner_loan.loan, created, drops, owner_type)
                } else {
                    let (owner, owner_type, receiver_expr) =
                        self.projection_owner(projection.receiver(), span)?;
                    let owner_loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                    let mut created = Vec::new();
                    if owner_loan.scoped {
                        created.push(owner_loan.loan);
                    }
                    let drops = receiver_expr.into_iter().collect();
                    (owner_loan.loan, created, drops, owner_type)
                }
            }
            _ => {
                let (owner, owner_type, receiver_expr) =
                    self.projection_owner(projection.receiver(), span)?;
                let owner_loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                let mut created = Vec::new();
                if owner_loan.scoped {
                    created.push(owner_loan.loan);
                }
                let drops = receiver_expr.into_iter().collect();
                (owner_loan.loan, created, drops, owner_type)
            }
        };
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.ssa_type(self.c(projection.ty()), span)?;
        let op = if self.is_value_class(owner_type) {
            ROp::SharedFieldLoan {
                base: base_loan,
                field,
            }
        } else {
            ROp::SharedHeapFieldLoan {
                base: base_loan,
                field,
            }
        };
        let field_loan = self.value(
            op,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        created.push(field_loan);
        Ok((field_loan, created, drops))
    }

    fn is_class(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.inputs.typed.types().get(ty)
        else {
            return false;
        };
        self.inputs
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| nominal.kind() == NominalKind::Class)
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
