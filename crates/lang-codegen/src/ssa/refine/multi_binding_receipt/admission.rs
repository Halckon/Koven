//! Independently bind a complete multi-use interval to typed source-plan call identities.
use super::super::{RefineBody, RefineInputs};
use super::*;
use lang_frontend::{
    ownership_checking::{
        BorrowBindingStorage, LoanKind as FrontendLoanKind, UnitBorrowMultiBindingRegion,
        UnitCallArgumentOwnershipKind, UnitIterationCleanupAction, UnitLoanAction,
        UnitLoanActionCoverage, UnitLoanFact, UnitLoanTarget, UnitMultiBindingSourceState,
    },
    parser::{Expression, Item, Statement},
    type_checking::{
        BorrowReturnOrigin, BuiltinType, CallableResultSource, ExpressionCategory, ParameterMode,
        UnitCallTarget, UnitCallableTarget, UnitTypeKind,
    },
};
impl Plans {
    pub(in crate::ssa::refine) fn new(inputs: &RefineInputs<'_>) -> Result<Self, LoweringError> {
        let mut result = Self::default();
        let Some(item) = inputs.function_item else {
            return Ok(result);
        };
        let callable = UnitItemId::new(inputs.source_unit, item);
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        for observation in inputs.owned.loan_action_observations() {
            for region in observation.multi_binding_regions().iter().filter(|r| {
                r.coverage() == UnitLoanActionCoverage::Complete
                    && r.retirement().use_set().callable() == callable
            }) {
                if observation.source() != inputs.source_unit
                    || !observation.owns_multi_binding_region(region)
                    || !result.entries.is_empty()
                {
                    return Err(fail());
                }
                let retirement = region.retirement();
                let set = retirement.use_set();
                let binding_id = set.binding();
                if !observation.owns_binding_use_set_retirement(retirement)
                    || !observation.owns_binding_use_set(set)
                    || observation
                        .binding_use_set_retirements()
                        .iter()
                        .filter(|r| *r == retirement)
                        .count()
                        != 1
                    || observation
                        .binding_use_sets()
                        .iter()
                        .filter(|s| *s == set)
                        .count()
                        != 1
                {
                    return Err(fail());
                }
                let count = region.auxiliary_uses().len();
                let bcount = set.uses().len();
                if !(1..=MAX_USES).contains(&count) || !(2..=MAX_USES).contains(&bcount) {
                    return Err(fail());
                }
                let order = validate_scope(inputs, region)?;
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = catalog_actions(observation, region, &ids, inputs.span)?;
                if actions.len() != 7 + bcount + 2 * count {
                    return Err(fail());
                }
                let [
                    UnitLoanAction::Begin(first),
                    UnitLoanAction::Begin(second),
                    UnitLoanAction::BorrowHandoffCommitted(handoff),
                ] = actions.get(..3).ok_or_else(fail)?
                else {
                    return Err(fail());
                };
                let source_index = if handoff.source_loan() == first {
                    0
                } else if handoff.source_loan() == second {
                    1
                } else {
                    return Err(fail());
                };
                let auxiliary_index = 1 - source_index;
                let (source, auxiliary) = if source_index == 0 {
                    (first, second)
                } else {
                    (second, first)
                };
                let UnitLoanTarget::Place(source_place) = source.target() else {
                    return Err(fail());
                };
                let UnitLoanTarget::Place(auxiliary_place) = auxiliary.target() else {
                    return Err(fail());
                };
                let root = source_place.root();
                let auxiliary_root = auxiliary_place.root();
                let source_owner = retirement.source_owner();
                let auxiliary_owner = region.auxiliary_uses()[0].owner();
                if !source_place.is_root()
                    || !auxiliary_place.is_root()
                    || root == auxiliary_root
                    || root == binding_id
                    || auxiliary_root == binding_id
                    || source_owner == auxiliary_owner
                    || source.kind() != FrontendLoanKind::Shared
                    || auxiliary.kind() != FrontendLoanKind::Shared
                    || source.call() != auxiliary.call()
                    || source.argument() == auxiliary.argument()
                    || handoff.binding() != binding_id
                    || handoff.auxiliary_loan_ends() != std::slice::from_ref(auxiliary)
                    || set.source_begin() != ids[source_index]
                    || set.handoff() != ids[2]
                    || observation
                        .distinct_auxiliary_retirements()
                        .iter()
                        .filter(|r| {
                            observation.owns_distinct_auxiliary_retirement(r)
                                && r.source_begin() == ids[source_index]
                                && r.auxiliary_begin() == ids[auxiliary_index]
                                && r.handoff() == ids[2]
                                && r.auxiliary_drop().is_none()
                                && r.source_parameter() == source_index
                                && r.source_owner() == source_owner
                                && r.auxiliary_owner() == auxiliary_owner
                                && r.point() == UnitDropPoint::CallReturn(source.call())
                        })
                        .count()
                        != 1
                {
                    return Err(fail());
                }
                let Some(UnitLoanAction::BorrowBindingEnd(end)) =
                    observation.action(&set.binding_end())
                else {
                    return Err(fail());
                };
                let Some(UnitLoanAction::Cleanup {
                    point,
                    action: UnitIterationCleanupAction::Drop(drop),
                }) = observation.action(&retirement.source_drop())
                else {
                    return Err(fail());
                };
                let Some(UnitLoanAction::Cleanup {
                    point: auxiliary_point,
                    action: UnitIterationCleanupAction::Drop(auxiliary_drop),
                }) = observation.action(&region.auxiliary_drop())
                else {
                    return Err(fail());
                };
                if end.binding() != binding_id
                    || end.point() != *point
                    || drop.point() != *point
                    || drop.target() != UnitDropTarget::Named(root)
                    || auxiliary_drop.point() != *auxiliary_point
                    || auxiliary_drop.target() != UnitDropTarget::Named(auxiliary_root)
                {
                    return Err(fail());
                }
                let bindings = inputs
                    .owned
                    .borrow_results()
                    .bindings()
                    .iter()
                    .filter(|b| b.binding() == binding_id)
                    .collect::<Vec<_>>();
                let [binding] = bindings.as_slice() else {
                    return Err(fail());
                };
                let transports = inputs
                    .owned
                    .borrow_binding_transports()
                    .iter()
                    .filter(|t| t.binding() == binding_id)
                    .collect::<Vec<_>>();
                let [transport] = transports.as_slice() else {
                    return Err(fail());
                };
                if binding.initializer() != source.call()
                    || binding.origin() != source.target()
                    || binding.parent().is_some()
                    || binding.storage() != BorrowBindingStorage::BorrowedStorage
                    || !binding.source_loan().is_some_and(|s| {
                        s.call() == source.call() && s.argument() == source.argument()
                    })
                    || transport.source_loan() != source
                    || transport.point() != *point
                    || inputs
                        .owned
                        .borrow_results()
                        .ends()
                        .iter()
                        .filter(|e| e.binding() == binding_id)
                        .collect::<Vec<_>>()
                        != [end]
                    || inputs
                        .owned
                        .borrow_call_handoffs()
                        .iter()
                        .filter(|h| h.binding() == binding_id)
                        .collect::<Vec<_>>()
                        != [handoff]
                    || inputs
                        .owned
                        .loans()
                        .iter()
                        .filter(|l| l.call() == source.call())
                        .collect::<Vec<_>>()
                        != [first, second]
                {
                    return Err(fail());
                }
                let drop_indices = inputs
                    .owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| *d == drop)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let [drop_index] = drop_indices.as_slice() else {
                    return Err(fail());
                };
                let aux_indices = inputs
                    .owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| *d == auxiliary_drop)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let [auxiliary_drop_index] = aux_indices.as_slice() else {
                    return Err(fail());
                };
                if drop_index == auxiliary_drop_index {
                    return Err(fail());
                }
                let source_callee =
                    validate_call(inputs, source, root, Some(source_index), source_index)?;
                if validate_call(
                    inputs,
                    auxiliary,
                    auxiliary_root,
                    Some(source_index),
                    auxiliary_index,
                )? != source_callee
                {
                    return Err(fail());
                }
                let mut uses = Vec::with_capacity(count);
                let mut consumers = Vec::with_capacity(bcount);
                let mut cursor = 3;
                let mut next_a = 0;
                let mut next_b = 0;
                for role in &order {
                    match *role {
                        UseRole::Auxiliary(index) => {
                            if index != next_a {
                                return Err(fail());
                            }
                            let actual = &region.auxiliary_uses()[index];
                            let loan = actual.loan();
                            if !matches!(actions.get(cursor), Some(UnitLoanAction::Begin(l)) if l == loan)
                                || actual.begin() != ids[cursor]
                                || actual.end() != ids[cursor + 1]
                                || actual.owner() != auxiliary_owner
                                || actual.returned_binding_uses() != next_b
                                || loan.target() != auxiliary.target()
                                || loan.kind() != FrontendLoanKind::Shared
                                || !matches!(actions.get(cursor+1),Some(UnitLoanAction::Cleanup{point,action:UnitIterationCleanupAction::EndCallLoan(l)}) if l==loan&&*point==UnitDropPoint::CallReturn(loan.call()))
                                || inputs
                                    .owned
                                    .loans()
                                    .iter()
                                    .filter(|l| l.call() == loan.call())
                                    .collect::<Vec<_>>()
                                    != [loan]
                            {
                                return Err(fail());
                            }
                            match actual.source_state() {
                                UnitMultiBindingSourceState::Active {
                                    owner,
                                    source_begin,
                                    handoff,
                                } if next_b < bcount
                                    && owner == source_owner
                                    && *source_begin == set.source_begin()
                                    && *handoff == set.handoff() => {}
                                UnitMultiBindingSourceState::Retired { binding_end, drop }
                                    if next_b == bcount
                                        && *binding_end == set.binding_end()
                                        && *drop == retirement.source_drop() => {}
                                _ => return Err(fail()),
                            }
                            uses.push(Use {
                                call: loan.call(),
                                argument: loan.argument(),
                                callee: validate_call(inputs, loan, auxiliary_root, None, 0)?,
                                place: None,
                                loan: None,
                                authorized: false,
                            });
                            cursor += 2;
                            next_a += 1;
                            if next_a == count {
                                if ids.get(cursor) != Some(&region.auxiliary_drop())
                                    || *auxiliary_point != UnitDropPoint::CallReturn(loan.call())
                                {
                                    return Err(fail());
                                }
                                cursor += 1;
                            }
                        }
                        UseRole::Binding(index) => {
                            if index != next_b {
                                return Err(fail());
                            }
                            let actual = &set.uses()[index];
                            let loan = actual.loan();
                            if !matches!(actions.get(cursor),Some(UnitLoanAction::Begin(l)) if l==loan)
                                || actual.begin() != ids[cursor]
                                || loan.target() != source.target()
                                || loan.kind() != FrontendLoanKind::Shared
                                || actual.return_point() != UnitDropPoint::CallReturn(loan.call())
                                || observation
                                    .actions()
                                    .get(actual.return_cursor().saturating_sub(1))
                                    .is_none_or(|a| !std::ptr::eq(a, actions[cursor]))
                                || inputs
                                    .owned
                                    .loans()
                                    .iter()
                                    .filter(|l| l.call() == loan.call())
                                    .collect::<Vec<_>>()
                                    != [loan]
                            {
                                return Err(fail());
                            }
                            let begin = cursor;
                            cursor += 1;
                            if index == 0 {
                                let Some(UnitLoanAction::BorrowBindingUseReturned(old)) =
                                    actions.get(cursor)
                                else {
                                    return Err(fail());
                                };
                                if old.binding() != binding_id
                                    || old.loan() != loan
                                    || old.begin() != Some(ids[begin].clone())
                                    || old.source_begin() != Some(set.source_begin())
                                    || old.handoff() != Some(set.handoff())
                                {
                                    return Err(fail());
                                }
                                cursor += 1;
                            }
                            consumers.push(Consumer {
                                call: loan.call(),
                                argument: loan.argument(),
                                callee: validate_call(inputs, loan, binding_id, None, 0)?,
                                authorized: false,
                            });
                            next_b += 1;
                            if next_b == bcount {
                                if ids.get(cursor) != Some(&set.binding_end())
                                    || ids.get(cursor + 1) != Some(&retirement.source_drop())
                                {
                                    return Err(fail());
                                }
                                cursor += 2;
                            }
                        }
                    }
                }
                if cursor != actions.len() || next_a != count || next_b != bcount {
                    return Err(fail());
                }
                let caller_key = inputs
                    .source_plan
                    .source(inputs.source_token)
                    .ok_or_else(fail)?;
                let UnitCallableTarget::Declaration(id) = caller_key.target() else {
                    return Err(fail());
                };
                let declaration = inputs
                    .names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.id() == id)
                    .ok_or_else(fail)?;
                if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable
                    || caller_key.is_specialized()
                    || caller_key.deinit_owner().is_some()
                {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                if caller == source_callee
                    || consumers.iter().any(|c| c.callee == caller)
                    || uses.iter().any(|u| u.callee == caller)
                {
                    return Err(fail());
                }
                result.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    binding: binding_id,
                    root,
                    auxiliary_root,
                    uses,
                    consumers,
                    order,
                    source_call: source.call(),
                    source_argument: source.argument(),
                    source_index,
                    auxiliary_argument: auxiliary.argument(),
                    entry_statement: region.entry_statement(),
                    completion_statement: region.completion_statement(),
                    end_point: *point,
                    source_callee,
                    drop_index: *drop_index,
                    auxiliary_drop_index: *auxiliary_drop_index,
                    auxiliary_owner: None,
                    auxiliary_drop_authorized: false,
                    owner: None,
                    place: None,
                    source: None,
                    auxiliary_place: None,
                    auxiliary: None,
                    result: None,
                    stage: 0,
                    activated: false,
                    source_authorized: false,
                    auxiliary_authorized: false,
                    committed: false,
                    retired: false,
                    drop_authorized: false,
                    completed: false,
                    block: None,
                });
            }
        }
        Ok(result)
    }
}
fn validate_scope(
    inputs: &RefineInputs<'_>,
    region: &UnitBorrowMultiBindingRegion,
) -> Result<Vec<UseRole>, LoweringError> {
    let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
    let local = inputs.source_unit;
    let set = region.retirement().use_set();
    let RefineBody::Block(body) = inputs.body else {
        return Err(fail());
    };
    let Item::Function {
        parameters,
        type_parameters,
        extension_receiver: None,
        ..
    } = inputs
        .parsed
        .ast()
        .items()
        .get(set.callable().item())
        .map_err(|_| fail())?
        .payload()
    else {
        return Err(fail());
    };
    if !parameters.is_empty()
        || !type_parameters.is_empty()
        || !inputs.parameters.is_empty()
        || !inputs.substitutions.is_empty()
        || inputs.receiver.is_some()
        || inputs.static_self.is_some()
        || inputs.borrow_return.is_some()
        || inputs.carrier_return.is_some()
        || !matches!(
            inputs.typed.types().get(inputs.return_type),
            Some(UnitTypeKind::Builtin(BuiltinType::Unit))
        )
        || set.binding().source_unit() != local
        || set.callable().source_unit() != local
        || region.entry_statement().source_unit() != local
        || region.completion_statement() != UnitStatementId::new(local, body)
    {
        return Err(fail());
    }
    let Statement::Block { elements } = inputs
        .parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|_| fail())?
        .payload()
    else {
        return Err(fail());
    };
    let count = region.auxiliary_uses().len();
    let bcount = set.uses().len();
    if elements.len() != 3 + count + bcount
        || region.entry_statement() != UnitStatementId::new(local, elements[2])
    {
        return Err(fail());
    }
    let mut order = Vec::new();
    let mut ai = 0;
    let mut bi = 0;
    let mut last_b = None;
    for &statement in &elements[3..] {
        let Statement::Expression { expression } = inputs
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| fail())?
            .payload()
        else {
            return Err(fail());
        };
        let call = UnitExpressionId::new(local, *expression);
        let a = region
            .auxiliary_uses()
            .get(ai)
            .is_some_and(|u| u.loan().call() == call);
        let b = set.uses().get(bi).is_some_and(|u| u.loan().call() == call);
        match (a, b) {
            (true, false) => {
                order.push(UseRole::Auxiliary(ai));
                ai += 1;
            }
            (false, true) => {
                order.push(UseRole::Binding(bi));
                bi += 1;
                last_b = Some(UnitStatementId::new(local, statement));
            }
            _ => return Err(fail()),
        }
    }
    if ai != count || bi != bcount {
        return Err(fail());
    }
    let ends = inputs
        .owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|e| e.binding() == set.binding())
        .collect::<Vec<_>>();
    if !matches!(ends.as_slice(),[end] if Some(end.point())==last_b.map(UnitDropPoint::AfterStatement))
    {
        return Err(fail());
    }
    Ok(order)
}
/// Resolve every original ID through the exact independently owned catalog.
pub(super) fn catalog_actions<'a>(
    observation: &'a lang_frontend::ownership_checking::UnitLoanActionObservation,
    region: &UnitBorrowMultiBindingRegion,
    ids: &[UnitLoanActionId],
    span: Span,
) -> Result<Vec<&'a UnitLoanAction>, LoweringError> {
    if !observation.owns_multi_binding_region(region)
        || observation.source() != region.retirement().use_set().callable().source_unit()
        || region.action_ids().collect::<Vec<_>>() != ids
    {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    let actions = ids
        .iter()
        .map(|id| {
            observation
                .action(id)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let start = observation
        .actions()
        .iter()
        .position(|a| actions.first().is_some_and(|first| std::ptr::eq(a, *first)))
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
    if actions.iter().enumerate().any(|(i, a)| {
        observation
            .actions()
            .get(start + i)
            .is_none_or(|actual| !std::ptr::eq(*a, actual))
    }) {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    Ok(actions)
}
fn validate_call(
    inputs: &RefineInputs<'_>,
    loan: &UnitLoanFact,
    binding: UnitSymbolId,
    source_index: Option<usize>,
    index: usize,
) -> Result<FunctionId, LoweringError> {
    let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
    if loan.call().source_unit() != inputs.source_unit
        || loan.argument().source_unit() != inputs.source_unit
    {
        return Err(fail());
    }
    let descriptor = inputs.typed.call(loan.call()).ok_or_else(fail)?;
    let UnitCallTarget::Declaration(declaration) = descriptor.target() else {
        return Err(fail());
    };
    let count = if source_index.is_some() { 2 } else { 1 };
    if descriptor.arguments().len() != count {
        return Err(fail());
    }
    let argument = descriptor.arguments().get(index).ok_or_else(fail)?;
    let contracts = inputs
        .owned
        .call_argument_contracts()
        .iter()
        .filter(|c| c.call() == loan.call())
        .collect::<Vec<_>>();
    if contracts.len() != count {
        return Err(fail());
    }
    let selected = contracts
        .iter()
        .copied()
        .filter(|c| c.argument() == loan.argument())
        .collect::<Vec<_>>();
    let [contract] = selected.as_slice() else {
        return Err(fail());
    };
    let expressions = inputs.parsed.ast().expressions();
    let Expression::Call { arguments, .. } = expressions
        .get(loan.call().expression())
        .map_err(|_| fail())?
        .payload()
    else {
        return Err(fail());
    };
    if arguments.len() != count {
        return Err(fail());
    }
    let operand = arguments.get(index).ok_or_else(fail)?;
    let node = expressions.get(operand.value).map_err(|_| fail())?;
    if UnitExpressionId::new(inputs.source_unit, operand.value) != loan.argument()
        || !matches!(node.payload(), Expression::Name)
        || inputs
            .references
            .get(&(node.span().start(), node.span().end()))
            != Some(&binding)
        || descriptor.expression() != loan.call()
        || descriptor.receiver().is_some()
        || descriptor.aborts()
        || !descriptor.instance().type_arguments().is_empty()
        || argument.mode() != ParameterMode::Borrow
        || argument.argument_index() != index
        || argument.parameter_index() != index
        || argument.category() != ExpressionCategory::Place
        || argument.crosses_thread()
        || !matches!(
            inputs.typed.types().get(argument.parameter_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        )
        || inputs.typed.expression_type(loan.argument()) != Some(argument.parameter_type())
        || inputs.typed.symbol_type(binding) != Some(argument.parameter_type())
        || contract.argument() != loan.argument()
        || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
        || contract.parameter_index() != argument.parameter_index()
        || contract.parameter_type() != argument.parameter_type()
        || contract.category() != argument.category()
        || contract.crosses_thread()
        || contract.argument_span() != loan.begin_span()
        || contract.loan_begin_span() != loan.begin_span()
        || contract.call_span() != loan.end_span()
        || contract.parameter_span() != loan.parameter_span()
    {
        return Err(fail());
    }
    if let Some(source_index) = source_index {
        if source_index > 1
            || descriptor.borrow_return().map(|b| b.origin())
                != Some(BorrowReturnOrigin::Parameter(source_index))
            || descriptor.return_type() != argument.parameter_type()
        {
            return Err(fail());
        }
    } else if descriptor.result_source() != CallableResultSource::Owned
        || !matches!(
            inputs.typed.types().get(descriptor.return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Unit))
        )
    {
        return Err(fail());
    }
    let site = inputs
        .source_plan
        .call_site(inputs.source_token, loan.call())
        .ok_or_else(fail)?;
    let key = site.dispatch_key().ok_or_else(fail)?;
    if !site.delegation().is_empty()
        || key != site.key()
        || key.target() != UnitCallableTarget::Declaration(declaration)
        || key.type_arguments() != descriptor.instance().type_arguments()
        || key.is_specialized()
        || key.deinit_owner().is_some()
    {
        return Err(fail());
    }
    inputs.function_ids.get(key).copied().ok_or_else(fail)
}
