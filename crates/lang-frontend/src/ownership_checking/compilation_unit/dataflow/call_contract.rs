//! 应用已选定的调用实参所有权合同。
use super::*;

impl Checker<'_> {
    pub(super) fn copy_to_nullable_borrow(
        &self,
        contract: UnitCallArgumentOwnershipContract,
    ) -> bool {
        contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
            && contract.category() == ExpressionCategory::Place
            && self
                .typed
                .expression_type(contract.argument())
                .is_some_and(|ty| {
                    self.typed.copyability(ty) == crate::type_checking::Copyability::Copyable
                })
            && self.borrow_needs_nullable_conversion(
                contract.argument().expression(),
                contract.parameter_type(),
            )
    }

    pub(super) fn apply_contract(
        &mut self,
        contract: UnitCallArgumentOwnershipContract,
        copy_to_nullable: bool,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        match contract.kind() {
            UnitCallArgumentOwnershipKind::Value => {
                let Some((kind, source)) = self.value_delivery(contract, state)? else {
                    return Ok(());
                };
                self.value_deliveries.push(UnitValueDeliveryFact::new(
                    contract.call(),
                    contract.argument(),
                    source,
                    kind,
                    contract.argument_span(),
                    contract.parameter_span(),
                ));
                self.record_nullable_when_delivery(contract, state)?;
            }
            UnitCallArgumentOwnershipKind::SharedLoan
            | UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                let kind = if contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan {
                    LoanKind::Shared
                } else {
                    LoanKind::Exclusive
                };
                if kind == LoanKind::Exclusive
                    && !self.is_mutable_place(contract.argument().expression())?
                {
                    let mut diagnostic = Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.codes.immutable_inout,
                        "inout argument is not a mutable place",
                        contract.loan_begin_span(),
                    )?;
                    if let Some(place) = self.place(contract.argument().expression())? {
                        diagnostic.add_label(
                            self.sources,
                            self.symbol_span(place.root())?,
                            "immutable binding declared here",
                        )?;
                    }
                    add_parameter_label(self.sources, &mut diagnostic, contract.parameter_span())?;
                    self.diagnostics.push(diagnostic);
                    return Ok(());
                }
                let borrow_use = (kind == LoanKind::Shared)
                    .then(|| {
                        self.borrow_call_use(
                            contract.call().expression(),
                            contract.argument().expression(),
                        )
                    })
                    .flatten();
                let copied_borrow = borrow_use.is_some_and(|fact| {
                    fact.end_point()
                        == crate::ownership_checking::UnitDropPoint::AfterExpression(
                            contract.argument(),
                        )
                });
                let range_root = (kind == LoanKind::Shared)
                    .then(|| {
                        borrow_use
                            .filter(|_| !copied_borrow)
                            .map(|fact| fact.origin().clone())
                            .or_else(|| {
                                self.range_use(contract.argument().expression())
                                    .map(|fact| fact.origin().clone())
                            })
                    })
                    .flatten();
                let target = if let Some(target) = range_root {
                    if let UnitLoanTarget::Place(place) = &target {
                        if !self.access_place(
                            place,
                            AccessKind::SharedLoan,
                            contract.loan_begin_span(),
                            contract.parameter_span(),
                            state,
                        )? {
                            return Ok(());
                        }
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Call(contract.call()),
                            target: ActiveLoanTarget::Place(place.clone()),
                            kind,
                            reserved: false,
                            origin: contract.loan_begin_span(),
                        });
                    }
                    target
                } else if copy_to_nullable || copied_borrow {
                    // The operand has already been read. The shared loan is of the wrapper.
                    UnitLoanTarget::Temporary(contract.argument())
                } else {
                    match contract.category() {
                        ExpressionCategory::Temporary => {
                            if kind == LoanKind::Exclusive {
                                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                                    source_unit: contract.argument().source_unit().index(),
                                    expression: contract.argument().expression().index(),
                                });
                            }
                            UnitLoanTarget::Temporary(
                                self.constant_temporary_origin(contract.argument().expression())
                                    .map_or(contract.argument(), |(owner, _)| owner),
                            )
                        }
                        ExpressionCategory::Place => {
                            if self.expression_is_this(contract.argument().expression())? {
                                let Some(target) =
                                    self.apply_this_argument_loan(contract, kind, state)?
                                else {
                                    return Ok(());
                                };
                                target
                            } else if let Some(place) =
                                self.loan_place(contract.argument().expression())?
                            {
                                let access = if kind == LoanKind::Shared {
                                    AccessKind::SharedLoan
                                } else {
                                    AccessKind::ExclusiveLoan
                                };
                                if !self.access_place(
                                    &place,
                                    access,
                                    contract.loan_begin_span(),
                                    contract.parameter_span(),
                                    state,
                                )? {
                                    return Ok(());
                                }
                                let place = self.canonical_borrow_place(&place, state);
                                state.loans.push(ActiveLoan {
                                    owner: ActiveLoanOwner::Call(contract.call()),
                                    target: ActiveLoanTarget::Place(place.clone()),
                                    kind,
                                    reserved: false,
                                    origin: contract.loan_begin_span(),
                                });
                                UnitLoanTarget::Place(place)
                            } else if kind == LoanKind::Shared
                                && let Some(owner) =
                                    self.temporary_field_owner(contract.argument().expression())?
                            {
                                UnitLoanTarget::Temporary(owner)
                            } else if let Some(owner) =
                                self.temporary_projection_owner(contract.argument().expression())?
                            {
                                UnitLoanTarget::Temporary(owner)
                            } else {
                                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                                    source_unit: contract.argument().source_unit().index(),
                                    expression: contract.argument().expression().index(),
                                });
                            }
                        }
                    }
                };
                self.loans.push(UnitLoanFact::new(
                    contract.call(),
                    contract.argument(),
                    target,
                    kind,
                    contract.loan_begin_span(),
                    contract.call_span(),
                    contract.parameter_span(),
                ));
                self.record_nullable_when_loan(contract)?;
            }
        }
        Ok(())
    }
}
