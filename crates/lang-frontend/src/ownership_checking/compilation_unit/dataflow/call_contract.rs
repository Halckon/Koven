//! Apply selected argument ownership contracts.
use super::*;
impl Checker<'_> {
    pub(super) fn apply_contract(
        &mut self,
        contract: UnitCallArgumentOwnershipContract,
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
                let target = match contract.category() {
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
                            state.loans.push(ActiveLoan {
                                owner: ActiveLoanOwner::Call(contract.call()),
                                target: ActiveLoanTarget::Place(place.clone()),
                                kind,
                                reserved: false,
                                origin: contract.loan_begin_span(),
                            });
                            UnitLoanTarget::Place(place)
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
            }
        }
        Ok(())
    }
}
