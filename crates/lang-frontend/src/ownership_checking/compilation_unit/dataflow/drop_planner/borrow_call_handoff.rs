//! Observe the existing normal-return token retirement without changing its cleanup.
#[path = "distinct_auxiliary.rs"]
pub(super) mod distinct_auxiliary;
use super::{DropPlanner, ValueState};
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        LoanKind, UnitBorrowCallHandoffFact, UnitIterationCleanupAction, UnitLoanTarget,
    },
    type_checking::{CallableResultSource, UnitCallTarget},
};

pub(super) struct PendingHandoff {
    fact: UnitBorrowCallHandoffFact,
    source_begin: Option<super::action_source::ActionHandle>,
    auxiliary_begins: Vec<Option<super::action_source::ActionHandle>>,
    source_live: bool,
    narrow_shape: bool,
    distinct: Option<distinct_auxiliary::Witness>,
}

impl DropPlanner<'_, '_> {
    pub(super) fn observe_borrow_call_handoff(&mut self, id: ExpressionId, state: &mut ValueState) {
        let call_id = self.checker.unit_expression(id);
        let Some(call) = self.checker.typed.call(call_id) else {
            return;
        };
        if call.receiver().is_some()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || !matches!(call.result_source(), CallableResultSource::Borrow(_))
        {
            return;
        }
        let Some(binding) = self
            .borrow_bindings_initialized_by(call_id)
            .find(|binding| {
                binding.parent().is_none()
                    && state.pending_borrow_results.contains(&binding.binding())
                    && binding
                        .source_loan()
                        .is_some_and(|source| source.call() == call_id)
            })
        else {
            return;
        };
        let source = binding.source_loan().expect("checked source");
        let loans: Vec<_> = state
            .pending_loans
            .iter()
            .rev()
            .filter_map(|super::model::PendingLoan(action, _, _)| match action {
                UnitIterationCleanupAction::EndCallLoan(loan) if loan.call() == call_id => {
                    Some(loan.clone())
                }
                _ => None,
            })
            .collect();
        // The source stays a Shared owned root. Auxiliary roots may be Exclusive;
        // auxiliary temporaries require the exact still-pending Shared owner.
        if loans.iter().any(|loan| {
            !(matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root())
                || self.observes_temporary_borrow_auxiliary(loan, state))
        }) {
            return;
        }
        let matching: Vec<_> = loans
            .iter()
            .filter(|loan| loan.argument() == source.argument())
            .collect();
        let [source_loan] = matching.as_slice() else {
            return;
        };
        if source_loan.kind() != LoanKind::Shared || source_loan.target() != binding.origin() {
            return;
        }
        let UnitLoanTarget::Place(place) = source_loan.target() else {
            return;
        };
        if !self.loan_provenance().formed_owned.contains(&place.root())
            || state.position(place.root()).is_none()
        {
            return;
        }
        let source_loan = (*source_loan).clone();
        let mut source_tokens = state.pending_loans.iter().filter(|pending| {
            matches!(&pending.0, UnitIterationCleanupAction::EndCallLoan(loan) if *loan == source_loan)
        });
        let source_begin = source_tokens.next().and_then(|pending| pending.2.clone());
        let source_unique = source_tokens.next().is_none();
        let binding = binding.binding();
        let mut auxiliary_loan_ends = Vec::new();
        let mut auxiliary_begins = Vec::new();
        // These exact tokens would otherwise be removed by the immediately following
        // call-local retain. Observe that retirement, not descriptor end spans.
        let mut index = 0;
        while index < state.pending_loans.len() {
            let UnitIterationCleanupAction::EndCallLoan(loan) = &state.pending_loans[index].0
            else {
                index += 1;
                continue;
            };
            if loan.call() != call_id
                || loan.argument() == source_loan.argument()
                || !(self.observes_borrow_root(loan, state)
                    || self.observes_temporary_borrow_auxiliary(loan, state))
            {
                index += 1;
                continue;
            }
            let super::model::PendingLoan(UnitIterationCleanupAction::EndCallLoan(loan), _, begin) =
                state.pending_loans.remove(index)
            else {
                unreachable!("matched actual pending token")
            };
            auxiliary_loan_ends.push(loan);
            auxiliary_begins.push(begin);
        }
        let source_live = source_unique && source_begin.as_ref().is_some_and(|begin| {
            let matching = state.pending_loans.iter().filter(|pending| {
                matches!(&pending.0, UnitIterationCleanupAction::EndCallLoan(loan) if *loan == source_loan)
                    && pending.2.as_ref().is_some_and(|actual| actual.same_record(begin))
            }).count();
            matching == 1
        });
        let narrow_shape =
            self.auxiliary_retirement_shape(&source_loan, &auxiliary_loan_ends, state);
        let distinct = self
            .distinct_auxiliary_witness(&source_loan, &auxiliary_loan_ends, state)
            .filter(|witness| source_live && witness.still_live(state));
        self.pending_borrow_handoffs.push(PendingHandoff {
            fact: UnitBorrowCallHandoffFact {
                source_loan,
                binding,
                auxiliary_loan_ends,
            },
            source_begin,
            auxiliary_begins,
            source_live,
            narrow_shape,
            distinct,
        });
    }

    fn auxiliary_retirement_shape(
        &self,
        source: &crate::ownership_checking::UnitLoanFact,
        auxiliaries: &[crate::ownership_checking::UnitLoanFact],
        state: &ValueState,
    ) -> bool {
        use crate::parser::Expression;
        use crate::type_checking::{
            BorrowReturnOrigin, BuiltinType, ExpressionCategory, ParameterMode, UnitTypeKind,
        };
        if !self.loop_boundaries.is_empty()
            || self.owner_loop.is_some()
            || self.owner_choice.is_some()
            || self.owner_branch_depth != 0
            || self.scope_depth != 1
            || state.lambda.is_some()
            || state.this.is_some()
            || !state.iterations.is_empty()
        {
            return false;
        }
        let ast = self.checker.parsed.ast().expressions();
        let Ok(node) = ast.get(source.call().expression()) else {
            return false;
        };
        let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            return false;
        };
        let [first_argument, second_argument] = arguments.as_slice() else {
            return false;
        };
        let Some(call) = self.checker.typed.call(source.call()) else {
            return false;
        };
        let source_index = match call.borrow_return().map(|r| r.origin()) {
            Some(BorrowReturnOrigin::Parameter(index @ (0 | 1))) => index,
            _ => return false,
        };
        let (source_argument, auxiliary_argument) = if source_index == 0 {
            (first_argument, second_argument)
        } else {
            (second_argument, first_argument)
        };
        if !ast
            .get(*callee)
            .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            || arguments.iter().any(|argument| {
                argument.mode_marker.is_some()
                    || !ast
                        .get(argument.value)
                        .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            })
            || self.checker.unit_expression(source_argument.value) != source.argument()
        {
            return false;
        }
        let [auxiliary] = auxiliaries else {
            return false;
        };
        if self.checker.unit_expression(auxiliary_argument.value) != auxiliary.argument() {
            return false;
        }
        let UnitCallTarget::Declaration(declaration) = call.target() else {
            return false;
        };
        let Some(signature) = self
            .checker
            .typed
            .signatures()
            .declaration(declaration)
            .and_then(|d| d.callable())
        else {
            return false;
        };
        let [first, second] = call.arguments() else {
            return false;
        };
        let [p0, p1] = signature.parameters() else {
            return false;
        };
        let UnitLoanTarget::Place(root) = source.target() else {
            return false;
        };
        if call.receiver().is_some()
            || call.aborts()
            || !call.instance().type_arguments().is_empty()
            || signature.receiver().is_some()
            || !signature.type_parameters().is_empty()
            || call
                .borrow_return()
                .is_none_or(|r| r.origin() != BorrowReturnOrigin::Parameter(source_index))
            || signature.result_source() != call.result_source()
            || signature.return_type() != call.return_type()
            || self.checker.typed.types().get(call.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || !root.is_root()
            || self.checker.typed.symbol_type(root.root()) != Some(call.return_type())
            || source.target() != auxiliary.target()
            || source.argument() == auxiliary.argument()
        {
            return false;
        }
        let Some(contracts) = self.checker.contracts_by_call.get(&source.call()) else {
            return false;
        };
        if contracts.len() != 2 {
            return false;
        }
        let ordered = if source_index == 0 {
            [(source, first, p0), (auxiliary, second, p1)]
        } else {
            [(auxiliary, first, p0), (source, second, p1)]
        };
        ordered
            .into_iter()
            .enumerate()
            .all(|(index, (loan, typed, parameter))| {
                let matching = contracts
                    .iter()
                    .filter(|c| c.argument() == loan.argument())
                    .collect::<Vec<_>>();
                let [contract] = matching.as_slice() else {
                    return false;
                };
                let Ok(argument) = self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                else {
                    return false;
                };
                loan.kind() == LoanKind::Shared
                    && loan.call() == source.call()
                    && typed.argument_index() == index
                    && typed.parameter_index() == index
                    && typed.mode() == ParameterMode::Borrow
                    && !typed.crosses_thread()
                    && typed.category() == ExpressionCategory::Place
                    && parameter.mode() == typed.mode()
                    && parameter.ty() == call.return_type()
                    && typed.parameter_type() == call.return_type()
                    && self.checker.typed.expression_type(loan.argument())
                        == Some(call.return_type())
                    && self.checker.typed.expression_category(loan.argument())
                        == Some(ExpressionCategory::Place)
                    && contract.parameter_index() == index
                    && contract.parameter_type() == call.return_type()
                    && contract.kind()
                        == crate::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
                    && !contract.crosses_thread()
                    && contract.category() == ExpressionCategory::Place
                    && contract.call() == source.call()
                    && contract.loan_begin_span() == loan.begin_span()
                    && contract.argument_span() == argument.span()
                    && contract.argument_span() == loan.begin_span()
                    && contract.call_span() == loan.end_span()
                    && contract.parameter_span() == loan.parameter_span()
                    && contract.parameter_span() == Some(parameter.span())
            })
    }
    fn observes_temporary_borrow_auxiliary(
        &self,
        loan: &crate::ownership_checking::UnitLoanFact,
        state: &ValueState,
    ) -> bool {
        // This proof is only for auxiliary retirement. It must not grant source
        // transport to a temporary or use a backing/parent owner for its projection.
        loan.kind() == LoanKind::Shared
            && loan.call().source_unit() == self.checker.source_unit
            && loan.argument().source_unit() == self.checker.source_unit
            && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == loan.argument())
            && state.pending_temporaries.iter().any(|pending| {
                pending.holds_borrowed_temporary(
                    loan.call().expression(),
                    loan.argument().expression(),
                )
            })
    }

    pub(super) fn observes_borrow_auxiliary(
        &self,
        loan: &crate::ownership_checking::UnitLoanFact,
        state: &ValueState,
    ) -> bool {
        loan.kind() == LoanKind::Shared && self.observes_borrow_root(loan, state)
    }

    // Keep Exclusive auxiliary retirement separate from Shared source transport.
    fn observes_borrow_root(
        &self,
        loan: &crate::ownership_checking::UnitLoanFact,
        state: &ValueState,
    ) -> bool {
        let UnitLoanTarget::Place(place) = loan.target() else {
            return false;
        };
        let root = place.root();
        let provenance = self.loan_provenance();
        place.is_root()
            && !provenance.iteration_providers.contains(&root)
            && provenance.formed_owned.contains(&root)
            && state.position(root).is_some()
            && !state.borrow_bindings.contains(&root)
            && self
                .checker
                .borrow_result_call(loan.argument().expression())
                .is_none()
            && self
                .checker
                .place(loan.argument().expression())
                .is_ok_and(|actual| actual.as_ref() == Some(place))
    }

    // Observe the real owner transition even when the legacy whole-callable owner
    // observer has rejected borrow syntax. This sidecar grants no legacy authority.
    pub(super) fn observe_distinct_auxiliary_owner_event(
        &mut self,
        event: &crate::ownership_checking::UnitOwnerTransportEvent,
    ) {
        for pending in &mut self.pending_borrow_handoffs {
            if let Some(witness) = &mut pending.distinct {
                witness.owner_events.push(event.clone());
            }
        }
    }

    pub(super) fn commit_borrow_call_handoff(
        &mut self,
        binding: crate::name_resolution::UnitSymbolId,
        continues: bool,
        state: &mut ValueState,
    ) {
        let mut remaining = Vec::new();
        for pending in std::mem::take(&mut self.pending_borrow_handoffs) {
            if pending.fact.binding() == binding {
                if continues {
                    let auxiliary_source = pending.fact.source_loan().clone();
                    let auxiliary_begin = match pending.auxiliary_begins.as_slice() {
                        [Some(begin)] if pending.source_live && pending.narrow_shape => {
                            Some(begin.clone())
                        }
                        _ => None,
                    };
                    let source_begin = pending.source_begin.clone();
                    let confirmation = pending.distinct.as_ref().and_then(|witness| {
                        self.confirm_distinct_auxiliary(
                            witness,
                            state,
                            pending.fact.source_loan().call(),
                        )
                    });
                    let handle = self.iteration_actions.commit_borrow_handoff(pending.fact);
                    if let Some(owner_drop) = confirmation {
                        self.iteration_actions
                            .register_distinct_auxiliary_retirement(
                                handle.clone(),
                                pending.source_begin.clone(),
                                pending.auxiliary_begins.clone(),
                                pending.source_live,
                                pending.distinct,
                                owner_drop,
                            );
                    }
                    self.iteration_actions.register_auxiliary_retirement(
                        handle.clone(),
                        pending.source_begin,
                        pending.auxiliary_begins,
                        pending.source_live && pending.narrow_shape,
                    );
                    for pending in &mut state.borrow_binding_transports {
                        pending.commit_handoff(binding, handle.clone());
                        pending.activate_auxiliary_observation(
                            binding,
                            &auxiliary_source,
                            source_begin.as_ref(),
                            auxiliary_begin.as_ref(),
                            &handle,
                        );
                    }
                }
            } else {
                remaining.push(pending);
            }
        }
        self.pending_borrow_handoffs = remaining;
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn borrow_call_prefix_ends(
        &self,
    ) -> Vec<crate::ownership_checking::UnitBorrowCallPrefixEndFact> {
        use super::PlannerDropPoint;
        use crate::ownership_checking::UnitBorrowCallPrefixEndFact;
        let mut preceding_drops = std::collections::BTreeMap::new();
        let mut preceding_call_loan_ends = std::collections::BTreeMap::<_, usize>::new();
        let mut ends = Vec::new();
        for (action_index, (point, action)) in self.iteration_actions.iter().enumerate() {
            let PlannerDropPoint::ControlTransfer(transfer) = point else {
                continue;
            };
            if !self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(*transfer)
                .is_ok_and(|node| {
                    matches!(node.payload(), crate::parser::Expression::Return { .. })
                })
            {
                continue;
            }
            let count = preceding_drops.entry(transfer.index()).or_insert(0);
            let loan = match action {
                UnitIterationCleanupAction::Drop(_) => {
                    *count += 1;
                    continue;
                }
                UnitIterationCleanupAction::EndCallLoan(loan) => loan,
                _ => continue,
            };
            // Count every actual call-loan end before applying publication gates.
            let loan_count = preceding_call_loan_ends
                .entry(transfer.index())
                .or_default();
            let preceding = *loan_count;
            *loan_count += 1;
            let Some(call) = self.checker.typed.call(loan.call()) else {
                continue;
            };
            let UnitLoanTarget::Place(place) = loan.target() else {
                continue;
            };
            if loan.kind() != LoanKind::Shared
                || !place.is_root()
                || !self.loan_provenance().formed_owned.contains(&place.root())
                || call.receiver().is_some()
                || !matches!(call.target(), UnitCallTarget::Declaration(_))
                || !matches!(call.result_source(), CallableResultSource::Borrow(_))
            {
                continue;
            }
            if !self.borrow_source_unwind_actions.contains(&action_index) {
                continue;
            }
            ends.push(UnitBorrowCallPrefixEndFact {
                loan: loan.clone(),
                point: point.into_unit(self.checker.source_unit),
                preceding_drops: *count,
                preceding_call_loan_ends: preceding,
            });
        }
        ends
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn borrow_initializer_candidate(
        &self,
        initializer: ExpressionId,
    ) -> Option<(
        crate::type_checking::UnitExpressionId,
        crate::type_checking::UnitExpressionId,
    )> {
        let initializer = self.checker.borrow_result_call(initializer)?;
        let call_id = self.checker.unit_expression(initializer);
        let call = self.checker.typed.call(call_id)?;
        if call.receiver().is_some()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || !matches!(call.result_source(), CallableResultSource::Borrow(_))
        {
            return None;
        }
        let source = self.checker.borrowed_call_source(initializer)?;
        Some((call_id, self.checker.unit_expression(source)))
    }

    pub(super) fn is_observed_borrow_source_unwind(
        &self,
        point: super::PlannerDropPoint,
        action: &UnitIterationCleanupAction,
        state: &ValueState,
    ) -> bool {
        let super::PlannerDropPoint::ControlTransfer(transfer) = point else {
            return false;
        };
        if !self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(transfer)
            .is_ok_and(|node| matches!(node.payload(), crate::parser::Expression::Return { .. }))
        {
            return false;
        }
        let UnitIterationCleanupAction::EndCallLoan(loan) = action else {
            return false;
        };
        if !state
            .borrow_initializer_sources
            .contains(&(loan.call(), loan.argument()))
            || loan.kind() != LoanKind::Shared
        {
            return false;
        }
        let UnitLoanTarget::Place(place) = loan.target() else {
            return false;
        };
        // Typed argument mapping selects the candidate only. The actual evaluated
        // token, original operand place and still-live owner prove this observation.
        place.is_root()
            && self.loan_provenance().formed_owned.contains(&place.root())
            && state.position(place.root()).is_some()
            && !state.borrow_bindings.contains(&place.root())
            && self
                .checker
                .place(loan.argument().expression())
                .is_ok_and(|actual| actual.as_ref() == Some(place))
    }
}
