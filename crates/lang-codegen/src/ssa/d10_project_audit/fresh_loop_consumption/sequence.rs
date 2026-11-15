//! Atomic certification of exactly two original sequential fresh-loop scopes.
use super::*;
use lang_frontend::{
    parser::{Expression, Statement},
    type_checking::UnitStatementId,
};

#[derive(Clone, PartialEq, Eq)]
struct Condition {
    expression: UnitExpressionId,
    symbol: lang_frontend::name_resolution::UnitSymbolId,
    span: lang_frontend::source::Span,
    parameter: usize,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Source {
    order: [UnitStatementId; 2],
    segments: [Vec<SourceConsumer>; 3],
    calls: Vec<Call>,
    abi: Vec<SsaTypeId>,
    flags: [Condition; 2],
}

pub(super) fn original_order(
    inputs: &RefineInputs<'_>,
    fact: &UnitLoopNoCarryFact,
    types: &RefineTypes<'_>,
) -> Option<Source> {
    let crate::ssa::refine::RefineBody::Block(body) = inputs.body else {
        return None;
    };
    let ast = inputs.parsed.ast();
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    let mut loops = Vec::new();
    let mut segments: [Vec<SourceConsumer>; 3] = std::array::from_fn(|_| Vec::new());
    for &statement in elements {
        if matches!(
            ast.statements().get(statement).ok()?.payload(),
            Statement::While { .. }
        ) {
            if loops.len() == 2 {
                return None;
            }
            loops.push(statement);
        } else {
            let segment = segments.get_mut(loops.len())?;
            if !segment.is_empty() {
                return None;
            }
            segment.push(crate::ssa::refine::fresh_loop_receipt::audit_consumer(
                inputs,
                statement,
                fact.source(),
            )?);
        }
    }
    let [first, second] = loops.as_slice() else {
        return None;
    };
    let calls = segments
        .iter()
        .flatten()
        .map(|consumer| {
            let site = inputs
                .source_plan
                .call_site(inputs.source_token, consumer.call)?;
            if !site.delegation().is_empty() {
                return None;
            }
            let key = site.dispatch_key()?;
            Some((key.clone(), *inputs.function_ids.get(key)?))
        })
        .collect::<Option<Vec<_>>>()?;
    let mut flags = Vec::new();
    for statement in [first, second] {
        let Statement::While { condition, .. } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        let node = ast.expressions().get(*condition).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let symbol = *inputs
            .references
            .get(&(node.span().start(), node.span().end()))?;
        flags.push(Condition {
            expression: UnitExpressionId::new(inputs.source_unit, *condition),
            symbol,
            span: node.span(),
            parameter: inputs.parameters.iter().position(|p| *p == symbol)?,
        });
    }
    if !matches!(inputs.parameters.len(), 2 | 3)
        || inputs.modes.len() != inputs.parameters.len()
        || inputs.modes.iter().any(|m| *m != ParameterMode::Borrow)
    {
        return None;
    }
    let source_index = inputs.parameters.iter().position(|p| *p == fact.source())?;
    let expected_flags: BTreeSet<_> = (0..inputs.parameters.len())
        .filter(|i| *i != source_index)
        .collect();
    if flags.iter().map(|f| f.parameter).collect::<BTreeSet<_>>() != expected_flags {
        return None;
    }
    let abi = inputs
        .parameters
        .iter()
        .enumerate()
        .map(|(i, symbol)| {
            use lang_frontend::type_checking::{BuiltinType, UnitTypeKind};
            let ty = inputs.typed.symbol_type(*symbol)?;
            let expected = if i == source_index {
                BuiltinType::String
            } else {
                BuiltinType::Boolean
            };
            if inputs.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(expected)) {
                return None;
            }
            types.type_ids.get(&ty).copied()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Source {
        abi,
        flags: flags.try_into().ok()?,
        order: [
            UnitStatementId::new(inputs.source_unit, *first),
            UnitStatementId::new(inputs.source_unit, *second),
        ],
        segments,
        calls,
    })
}

pub(super) fn finish(
    transaction: &Transaction,
    state: &State,
    group: &[&Expected],
    program: &Program,
    mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    owned: &dyn UnitLoweringFacts,
    used: &mut BTreeSet<usize>,
) -> Option<[Edge; 2]> {
    let [anchor, _] = group else {
        return None;
    };
    let source = anchor.sequence.as_ref()?;
    let order = source.order;
    if order[0] == order[1] {
        return None;
    }
    let mut expected = Vec::new();
    for statement in order {
        let matches: Vec<_> = group
            .iter()
            .copied()
            .filter(|e| e.fact.loop_statement() == statement)
            .collect();
        let [e] = matches.as_slice() else {
            return None;
        };
        if e.sequence.as_ref() != Some(source)
            || e.key != anchor.key
            || e.fact.callable() != anchor.fact.callable()
            || e.fact.source() != anchor.fact.source()
            || e.abi != anchor.abi
            || e.source_index != anchor.source_index
            || !matches!(e.fact.consumers().len(), 1 | 2)
        {
            return None;
        }
        expected.push(*e);
    }
    let UnitCallableTarget::Declaration(declaration) = anchor.key.target() else {
        return None;
    };
    if anchor.key != UnitFunctionInstanceKey::for_entry(declaration)
        || transaction.planned.get(&anchor.token)
            != Some(&(anchor.key.clone(), anchor.fact.callable()))
    {
        return None;
    }
    let originals: Vec<_> = owned
        .no_carry_loops()
        .iter()
        .filter(|f| f.callable() == anchor.fact.callable())
        .collect();
    if originals.len() != 2 {
        return None;
    }
    for e in &expected {
        let matches: Vec<_> = originals
            .iter()
            .filter(|f| f.loop_statement() == e.fact.loop_statement())
            .collect();
        let [original] = matches.as_slice() else {
            return None;
        };
        if !owned.owns_no_carry_loop(&e.fact)
            || !original.same_identity(&e.fact)
            || ***original != e.fact
            || owned.d10_fresh_loop_link(e.fact.loop_statement()).ok() != Some(e.fact.callable())
        {
            return None;
        }
    }
    let first = &expected[0].fact;
    let second = &expected[1].fact;
    if first.binding() == second.binding()
        || first.body() == second.body()
        || first.initializer() == second.initializer()
    {
        return None;
    }
    let caller = *mapping.get(&anchor.key)?;
    if mapping.values().filter(|id| **id == caller).count() != 1 {
        return None;
    }
    let matches: Vec<_> = state
        .snapshots
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.receipt.caller == caller
                || s.receipt
                    .loops
                    .iter()
                    .any(|l| order.contains(&l.fact.loop_statement()))
        })
        .collect();
    let [(index, snapshot)] = matches.as_slice() else {
        return None;
    };
    if used.contains(index)
        || snapshot.receipt.caller != caller
        || snapshot.receipt.loops.len() != 2
        || (source.calls.is_empty() && !snapshot.receipt.has_empty_segments(3))
        || !snapshot.receipt.matches_source_segments(&source.segments)
        || !snapshot.receipt.matches_borrowed_flags(
            &source
                .flags
                .iter()
                .map(|f| (f.parameter, f.span))
                .collect::<Vec<_>>(),
        )
    {
        return None;
    }
    // Validate the complete caller CFG once, including both loop exits and their connecting edge.
    let function = validate_caller(snapshot, program, &source.abi, anchor.source_index?)?;
    validate_mapping(&source.calls, program, mapping)?;
    validate_segments(snapshot, function, &source.calls)?;
    let mut edges = Vec::new();
    for (index, (e, local)) in expected.iter().zip(&snapshot.receipt.loops).enumerate() {
        if !local.fact.same_identity(&e.fact) || local.fact != e.fact {
            return None;
        }
        let calls = e.calls.as_ref()?;
        if calls.len() != 1 + e.fact.consumers().len() {
            return None;
        }
        validate_mapping(calls, program, mapping)?;
        edges.push(validate_loop(snapshot, function, calls, index)?);
    }
    let edges: [Edge; 2] = edges.try_into().ok()?;
    if edges[0] == edges[1] {
        return None;
    }
    used.insert(*index);
    Some(edges)
}

fn validate_mapping(
    calls: &[Call],
    program: &Program,
    mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
) -> Option<()> {
    for (key, id) in calls {
        if mapping.get(key) != Some(id)
            || mapping.values().filter(|v| *v == id).count() != 1
            || program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| f.id == *id)
                .count()
                != 1
        {
            return None;
        }
    }
    Some(())
}
