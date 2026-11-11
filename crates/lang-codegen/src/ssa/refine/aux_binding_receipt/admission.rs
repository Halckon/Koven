//! Same-catalog selection, exact causal references and unchanged typed call contracts.
use super::super::{RefineBody, RefineInputs};
use super::*;
use lang_frontend::{
    ownership_checking::{
        BorrowBindingStorage, LoanKind as FrontendLoanKind, UnitBorrowAuxiliaryBindingRegion,
        UnitCallArgumentOwnershipKind, UnitIterationCleanupAction, UnitLoanAction,
        UnitLoanActionCoverage, UnitLoanFact, UnitLoanTarget,
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
            for region in observation.auxiliary_binding_regions().iter().filter(|r| {
                r.coverage() == UnitLoanActionCoverage::Complete && r.callable() == callable
            }) {
                if observation.source() != inputs.source_unit
                    || !observation.owns_auxiliary_binding_region(region)
                    || !result.entries.is_empty()
                {
                    return Err(fail());
                }
                validate_scope(inputs, region)?;
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = catalog_actions(observation, region, &ids, inputs.span)?;
                let [
                    UnitLoanAction::Begin(first),
                    UnitLoanAction::Begin(second),
                    UnitLoanAction::BorrowHandoffCommitted(handoff),
                    UnitLoanAction::Begin(consumer),
                    UnitLoanAction::BorrowBindingUseReturned(returned),
                    UnitLoanAction::BorrowBindingEnd(end),
                    UnitLoanAction::BorrowBindingSourceRetired(retired),
                    UnitLoanAction::Cleanup {
                        point,
                        action: UnitIterationCleanupAction::Drop(drop),
                    },
                ] = actions.as_slice()
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
                let UnitLoanTarget::Place(root) = source.target() else {
                    return Err(fail());
                };
                let transaction = retired.transaction();
                if !root.is_root()
                    || root.root() != region.root()
                    || source.call() != region.source_call()
                    || consumer.call() != region.consumer_call()
                    || consumer.target() != source.target()
                    || source.kind() != FrontendLoanKind::Shared
                    || consumer.kind() != FrontendLoanKind::Shared
                    || handoff.binding() != region.binding()
                    || handoff.source_loan() != source
                    || handoff.auxiliary_loan_ends() != std::slice::from_ref(auxiliary)
                    || auxiliary.call() != source.call()
                    || auxiliary.argument() == source.argument()
                    || auxiliary.target() != source.target()
                    || auxiliary.kind() != FrontendLoanKind::Shared
                    || observation
                        .auxiliary_retirements()
                        .iter()
                        .filter(|r| {
                            r.source_begin() == ids[source_index]
                                && r.auxiliary_begin() == ids[auxiliary_index]
                                && r.handoff() == ids[2]
                        })
                        .count()
                        != 1
                    || returned.binding() != region.binding()
                    || returned.loan() != consumer
                    || returned.source_begin().as_ref() != ids.get(source_index)
                    || returned.handoff().as_ref() != ids.get(2)
                    || returned.begin().as_ref() != ids.get(3)
                    || end.binding() != region.binding()
                    || end.point() != *point
                    || transaction.binding() != region.binding()
                    || transaction.source_loan() != source
                    || transaction.point() != *point
                    || retired.source_begin().as_ref() != ids.get(source_index)
                    || retired.handoff().as_ref() != ids.get(2)
                    || retired.use_return().as_ref() != ids.get(4)
                    || retired.binding_end().as_ref() != ids.get(5)
                    || drop.point() != *point
                    || drop.target() != UnitDropTarget::Named(region.root())
                {
                    return Err(fail());
                }
                let bindings = inputs
                    .owned
                    .borrow_results()
                    .bindings()
                    .iter()
                    .filter(|b| b.binding() == region.binding())
                    .collect::<Vec<_>>();
                let [binding] = bindings.as_slice() else {
                    return Err(fail());
                };
                if binding.initializer() != source.call()
                    || binding.origin() != source.target()
                    || binding.parent().is_some()
                    || binding.storage() != BorrowBindingStorage::BorrowedStorage
                    || !binding.source_loan().is_some_and(|s| {
                        s.call() == source.call() && s.argument() == source.argument()
                    })
                    || inputs
                        .owned
                        .borrow_results()
                        .ends()
                        .iter()
                        .filter(|e| e.binding() == region.binding())
                        .collect::<Vec<_>>()
                        != [end]
                    || inputs
                        .owned
                        .borrow_call_handoffs()
                        .iter()
                        .filter(|h| h.binding() == region.binding())
                        .collect::<Vec<_>>()
                        != [handoff]
                    || inputs
                        .owned
                        .borrow_binding_transports()
                        .iter()
                        .filter(|t| t.binding() == region.binding())
                        .collect::<Vec<_>>()
                        != [transaction]
                    || inputs
                        .owned
                        .loans()
                        .iter()
                        .filter(|l| l.call() == source.call())
                        .collect::<Vec<_>>()
                        != [first, second]
                    || inputs
                        .owned
                        .loans()
                        .iter()
                        .filter(|l| l.call() == consumer.call())
                        .collect::<Vec<_>>()
                        != [consumer]
                {
                    return Err(fail());
                }
                let indices = inputs
                    .owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| **d == *drop)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let [drop_index] = indices.as_slice() else {
                    return Err(fail());
                };
                let source_callee = validate_call(
                    inputs,
                    source,
                    region.root(),
                    Some(source_index),
                    source_index,
                )?;
                if validate_call(
                    inputs,
                    auxiliary,
                    region.root(),
                    Some(source_index),
                    auxiliary_index,
                )? != source_callee
                {
                    return Err(fail());
                }
                let consumer_callee = validate_call(inputs, consumer, region.binding(), None, 0)?;
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
                if caller == source_callee || caller == consumer_callee {
                    return Err(fail());
                }
                result.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    binding: region.binding(),
                    root: region.root(),
                    source_call: source.call(),
                    source_argument: source.argument(),
                    source_index,
                    auxiliary_argument: auxiliary.argument(),
                    consumer_call: consumer.call(),
                    consumer_argument: consumer.argument(),
                    entry_statement: region.entry_statement(),
                    completion_statement: region.completion_statement(),
                    end_point: *point,
                    source_callee,
                    consumer_callee,
                    drop_index: *drop_index,
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
                    consumer_authorized: false,
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
    region: &UnitBorrowAuxiliaryBindingRegion,
) -> Result<(), LoweringError> {
    let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
    let local = inputs.source_unit;
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
        .get(region.callable().item())
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
        || region.root() == region.binding()
        || region.root().source_unit() != local
        || region.binding().source_unit() != local
        || region.source_call().source_unit() != local
        || region.consumer_call().source_unit() != local
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
    let [_, entry, consumer] = elements.as_slice() else {
        return Err(fail());
    };
    let Statement::Expression { expression } = inputs
        .parsed
        .ast()
        .statements()
        .get(*consumer)
        .map_err(|_| fail())?
        .payload()
    else {
        return Err(fail());
    };
    if region.entry_statement() != UnitStatementId::new(local, *entry)
        || region.consumer_call() != UnitExpressionId::new(local, *expression)
    {
        return Err(fail());
    }
    // Exact statement identity establishes retirement; no Span pairing or latest-event search.
    let ends = inputs
        .owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|e| e.binding() == region.binding())
        .collect::<Vec<_>>();
    if !matches!(ends.as_slice(), [e] if e.point() == UnitDropPoint::AfterStatement(UnitStatementId::new(local, *consumer)))
    {
        return Err(fail());
    }
    Ok(())
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

/// All selected IDs must belong to the same independently owned region catalog.
pub(super) fn catalog_actions<'a>(
    observation: &'a lang_frontend::ownership_checking::UnitLoanActionObservation,
    region: &UnitBorrowAuxiliaryBindingRegion,
    ids: &[UnitLoanActionId],
    span: Span,
) -> Result<Vec<&'a UnitLoanAction>, LoweringError> {
    if !observation.owns_auxiliary_binding_region(region)
        || observation.source() != region.callable().source_unit()
        || region.action_ids().collect::<Vec<_>>() != ids
    {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    ids.iter()
        .map(|id| {
            observation
                .action(id)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        })
        .collect()
}
