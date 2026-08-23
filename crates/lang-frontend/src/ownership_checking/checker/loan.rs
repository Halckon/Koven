use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity},
    parser::{
        CallArgument, Expression, LiteralKind, ParameterModeMarker, PrefixOperator, VariableKind,
    },
    source::Span,
    type_checking::{
        AggregateProjectionKind, Copyability, ElementPlaceDescriptor, ExpressionCategory,
        NominalKind, ParameterMode, TypeKind,
    },
};

use crate::ownership_checking::{
    ElementIndexIdentity, LoanFact, LoanKind, LoanTarget, OwnershipDeferredFact,
    OwnershipDeferredReason, OwnershipPlace,
};

use super::{AccessKind, ActiveLoan, Checker, Flows, OwnershipCheckingError, State};

impl Checker<'_> {
    pub(super) fn place(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<OwnershipPlace>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name => Ok(self
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
                let Some(mut place) = self.place(projection.receiver())? else {
                    return Ok(None);
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
                if place.push_element(index) {
                    Ok(Some(place))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
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
        self.temporary_expression_origin(descriptor.receiver())
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
            Expression::Literal(LiteralKind::Integer(_)) => self
                .sources
                .slice(node.span())?
                .trim_end_matches(['L', 'l', 'U', 'u'])
                .parse::<i128>()
                .ok(),
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
        if access == AccessKind::Move
            && move_only
            && matches!(
                self.typed.parameter_mode(place.root()),
                Some(ParameterMode::Borrow | ParameterMode::Inout)
            )
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
                binding.span(),
                "non-owning parameter declared here",
            )?;
            self.diagnostics.push(diagnostic);
            return Ok(false);
        }

        if access == AccessKind::Mutation
            && self.typed.parameter_mode(place.root()) == Some(ParameterMode::Borrow)
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

        let conflict = state.loans.iter().find(|loan| {
            loan.place.overlaps(place)
                && !matches!(
                    (loan.kind, access),
                    (LoanKind::Shared, AccessKind::Read | AccessKind::SharedLoan)
                )
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
        Ok(true)
    }

    fn emit_loan_conflict(
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
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(state) = flows.next.as_mut() else {
            return Ok(());
        };
        let call_span = self.parsed.ast().expressions().get(call)?.span();
        let operand_span = self.parsed.ast().expressions().get(argument.value)?.span();
        match mode {
            ParameterMode::Value => {}
            ParameterMode::Borrow => {
                if let Some(place) = self.place(argument.value)? {
                    if self.access_place(
                        &place,
                        AccessKind::SharedLoan,
                        false,
                        operand_span,
                        state,
                    )? {
                        self.loans.push(LoanFact::new(
                            call,
                            argument.value,
                            LoanTarget::Place(place.clone()),
                            LoanKind::Shared,
                            operand_span,
                            call_span,
                        ));
                        state.loans.push(ActiveLoan {
                            call,
                            place,
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
                if !self.is_mutable_place(argument.value)? {
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
                        self.loans.push(LoanFact::new(
                            call,
                            argument.value,
                            LoanTarget::Temporary(temporary),
                            LoanKind::Exclusive,
                            primary,
                            call_span,
                        ));
                    }
                    return Ok(());
                };
                if self.access_place(&place, AccessKind::ExclusiveLoan, false, primary, state)? {
                    self.loans.push(LoanFact::new(
                        call,
                        argument.value,
                        LoanTarget::Place(place.clone()),
                        LoanKind::Exclusive,
                        primary,
                        call_span,
                    ));
                    state.loans.push(ActiveLoan {
                        call,
                        place,
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
            state.loans.retain(|loan| loan.call != call);
        }
    }

    fn is_mutable_place(&self, expression: ExpressionId) -> Result<bool, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name => {
                let Some(symbol) = self.reference_symbol(node.span()) else {
                    return Ok(false);
                };
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
                if self.expression_nominal_kind(projection.receiver()) == Some(NominalKind::Class) {
                    let Some(place) = self.place(projection.receiver())? else {
                        return Ok(false);
                    };
                    return Ok(
                        self.typed.parameter_mode(place.root()) != Some(ParameterMode::Borrow)
                    );
                }
                self.is_mutable_place(projection.receiver())
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
                Ok(self.typed.parameter_mode(place.root()) != Some(ParameterMode::Borrow))
            }
            _ => Ok(false),
        }
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
