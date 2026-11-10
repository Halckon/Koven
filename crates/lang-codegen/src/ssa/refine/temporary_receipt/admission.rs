//! Same-catalog Complete1, narrowed by the existing caller shape rather than coverage alone.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage,
        UnitLoanTarget,
    },
    type_checking::{BuiltinType, UnitCallableTarget, UnitTypeKind},
};
impl Plans {
    pub(in crate::ssa::refine) fn new(
        inputs: &super::super::RefineInputs<'_>,
    ) -> Result<Self, LoweringError> {
        let mut result = Self::default();
        let Some(call) = super::super::subset::temporary_route::bounded_temporary_call(inputs)
        else {
            return Ok(result);
        };
        // Stage one migrates the caller only. Receipt admission still covers the
        // previously implemented source-order parameter mapping until stage two.
        if inputs.typed.call(call).is_none_or(|call| {
            call.arguments()
                .iter()
                .any(|a| a.argument_index() != a.parameter_index())
        }) {
            return Ok(result);
        }
        let item = inputs.function_item.ok_or_else(|| invalid(inputs.span))?;
        let callable = UnitItemId::new(inputs.source_unit, item);
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        for observation in inputs.owned.loan_action_observations() {
            for region in observation.regions().iter().filter(|r| {
                r.protocol_version() == 1
                    && r.coverage() == UnitLoanActionCoverage::Complete
                    && r.callable() == Some(callable)
            }) {
                if observation.source() != inputs.source_unit
                    || !observation.owns_region(region)
                    || region.call() != call
                    || !result.entries.is_empty()
                {
                    return Err(fail());
                }
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = ids
                    .iter()
                    .map(|id| observation.action(id).ok_or_else(fail))
                    .collect::<Result<Vec<_>, _>>()?;
                let operands = operands(inputs, call, &actions)?;
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
                if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                let site = inputs
                    .source_plan
                    .call_site(inputs.source_token, call)
                    .ok_or_else(fail)?;
                if !site.delegation().is_empty() {
                    return Err(fail());
                }
                let callee = *inputs
                    .function_ids
                    .get(site.dispatch_key().ok_or_else(fail)?)
                    .ok_or_else(fail)?;
                if callee == caller {
                    return Err(fail());
                }
                let ty = inputs
                    .concrete(inputs.typed.call(call).ok_or_else(fail)?.return_type())
                    .ok_or_else(fail)?;
                let returns_value = match inputs.typed.types().get(ty) {
                    Some(UnitTypeKind::Builtin(BuiltinType::Int)) => true,
                    Some(UnitTypeKind::Builtin(BuiltinType::Unit)) => false,
                    _ => return Err(fail()),
                };
                result.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    call,
                    callee,
                    operands,
                    returns_value,
                    stage: 0,
                    block: None,
                });
            }
        }
        Ok(result)
    }
}

fn operands(
    inputs: &super::super::RefineInputs<'_>,
    call: UnitExpressionId,
    actions: &[&UnitLoanAction],
) -> Result<Vec<Operand>, LoweringError> {
    let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
    let typed = inputs.typed.call(call).ok_or_else(fail)?;
    let n = typed.arguments().len();
    let lang_frontend::parser::Expression::Call { arguments: raw, .. } = inputs
        .parsed
        .ast()
        .expressions()
        .get(call.expression())
        .map_err(|_| fail())?
        .payload()
    else {
        return Err(fail());
    };
    let contracts = inputs
        .owned
        .call_argument_contracts()
        .iter()
        .filter(|c| c.call() == call)
        .collect::<Vec<_>>();
    let point = UnitDropPoint::CallReturn(call);
    if !(1..=2).contains(&n)
        || actions.len() != 3 * n
        || raw.len() != n
        || contracts.len() != n
        || inputs
            .owned
            .loans()
            .iter()
            .filter(|f| f.call() == call)
            .count()
            != n
        || inputs
            .owned
            .direct_call_loan_ends()
            .iter()
            .filter(|f| f.loan().call() == call)
            .count()
            != n
    {
        return Err(fail());
    }
    let mut operands = Vec::<Operand>::new();
    for (index, argument) in typed.arguments().iter().enumerate() {
        let (
            UnitLoanAction::Begin(begin),
            UnitLoanAction::Cleanup {
                point: end_point,
                action: UnitIterationCleanupAction::EndCallLoan(end),
            },
            UnitLoanAction::Cleanup {
                point: drop_point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
        ) = (
            actions[index],
            actions[2 * n - 1 - index],
            actions[3 * n - 1 - index],
        )
        else {
            return Err(fail());
        };
        let id = UnitExpressionId::new(inputs.source_unit, raw[index].value);
        let matching = contracts
            .iter()
            .copied()
            .filter(|c| c.argument() == id)
            .collect::<Vec<_>>();
        let [contract] = matching.as_slice() else {
            return Err(fail());
        };
        if begin.call() != call
            || begin.argument() != id
            || begin.target() != &UnitLoanTarget::Temporary(id)
            || end != begin
            || argument.argument_index() != index
            || argument.parameter_index() != index
            || inputs.owned.loans().iter().filter(|f| *f == begin).count() != 1
            || inputs
                .owned
                .direct_call_loan_ends()
                .iter()
                .filter(|f| f.loan() == begin && f.point() == point)
                .count()
                != 1
            || *end_point != point
            || *drop_point != point
            || drop.point() != point
            || drop.target() != UnitDropTarget::Temporary(id)
            || drop.value_origin() != begin.begin_span()
            || contract.kind()
                != lang_frontend::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
            || contract.parameter_index() != argument.parameter_index()
            || contract.parameter_type() != argument.parameter_type()
            || contract.category() != argument.category()
            || contract.crosses_thread()
            || argument.crosses_thread()
            || contract.argument_span() != begin.begin_span()
            || contract.loan_begin_span() != begin.begin_span()
            || contract.call_span() != begin.end_span()
            || contract.parameter_span() != begin.parameter_span()
        {
            return Err(fail());
        }
        let indices = inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(_, f)| **f == *drop)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let [drop_index] = indices.as_slice() else {
            return Err(fail());
        };
        if operands
            .iter()
            .any(|a| a.fact.argument() == id || a.drop_index == *drop_index)
        {
            return Err(fail());
        }
        operands.push(Operand {
            fact: begin.clone(),
            literal: None,
            borrowed: false,
            drop_index: *drop_index,
            owner: None,
            place: None,
            loan: None,
            drop_authorized: false,
        });
    }
    let actual = inputs
        .owned
        .drops()
        .iter()
        .enumerate()
        .filter(|(_, f)| f.point() == point)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if actual
        != operands
            .iter()
            .rev()
            .map(|a| a.drop_index)
            .collect::<Vec<_>>()
    {
        return Err(fail());
    }
    Ok(operands)
}
