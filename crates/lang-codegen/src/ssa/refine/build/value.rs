//! Copyable `value class` in the refined CFG (SPEC-0361): construction builds the inline
//! aggregate and a field read projects a copy out of the owner's value.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{AssignmentOperator, Expression},
    source::Span,
    type_checking::{NominalKind, UnitAggregateProjectionReceiver, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    integer::assignment_operator,
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{RAbort, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    /// Whether `ty` is a value class.
    pub(super) fn is_value_class(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.inputs.typed.types().get(ty)
        else {
            return false;
        };
        self.inputs
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| nominal.kind() == NominalKind::ValueClass)
    }

    /// Whether `ty` is a stateless `object` singleton.
    pub(super) fn is_object_type(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.inputs.typed.types().get(ty)
        else {
            return false;
        };
        arguments.is_empty()
            && self
                .inputs
                .typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nominal| {
                    nominal.kind() == NominalKind::Object && nominal.fields().is_empty()
                })
    }

    /// Fields evaluate in source order and land in declaration order in the aggregate.
    pub(super) fn value_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut fields: Vec<Option<Slot>> = vec![None; arguments.len()];
        let mut delivered = Vec::with_capacity(arguments.len());
        for argument in arguments {
            if argument.argument().source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let value_expression = argument.argument().expression();
            let Some(value) = self.required(value_expression)? else {
                return Ok(Lowered::Diverged);
            };
            let value = self.adapt(value, value_expression, self.c(argument.parameter_type()))?;
            self.repoint_temporary(value_expression, value);
            delivered.push(value_expression);
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(value).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        for value_expression in delivered {
            self.deliver(value_expression);
        }
        let fields = fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let aggregate = self.ssa_type(self.c(descriptor.result_type()), span)?;
        Ok(Lowered::Value(self.value(
            ROp::AggregateConstruct { aggregate, fields },
            EntityType::Value(aggregate),
            span,
        )?))
    }

    pub(super) fn local_destructuring(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement_id =
            lang_frontend::type_checking::UnitStatementId::new(self.inputs.source_unit, statement);
        let descriptor = self
            .inputs
            .typed
            .destructuring(statement_id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(source) = self.required(initializer)? else {
            return Ok(Lowered::Diverged);
        };
        let mut results = Vec::with_capacity(descriptor.components().len());
        for component in descriptor.components() {
            let ty = self.ssa_type(self.c(component.ty()), span)?;
            let slot = self.body.slot(EntityType::Value(ty));
            results.push(slot);
        }
        let op = match descriptor.mode() {
            lang_frontend::type_checking::DestructuringMode::Copy => {
                ROp::AggregateCopyExplode { aggregate: source }
            }
            lang_frontend::type_checking::DestructuringMode::Consume => {
                self.deliver(initializer);
                ROp::AggregateExplode { aggregate: source }
            }
        };
        self.push(op, results.clone(), span)?;
        for (component, slot) in descriptor.components().iter().zip(results) {
            self.bindings.insert(component.symbol(), slot);
        }
        Ok(Lowered::Unit)
    }

    /// Whether the projection reads a field of a value class.
    pub(super) fn is_value_projection(&self, expression: ExpressionId) -> bool {
        let Some(projection) = self
            .inputs
            .typed
            .aggregate_projection(self.unit(expression))
        else {
            return false;
        };
        let owner = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(_) => {
                self.inputs.receiver.map(|receiver| receiver.ty)
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                let mut inner = receiver.expression();
                while let Ok((Expression::Group { expression }, _)) = self.node(inner) {
                    inner = expression;
                }
                self.inputs
                    .typed
                    .non_null_use(self.unit(inner))
                    .map(|fact| self.c(fact.narrowed_type()))
                    .or_else(|| self.expression_type_of(receiver))
            }
        };
        owner.is_some_and(|owner| self.is_value_class(owner))
    }

    /// Copies a field out of the owner's value: a name (a Borrow loan is read by copy) or
    /// the current receiver.
    pub(super) fn value_projection(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let projection = self
            .inputs
            .typed
            .aggregate_projection(self.unit(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        // A loan owner (Borrow receiver or parameter) is never copied whole: one shared
        // field loan is read and ended. An owned value projects its field directly.
        let (owner_type, owner) = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(declaration) => {
                let receiver = self
                    .inputs
                    .receiver
                    .filter(|receiver| receiver.owner == declaration)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = self
                    .receiver
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                (receiver.ty, slot)
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let mut receiver = receiver.expression();
                while let (Expression::Group { expression: inner }, _) = self.node(receiver)? {
                    receiver = inner;
                }
                let owner_type = match self.inputs.typed.non_null_use(self.unit(receiver)) {
                    Some(fact) => self.c(fact.narrowed_type()),
                    None => self.expression_type(receiver)?,
                };
                let (payload, receiver_span) = self.node(receiver)?;
                let owner = match payload {
                    Expression::This => self
                        .receiver
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                    Expression::Name => {
                        let symbol = self.reference(receiver_span)?;
                        match self.borrowed.get(&symbol).copied() {
                            Some(loan)
                                if matches!(
                                    self.body.slots[loan.0],
                                    EntityType::Loan {
                                        kind: LoanKind::Shared,
                                        ..
                                    }
                                ) =>
                            {
                                loan
                            }
                            _ => self
                                .required(receiver)?
                                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                        }
                    }
                    _ => self
                        .required(receiver)?
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                };
                (owner_type, owner)
            }
        };
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result = self.ssa_type(self.expression_type(expression)?, span)?;
        if let EntityType::Loan { kind, target } = self.body.slots[owner.0] {
            let (base, reborrow) = match kind {
                LoanKind::Shared => (owner, None),
                LoanKind::Exclusive => {
                    let reborrow = self.value(
                        ROp::SharedReborrow { source: owner },
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        },
                        span,
                    )?;
                    (reborrow, Some(reborrow))
                }
            };
            let loan = self.value(
                ROp::SharedFieldLoan { base, field },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: result,
                },
                span,
            )?;
            let value = self.value(ROp::ReadLoan { loan }, EntityType::Value(result), span)?;
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            if let Some(reborrow) = reborrow {
                self.push(ROp::BorrowEnd { loan: reborrow }, Vec::new(), span)?;
            }
            return Ok(Lowered::Value(value));
        }
        Ok(Lowered::Value(self.value(
            ROp::AggregateProject {
                aggregate: owner,
                field,
            },
            EntityType::Value(result),
            span,
        )?))
    }

    pub(super) fn assignment(
        &mut self,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let symbol = self.reference(self.span(target))?;
        let Some(slot) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let declared = self
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let slot = self.adapt(slot, value, declared)?;
        self.deliver(value);
        if let Some(loan) = self.inout_loan(symbol) {
            // The caller's place now holds the new value; nothing is rebound here.
            self.push(ROp::LoanMutate { loan, value: slot }, Vec::new(), span)?;
            return Ok(Lowered::Unit);
        }
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.end_views_of(symbol, span)?;
        self.sync_dead();
        self.dead.remove(&symbol);
        self.push(ROp::Alias { source: slot }, vec![binding], span)?;
        self.narrowed.remove(&symbol);
        Ok(Lowered::Unit)
    }

    pub(super) fn compound_assignment(
        &mut self,
        expression: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if self.is_element_target(target) {
            return self.element_compound_assignment(expression, target, operator, value, span);
        }
        let symbol = self.reference(self.span(target))?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(right) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let ty = self.ssa_type(self.expression_type(target)?, span)?;
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_type(boolean, span)?;
        let operator = assignment_operator(operator)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let result = self.body.slot(EntityType::Value(ty));
        let failed = self.body.slot(EntityType::Value(boolean));
        self.push(
            ROp::CheckedArithmetic {
                operator,
                left: binding,
                right,
            },
            vec![result, failed],
            span,
        )?;
        let abort = self.body.block();
        let success = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: failed,
                when_true: abort,
                when_false: success,
            },
            span,
        )?;
        self.block = Some(abort);
        self.terminate(
            RTerm::Abort(RAbort::CheckedArithmetic {
                operator,
                divisor: right,
            }),
            span,
        )?;
        self.block = Some(success);
        self.end_views_of(symbol, span)?;
        self.sync_dead();
        self.dead.remove(&symbol);
        self.push(ROp::Alias { source: result }, vec![binding], span)?;
        self.narrowed.remove(&symbol);
        Ok(Lowered::Unit)
    }

    fn element_compound_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let mut root_target = target;
        while let (Expression::Group { expression: inner }, _) = self.node(root_target)? {
            root_target = inner;
        }
        let descriptor = self
            .inputs
            .typed
            .element_place(self.unit(root_target))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            let replaced =
                self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
            self.finish_drops(&replaced);
            return Ok(Lowered::Diverged);
        };
        // 元素槽位定位与边界检查必须先于 RHS 求值，否则 RHS 副作用会在越界 abort 前生效。
        let initial_owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.c(descriptor.element_type());
        let ssa_element_ty = self.ssa_type(element_ty, span)?;
        let place = self.body.slot(EntityType::Place(ssa_element_ty));
        self.push(
            ROp::ContainerElementPlace {
                owner: initial_owner,
                index,
            },
            vec![place],
            span,
        )?;
        let current = self.body.slot(EntityType::Value(ssa_element_ty));
        self.push(ROp::ReadPlace { place }, vec![current], span)?;
        let Some(right) = self.required(value)? else {
            let replaced =
                self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
            self.finish_drops(&replaced);
            return Ok(Lowered::Diverged);
        };
        self.deliver(value);
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_type(boolean, span)?;
        let operator = assignment_operator(operator)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let result = self.body.slot(EntityType::Value(ssa_element_ty));
        let failed = self.body.slot(EntityType::Value(boolean));
        self.push(
            ROp::CheckedArithmetic {
                operator,
                left: current,
                right,
            },
            vec![result, failed],
            span,
        )?;
        let abort = self.body.block();
        let success = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: failed,
                when_true: abort,
                when_false: success,
            },
            span,
        )?;
        self.block = Some(abort);
        self.terminate(
            RTerm::Abort(RAbort::CheckedArithmetic {
                operator,
                divisor: right,
            }),
            span,
        )?;
        self.block = Some(success);
        // RHS 可能重新绑定容器变量，写回必须使用 RHS 之后的绑定。
        let write_owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerReplace {
                owner: write_owner,
                index,
                value: result,
            },
            Vec::new(),
            span,
        )?;
        let replaced = self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }

    pub(super) fn snapshot_binding_if_name(
        &mut self,
        expression: ExpressionId,
        slot: Slot,
    ) -> Result<Slot, LoweringError> {
        let mut target = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(target)? {
            target = inner;
        }
        if let (Expression::Name, span) = self.node(target)?
            && let Ok(symbol) = self.reference(span)
            && self
                .symbol_type(symbol)
                .is_some_and(|ty| !self.move_only(ty))
            && self.bindings.get(&symbol).copied() == Some(slot)
        {
            let snapshot = self.body.slot(self.body.slots[slot.0]);
            self.push(ROp::Alias { source: slot }, vec![snapshot], span)?;
            return Ok(snapshot);
        }
        Ok(slot)
    }

    /// A Borrow argument of type `T` for a `T?` parameter: the temporary is wrapped and
    /// its drop obligation moves to the wrapper.
    pub(super) fn adapt_borrowed(
        &mut self,
        value: Slot,
        expression: ExpressionId,
        parameter_type: lang_frontend::type_checking::UnitTypeId,
    ) -> Result<Slot, LoweringError> {
        let id = self.unit(expression);
        let temporary = self.temporaries.contains_key(&id);
        let adapted = self.adapt(value, expression, self.c(parameter_type))?;
        if adapted != value && temporary {
            self.temporaries.insert(id, adapted);
        }
        Ok(adapted)
    }

    pub(super) fn return_expression(
        &mut self,
        expression: ExpressionId,
        value: Option<ExpressionId>,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if self.inputs.borrow_return.is_some() {
            let value = value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            return self.lower_borrow_return(expression, value, span);
        }
        if self.inputs.carrier_return.is_some() {
            let value = value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            return self.lower_carrier_return(expression, value, span);
        }
        let invariant = self.invariant_return_entry(expression, value, span)?;
        let result = match value {
            Some(value) => {
                let saved_expected = self.expected_nullable;
                self.expected_nullable = if matches!(
                    self.inputs.typed.types().get(self.inputs.return_type),
                    Some(lang_frontend::type_checking::UnitTypeKind::Nullable(_))
                ) {
                    self.ssa_type(self.inputs.return_type, span).ok()
                } else {
                    None
                };
                let slot = self.required(value)?;
                self.expected_nullable = saved_expected;
                let Some(slot) = slot else {
                    self.unreachable_drop_points
                        .push(UnitDropPoint::ControlTransfer(self.unit(expression)));
                    return Ok(Lowered::Diverged);
                };
                let slot = self.adapt(slot, value, self.inputs.return_type)?;
                self.deliver(value);
                Some(slot)
            }
            None => None,
        };
        for loan in std::mem::take(&mut self.pending_call_loans)
            .into_iter()
            .rev()
        {
            self.end_short_call_loan(loan, span)?;
        }
        self.end_all_views(span)?;
        self.drops(UnitDropPoint::ControlTransfer(self.unit(expression)))?;
        let unit_return = self.is_unit(self.inputs.return_type);
        self.owner_return(expression, result, unit_return, span)?;
        self.invariant_return_emitted(invariant.as_ref(), span)?;
        Ok(Lowered::Diverged)
    }
}

pub(in crate::ssa::refine::build) mod field {
    //! Field replacement and owner-loan acquisition in the refined CFG (SPEC-0358): a local
    //! owner is lent for the access only; a Borrow/Inout parameter or the current receiver
    //! already arrives as a loan of the right kind.
    use lang_frontend::{
        ast::ExpressionId, name_resolution::DeclarationId, ownership_checking::UnitDropPoint,
        parser::Expression, source::Span,
    };

    use super::super::{Builder, Lowered};
    use crate::ssa::{
        lowering_support::{LoweringError, LoweringErrorKind, error},
        model::{EntityType, LoanKind},
        refine::body::{ROp, Slot},
    };

    /// The owner a field access or receiver goes through.
    #[derive(Clone, Copy, Debug)]
    pub(in crate::ssa::refine::build) enum OwnerRef {
        /// An explicit receiver expression: a name, `this`, or a temporary.
        Expression(ExpressionId),
        /// The implicit `this` of the current member of `DeclarationId`.
        This(DeclarationId),
    }

    /// A loan of an owner; `scoped` loans were begun for this access and end right after it.
    #[derive(Clone, Copy, Debug)]
    pub(in crate::ssa::refine::build) struct OwnerLoan {
        pub(in crate::ssa::refine::build) loan: Slot,
        pub(in crate::ssa::refine::build) scoped: bool,
    }

    impl Builder<'_, '_> {
        /// A loan of `owner` of at least `kind`: an exclusive loan also serves a read.
        pub(in crate::ssa::refine::build) fn owner_loan(
            &mut self,
            owner: OwnerRef,
            kind: LoanKind,
            span: Span,
        ) -> Result<OwnerLoan, LoweringError> {
            let existing = match owner {
                OwnerRef::This(declaration) => {
                    if self.inputs.receiver.map(|receiver| receiver.owner) != Some(declaration) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    self.receiver
                }
                OwnerRef::Expression(mut expression) => {
                    while let (Expression::Group { expression: inner }, _) =
                        self.node(expression)?
                    {
                        expression = inner;
                    }
                    let (payload, expression_span) = self.node(expression)?;
                    match payload {
                        Expression::This => self.receiver,
                        Expression::Name => {
                            if let Some(view) = self.narrowed_view(expression)? {
                                if kind == LoanKind::Shared {
                                    return Ok(OwnerLoan {
                                        loan: view,
                                        scoped: false,
                                    });
                                }
                                return Err(error(LoweringErrorKind::InvalidModel, span));
                            }
                            if let Ok(symbol) = self.reference(expression_span) {
                                match self.borrowed.get(&symbol).copied() {
                                    Some(loan) => Some(loan),
                                    None => {
                                        let owner =
                                            *self.bindings.get(&symbol).ok_or_else(|| {
                                                error(
                                                    LoweringErrorKind::MissingFact,
                                                    expression_span,
                                                )
                                            })?;
                                        return self.lend(owner, kind, span);
                                    }
                                }
                            } else {
                                let owner = self
                                    .required(expression)?
                                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                                return self.lend(owner, kind, span);
                            }
                        }
                        _ => {
                            let owner = self
                                .required(expression)?
                                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                            return self.lend(owner, kind, span);
                        }
                    }
                }
            };
            let loan = existing.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            match (self.body.slots[loan.0], kind) {
                (EntityType::Loan { kind: held, .. }, wanted)
                    if held == wanted || held == LoanKind::Exclusive =>
                {
                    Ok(OwnerLoan {
                        loan,
                        scoped: false,
                    })
                }
                (EntityType::Value(_), LoanKind::Shared) => self.lend(loan, LoanKind::Shared, span),
                _ => Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }

        /// Lends an owned binding's root place for one access.
        pub(in crate::ssa::refine::build) fn lend(
            &mut self,
            owner: Slot,
            kind: LoanKind,
            span: Span,
        ) -> Result<OwnerLoan, LoweringError> {
            let EntityType::Value(target) = self.body.slots[owner.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
            let loan = self.value(
                ROp::BorrowBegin { place, kind },
                EntityType::Loan { kind, target },
                span,
            )?;
            Ok(OwnerLoan { loan, scoped: true })
        }

        /// A shared call operand: an exclusive loan held by this function is narrowed to a
        /// call-scoped shared reborrow.
        pub(in crate::ssa::refine::build) fn shared_operand(
            &mut self,
            loan: OwnerLoan,
            span: Span,
        ) -> Result<OwnerLoan, LoweringError> {
            let EntityType::Loan { kind, target } = self.body.slots[loan.loan.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            if kind == LoanKind::Shared {
                return Ok(loan);
            }
            let reborrow = self.value(
                ROp::SharedReborrow { source: loan.loan },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                span,
            )?;
            Ok(OwnerLoan {
                loan: reborrow,
                scoped: true,
            })
        }

        /// Ends a loan begun for one access.
        pub(in crate::ssa::refine::build) fn end_owner_loan(
            &mut self,
            loan: OwnerLoan,
            span: Span,
        ) -> Result<(), LoweringError> {
            if loan.scoped {
                self.push(ROp::BorrowEnd { loan: loan.loan }, Vec::new(), span)?;
            }
            Ok(())
        }

        pub(in crate::ssa::refine::build) fn is_field_target(&self, target: ExpressionId) -> bool {
            self.inputs
                .typed
                .aggregate_projection(self.unit(target))
                .is_some()
        }

        /// `owner.f = v` on a plain class owner: a local, an Inout parameter or the Inout
        /// receiver (explicit or implicit `this`).
        pub(in crate::ssa::refine::build) fn field_assignment(
            &mut self,
            assignment: ExpressionId,
            target: ExpressionId,
            value: ExpressionId,
            span: Span,
        ) -> Result<Lowered, LoweringError> {
            let projection = self
                .inputs
                .typed
                .aggregate_projection(self.unit(target))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (owner, owner_type, _) = self.projection_owner(projection.receiver(), span)?;
            let field = self
                .types
                .field_indices
                .get(&(owner_type, projection.field()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            // Guide: the RHS is evaluated before the old value is dropped and replaced.
            let Some(replacement) = self.required(value)? else {
                let replaced =
                    self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
                self.finish_drops(&replaced);
                return Ok(Lowered::Diverged);
            };
            let replacement = self.adapt(replacement, value, self.c(projection.ty()))?;
            self.deliver(value);
            let loan = self.owner_loan(owner, LoanKind::Exclusive, span)?;
            let op = if self.is_value_class(owner_type) {
                ROp::InlineFieldReplace {
                    receiver: loan.loan,
                    field,
                    value: replacement,
                }
            } else {
                ROp::HeapFieldReplace {
                    receiver: loan.loan,
                    field,
                    value: replacement,
                }
            };
            self.push(op, Vec::new(), span)?;
            self.end_owner_loan(loan, span)?;
            let replaced =
                self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
            self.finish_drops(&replaced);
            Ok(Lowered::Unit)
        }

        pub(in crate::ssa::refine::build) fn local_variable(
            &mut self,
            mut declaration: lang_frontend::ast::ItemId,
            span: Span,
        ) -> Result<Lowered, LoweringError> {
            while let Ok(node) = self.inputs.parsed.ast().items().get(declaration)
                && let lang_frontend::parser::Item::Modified {
                    declaration: inner, ..
                } = node.payload()
            {
                declaration = *inner;
            }
            let node = self
                .inputs
                .parsed
                .ast()
                .items()
                .get(declaration)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let lang_frontend::parser::Item::Variable {
                name,
                initializer,
                kind,
                ..
            } = node.payload().clone()
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let lang_frontend::parser::NameMarker::Present(name_span) = name else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if matches!(kind, lang_frontend::parser::VariableKind::BorrowVal(_))
                && self
                    .inputs
                    .typed
                    .expression_falls_through(self.unit(initializer))
                    == Some(false)
            {
                if self.expression(initializer)? != Lowered::Diverged {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
                return Ok(Lowered::Diverged);
            }
            if let Some(fact) = self
                .inputs
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .find(|fact| fact.initializer() == self.unit(initializer))
            {
                return self.local_borrow_binding(fact.binding(), initializer, span);
            }
            let symbol = self.declared_symbol(name_span)?;
            let declared = self
                .symbol_type(symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if self.is_unit(declared) {
                if self.expression(initializer)? == Lowered::Diverged {
                    return Ok(Lowered::Diverged);
                }
                return Ok(Lowered::Unit);
            }
            let saved_expected = self.expected_nullable;
            if matches!(
                self.inputs.typed.types().get(declared),
                Some(lang_frontend::type_checking::UnitTypeKind::Nullable(_))
            ) {
                self.expected_nullable = self.ssa_type(declared, span).ok();
            } else {
                self.expected_nullable = None;
            }
            let lowered = self.expression(initializer);
            self.expected_nullable = saved_expected;
            let Lowered::Value(value) = lowered? else {
                return Ok(Lowered::Diverged);
            };
            let value = self.adapt_with_span(value, initializer, declared, span)?;
            self.deliver(initializer);
            let EntityType::Value(ty) = self.body.slots[value.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            self.owner_binding_authorization(self.unit(initializer), symbol, value, span)?;
            let counter =
                self.counter_binding_authorization(self.unit(initializer), symbol, value, span)?;
            let bridge =
                self.bridge_binding_authorization(self.unit(initializer), symbol, value, span)?;
            let suffix = self.bridge_suffix_binding(self.unit(initializer), symbol, value, span)?;
            let entry = self.bridge_entry_binding(self.unit(initializer), symbol, value, span)?;
            if (entry.is_some() && (suffix.is_some() || bridge.is_some() || counter.is_some()))
                || (suffix.is_some() && (bridge.is_some() || counter.is_some()))
                || (bridge.is_some() && counter.is_some())
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            let binding = self.value(
                ROp::Alias { source: value },
                crate::ssa::model::EntityType::Value(ty),
                span,
            )?;
            self.counter_binding_emitted(counter, binding, span)?;
            self.bridge_emitted(bridge, Some(binding), span)?;
            self.bridge_suffix_emitted(suffix, Some(binding), span)?;
            self.bridge_entry_emitted(entry, binding, span)?;
            self.bindings.insert(symbol, binding);
            self.owner_binding_emitted(symbol, binding);
            #[cfg(test)]
            if self.owner_is_suffix(symbol)
                && super::super::owner_consumer_test_support::take_fault(
                    super::super::owner_consumer_test_support::OwnerFault::DuplicateBind,
                )
            {
                self.owner_binding_authorization(self.unit(initializer), symbol, value, span)?;
            }
            Ok(Lowered::Unit)
        }

        fn declared_symbol(
            &self,
            name_span: Span,
        ) -> Result<lang_frontend::name_resolution::UnitSymbolId, LoweringError> {
            self.inputs
                .names
                .names()
                .source_units()
                .get(self.inputs.source_unit.index())
                .and_then(|source| {
                    source.resolution().symbols().iter().find(|symbol| {
                        symbol.span() == name_span
                            && symbol.kind() == lang_frontend::name_resolution::SymbolKind::Variable
                    })
                })
                .and_then(|local| {
                    self.inputs
                        .typed
                        .body_symbol_types()
                        .keys()
                        .copied()
                        .find(|symbol| {
                            symbol.source_unit() == self.inputs.source_unit
                                && symbol.symbol() == local.id()
                        })
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))
        }
    }
}
