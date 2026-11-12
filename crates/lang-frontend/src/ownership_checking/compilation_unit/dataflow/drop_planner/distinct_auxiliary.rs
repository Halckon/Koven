//! Independent two-root auxiliary witness; never grants the legacy same-root shape.
use super::{DropPlanner, ValueState};
use crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot;
use crate::ownership_checking::{LoanKind, UnitLoanFact, UnitLoanTarget, UnitOwnerCallable};
use crate::ownership_checking::{UnitDropPoint, UnitOwnerTransportEvent};
use crate::type_checking::{UnitCallTarget, UnitExpressionId};

#[derive(Clone, PartialEq, Eq)]
pub(in super::super) struct Witness {
    pub(in super::super) source_owner: OwnerSnapshot,
    pub(in super::super) auxiliary_owner: OwnerSnapshot,
    pub(in super::super) source_parameter: usize,
    pub(in super::super) owner_events: Vec<UnitOwnerTransportEvent>,
    pub(in super::super) call: UnitExpressionId,
}

fn snapshot_live(expected: &OwnerSnapshot, state: &ValueState) -> bool {
    let mut owners = state
        .values
        .iter()
        .filter(|value| value.symbol == expected.symbol);
    let Some(actual) = owners.next() else {
        return false;
    };
    owners.next().is_none()
        && expected.owner.is_some()
        && actual.owner == expected.owner
        && actual.origin == expected.origin
        && actual.declaration == expected.declaration
        && actual.scope_depth == expected.scope_depth
}
impl Witness {
    pub(in super::super) fn still_live(&self, state: &ValueState) -> bool {
        snapshot_live(&self.source_owner, state) && snapshot_live(&self.auxiliary_owner, state)
    }
}

impl DropPlanner<'_, '_> {
    pub(in super::super) fn distinct_auxiliary_witness(
        &self,
        source: &crate::ownership_checking::UnitLoanFact,
        auxiliaries: &[crate::ownership_checking::UnitLoanFact],
        state: &ValueState,
    ) -> Option<Witness> {
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
            return None;
        }
        let ast = self.checker.parsed.ast().expressions();
        let Ok(node) = ast.get(source.call().expression()) else {
            return None;
        };
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = node.payload()
        else {
            return None;
        };
        let [first_argument, second_argument] = arguments.as_slice() else {
            return None;
        };
        let call = self.checker.typed.call(source.call())?;
        let source_index = match call.borrow_return().map(|r| r.origin()) {
            Some(BorrowReturnOrigin::Parameter(index @ (0 | 1))) => index,
            _ => return None,
        };
        let (source_argument, auxiliary_argument) = if source_index == 0 {
            (first_argument, second_argument)
        } else {
            (second_argument, first_argument)
        };
        if !type_arguments.is_empty()
            || !ast
                .get(*callee)
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            || arguments.iter().any(|argument| {
                argument.named_prefix.is_some()
                    || argument.mode_marker.is_some()
                    || !ast
                        .get(argument.value)
                        .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            })
            || self.checker.unit_expression(source_argument.value) != source.argument()
        {
            return None;
        }
        let [auxiliary] = auxiliaries else {
            return None;
        };
        if self.checker.unit_expression(auxiliary_argument.value) != auxiliary.argument() {
            return None;
        }
        let UnitCallTarget::Declaration(declaration) = call.target() else {
            return None;
        };
        let signature = self
            .checker
            .typed
            .signatures()
            .declaration(declaration)
            .and_then(|d| d.callable())?;
        let [first, second] = call.arguments() else {
            return None;
        };
        let [p0, p1] = signature.parameters() else {
            return None;
        };
        let UnitLoanTarget::Place(root) = source.target() else {
            return None;
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
            || source.target() == auxiliary.target()
            || source.argument() == auxiliary.argument()
        {
            return None;
        }
        let contracts = self.checker.contracts_by_call.get(&source.call())?;
        if contracts.len() != 2 {
            return None;
        }
        let ordered = if source_index == 0 {
            [(source, first, p0), (auxiliary, second, p1)]
        } else {
            [(auxiliary, first, p0), (source, second, p1)]
        };
        let mapping_valid =
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
                });
        if !mapping_valid {
            return None;
        }
        let source_owner = self.distinct_auxiliary_owner(source, state)?;
        let auxiliary_owner = self.distinct_auxiliary_owner(auxiliary, state)?;
        let source_id = source_owner.owner.as_ref()?;
        let auxiliary_id = auxiliary_owner.owner.as_ref()?;
        if source_owner.symbol == auxiliary_owner.symbol
            || source_id == auxiliary_id
            || source_id.callable != auxiliary_id.callable
            || !std::sync::Arc::ptr_eq(&source_id.identity, &auxiliary_id.identity)
        {
            return None;
        }
        Some(Witness {
            source_owner,
            auxiliary_owner,
            source_parameter: source_index,
            owner_events: Vec::new(),
            call: source.call(),
        })
    }

    /// Confirm only this call's handoff window, not a full drop-region capability.
    pub(in super::super) fn confirm_distinct_auxiliary(
        &self,
        witness: &Witness,
        state: &ValueState,
        call: UnitExpressionId,
    ) -> Option<Option<UnitOwnerTransportEvent>> {
        if call != witness.call || !snapshot_live(&witness.source_owner, state) {
            return None;
        }
        let source = witness.source_owner.owner.as_ref()?;
        let auxiliary = witness.auxiliary_owner.owner.as_ref()?;
        let observation = self.owner_observation.as_ref()?;
        if source == auxiliary
            || witness.source_owner.symbol == witness.auxiliary_owner.symbol
            || source.callable != observation.callable
            || auxiliary.callable != observation.callable
            || !std::sync::Arc::ptr_eq(&source.identity, &observation.identity)
            || !std::sync::Arc::ptr_eq(&auxiliary.identity, &observation.identity)
        {
            return None;
        }
        // These are captured by the actual owner-event hook for this pending
        // handoff, independently of the legacy directory's publication status.
        let events = &witness.owner_events;
        let mut dropped = None;
        for event in events {
            // This narrow window admits cleanup only. Moves, replacement, rebinding,
            // branch subtraces, or other transport cannot silently restore a snapshot.
            let UnitOwnerTransportEvent::Drop {
                point,
                symbol,
                owner,
            } = event
            else {
                return None;
            };
            if *symbol == witness.source_owner.symbol || owner == source {
                return None;
            }
            if *symbol == witness.auxiliary_owner.symbol || owner == auxiliary {
                if *symbol != witness.auxiliary_owner.symbol
                    || owner != auxiliary
                    || *point != UnitDropPoint::CallReturn(call)
                    || dropped.is_some()
                {
                    return None;
                }
                dropped = Some(event.clone());
            }
        }
        if snapshot_live(&witness.auxiliary_owner, state) {
            return dropped.is_none().then_some(None);
        }
        if state.values.iter().any(|value| {
            value.symbol == witness.auxiliary_owner.symbol
                || value.owner.as_ref() == Some(auxiliary)
        }) {
            return None;
        }
        dropped.map(Some)
    }

    fn distinct_auxiliary_owner(
        &self,
        loan: &UnitLoanFact,
        state: &ValueState,
    ) -> Option<OwnerSnapshot> {
        let UnitLoanTarget::Place(place) = loan.target() else {
            return None;
        };
        if loan.call().source_unit() != self.checker.source_unit
            || loan.argument().source_unit() != self.checker.source_unit
            || !place.is_root()
            || self.checker.typed.symbol_type(place.root())
                != Some(self.checker.typed.call(loan.call())?.return_type())
            || !self.loan_provenance().formed_owned.contains(&place.root())
            || self
                .loan_provenance()
                .iteration_providers
                .contains(&place.root())
            || state.borrow_bindings.contains(&place.root())
            || self
                .checker
                .borrow_result_call(loan.argument().expression())
                .is_some()
            || self
                .checker
                .place(loan.argument().expression())
                .ok()?
                .as_ref()
                != Some(place)
        {
            return None;
        }
        let mut owners = state
            .values
            .iter()
            .filter(|value| value.symbol == place.root());
        let owner = owners.next()?;
        if owners.next().is_some() || owner.scope_depth != self.scope_depth {
            return None;
        }
        // Actual live state is the identity anchor. The legacy directory may
        // already be Unsupported and have cleared definitions; do not promote it.
        let id = owner.owner.as_ref()?;
        let observation = self.owner_observation.as_ref()?;
        if !matches!(id.callable, UnitOwnerCallable::Function(item) if item.source_unit() == self.checker.source_unit)
            || id.callable != observation.callable
            || !std::sync::Arc::ptr_eq(&id.identity, &observation.identity)
        {
            return None;
        }
        Some(OwnerSnapshot {
            owner: Some(id.clone()),
            symbol: owner.symbol,
            origin: owner.origin,
            declaration: owner.declaration,
            scope_depth: owner.scope_depth,
        })
    }
}
