//! Independent raw/typed admission and same-catalog partial lineage binding.
use super::super::{RefineBody, RefineInputs};
use super::*;
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitLoanAction, UnitLoanActionCoverage, UnitLoanTarget},
    parser::{Expression, Item, Statement},
    type_checking::UnitCallTarget,
};
impl Plans {
    pub(in crate::ssa::refine) fn new(inputs: &RefineInputs<'_>) -> Result<Self, LoweringError> {
        let mut result = Self::default();
        // Receipt qualification is deliberately narrower than valid lowering.
        if inputs.parameters.len() != 1
            || inputs.modes.as_slice() != [lang_frontend::type_checking::ParameterMode::Borrow]
            || inputs.receiver.is_some()
            || !inputs.substitutions.is_empty()
            || inputs.static_self.is_some()
        {
            return Ok(result);
        }

        let RefineBody::Block(body) = inputs.body else {
            return Ok(result);
        };
        let Statement::Block { elements } = inputs
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| invalid(inputs.span))?
            .payload()
        else {
            return Ok(result);
        };
        let [owner_statement, _, while_statement] = elements.as_slice() else {
            return Ok(result);
        };
        for catalog in inputs.owned.loan_action_observations() {
            for lineage in catalog.loop_invariant_lineages().iter().filter(|l| {
                l.statement().source_unit() == inputs.source_unit
                    && l.statement().statement() == *while_statement
            }) {
                if result.plan.is_some()
                    || catalog.source() != inputs.source_unit
                    || !catalog.owns_loop_invariant_lineage(lineage)
                    || lineage.coverage() != UnitLoanActionCoverage::Partial
                    || lineage.uses().is_empty()
                {
                    return Err(invalid(inputs.span));
                }
                let mut facts = inputs
                    .owned
                    .borrow_results()
                    .bindings()
                    .iter()
                    .filter(|f| f.binding() == lineage.binding());
                let fact = facts.next().ok_or_else(|| invalid(inputs.span))?;
                if facts.next().is_some()
                    || crate::ssa::borrow_result_support::invariant_while::admitted(
                        inputs.parsed,
                        inputs.names,
                        inputs.typed,
                        inputs.owned,
                        fact,
                    ) != Some(*while_statement)
                {
                    return Err(invalid(inputs.span));
                }
                let Some(UnitLoanAction::Begin(source)) = catalog.action(&lineage.source_begin())
                else {
                    return Err(invalid(inputs.span));
                };
                let Some(UnitLoanAction::BorrowHandoffCommitted(handoff)) =
                    catalog.action(&lineage.handoff())
                else {
                    return Err(invalid(inputs.span));
                };
                let Some(UnitLoanAction::BorrowBindingEnd(end)) =
                    catalog.action(&lineage.binding_end())
                else {
                    return Err(invalid(inputs.span));
                };
                let UnitLoanTarget::Place(place) = source.target() else {
                    return Err(invalid(inputs.span));
                };
                if !place.is_root()
                    || handoff.binding() != fact.binding()
                    || handoff.source_loan() != source
                    || end.binding() != fact.binding()
                    || end.point()
                        != UnitDropPoint::AfterStatement(
                            lang_frontend::type_checking::UnitStatementId::new(
                                inputs.source_unit,
                                body,
                            ),
                        )
                    || fact.origin() != source.target()
                    || !fact.source_loan().is_some_and(|s| {
                        s.call() == source.call() && s.argument() == source.argument()
                    })
                {
                    return Err(invalid(inputs.span));
                }
                let Statement::While {
                    condition,
                    body: loop_body,
                    ..
                } = inputs
                    .parsed
                    .ast()
                    .statements()
                    .get(*while_statement)
                    .map_err(|_| invalid(inputs.span))?
                    .payload()
                else {
                    return Err(invalid(inputs.span));
                };
                if lineage.condition().expression() != *condition
                    || lineage.body().statement() != *loop_body
                    || !matches!(
                        inputs
                            .parsed
                            .ast()
                            .expressions()
                            .get(*condition)
                            .map_err(|_| invalid(inputs.span))?
                            .payload(),
                        Expression::Name
                    )
                {
                    return Err(invalid(inputs.span));
                }
                let Statement::Block {
                    elements: consumers,
                } = inputs
                    .parsed
                    .ast()
                    .statements()
                    .get(*loop_body)
                    .map_err(|_| invalid(inputs.span))?
                    .payload()
                else {
                    return Err(invalid(inputs.span));
                };
                if consumers.len() != lineage.uses().len() {
                    return Err(invalid(inputs.span));
                }
                let condition_span = inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(*condition)
                    .map_err(|_| invalid(inputs.span))?
                    .span();
                let flag_symbol = inputs
                    .references
                    .get(&(condition_span.start(), condition_span.end()))
                    .ok_or_else(|| invalid(inputs.span))?;
                let flag_parameter = inputs
                    .parameters
                    .iter()
                    .position(|p| p == flag_symbol)
                    .ok_or_else(|| invalid(inputs.span))?;
                // This first receipt observes a borrowed Boolean read; other valid
                // condition modes retain existing lowering without a new receipt.
                if inputs.modes.get(flag_parameter)
                    != Some(&lang_frontend::type_checking::ParameterMode::Borrow)
                {
                    continue;
                }
                let special = |call| {
                    inputs
                        .typed
                        .call(call)
                        .is_none_or(|d| d.prints_line() || d.text_primitive().is_some())
                };
                if special(source.call()) || lineage.uses().iter().any(|u| special(u.loan().call()))
                {
                    continue;
                }
                let mut callees = Vec::new();
                let bounds = lineage.boundary_cursors();
                if bounds[0] > bounds[1]
                    || bounds[1] >= bounds[2]
                    || bounds[2] > bounds[3]
                    || lineage.condition().source_unit() != inputs.source_unit
                    || lineage.body().source_unit() != inputs.source_unit
                    || source.call().source_unit() != inputs.source_unit
                    || source.argument().source_unit() != inputs.source_unit
                {
                    return Err(invalid(inputs.span));
                }
                let mut previous = None;
                for (statement, usage) in consumers.iter().zip(lineage.uses()) {
                    let Statement::Expression { expression } = inputs
                        .parsed
                        .ast()
                        .statements()
                        .get(*statement)
                        .map_err(|_| invalid(inputs.span))?
                        .payload()
                    else {
                        return Err(invalid(inputs.span));
                    };
                    if usage.loan().call().source_unit() != inputs.source_unit
                        || usage.loan().argument().source_unit() != inputs.source_unit
                        || usage.return_cursor() <= bounds[1]
                        || usage.return_cursor() > bounds[2]
                        || usage.loan().call().expression() != *expression
                        || usage.loan().target() != source.target()
                        || !matches!(catalog.action(&usage.begin()),Some(UnitLoanAction::Begin(loan)) if loan==usage.loan())
                        || usage.return_point() != UnitDropPoint::CallReturn(usage.loan().call())
                        || previous.is_some_and(|p| p >= usage.return_cursor())
                    {
                        return Err(invalid(inputs.span));
                    }
                    previous = Some(usage.return_cursor());
                    callees.push(callee(inputs, usage.loan().call())?);
                }
                let Statement::LocalVariable { declaration } = inputs
                    .parsed
                    .ast()
                    .statements()
                    .get(*owner_statement)
                    .map_err(|_| invalid(inputs.span))?
                    .payload()
                else {
                    return Err(invalid(inputs.span));
                };
                let Item::Variable { initializer, .. } = inputs
                    .parsed
                    .ast()
                    .items()
                    .get(*declaration)
                    .map_err(|_| invalid(inputs.span))?
                    .payload()
                else {
                    return Err(invalid(inputs.span));
                };
                let owner_span = inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(*initializer)
                    .map_err(|_| invalid(inputs.span))?
                    .span();
                let key = inputs
                    .source_plan
                    .source(inputs.source_token)
                    .ok_or_else(|| invalid(inputs.span))?;
                let caller = *inputs
                    .function_ids
                    .get(key)
                    .ok_or_else(|| invalid(inputs.span))?;
                result.plan = Some(Plan {
                    caller,
                    binding: fact.binding(),
                    root: place.root(),
                    lineage: lineage.clone(),
                    callees,
                    source_callee: callee(inputs, source.call())?,
                    condition: lineage.condition(),
                    owner_span,
                    owner_binding_span: inputs
                        .parsed
                        .ast()
                        .statements()
                        .get(*owner_statement)
                        .map_err(|_| invalid(inputs.span))?
                        .span(),
                    literal: None,
                    owner_alias: None,
                    condition_span,
                    flag_parameter,
                    owner: None,
                    place: None,
                    source: None,
                    result: None,
                    flag: None,
                    condition_value: None,
                    blocks: None,
                    events: Vec::new(),
                    terms: Vec::new(),
                    folds: Vec::new(),
                    boundary: 0,
                });
            }
        }
        Ok(result)
    }
}
fn callee(inputs: &RefineInputs<'_>, call: UnitExpressionId) -> Result<FunctionId, LoweringError> {
    let fail = || invalid(inputs.span);
    let descriptor = inputs.typed.call(call).ok_or_else(fail)?;
    let UnitCallTarget::Declaration(declaration) = descriptor.target() else {
        return Err(fail());
    };
    let site = inputs
        .source_plan
        .call_site(inputs.source_token, call)
        .ok_or_else(fail)?;
    let key = site.dispatch_key().ok_or_else(fail)?;
    if !site.delegation().is_empty()
        || key != site.key()
        || key.target()
            != lang_frontend::type_checking::UnitCallableTarget::Declaration(declaration)
        || key.is_specialized()
        || key.deinit_owner().is_some()
    {
        return Err(fail());
    }
    inputs.function_ids.get(key).copied().ok_or_else(fail)
}
