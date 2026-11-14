//! Whole-Program admission for a sealed named wrapper. No synthetic or filtered Program.
use super::{
    d10_after_child_graph::{GraphError, support::*},
    d10_before_child_graph::{CalleeFacts, inspect_function},
    model::*,
};
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        OwnedCompilationUnitView, UnitDirectCallLoanEndFact, UnitDropPoint, UnitLoanFact,
        UnitLoanTarget,
        d10_catalog_experiment::{BoundNamedAfterChildWrapper, BoundNamedWrapper},
    },
    parser::{Expression, LiteralKind},
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::collections::BTreeSet;

pub(crate) fn check(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &BoundNamedWrapper,
    program: &Program,
    entry: FunctionId,
) -> Result<(), GraphError> {
    if !bound.matches(unit)
        || unit.inputs().len() != 1
        || unit.names().names().source_units().len() != 1
    {
        return Err(GraphError::Rejected);
    }
    let callee = bound.callee();
    inspect(
        unit,
        &WrapperFacts::from_before(bound),
        program,
        entry,
        |module, function| {
            inspect_function(unit, &CalleeFacts::from_named(&callee), module, function)
        },
    )
    .ok_or(GraphError::Rejected)
}

pub(crate) fn check_after_child(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &BoundNamedAfterChildWrapper,
    program: &Program,
    entry: FunctionId,
) -> Result<(), GraphError> {
    if !bound.matches(unit)
        || unit.inputs().len() != 1
        || unit.names().names().source_units().len() != 1
    {
        return Err(GraphError::Rejected);
    }
    let callee = bound.callee();
    inspect(
        unit,
        &WrapperFacts::from_after(bound),
        program,
        entry,
        |module, function| {
            super::d10_after_child_graph::inspect_function(
                unit,
                &super::d10_after_child_graph::AfterCalleeFacts::from_named(&callee),
                module,
                function,
            )
        },
    )
    .ok_or(GraphError::Rejected)
}

/// The full split capability is checked before borrowing any function-local projection.
pub(crate) fn check_split(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &lang_frontend::ownership_checking::d10_catalog_experiment::BoundSplitNamedEntry,
    program: &Program,
    entry: FunctionId,
) -> Result<(), GraphError> {
    use lang_frontend::ownership_checking::d10_catalog_experiment::NamedEntryOrder;
    if !bound.matches(unit)
        || unit.inputs().len() != 2
        || unit.names().names().source_units().len() != 2
        || bound.entry().source_unit() == bound.callee().source_unit()
    {
        return Err(GraphError::Rejected);
    }
    for source in unit.names().names().index().source_units() {
        if source_for(unit, source.id())
            .ok_or(GraphError::Rejected)?
            .roots()
            .len()
            != 1
        {
            return Err(GraphError::Rejected);
        }
    }
    let callee = bound.callee_facts();
    let facts = WrapperFacts {
        entry: bound.entry(),
        callee: bound.callee(),
        roots: bound.wrapper_roots(),
        call: bound.call(),
        initializers: bound.initializers(),
        arguments: bound.arguments(),
        loans: bound.loans(),
        ends: bound.ends(),
    };
    inspect(
        unit,
        &facts,
        program,
        entry,
        |module, function| match bound.order() {
            NamedEntryOrder::BeforeChild => {
                inspect_function(unit, &CalleeFacts::from_split(&callee), module, function)
            }
            NamedEntryOrder::AfterChild => super::d10_after_child_graph::inspect_function(
                unit,
                &super::d10_after_child_graph::AfterCalleeFacts::from_split(&callee)?,
                module,
                function,
            ),
        },
    )
    .ok_or(GraphError::Rejected)
}

fn inspect(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &WrapperFacts<'_>,
    program: &Program,
    entry: FunctionId,
    check_callee: impl FnOnce(&Module, &Function) -> Option<()>,
) -> Option<()> {
    super::verify::verify_program(program).ok()?;
    let [module] = program.modules.as_slice() else {
        return None;
    };
    if module.functions.len() != 2 || entry.module() != module.id {
        return None;
    }
    let source = bound.entry.source_unit();
    let ast = source_for(unit, source)?.ast();
    let callee_ast = source_for(unit, bound.callee.source_unit())?.ast();
    let callable = bound.callee;
    if bound.entry == callable {
        return None;
    }
    let entry_origin = Origin::Source(ast.items().get(bound.entry.item()).ok()?.span());
    let callee_origin = Origin::Source(callee_ast.items().get(callable.item()).ok()?.span());
    let f = module.function(entry)?;
    let mut callees = module
        .functions
        .iter()
        .filter(|f| f.origin == callee_origin);
    let callee = callees.next()?;
    if callees.next().is_some() || callee.id() == entry || f.origin != entry_origin {
        return None;
    }
    check_callee(module, callee)?;
    if f.receiver.is_some()
        || f.never_returns
        || !f.return_types.is_empty()
        || f.borrow_return.is_some()
        || f.carrier_return.is_some()
        || f.blocks.len() != 1
    {
        return None;
    }
    let block = f.block(f.entry_block()?)?;
    if !block.parameters.is_empty() {
        return None;
    }
    let mut graph = Graph {
        f,
        module,
        used: BTreeSet::new(),
    };
    let origin = |id: UnitExpressionId| -> Option<Origin> {
        if id.source_unit() != source {
            return None;
        }
        Some(Origin::Source(
            ast.expressions().get(id.expression()).ok()?.span(),
        ))
    };
    let call_origin = origin(bound.call)?;
    let call = graph.unique(&call_origin, |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee: target,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return None;
    };
    if *target != callee.id() || !call.results.is_empty() || arguments.len() != 2 {
        return None;
    }
    let callee_graph = Graph {
        f: callee,
        module,
        used: BTreeSet::new(),
    };
    let callee_params = &callee.block(callee.entry_block()?)?.parameters;
    if callee_params.len() != 2 {
        return None;
    }
    let Expression::Call {
        arguments: source_args,
        ..
    } = ast
        .expressions()
        .get(bound.call.expression())
        .ok()?
        .payload()
    else {
        return None;
    };
    if source_args.len() != 2 {
        return None;
    }
    let mut constants = Vec::new();
    let mut preparations = Vec::new();
    let mut endings = Vec::new();
    for index in 0..2 {
        let fact = &bound.loans[index];
        let end = &bound.ends[1 - index];
        if fact.call() != bound.call
            || fact.argument() != bound.arguments[index]
            || fact.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || fact.argument().expression() != source_args[index].value
            || end.loan() != fact
            || end.point() != UnitDropPoint::CallReturn(bound.call)
            || end.preceding_drops() != 0
            || end.preceding_call_loan_ends() != 1 - index
        {
            return None;
        }
        let UnitLoanTarget::Place(place) = fact.target() else {
            return None;
        };
        if !place.is_root() || place.root() != bound.roots[index] {
            return None;
        }
        let argument = arguments[index];
        let loan = graph.shared_int(argument)?;
        callee_graph.shared_int(callee_params[index])?;
        if f.entity(argument)?.ty != callee.entity(callee_params[index])?.ty {
            return None;
        }
        let begin = graph.unique(&Origin::Source(fact.begin_span()), |op| {
            matches!(op, Operation::BorrowBegin { .. })
        })?;
        if begin.results.as_slice() != [argument] {
            return None;
        }
        let Operation::BorrowBegin {
            place: p,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return None;
        };
        let root = graph.unique(&Origin::Source(source_args[index].span), |op| {
            matches!(op, Operation::RootPlace { .. })
        })?;
        if root.results.as_slice() != [EntityId::Place(p)] {
            return None;
        }
        let Operation::RootPlace { owner } = root.operation else {
            return None;
        };
        let init = bound.initializers[index];
        let node = ast.expressions().get(init.expression()).ok()?;
        let Expression::Literal(LiteralKind::Integer(kind)) = node.payload() else {
            return None;
        };
        let value_expected = super::lowering_support::parse_integer_literal_text(
            unit.sources().slice(node.span()).ok()?,
            *kind,
            node.span(),
        )
        .ok()?;
        let constant = graph.unique(&origin(init)?, |op| matches!(op, Operation::Constant(_)))?;
        if constant.operation != Operation::Constant(ScalarConstant::Integer(value_expected))
            || value(constant)? != owner
        {
            return None;
        }
        graph.integer(owner)?;
        let finish = graph.unique(
            &Origin::Source(fact.end_span()),
            |op| matches!(op, Operation::BorrowEnd { loan: actual } if *actual == loan),
        )?;
        if !finish.results.is_empty() {
            return None;
        }
        constants.push(constant);
        preparations.extend([root, begin]);
        endings.push(finish);
    }
    // Source local declarations precede argument evaluation; cleanup follows actual End order.
    let mut sequence = constants;
    sequence.extend(preparations);
    sequence.push(call);
    sequence.extend(endings.into_iter().rev());
    graph.sequence(block.id, &sequence, &entry_origin, &entry_origin)?;
    if f.instructions.len() != sequence.len()
        || f.instructions
            .iter()
            .any(|i| sequence.iter().filter(|e| e.id == i.id).count() != 1)
    {
        return None;
    }
    if !matches!(&graph.term(block.id)?.kind, TerminatorKind::Return { values } if values.is_empty())
    {
        return None;
    }
    graph.no_abort(block.id)?;
    Some(())
}

/// Order-neutral wrapper facts, constructed only from each sealed handoff at its own entry.
struct WrapperFacts<'a> {
    entry: UnitItemId,
    callee: UnitItemId,
    roots: [UnitSymbolId; 2],
    call: UnitExpressionId,
    initializers: [UnitExpressionId; 2],
    arguments: [UnitExpressionId; 2],
    loans: &'a [UnitLoanFact; 2],
    ends: &'a [UnitDirectCallLoanEndFact; 2],
}
impl<'a> WrapperFacts<'a> {
    fn from_before(bound: &'a BoundNamedWrapper) -> Self {
        Self {
            entry: bound.entry(),
            callee: bound.callee().controls().0,
            roots: bound.wrapper_roots(),
            call: bound.call(),
            initializers: bound.initializers(),
            arguments: bound.arguments(),
            loans: bound.loans(),
            ends: bound.ends(),
        }
    }
    fn from_after(bound: &'a BoundNamedAfterChildWrapper) -> Self {
        Self {
            entry: bound.entry(),
            callee: bound.callee().controls().0,
            roots: bound.wrapper_roots(),
            call: bound.call(),
            initializers: bound.initializers(),
            arguments: bound.arguments(),
            loans: bound.loans(),
            ends: bound.ends(),
        }
    }
}
