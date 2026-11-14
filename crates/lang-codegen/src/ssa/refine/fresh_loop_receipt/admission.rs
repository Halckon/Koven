//! A deliberately narrower receipt qualification; unsupported receipt shapes still lower normally.
use super::super::{RefineBody, RefineInputs, RefineTypes};
use super::*;
use lang_frontend::{
    parser::{Expression, Statement},
    type_checking::{BuiltinType, ParameterMode, UnitCallTarget, UnitTypeKind},
};
impl Plans {
    pub(in crate::ssa::refine) fn new(
        inputs: &RefineInputs<'_>,
        types: &RefineTypes<'_>,
    ) -> Result<Self, LoweringError> {
        let empty = Self::default();
        if inputs.parameters.len() < 2
            || inputs.modes.len() != inputs.parameters.len()
            || inputs
                .modes
                .iter()
                .any(|mode| !matches!(mode, ParameterMode::Borrow | ParameterMode::Value))
            || inputs.receiver.is_some()
            || !inputs.substitutions.is_empty()
            || inputs.static_self.is_some()
        {
            return Ok(empty);
        }
        let RefineBody::Block(body) = inputs.body else {
            return Ok(empty);
        };
        let Statement::Block { elements } = inputs
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| invalid(inputs.span))?
            .payload()
        else {
            return Ok(empty);
        };
        if elements.is_empty() {
            return Ok(empty);
        }
        let mut loops = Vec::new();
        let mut statements = vec![Vec::new()];
        for statement in elements {
            let node = inputs
                .parsed
                .ast()
                .statements()
                .get(*statement)
                .map_err(|_| invalid(inputs.span))?;
            if !matches!(node.payload(), Statement::While { .. }) {
                statements
                    .last_mut()
                    .ok_or_else(|| invalid(inputs.span))?
                    .push(*statement);
                continue;
            }
            let Some(plan) = loop_plan(inputs, types, *statement)? else {
                return Ok(empty);
            };
            if let Some(first) = loops.first() {
                let first: &Plan = first;
                if plan.fact.source() != first.fact.source()
                    || plan.fact.callable() != first.fact.callable()
                    || plan.source_parameter != first.source_parameter
                {
                    return Ok(empty);
                }
            }
            loops.push(plan);
            statements.push(Vec::new());
        }
        let Some(first) = loops.first() else {
            return Ok(empty);
        };
        let mut segments = Vec::new();
        for statements in statements {
            let mut segment = SourceSegment::default();
            for statement in statements {
                let Some(consumer) =
                    super::source_admission::consumer(inputs, statement, first.fact.source())?
                else {
                    return Ok(empty);
                };
                segment.consumers.push(consumer);
            }
            segments.push(segment);
        }
        for (index, plan) in loops.iter().enumerate() {
            if loops[..index].iter().any(|previous| {
                previous.fact.binding() == plan.fact.binding()
                    || previous.fact.body() == plan.fact.body()
                    || previous.fact.loop_statement() == plan.fact.loop_statement()
                    || previous.fact.initializer() == plan.fact.initializer()
            }) {
                return Err(invalid(inputs.span));
            }
        }
        let source_parameter = loops[0].source_parameter;
        if inputs.modes.get(source_parameter) != Some(&ParameterMode::Borrow) {
            return Ok(empty);
        }
        let mut abi_types = Vec::with_capacity(inputs.parameters.len());
        for (index, symbol) in inputs.parameters.iter().enumerate() {
            let ty = inputs
                .typed
                .symbol_type(*symbol)
                .ok_or_else(|| invalid(inputs.span))?;
            let expected = if index == source_parameter {
                BuiltinType::String
            } else {
                BuiltinType::Boolean
            };
            if inputs.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(expected)) {
                return Ok(empty);
            }
            abi_types.push(
                *types
                    .type_ids
                    .get(&ty)
                    .ok_or_else(|| invalid(inputs.span))?,
            );
        }
        Ok(Self {
            abi_types,
            abi_modes: inputs.modes.clone(),
            abi_slots: None,
            loops,
            segments,
            active: None,
            folds: Vec::new(),
        })
    }
}

fn loop_plan(
    inputs: &RefineInputs<'_>,
    types: &RefineTypes<'_>,
    statement: lang_frontend::ast::StatementId,
) -> Result<Option<Plan>, LoweringError> {
    let Some(fact) = inputs.owned.no_carry_loops().iter().find(|f| {
        f.loop_statement().source_unit() == inputs.source_unit
            && f.loop_statement().statement() == statement
    }) else {
        return Ok(None);
    };
    if !inputs.owned.owns_no_carry_loop(fact)
        || inputs.function_item != Some(fact.callable().item())
        || fact.callable().source_unit() != inputs.source_unit
        || fact.consumers().is_empty()
    {
        return Err(invalid(inputs.span));
    }
    let Statement::While {
        condition,
        body: actual,
        ..
    } = inputs
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| invalid(inputs.span))?
        .payload()
    else {
        return Err(invalid(inputs.span));
    };
    if *actual != fact.body().statement() || fact.body().source_unit() != inputs.source_unit {
        return Err(invalid(inputs.span));
    }
    let node = inputs
        .parsed
        .ast()
        .expressions()
        .get(*condition)
        .map_err(|_| invalid(inputs.span))?;
    if !matches!(node.payload(), Expression::Name) {
        return Err(invalid(inputs.span));
    }
    let condition_span = node.span();
    let flag_symbol = inputs
        .references
        .get(&(condition_span.start(), condition_span.end()))
        .ok_or_else(|| invalid(inputs.span))?;
    let source_parameter = inputs
        .parameters
        .iter()
        .position(|p| *p == fact.source())
        .ok_or_else(|| invalid(inputs.span))?;
    let flag_parameter = inputs
        .parameters
        .iter()
        .position(|p| p == flag_symbol)
        .ok_or_else(|| invalid(inputs.span))?;
    if source_parameter == flag_parameter {
        return Err(invalid(inputs.span));
    }
    let source_ty = inputs
        .typed
        .symbol_type(fact.source())
        .ok_or_else(|| invalid(inputs.span))?;
    let flag_ty = inputs
        .typed
        .symbol_type(*flag_symbol)
        .ok_or_else(|| invalid(inputs.span))?;
    if inputs.typed.types().get(source_ty) != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || inputs.typed.types().get(flag_ty) != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
    {
        return Ok(None);
    }
    // Intrinsic/special call lowering has different actual op protocols, not an error in execution.
    let calls =
        std::iter::once(fact.initializer()).chain(fact.consumers().iter().map(|(_, c, _)| *c));
    for call in calls {
        let Some(d) = inputs.typed.call(call) else {
            return Err(invalid(inputs.span));
        };
        if d.prints_line()
            || d.text_primitive().is_some()
            || d.receiver().is_some()
            || !matches!(d.target(), UnitCallTarget::Declaration(_))
        {
            return Ok(None);
        }
    }
    let binding = inputs
        .owned
        .borrow_results()
        .bindings()
        .iter()
        .find(|b| b.binding() == fact.binding())
        .ok_or_else(|| invalid(inputs.span))?;
    if crate::ssa::borrow_result_support::no_carry::admitted(inputs.parsed, inputs.owned, binding)
        != Some(statement)
    {
        return Err(invalid(inputs.span));
    }
    let expression_span = |id: UnitExpressionId| -> Result<Span, LoweringError> {
        if id.source_unit() != inputs.source_unit {
            return Err(invalid(inputs.span));
        }
        Ok(inputs
            .parsed
            .ast()
            .expressions()
            .get(id.expression())
            .map_err(|_| invalid(inputs.span))?
            .span())
    };
    let source_span = expression_span(fact.initializer())?;
    let consumer_spans = fact
        .consumers()
        .iter()
        .map(|(_, c, _)| expression_span(*c))
        .collect::<Result<Vec<_>, _>>()?;
    let lang_frontend::ownership_checking::UnitDropPoint::AfterStatement(end) = fact.end() else {
        return Err(invalid(inputs.span));
    };
    if fact.consumers().last().map(|(s, _, _)| *s) != Some(end) {
        return Err(invalid(inputs.span));
    }
    let end_span = binding.marker_span();
    let key = inputs
        .source_plan
        .source(inputs.source_token)
        .ok_or_else(|| invalid(inputs.span))?;
    let caller = *inputs
        .function_ids
        .get(key)
        .ok_or_else(|| invalid(inputs.span))?;
    let callees = fact
        .consumers()
        .iter()
        .map(|(_, c, _)| callee(inputs, *c))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(Plan {
        caller,
        fact: fact.clone(),
        source_callee: callee(inputs, fact.initializer())?,
        callees,
        condition: UnitExpressionId::new(inputs.source_unit, *condition),
        condition_span,
        source_span,
        consumer_spans,
        end_span,
        source_parameter,
        flag_parameter,
        flag_owned: inputs.modes.get(flag_parameter) == Some(&ParameterMode::Value),
        condition_used: false,
        source_type: *types
            .type_ids
            .get(&source_ty)
            .ok_or_else(|| invalid(inputs.span))?,
        flag_type: *types
            .type_ids
            .get(&flag_ty)
            .ok_or_else(|| invalid(inputs.span))?,
        source: None,
        flag: None,
        condition_value: None,
        result: None,
        blocks: None,
        events: Vec::new(),
        terms: Vec::new(),
        boundary: 0,
    }))
}
pub(in crate::ssa::refine) fn callee(
    inputs: &RefineInputs<'_>,
    call: UnitExpressionId,
) -> Result<FunctionId, LoweringError> {
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
