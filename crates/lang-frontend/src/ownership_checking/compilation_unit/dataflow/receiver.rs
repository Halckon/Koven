//! Instance receiver 作为第零调用操作数的 ownership 效果。

use crate::{
    diagnostic::{Diagnostic, Severity},
    ownership_checking::{LoanKind, UnitCallArgumentOwnershipKind},
    parser::Expression,
    type_checking::{
        Copyability, ExpressionCategory, NominalKind, ParameterMode, UnitCallReceiverOrigin,
        UnitCallTarget, UnitTypeKind,
    },
};

use super::{
    AccessKind, ActiveLoan, ActiveLoanOwner, ActiveLoanTarget, Checker, OwnershipBindingKind,
    OwnershipCheckingError, State, UnitCallReceiverOwnershipContract,
    UnitConditionalReceiverDeliveryFact, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
    UnitReceiverOwnershipTarget, add_parameter_label,
};

impl Checker<'_> {
    pub(super) fn expression_is_this(
        &self,
        expression: crate::ast::ExpressionId,
    ) -> Result<bool, OwnershipCheckingError> {
        match self.parsed.ast().expressions().get(expression)?.payload() {
            Expression::This => Ok(self
                .reference_symbol(self.parsed.ast().expressions().get(expression)?.span())
                .is_none()),
            Expression::Group { expression } => self.expression_is_this(*expression),
            _ => Ok(false),
        }
    }

    pub(super) fn use_this(
        &mut self,
        expression: crate::ast::ExpressionId,
        span: crate::source::Span,
        usage: super::ExpressionUse,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(receiver) = self.current_receiver else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        };
        if !self.ensure_this_available_at(span, None, state)? {
            return Ok(());
        }
        match usage {
            super::ExpressionUse::Read => {
                self.access_this_at(AccessKind::Read, span, None, state)?;
            }
            super::ExpressionUse::Place { .. } => {}
            super::ExpressionUse::Consume { parameter_span } => {
                if self.typed.copyability(receiver.ty) == Copyability::MoveOnly
                    && self.has_reserved_this_loan(state)
                    && !self.access_this_at(AccessKind::Move, span, parameter_span, state)?
                {
                    return Ok(());
                }
                if let Some(capture) = state.loans.iter().find(|loan| {
                    matches!(loan.target, ActiveLoanTarget::This)
                        && loan.kind == LoanKind::Shared
                        && matches!(loan.owner, ActiveLoanOwner::Closure(_))
                }) {
                    match self.typed.copyability(receiver.ty) {
                        Copyability::Copyable => {
                            self.access_this_at(AccessKind::Read, span, parameter_span, state)?;
                        }
                        Copyability::MoveOnly => {
                            let mut diagnostic = Diagnostic::new(
                                self.sources,
                                Severity::Error,
                                self.codes.borrowed_move,
                                "cannot move a non-Copyable this receiver from a shared capture",
                                span,
                            )?;
                            diagnostic.add_label(
                                self.sources,
                                capture.origin,
                                "shared capture established here",
                            )?;
                            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
                            self.diagnostics.push(diagnostic);
                        }
                        Copyability::Unknown | Copyability::Error => {
                            return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                                source_unit: self.source_unit.index(),
                                expression: expression.index(),
                            });
                        }
                    }
                    return Ok(());
                }
                match self.typed.copyability(receiver.ty) {
                    Copyability::Copyable => {
                        self.access_this_at(AccessKind::Read, span, parameter_span, state)?;
                    }
                    Copyability::MoveOnly if receiver.mode == ParameterMode::Value => {
                        if self.access_this_at(AccessKind::Move, span, parameter_span, state)? {
                            state.this_moved = Some(span);
                        }
                    }
                    Copyability::MoveOnly => {
                        let mut diagnostic = Diagnostic::new(
                            self.sources,
                            Severity::Error,
                            self.codes.borrowed_move,
                            "cannot move a non-Copyable this receiver from a non-owning method",
                            span,
                        )?;
                        diagnostic.add_label(
                            self.sources,
                            receiver.declaration_span,
                            "non-owning receiver declared here",
                        )?;
                        add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
                        self.diagnostics.push(diagnostic);
                    }
                    Copyability::Unknown | Copyability::Error => {
                        return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                            source_unit: self.source_unit.index(),
                            expression: expression.index(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// `this` is a stable non-owning argument binding, including transparent groups.
    pub(super) fn apply_this_argument_loan(
        &mut self,
        contract: super::UnitCallArgumentOwnershipContract,
        kind: LoanKind,
        state: &mut State,
    ) -> Result<Option<super::UnitLoanTarget>, OwnershipCheckingError> {
        let Some(receiver) = self.current_receiver else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: contract.argument().expression().index(),
            });
        };
        let access = if kind == LoanKind::Shared {
            AccessKind::SharedLoan
        } else {
            AccessKind::ExclusiveLoan
        };
        if !self.ensure_this_available_at(
            contract.loan_begin_span(),
            contract.parameter_span(),
            state,
        )? || !self.access_this_at(
            access,
            contract.loan_begin_span(),
            contract.parameter_span(),
            state,
        )? {
            return Ok(None);
        }
        state.loans.push(ActiveLoan {
            owner: ActiveLoanOwner::Call(contract.call()),
            target: ActiveLoanTarget::This,
            kind,
            reserved: false,
            origin: contract.loan_begin_span(),
        });
        Ok(Some(super::UnitLoanTarget::This(receiver.owner)))
    }

    pub(super) fn apply_receiver_contract(
        &mut self,
        contract: UnitCallReceiverOwnershipContract,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let effect = match contract.source() {
            UnitCallReceiverOrigin::Expression(expression) => {
                if expression.source_unit() != self.source_unit {
                    return Err(OwnershipCheckingError::InvalidUnitCall {
                        source_unit: contract.call().source_unit().index(),
                        expression: contract.call().expression().index(),
                    });
                }
                self.explicit_receiver_effect(contract, expression.expression(), state)?
            }
            UnitCallReceiverOrigin::ImplicitThis(owner) => {
                self.implicit_receiver_effect(contract, owner, state)?
            }
        };
        let Some((target, kind)) = effect else {
            return Ok(());
        };
        // Only a proven range producer or definite Map result continues this actual source loan.
        if (self.typed.map_require_value(contract.call()).is_some()
            || self.typed.map_with_value(contract.call()).is_some()
            || self.range_expression_is_proven(contract.call().expression()))
            && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
            && let UnitCallReceiverOrigin::Expression(argument) = contract.source()
        {
            let source = match &target {
                UnitReceiverOwnershipTarget::Place(place) => {
                    Some(super::UnitLoanTarget::Place(place.clone()))
                }
                UnitReceiverOwnershipTarget::Temporary(expression) => {
                    Some(super::UnitLoanTarget::Temporary(*expression))
                }
                UnitReceiverOwnershipTarget::This(_) => None,
            };
            if let Some(source) = source {
                self.loans
                    .push(crate::ownership_checking::UnitLoanFact::new(
                        contract.call(),
                        argument,
                        source,
                        LoanKind::Shared,
                        contract.receiver_span(),
                        contract.call_span(),
                        contract.declaration_span(),
                    ));
            }
        }
        self.receiver_facts.push(UnitReceiverOwnershipFact::new(
            contract.call(),
            contract.source(),
            target,
            kind,
            contract.receiver_type(),
            contract.receiver_span(),
            contract.call_span(),
            contract.declaration_span(),
        ));
        Ok(())
    }

    /// Activate only the receiver reservation; ordinary Inout arguments are already exclusive.
    pub(super) fn activate_receiver(
        &mut self,
        call: crate::type_checking::UnitExpressionId,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(index) = state
            .loans
            .iter()
            .position(|loan| loan.owner == ActiveLoanOwner::Call(call) && loan.reserved)
        else {
            return Ok(());
        };
        let reservation = &state.loans[index];
        let conflict = state
            .loans
            .iter()
            .enumerate()
            .find_map(|(other_index, loan)| {
                if other_index == index {
                    return None;
                }
                let overlaps = match (&reservation.target, &loan.target) {
                    (ActiveLoanTarget::Place(left), ActiveLoanTarget::Place(right)) => {
                        left.overlaps(right)
                    }
                    (ActiveLoanTarget::This, ActiveLoanTarget::This) => true,
                    (ActiveLoanTarget::This, ActiveLoanTarget::Place(place))
                    | (ActiveLoanTarget::Place(place), ActiveLoanTarget::This) => {
                        self.symbol_kind(place.root())
                            == Some(crate::name_resolution::SymbolKind::Field)
                    }
                };
                overlaps.then_some(loan.origin)
            });
        if let Some(primary) = conflict {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.loan_conflict,
                "argument loan conflicts with receiver activation",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                reservation.origin,
                "conflicting loan starts here",
            )?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        let fact = self
            .receiver_facts
            .iter_mut()
            .find(|fact| fact.call() == call)
            .ok_or(OwnershipCheckingError::InvalidUnitCall {
                source_unit: call.source_unit().index(),
                expression: call.expression().index(),
            })?;
        state.loans[index].reserved = false;
        fact.activate();
        Ok(())
    }

    fn explicit_receiver_effect(
        &mut self,
        contract: UnitCallReceiverOwnershipContract,
        expression: crate::ast::ExpressionId,
        state: &mut State,
    ) -> Result<
        Option<(UnitReceiverOwnershipTarget, UnitReceiverOwnershipKind)>,
        OwnershipCheckingError,
    > {
        if self.expression_is_this(expression)? {
            let Some(receiver) = self.current_receiver else {
                return Err(OwnershipCheckingError::InvalidUnitCall {
                    source_unit: self.source_unit.index(),
                    expression: contract.call().expression().index(),
                });
            };
            return self.implicit_receiver_effect(contract, receiver.owner, state);
        }
        if contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
            && let Some(fact) = self.range_use(expression)
        {
            let target = fact.origin().clone();
            return match target {
                super::UnitLoanTarget::Place(place) => {
                    if !self.access_place(
                        &place,
                        AccessKind::SharedLoan,
                        contract.receiver_span(),
                        contract.declaration_span(),
                        state,
                    )? {
                        return Ok(None);
                    }
                    state.loans.push(ActiveLoan {
                        owner: ActiveLoanOwner::Call(contract.call()),
                        target: ActiveLoanTarget::Place(place.clone()),
                        kind: LoanKind::Shared,
                        reserved: false,
                        origin: contract.receiver_span(),
                    });
                    Ok(Some((
                        UnitReceiverOwnershipTarget::Place(place),
                        UnitReceiverOwnershipKind::SharedLoan,
                    )))
                }
                super::UnitLoanTarget::Temporary(expression) => Ok(Some((
                    UnitReceiverOwnershipTarget::Temporary(expression),
                    UnitReceiverOwnershipKind::SharedLoan,
                ))),
                super::UnitLoanTarget::This(_) => {
                    Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                        source_unit: self.source_unit.index(),
                        expression: expression.index(),
                    })
                }
            };
        }
        if contract.category() == ExpressionCategory::Temporary {
            return match contract.kind() {
                UnitCallArgumentOwnershipKind::SharedLoan => Ok(Some((
                    UnitReceiverOwnershipTarget::Temporary(self.unit_expression(expression)),
                    UnitReceiverOwnershipKind::SharedLoan,
                ))),
                UnitCallArgumentOwnershipKind::Value => Ok(Some((
                    UnitReceiverOwnershipTarget::Temporary(self.unit_expression(expression)),
                    UnitReceiverOwnershipKind::Temporary,
                ))),
                UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                    self.emit_immutable_receiver(contract, None)?;
                    Ok(None)
                }
            };
        }
        let Some(place) = self.loan_place(expression)? else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        };
        match contract.kind() {
            UnitCallArgumentOwnershipKind::Value => {
                let kind = match self.typed.copyability(contract.receiver_type()) {
                    Copyability::Copyable => UnitReceiverOwnershipKind::Copy,
                    Copyability::MoveOnly => UnitReceiverOwnershipKind::Move,
                    Copyability::Unknown | Copyability::Error => {
                        return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                            source_unit: self.source_unit.index(),
                            expression: expression.index(),
                        });
                    }
                };
                Ok(Some((UnitReceiverOwnershipTarget::Place(place), kind)))
            }
            UnitCallArgumentOwnershipKind::SharedLoan
            | UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                let (loan, access, effect) =
                    if contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan {
                        (
                            LoanKind::Shared,
                            AccessKind::SharedLoan,
                            UnitReceiverOwnershipKind::SharedLoan,
                        )
                    } else {
                        if !self.is_mutable_receiver_place(expression)? {
                            self.emit_immutable_receiver(contract, Some(&place))?;
                            return Ok(None);
                        }
                        (
                            LoanKind::Exclusive,
                            AccessKind::ExclusiveLoan,
                            UnitReceiverOwnershipKind::ExclusiveLoan,
                        )
                    };
                if !self.access_place(
                    &place,
                    access,
                    contract.receiver_span(),
                    contract.declaration_span(),
                    state,
                )? {
                    return Ok(None);
                }
                let place = self.canonical_borrow_place(&place, state);
                state.loans.push(ActiveLoan {
                    owner: ActiveLoanOwner::Call(contract.call()),
                    target: ActiveLoanTarget::Place(place.clone()),
                    kind: loan,
                    reserved: loan == LoanKind::Exclusive,
                    origin: contract.receiver_span(),
                });
                Ok(Some((UnitReceiverOwnershipTarget::Place(place), effect)))
            }
        }
    }

    fn implicit_receiver_effect(
        &mut self,
        contract: UnitCallReceiverOwnershipContract,
        owner: crate::name_resolution::DeclarationId,
        state: &mut State,
    ) -> Result<
        Option<(UnitReceiverOwnershipTarget, UnitReceiverOwnershipKind)>,
        OwnershipCheckingError,
    > {
        if !self.ensure_this_available(contract, state)? {
            return Ok(None);
        }
        let Some(current) = self.current_receiver else {
            return Err(OwnershipCheckingError::InvalidUnitCall {
                source_unit: self.source_unit.index(),
                expression: contract.call().expression().index(),
            });
        };
        if contract.kind() == UnitCallArgumentOwnershipKind::Value
            && self.typed.copyability(contract.receiver_type()) != Copyability::Copyable
            && self.has_reserved_this_loan(state)
            && !self.access_this_at(
                AccessKind::Move,
                contract.receiver_span(),
                contract.declaration_span(),
                state,
            )?
        {
            return Ok(None);
        }
        match contract.kind() {
            UnitCallArgumentOwnershipKind::ExclusiveLoan
                if current.mode != ParameterMode::Inout =>
            {
                self.emit_this_capability(
                    contract.receiver_span(),
                    contract.declaration_span(),
                    "current receiver cannot provide exclusive access",
                    self.codes.immutable_inout,
                    current.declaration_span,
                )?;
                return Ok(None);
            }
            UnitCallArgumentOwnershipKind::Value if current.mode != ParameterMode::Value => {
                self.emit_this_capability(
                    contract.receiver_span(),
                    contract.declaration_span(),
                    "current receiver cannot deliver an owned value",
                    self.codes.borrowed_move,
                    current.declaration_span,
                )?;
                return Ok(None);
            }
            _ => {}
        }
        let effect = match contract.kind() {
            UnitCallArgumentOwnershipKind::SharedLoan => {
                if !self.access_this(contract, LoanKind::Shared, state)? {
                    return Ok(None);
                }
                state.loans.push(ActiveLoan {
                    owner: ActiveLoanOwner::Call(contract.call()),
                    target: ActiveLoanTarget::This,
                    kind: LoanKind::Shared,
                    reserved: false,
                    origin: contract.receiver_span(),
                });
                UnitReceiverOwnershipKind::SharedLoan
            }
            UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                if !self.access_this(contract, LoanKind::Exclusive, state)? {
                    return Ok(None);
                }
                state.loans.push(ActiveLoan {
                    owner: ActiveLoanOwner::Call(contract.call()),
                    target: ActiveLoanTarget::This,
                    kind: LoanKind::Exclusive,
                    reserved: true,
                    origin: contract.receiver_span(),
                });
                UnitReceiverOwnershipKind::ExclusiveLoan
            }
            UnitCallArgumentOwnershipKind::Value => {
                let static_self = matches!(
                    self.typed.types().get(contract.receiver_type()),
                    Some(UnitTypeKind::StaticSelf(_))
                );
                let copyability = self.typed.copyability(contract.receiver_type());
                if (static_self || copyability == Copyability::MoveOnly)
                    && let Some(origin) = state
                        .loans
                        .iter()
                        .find(|loan| {
                            matches!(loan.target, ActiveLoanTarget::This)
                                && loan.kind == LoanKind::Shared
                                && matches!(loan.owner, ActiveLoanOwner::Closure(_))
                        })
                        .map(|loan| loan.origin)
                {
                    let mut diagnostic = Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.codes.borrowed_move,
                        "cannot deliver this by value from a shared capture",
                        contract.receiver_span(),
                    )?;
                    diagnostic.add_label(
                        self.sources,
                        origin,
                        "shared capture established here",
                    )?;
                    self.diagnostics.push(diagnostic);
                    return Ok(None);
                }
                if static_self {
                    self.publish_conditional_receiver_delivery(contract, current, owner)?;
                    return Ok(None);
                }
                match copyability {
                    Copyability::Copyable => {
                        if !self.access_this_at(
                            AccessKind::Read,
                            contract.receiver_span(),
                            contract.declaration_span(),
                            state,
                        )? {
                            return Ok(None);
                        }
                        UnitReceiverOwnershipKind::Copy
                    }
                    Copyability::MoveOnly => {
                        if !self.access_this_at(
                            AccessKind::Move,
                            contract.receiver_span(),
                            contract.declaration_span(),
                            state,
                        )? {
                            return Ok(None);
                        }
                        state.this_moved = Some(contract.receiver_span());
                        UnitReceiverOwnershipKind::Move
                    }
                    Copyability::Unknown | Copyability::Error => {
                        return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                            source_unit: self.source_unit.index(),
                            expression: contract.call().expression().index(),
                        });
                    }
                }
            }
        };
        Ok(Some((UnitReceiverOwnershipTarget::This(owner), effect)))
    }

    fn publish_conditional_receiver_delivery(
        &mut self,
        contract: UnitCallReceiverOwnershipContract,
        current: super::ReceiverContext,
        owner: crate::name_resolution::DeclarationId,
    ) -> Result<(), OwnershipCheckingError> {
        let selected_receiver =
            super::super::contracts::source_callable_signature(self.typed, contract.target())
                .and_then(crate::type_checking::UnitCallableSignature::receiver);
        let valid_template =
            self.static_self_owner(contract.receiver_type()) == Some(current.owner);
        let selected_owner = selected_receiver
            .filter(|receiver| receiver.mode() == ParameterMode::Value)
            .and_then(|receiver| self.static_self_owner(receiver.ty()));
        let selected_owner_is_reachable = selected_owner.is_some_and(|selected_owner| {
            selected_owner == current.owner
                || self
                    .typed
                    .signatures()
                    .declaration(current.owner)
                    .and_then(|declaration| declaration.nominal())
                    .is_some_and(|nominal| {
                        nominal.interfaces().iter().any(|interface| {
                            matches!(
                                self.typed.types().get(*interface),
                                Some(UnitTypeKind::Nominal { declaration, .. })
                                    if *declaration == selected_owner
                            )
                        })
                    })
        });
        if !valid_template
            || current.owner != owner
            || current.mode != ParameterMode::Value
            || current.ty != contract.receiver_type()
            || !matches!(contract.target(), UnitCallTarget::Symbol(_))
            || !selected_owner_is_reachable
            || self
                .conditional_receiver_deliveries
                .iter()
                .any(|fact| fact.call() == contract.call())
        {
            return Err(OwnershipCheckingError::InvalidUnitCall {
                source_unit: contract.call().source_unit().index(),
                expression: contract.call().expression().index(),
            });
        }
        self.conditional_receiver_deliveries
            .push(UnitConditionalReceiverDeliveryFact::new(
                contract.call(),
                contract.source(),
                current.owner,
                contract.target(),
                contract.receiver_type(),
                current.declaration_span,
                contract.receiver_span(),
            ));
        Ok(())
    }

    fn static_self_owner(
        &self,
        receiver_type: crate::type_checking::UnitTypeId,
    ) -> Option<crate::name_resolution::DeclarationId> {
        let UnitTypeKind::StaticSelf(interface) = self.typed.types().get(receiver_type)? else {
            return None;
        };
        match self.typed.types().get(*interface)? {
            UnitTypeKind::Nominal { declaration, .. } => Some(*declaration),
            _ => None,
        }
    }

    pub(super) fn require_mutable_this(
        &mut self,
        primary: crate::source::Span,
        declaration_span: Option<crate::source::Span>,
    ) -> Result<bool, OwnershipCheckingError> {
        let Some(receiver) = self.current_receiver else {
            return Ok(false);
        };
        if receiver.mode == ParameterMode::Inout {
            return Ok(true);
        }
        self.emit_this_capability(
            primary,
            declaration_span,
            "current receiver is not exclusively mutable",
            self.codes.immutable_inout,
            receiver.declaration_span,
        )?;
        Ok(false)
    }

    fn emit_this_capability(
        &mut self,
        primary: crate::source::Span,
        parameter_span: Option<crate::source::Span>,
        message: &'static str,
        code: crate::diagnostic::DiagnosticCode,
        receiver_declaration: crate::source::Span,
    ) -> Result<(), OwnershipCheckingError> {
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        diagnostic.add_label(
            self.sources,
            receiver_declaration,
            "receiver capability declared here",
        )?;
        add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn ensure_this_available(
        &mut self,
        contract: UnitCallReceiverOwnershipContract,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        self.ensure_this_available_at(contract.receiver_span(), contract.declaration_span(), state)
    }

    pub(super) fn ensure_this_available_at(
        &mut self,
        primary: crate::source::Span,
        declaration_span: Option<crate::source::Span>,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        let Some(origin) = state.this_moved else {
            return Ok(true);
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.use_after_move,
            "use of moved this receiver",
            primary,
        )?;
        diagnostic.add_label(self.sources, origin, "receiver was moved here")?;
        add_parameter_label(self.sources, &mut diagnostic, declaration_span)?;
        self.diagnostics.push(diagnostic);
        Ok(false)
    }

    fn access_this(
        &mut self,
        contract: UnitCallReceiverOwnershipContract,
        kind: LoanKind,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        let access = if kind == LoanKind::Shared {
            AccessKind::SharedLoan
        } else {
            AccessKind::ExclusiveLoan
        };
        self.access_this_at(
            access,
            contract.receiver_span(),
            contract.declaration_span(),
            state,
        )
    }

    fn has_reserved_this_loan(&self, state: &State) -> bool {
        state.loans.iter().any(|loan| {
            loan.reserved
                && match &loan.target {
                    ActiveLoanTarget::This => true,
                    ActiveLoanTarget::Place(place) => {
                        self.symbol_kind(place.root())
                            == Some(crate::name_resolution::SymbolKind::Field)
                    }
                }
        })
    }

    pub(super) fn access_this_at(
        &mut self,
        access: AccessKind,
        primary: crate::source::Span,
        declaration_span: Option<crate::source::Span>,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        let conflict = state.loans.iter().find(|loan| {
            let overlaps = match &loan.target {
                ActiveLoanTarget::This => true,
                ActiveLoanTarget::Place(place) => {
                    self.symbol_kind(place.root())
                        == Some(crate::name_resolution::SymbolKind::Field)
                }
            };
            overlaps && loan.conflicts_with(access)
        });
        let Some(conflict) = conflict else {
            return Ok(true);
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.loan_conflict,
            "receiver loan conflicts with an active loan",
            primary,
        )?;
        diagnostic.add_label(
            self.sources,
            conflict.origin,
            "conflicting loan starts here",
        )?;
        add_parameter_label(self.sources, &mut diagnostic, declaration_span)?;
        self.diagnostics.push(diagnostic);
        Ok(false)
    }

    fn emit_immutable_receiver(
        &mut self,
        contract: UnitCallReceiverOwnershipContract,
        place: Option<&crate::ownership_checking::UnitOwnershipPlace>,
    ) -> Result<(), OwnershipCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.immutable_inout,
            "inout receiver is not an exclusively mutable place",
            contract.receiver_span(),
        )?;
        if let Some(place) = place {
            diagnostic.add_label(
                self.sources,
                self.symbol_span(place.root())?,
                "immutable binding declared here",
            )?;
        }
        add_parameter_label(self.sources, &mut diagnostic, contract.declaration_span())?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn is_mutable_receiver_place(
        &self,
        expression: crate::ast::ExpressionId,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.expression_nominal_kind(expression) == Some(NominalKind::Class) {
            let Some(place) = self.place(expression)? else {
                return Ok(false);
            };
            return Ok(self
                .bindings
                .get(&place.root())
                .is_none_or(|binding| binding.kind() != OwnershipBindingKind::Shared));
        }
        self.is_mutable_place(expression)
    }
}

#[derive(Clone, Copy)]
pub(super) struct ReceiverContext {
    pub(super) owner: super::DeclarationId,
    pub(super) mode: ParameterMode,
    pub(super) ty: super::UnitTypeId,
    pub(super) declaration_span: crate::source::Span,
}
