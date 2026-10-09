use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    name_resolution::{SymbolId, SymbolKind},
    parser::{
        CallArgument, Expression, LiteralKind, ParameterModeMarker, PrefixOperator, VariableKind,
    },
    source::Span,
    type_checking::{
        AggregateProjectionKind, AggregateProjectionReceiver, Copyability, ElementPlaceDescriptor,
        ExpressionCategory, NominalKind, ParameterMode, TypeKind,
    },
};

use crate::ownership_checking::{
    ElementIndexIdentity, LoanFact, LoanKind, LoanTarget, OwnershipDeferredFact,
    OwnershipDeferredReason, OwnershipPlace,
};

use super::{AccessKind, Checker, Flows, OwnershipCheckingError, State};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActiveLoan {
    pub(super) owner: ActiveLoanOwner,
    pub(super) target: ActiveLoanTarget,
    pub(super) kind: LoanKind,
    pub(super) origin: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ActiveLoanTarget {
    Place(OwnershipPlace),
    This,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActiveLoanOwner {
    Call(ExpressionId),
    BorrowBinding(SymbolId),
    ReservedReceiver(ExpressionId),
    IterationSource(StatementId),
    IterationElement(StatementId),
    Closure(ExpressionId),
}

impl Checker<'_> {
    pub(super) fn place(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<OwnershipPlace>, OwnershipCheckingError> {
        if self.is_constant_use(expression) {
            return Ok(None);
        }
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name | Expression::This => Ok(self
                .reference_symbol(node.span())
                .map(|root| OwnershipPlace::new(root, Vec::new()))),
            Expression::Group { expression } => self.place(*expression),
            Expression::Member { .. } => {
                let Some(projection) = self.typed.aggregate_projection(expression) else {
                    return Ok(None);
                };
                if projection.kind() != AggregateProjectionKind::Field {
                    return Ok(None);
                }
                let AggregateProjectionReceiver::Expression(receiver) = projection.receiver()
                else {
                    return Ok(None);
                };
                let Some(mut place) = self.place(receiver)? else {
                    // 显式 this.field 与裸字段共用同一字段 owner，不产生 temporary。
                    return Ok(self
                        .is_this_receiver(receiver)?
                        .then(|| OwnershipPlace::new(projection.field(), Vec::new())));
                };
                if place.push_field(projection.field()) {
                    Ok(Some(place))
                } else {
                    Ok(None)
                }
            }
            Expression::Index { .. } => {
                let Some(descriptor) = self.element_place_descriptor(expression)? else {
                    return Ok(None);
                };
                let Some(mut place) = self.place(descriptor.receiver())? else {
                    return Ok(None);
                };
                let index = self.element_index_identity(descriptor.index())?;
                place.push_element(index);
                Ok(Some(place))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn shared_receiver_place(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<OwnershipPlace>, OwnershipCheckingError> {
        if let Some(operation) = self.typed.rc_operation(expression)
            && operation.kind() == crate::type_checking::RcOperationKind::Value
        {
            return self.place(operation.receiver());
        }
        if let Expression::Group { expression } =
            self.parsed.ast().expressions().get(expression)?.payload()
        {
            return self.shared_receiver_place(*expression);
        }
        self.place(expression)
    }

    pub(super) fn temporary_element_owner(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        if let Expression::Group { expression } = node.payload() {
            return self.temporary_element_owner(*expression);
        }
        let Some(descriptor) = self.element_place_descriptor(expression)? else {
            return Ok(None);
        };
        if let Some(owner) = self.temporary_expression_origin(descriptor.receiver())? {
            Ok(Some(owner))
        } else {
            // 多层 element view 仍属于最初求值的容器 owner，不取得中间元素的所有权。
            self.temporary_element_owner(descriptor.receiver())
        }
    }

    pub(super) fn element_place_descriptor(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<ElementPlaceDescriptor>, OwnershipCheckingError> {
        if let Some(descriptor) = self.typed.element_place(expression) {
            return Ok(Some(descriptor));
        }
        let node = self.parsed.ast().expressions().get(expression)?;
        if let Expression::Group { expression } = node.payload() {
            return self.element_place_descriptor(*expression);
        }
        Ok(None)
    }

    fn temporary_expression_origin(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        if self.typed.expression_category(expression) == Some(ExpressionCategory::Temporary) {
            return Ok(Some(expression));
        }
        let node = self.parsed.ast().expressions().get(expression)?;
        if let Expression::Group { expression } = node.payload() {
            return self.temporary_expression_origin(*expression);
        }
        Ok(None)
    }

    fn element_index_identity(
        &self,
        expression: ExpressionId,
    ) -> Result<ElementIndexIdentity, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        let value = match node.payload() {
            Expression::Literal(LiteralKind::Integer(kind)) => {
                crate::type_checking::integer_literal_magnitude(
                    self.sources.slice(node.span())?,
                    *kind,
                )
                .and_then(|value| i128::try_from(value).ok())
            }
            Expression::Group { expression } => {
                return self.element_index_identity(*expression);
            }
            Expression::Prefix {
                operator: PrefixOperator::Minus,
                operand,
                ..
            } => match self.element_index_identity(*operand)? {
                ElementIndexIdentity::Known(value) => value.checked_neg(),
                ElementIndexIdentity::Unknown => None,
            },
            Expression::Prefix {
                operator: PrefixOperator::Plus,
                operand,
                ..
            } => match self.element_index_identity(*operand)? {
                ElementIndexIdentity::Known(value) => Some(value),
                ElementIndexIdentity::Unknown => None,
            },
            _ => None,
        };
        Ok(value.map_or(ElementIndexIdentity::Unknown, ElementIndexIdentity::Known))
    }

    pub(super) fn access_expression_place(
        &mut self,
        expression: ExpressionId,
        access: AccessKind,
        primary: Span,
        state: &mut State,
    ) -> Result<bool, OwnershipCheckingError> {
        let Some(place) = self.place(expression)? else {
            return Ok(true);
        };
        let move_only = self
            .typed
            .expression_type(expression)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(Copyability::MoveOnly);
        self.access_place(&place, access, move_only, primary, state)
    }

    pub(super) fn access_place(
        &mut self,
        place: &OwnershipPlace,
        access: AccessKind,
        move_only: bool,
        primary: Span,
        state: &mut State,
    ) -> Result<bool, OwnershipCheckingError> {
        let canonical = self.canonical_borrow_place(place, state);
        // Value delivery copies Copyable places, including member projections.
        let access = if access == AccessKind::Move && !move_only {
            AccessKind::Read
        } else {
            access
        };
        // Reservation conflicts are diagnosed at the attempted mutation/move, before
        // non-owning capability errors can obscure the active receiver protection.
        if matches!(
            access,
            AccessKind::Move | AccessKind::Mutation | AccessKind::ExclusiveLoan
        ) && let Some(loan) = state.loans.iter().find(|loan| {
            matches!(loan.owner, ActiveLoanOwner::ReservedReceiver(_))
                && match &loan.target {
                    ActiveLoanTarget::Place(target) => target.overlaps(&canonical),
                    ActiveLoanTarget::This => self
                        .names
                        .symbols()
                        .get(place.root().index())
                        .is_some_and(|symbol| symbol.kind() == SymbolKind::Field),
                }
        }) {
            self.emit_loan_conflict(
                primary,
                loan.origin,
                "access conflicts with a reserved receiver",
            )?;
            return Ok(false);
        }
        // source 的 provider loan 优先于参数的非 owning 限制；return 也不能先结束它。
        if matches!(
            access,
            AccessKind::Move | AccessKind::Mutation | AccessKind::ExclusiveLoan
        ) && let Some(loan) = state.loans.iter().find(|loan| {
            matches!(loan.owner, ActiveLoanOwner::IterationSource(_))
                && matches!(&loan.target, ActiveLoanTarget::Place(source) if source.overlaps(&canonical))
        }) {
            self.emit_loan_conflict(
                primary,
                loan.origin,
                "access conflicts with the active iteration source loan",
            )?;
            return Ok(false);
        }

        if matches!(access, AccessKind::Mutation | AccessKind::ExclusiveLoan)
            && !self.field_root_can_inout(place.root())
        {
            let Some(field) = self.names.symbols().get(place.root().index()) else {
                return Ok(false);
            };
            self.emit_loan_conflict(
                primary,
                field.span(),
                "field access exceeds the current receiver capability",
            )?;
            return Ok(false);
        }

        if access == AccessKind::ExclusiveLoan
            && let Some(origin) = state.non_owning.get(&place.root()).copied()
        {
            self.emit_loan_conflict(
                primary,
                origin,
                "exclusive loan conflicts with a non-owning binding",
            )?;
            return Ok(false);
        }

        if access == AccessKind::Move
            && move_only
            && (matches!(
                self.typed.parameter_mode(place.root()),
                Some(ParameterMode::Borrow | ParameterMode::Inout)
            ) || state.non_owning.contains_key(&place.root()))
        {
            let Some(binding) = self.names.symbols().get(place.root().index()) else {
                return Ok(false);
            };
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.borrowed_move_code,
                "cannot move a non-Copyable value out of a borrowed binding",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                state
                    .non_owning
                    .get(&place.root())
                    .copied()
                    .unwrap_or_else(|| binding.span()),
                "non-owning binding established here",
            )?;
            self.diagnostics.push(diagnostic);
            return Ok(false);
        }

        if access == AccessKind::Mutation
            && (self.typed.parameter_mode(place.root()) == Some(ParameterMode::Borrow)
                || state.borrow_bindings.contains_key(&place.root()))
        {
            let Some(binding) = self.names.symbols().get(place.root().index()) else {
                return Ok(false);
            };
            self.emit_loan_conflict(
                primary,
                binding.span(),
                "mutation conflicts with the shared parameter binding",
            )?;
            return Ok(false);
        }

        if access == AccessKind::Mutation
            && let Some(origin) = state.immutable_captures.get(&place.root()).copied()
        {
            self.emit_loan_conflict(
                primary,
                origin,
                "captured bindings are immutable in v1 closures",
            )?;
            return Ok(false);
        }

        let conflict = state.loans.iter().find(|loan| {
            let overlaps = match &loan.target {
                ActiveLoanTarget::Place(loaned) => loaned.overlaps(place),
                ActiveLoanTarget::This => self
                    .names
                    .symbols()
                    .get(place.root().index())
                    .is_some_and(|symbol| symbol.kind() == SymbolKind::Field),
            };
            overlaps
                && !(matches!(access, AccessKind::Read | AccessKind::SharedLoan)
                    && (loan.kind == LoanKind::Shared
                        || matches!(loan.owner, ActiveLoanOwner::ReservedReceiver(_))))
        });
        if let Some(conflict) = conflict {
            let message = match access {
                AccessKind::Read => "read conflicts with an active exclusive loan",
                AccessKind::Move => "move conflicts with an active loan",
                AccessKind::Mutation => "mutation conflicts with an active loan",
                AccessKind::SharedLoan | AccessKind::ExclusiveLoan => {
                    "new loan conflicts with an active loan"
                }
            };
            self.emit_loan_conflict(primary, conflict.origin, message)?;
            return Ok(false);
        }
        if matches!(
            access,
            AccessKind::Mutation | AccessKind::ExclusiveLoan | AccessKind::Move
        ) {
            state.nullable_views.remove(&place.root());
        }
        Ok(true)
    }

    pub(super) fn emit_loan_conflict(
        &mut self,
        primary: Span,
        origin: Span,
        message: &'static str,
    ) -> Result<(), OwnershipCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.loan_conflict_code,
            message,
            primary,
        )?;
        diagnostic.add_label(self.sources, origin, "conflicting loan starts here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn apply_argument_contract(
        &mut self,
        call: ExpressionId,
        argument: CallArgument,
        mode: ParameterMode,
        is_receiver: bool,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        if self.is_this_receiver(argument.value)?
            && let Some(nominal) = self
                .typed
                .expression_type(argument.value)
                .and_then(|ty| self.receiver_nominal(ty))
        {
            return self.apply_this_contract(
                call,
                argument.value,
                nominal,
                mode,
                is_receiver,
                flows,
            );
        }
        let Some(state) = flows.next.as_mut() else {
            return Ok(());
        };
        let call_span = self.parsed.ast().expressions().get(call)?.span();
        let operand_span = self.parsed.ast().expressions().get(argument.value)?.span();
        match mode {
            ParameterMode::Value => {}
            ParameterMode::Borrow => {
                if let Some(target) = self
                    .range_use(argument.value)
                    .map(|fact| fact.origin().clone())
                {
                    if let LoanTarget::Place(place) = &target {
                        if !self.access_place(
                            place,
                            AccessKind::SharedLoan,
                            false,
                            operand_span,
                            state,
                        )? {
                            return Ok(());
                        }
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Call(call),
                            target: ActiveLoanTarget::Place(place.clone()),
                            kind: LoanKind::Shared,
                            origin: operand_span,
                        });
                    }
                    self.loans.push(LoanFact::new(
                        call,
                        argument.value,
                        target,
                        LoanKind::Shared,
                        operand_span,
                        call_span,
                    ));
                } else if let Some(place) = self.shared_receiver_place(argument.value)? {
                    if self.access_place(
                        &place,
                        AccessKind::SharedLoan,
                        false,
                        operand_span,
                        state,
                    )? {
                        let place = self.canonical_borrow_place(&place, state);
                        self.loans.push(LoanFact::new(
                            call,
                            argument.value,
                            LoanTarget::Place(place.clone()),
                            LoanKind::Shared,
                            operand_span,
                            call_span,
                        ));
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Call(call),
                            target: ActiveLoanTarget::Place(place),
                            kind: LoanKind::Shared,
                            origin: operand_span,
                        });
                    }
                } else if let Some(temporary) =
                    self.temporary_element_owner(argument.value)?.or_else(|| {
                        (self.typed.expression_category(argument.value)
                            == Some(ExpressionCategory::Temporary))
                        .then_some(argument.value)
                    })
                {
                    let temporary = self
                        .constant_temporary_origin(temporary)
                        .map_or(temporary, |(owner, _)| owner);
                    self.loans.push(LoanFact::new(
                        call,
                        argument.value,
                        LoanTarget::Temporary(temporary),
                        LoanKind::Shared,
                        operand_span,
                        call_span,
                    ));
                }
            }
            ParameterMode::Inout => {
                let primary = match argument.mode_marker {
                    Some(ParameterModeMarker::Inout(span)) => span,
                    _ => operand_span,
                };
                let place = self.place(argument.value)?;
                if let Some(place) = &place
                    && let Some(origin) = state.moved.get(&place.root()).copied()
                {
                    let mut diagnostic = Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.use_after_move_code,
                        "use of moved value",
                        operand_span,
                    )?;
                    diagnostic.add_label(self.sources, origin, "value was moved here")?;
                    self.diagnostics.push(diagnostic);
                    return Ok(());
                }
                let receiver_class_handle = is_receiver
                    && self.expression_nominal_kind(argument.value) == Some(NominalKind::Class)
                    && place.as_ref().is_some_and(|place| {
                        self.typed.parameter_mode(place.root()) != Some(ParameterMode::Borrow)
                            && self.field_root_can_inout(place.root())
                    });
                if !receiver_class_handle && !self.is_mutable_place(argument.value)? {
                    let mut diagnostic = Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.immutable_inout_code,
                        "inout argument is not a mutable place",
                        primary,
                    )?;
                    if let Some(place) = &place
                        && let Some(root) = self.names.symbols().get(place.root().index())
                    {
                        diagnostic.add_label(
                            self.sources,
                            root.span(),
                            "immutable binding declared here",
                        )?;
                    }
                    self.diagnostics.push(diagnostic);
                    return Ok(());
                }
                let Some(place) = place else {
                    if let Some(temporary) = self.temporary_element_owner(argument.value)? {
                        let fact = LoanFact::new(
                            call,
                            argument.value,
                            LoanTarget::Temporary(temporary),
                            LoanKind::Exclusive,
                            primary,
                            call_span,
                        );
                        self.loans.push(if is_receiver {
                            fact.reserve_receiver()
                        } else {
                            fact
                        });
                    }
                    return Ok(());
                };
                if self.access_place(&place, AccessKind::ExclusiveLoan, false, primary, state)? {
                    let fact = LoanFact::new(
                        call,
                        argument.value,
                        LoanTarget::Place(place.clone()),
                        LoanKind::Exclusive,
                        primary,
                        call_span,
                    );
                    self.loans.push(if is_receiver {
                        fact.reserve_receiver()
                    } else {
                        fact
                    });
                    state.loans.push(ActiveLoan {
                        owner: if is_receiver {
                            ActiveLoanOwner::ReservedReceiver(call)
                        } else {
                            ActiveLoanOwner::Call(call)
                        },
                        target: ActiveLoanTarget::Place(place),
                        kind: LoanKind::Exclusive,
                        origin: primary,
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn end_call_loans(&self, call: ExpressionId, flows: &mut Flows) {
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            state.loans.retain(|loan| {
                (loan.owner != ActiveLoanOwner::Call(call)
                    || (self.allowed_borrow_call == Some(call)
                        && self.borrowed_call_source(call).and_then(|argument| {
                            self.loans
                                .iter()
                                .find(|fact| fact.call() == call && fact.argument() == argument)
                                .map(|fact| fact.begin_span())
                        }) == Some(loan.origin)))
                    && loan.owner != ActiveLoanOwner::ReservedReceiver(call)
            });
        }
    }

    pub(super) fn is_mutable_place(
        &self,
        expression: ExpressionId,
    ) -> Result<bool, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::This => Ok(self.current_receiver_mode == Some(ParameterMode::Inout)),
            Expression::Name => {
                let Some(symbol) = self.reference_symbol(node.span()) else {
                    return Ok(false);
                };
                if self
                    .names
                    .symbols()
                    .get(symbol.index())
                    .is_some_and(|symbol| symbol.kind() == SymbolKind::Field)
                {
                    return Ok(self.field_kinds.get(&symbol) == Some(&VariableKind::Var)
                        && self.current_receiver_mode == Some(ParameterMode::Inout));
                }
                if self.typed.parameter_mode(symbol) == Some(ParameterMode::Inout) {
                    return Ok(true);
                }
                Ok(self.variable_kinds.get(&symbol) == Some(&VariableKind::Var))
            }
            Expression::Group { expression } => self.is_mutable_place(*expression),
            Expression::Member { .. } => {
                let Some(projection) = self.typed.aggregate_projection(expression) else {
                    return Ok(false);
                };
                if projection.kind() != AggregateProjectionKind::Field
                    || self.field_kinds.get(&projection.field()) != Some(&VariableKind::Var)
                {
                    return Ok(false);
                }
                let AggregateProjectionReceiver::Expression(receiver) = projection.receiver()
                else {
                    return Ok(false);
                };
                if self.is_this_receiver(receiver)? {
                    return Ok(self.current_receiver_mode == Some(ParameterMode::Inout));
                }
                if self.expression_nominal_kind(receiver) == Some(NominalKind::Class) {
                    let Some(place) = self.place(receiver)? else {
                        return Ok(false);
                    };
                    return Ok(self.typed.parameter_mode(place.root())
                        != Some(ParameterMode::Borrow)
                        && self.field_root_can_inout(place.root()));
                }
                self.is_mutable_place(receiver)
            }
            Expression::Index { .. } => {
                let Some(descriptor) = self.element_place_descriptor(expression)? else {
                    return Ok(false);
                };
                if !descriptor.is_mutable() {
                    return Ok(false);
                }
                let Some(place) = self.place(expression)? else {
                    return Ok(self.temporary_element_owner(expression)?.is_some());
                };
                Ok(
                    self.typed.parameter_mode(place.root()) != Some(ParameterMode::Borrow)
                        && self.field_root_can_inout(place.root()),
                )
            }
            _ => Ok(false),
        }
    }

    pub(super) fn is_this_receiver(
        &self,
        mut expression: ExpressionId,
    ) -> Result<bool, OwnershipCheckingError> {
        loop {
            match self.parsed.ast().expressions().get(expression)?.payload() {
                Expression::This => {
                    return Ok(self
                        .reference_symbol(self.parsed.ast().expressions().get(expression)?.span())
                        .is_none());
                }
                Expression::Group { expression: inner } => expression = *inner,
                _ => return Ok(false),
            }
        }
    }

    fn field_root_can_inout(&self, root: SymbolId) -> bool {
        self.names.symbols().get(root.index()).is_none_or(|symbol| {
            symbol.kind() != SymbolKind::Field
                || self.current_receiver_mode == Some(ParameterMode::Inout)
        })
    }

    fn expression_nominal_kind(&self, expression: ExpressionId) -> Option<NominalKind> {
        let mut ty = self.typed.expression_type(expression)?;
        while let Some(TypeKind::Nullable(inner)) = self.typed.types().get(ty) {
            ty = *inner;
        }
        let TypeKind::Nominal { nominal, .. } = self.typed.types().get(ty)? else {
            return None;
        };
        self.typed
            .nominals()
            .iter()
            .find(|descriptor| descriptor.id() == *nominal)
            .map(|descriptor| descriptor.kind())
    }

    pub(super) fn defer(&mut self, expression: ExpressionId, reason: OwnershipDeferredReason) {
        let fact = OwnershipDeferredFact::new(expression, reason);
        if !self.deferred.contains(&fact) {
            self.deferred.push(fact);
        }
    }
}
