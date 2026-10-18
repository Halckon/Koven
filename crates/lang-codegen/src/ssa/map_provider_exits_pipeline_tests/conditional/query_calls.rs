//! Source-call boundaries: exact initializer results and the callee's own Borrow slots.
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
    },
    ast::{Anchors, Input},
};
use crate::ssa::model::*;
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, Item, NameMarker, ParameterModeMarker},
};
use std::collections::BTreeSet;

fn source<'a>(module: &'a Module, a: &Anchors, name: &str) -> Result<&'a Function, Failure> {
    let root = one(
        a.functions.iter().filter(|v| v.name == name),
        "conditional call AST root",
    )?;
    function(module, root.span)
}

pub(super) fn check(program: &Program, f: &Input, a: &Anchors) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "conditional call module")?;
    let scan = source(module, a, "scan")?;
    let factory = source(module, a, if f.map { "makeMap" } else { "makeMutable" })?;
    let entry = Graph::new(source(module, a, "entry")?)?;
    let scan_entry = scan
        .entry_block()
        .and_then(|id| scan.block(id))
        .ok_or_else(|| failure("conditional call scan entry", scan.id))?;
    let seed = *one(
        scan_entry.parameters.iter(),
        "conditional call scan sole parameter",
    )?;
    let data = scan
        .entity(seed)
        .ok_or_else(|| failure("conditional call scan parameter", seed))?;
    let map_type = data.ty.semantic_type();
    let (kind, key, val) = module
        .map_container(map_type)
        .ok_or_else(|| failure("conditional call source Map storage", data))?;
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    need(
        scan.receiver.is_none()
            && factory.receiver.is_none()
            && data.ty
                == if f.map {
                    EntityType::Value(map_type)
                } else {
                    shared
                }
            && kind
                == if f.map {
                    MapContainerKind::Map
                } else {
                    MapContainerKind::MutableMap
                }
            && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            && factory.return_types.as_slice() == [map_type]
            && a.entry_calls.len() == 5,
        "conditional call exact source signature",
        (scan.id, data, factory),
    )?;
    let mut uses = Uses::default();
    let mut protected = BTreeSet::new();
    let mut scan_calls = BTreeSet::new();
    let mut factory_calls = BTreeSet::new();
    let mut previous = None;
    for (index, anchor) in a.entry_calls.iter().enumerate() {
        let call_span = f.expression(anchor.call)?.1;
        let initializer_span = f.expression(anchor.source_initializer)?.1;
        let call = one(entry.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(call_span)
                && matches!(&i.operation, Operation::DirectCall { callee, receiver: None, .. } if *callee == scan.id)
        }), "conditional call actual entry scan")?;
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            return Err(failure("conditional call scan operation", call));
        };
        let argument = *one(arguments.iter(), "conditional call sole source argument")?;
        entry_order(&entry, previous, call.block)?;
        previous = Some(call.block);
        let producer = one(entry.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(initializer_span)
                && if index < 4 {
                    matches!(&i.operation, Operation::DirectCall { callee, receiver: None, .. } if *callee == factory.id)
                } else {
                    i.operation == Operation::MapConstruct { map_type }
                }
        }), "conditional call exact initializer producer")?;
        let owner = value(producer)?;
        let owner_id = EntityId::Value(owner);
        need(
            producer.block == call.block
                && entry.position(producer)? < entry.position(call)?
                && entry
                    .function
                    .entity(owner_id)
                    .is_some_and(|v| v.ty == EntityType::Value(map_type))
                && scan_calls.insert(call.id)
                && protected.insert(owner_id),
            "conditional call distinct actual owner before scan",
            (producer, call),
        )?;
        uses.protect(owner_id);
        if index < 4 {
            need(
                factory_calls.insert(producer.id),
                "conditional call distinct factory results",
                producer,
            )?;
            factory_argument(
                module,
                f,
                &entry,
                anchor.source_initializer,
                ["skip", "stop", "return", "pass"][index],
                producer,
                call,
            )?;
        }
        if f.map {
            need(
                argument == owner_id,
                "conditional call transfers exact owned initializer",
                call,
            )?;
            uses.permit(owner_id, call);
        } else {
            let begin = entry.definition(argument)?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                return Err(failure("conditional call shared source loan", begin));
            };
            let current = loan(begin)?;
            let place_id = EntityId::Place(place);
            let root = entry.definition(place_id)?;
            let end = one(
                entry
                    .instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
                "conditional call unique caller loan end",
            )?;
            let drop = one(
                entry
                    .instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "conditional call caller owner drop",
            )?;
            let argument_origin = Origin::Source(f.expression(anchor.argument)?.1);
            need(
                root.operation == Operation::RootPlace { owner }
                    && result(root)? == place_id
                    && root.origin == argument_origin
                    && begin.origin == argument_origin
                    && end.origin == Origin::Source(call_span)
                    && drop.origin == Origin::Source(anchor.source_binding)
                    && entry
                        .function
                        .entity(argument)
                        .is_some_and(|v| v.ty == shared)
                    && [root, begin, end, drop]
                        .iter()
                        .all(|i| i.block == call.block)
                    && entry.position(producer)? < entry.position(root)?
                    && entry.position(root)? < entry.position(begin)?
                    && entry.position(begin)? < entry.position(call)?
                    && entry.position(call)? < entry.position(end)?
                    && entry.position(end)? < entry.position(drop)?,
                "conditional call exact caller root/loan/order/drop",
                (producer, root, begin, call, end, drop),
            )?;
            uses.permit(owner_id, root);
            uses.permit(owner_id, drop);
            uses.permit(place_id, begin);
            uses.permit(argument, call);
            uses.permit(argument, end);
            protected.extend([place_id, argument]);
        }
    }
    for (callee, expected) in [(scan.id, scan_calls), (factory.id, factory_calls)] {
        let actual = entry
            .function
            .instructions
            .iter()
            .filter_map(|i| {
                matches!(&i.operation, Operation::DirectCall { callee: c, .. } if *c == callee)
                    .then_some(i.id)
            })
            .collect::<BTreeSet<_>>();
        need(
            actual == expected,
            "conditional call complete actual call set",
            (callee, actual, expected),
        )?;
    }
    // Empty MapConstruct has no elements; protected uses forbid inserting or replacing payload.
    // This is a static argument proof, not an assertion of dynamic empty/native execution.
    need(
        entry
            .edges
            .iter()
            .all(|(_, e)| e.arguments.iter().all(|v| !protected.contains(v))),
        "conditional call source lifetime ends before entry CFG edges",
        &entry.edges,
    )?;
    uses.check(&entry, None)?;
    component_boundaries(module, f, a)
}

fn entry_order(
    graph: &Graph<'_>,
    previous: Option<BlockId>,
    current: BlockId,
) -> Result<(), Failure> {
    let Some(previous) = previous else {
        need(
            graph.function.entry_block() == Some(current),
            "conditional first factory in entry",
            current,
        )?;
        return graph.incoming_from(current, &[]);
    };
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &graph.term(previous)?.kind
    else {
        return Err(failure(
            "conditional previous case result branches",
            graph.term(previous)?,
        ));
    };
    need(
        previous != current
            && when_true.target != when_false.target
            && graph.branch(when_true.target)?.target == current
            && graph.branch(when_false.target)?.target == current,
        "conditional next initializer follows both previous result branches",
        (previous, current),
    )?;
    graph.incoming_from(when_true.target, &[previous])?;
    graph.incoming_from(when_false.target, &[previous])?;
    graph.incoming_from(current, &[when_true.target, when_false.target])
}

fn factory_argument(
    module: &Module,
    f: &Input,
    graph: &Graph<'_>,
    initializer: ExpressionId,
    expected: &str,
    call: &Instruction,
    scan_call: &Instruction,
) -> Result<(), Failure> {
    let (expression, call_span) = f.expression(initializer)?;
    let Expression::Call { arguments, .. } = expression else {
        return Err(failure("conditional factory source call", expression));
    };
    let source = one(arguments.iter(), "conditional factory sole source argument")?;
    let (literal, span) = f.expression(source.value)?;
    need(
        matches!(literal, Expression::String { .. })
            && f.sources.slice(span) == Ok(format!("\"{expected}\"").as_str()),
        "conditional factory actual key literal",
        (literal, span, expected),
    )?;
    let Operation::DirectCall {
        arguments,
        receiver: None,
        ..
    } = &call.operation
    else {
        return Err(failure("conditional factory actual source call", call));
    };
    let argument = *one(
        arguments.iter(),
        "conditional factory sole actual Borrow argument",
    )?;
    let begin = graph.definition(argument)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("conditional factory argument Shared loan", begin));
    };
    let current = loan(begin)?;
    let place_id = EntityId::Place(place);
    let root = graph.definition(place_id)?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("conditional factory literal root", root));
    };
    let owner_id = EntityId::Value(owner);
    let producer = graph.definition(owner_id)?;
    let Operation::StringLiteral { string, bytes } = &producer.operation else {
        return Err(failure(
            "conditional factory source StringLiteral",
            producer,
        ));
    };
    let end = one(
        graph
            .instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
        "conditional factory unique temporary loan end",
    )?;
    let drop = one(
        graph
            .instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "conditional factory unique temporary literal Drop",
    )?;
    need(
        bytes.as_slice() == expected.as_bytes()
            && module.type_kind(*string) == Some(&SsaTypeKind::StringOwner)
            && graph
                .function
                .entity(owner_id)
                .is_some_and(|d| d.ty == EntityType::Value(*string))
            && graph.function.entity(argument).is_some_and(|d| {
                d.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: *string,
                }
            })
            && result(producer)? == owner_id
            && result(root)? == place_id
            && [producer, root, begin, drop]
                .iter()
                .all(|i| i.origin == Origin::Source(span))
            && call.origin == Origin::Source(call_span)
            && end.origin == Origin::Source(call_span)
            && [producer, root, begin, end, drop, scan_call]
                .iter()
                .all(|i| i.block == call.block)
            && graph.position(producer)? < graph.position(root)?
            && graph.position(root)? < graph.position(begin)?
            && graph.position(begin)? < graph.position(call)?
            && graph.position(call)? < graph.position(end)?
            && graph.position(end)? < graph.position(drop)?
            && graph.position(drop)? < graph.position(scan_call)?,
        "conditional factory exact source literal/loan/call/cleanup order",
        (producer, root, begin, call, end, drop),
    )?;
    let mut uses = Uses::default();
    uses.permit(owner_id, root);
    uses.permit(owner_id, drop);
    uses.permit(place_id, begin);
    uses.permit(argument, call);
    uses.permit(argument, end);
    need(
        graph.edges.iter().all(|(_, edge)| {
            edge.arguments
                .iter()
                .all(|id| ![owner_id, place_id, argument].contains(id))
        }),
        "conditional factory temporaries do not cross case edges",
        &graph.edges,
    )?;
    uses.check(graph, None)
}

fn component_boundaries(module: &Module, f: &Input, a: &Anchors) -> Result<(), Failure> {
    let read = source(module, a, "read")?;
    let operand = source(module, a, "operand")?;
    let mut operand_parameters = Vec::new();
    for function in [read, operand] {
        let root = one(
            a.functions
                .iter()
                .filter(|r| function.origin == Origin::Source(r.span)),
            "conditional component actual AST declaration",
        )?;
        let ast = f
            .parsed
            .ast()
            .items()
            .get(root.item)
            .map_err(|e| failure("conditional component AST item", e))?;
        let Item::Function { parameters, .. } = ast.payload() else {
            return Err(failure("conditional component function parameters", ast));
        };
        let graph = Graph::new(function)?;
        let block = graph.block(
            function
                .entry_block()
                .ok_or_else(|| failure("conditional component entry", function.id))?,
        )?;
        need(
            function.receiver.is_none()
                && parameters.len() == 2
                && block.parameters.len() == 2
                && graph.blocks.as_slice() == [block.id]
                && graph.edges.is_empty(),
            "conditional component two source parameters in straight body",
            (parameters, block),
        )?;
        for (slot, (parameter, &entity)) in parameters.iter().zip(&block.parameters).enumerate() {
            let data = function
                .entity(entity)
                .ok_or_else(|| failure("conditional component parameter entity", entity))?;
            need(
                matches!(parameter.name, NameMarker::Present(n) if f.sources.slice(n) == Ok(["key", "value"][slot]))
                    && matches!(
                        parameter.mode_marker,
                        None | Some(ParameterModeMarker::Borrow(_))
                    )
                    && matches!(data.ty, EntityType::Loan { kind: LoanKind::Shared, target }
                    if module.type_kind(target) == Some(&SsaTypeKind::StringOwner))
                    && data.origin == Origin::Source(root.span)
                    && data.definition
                        == Definition::BlockParameter {
                            block: block.id,
                            index: slot,
                        }
                    && !graph.instructions.iter().any(|i| {
                        matches!(i.operation,
                    Operation::BorrowEnd { loan } if EntityId::Loan(loan) == entity)
                    }),
                "conditional component own ordered Shared String parameter",
                (parameter, data),
            )?;
        }
        if function.id == read.id {
            need(
                function.return_types.is_empty(),
                "conditional read Unit signature",
                function,
            )?;
        } else {
            operand_parameters = block.parameters.clone();
        }
    }
    let graph = Graph::new(operand)?;
    let call_span = f.expression(a.operand_read.0)?.1;
    let call = one(
        graph.instructions.iter().copied().filter(
            |i| matches!(&i.operation, Operation::DirectCall { callee, .. } if *callee == read.id),
        ),
        "conditional operand sole actual read",
    )?;
    need(
        call.origin == Origin::Source(call_span)
            && call.results.is_empty()
            && matches!(&call.operation, Operation::DirectCall { receiver: None, arguments, .. } if *arguments == operand_parameters),
        "conditional operand read receives its own ordered parameters",
        call,
    )?;
    let mut uses = Uses::default();
    for parameter in operand_parameters {
        uses.permit(parameter, call);
    }
    let root = one(
        a.functions.iter().filter(|r| r.name == "operand"),
        "conditional operand AST root",
    )?;
    let (transfer, literal) = one(
        f.parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Expression::Return {
                    value: Some(value), ..
                } if n.span().source_id() == root.span.source_id()
                    && root.span.start() <= n.span().start()
                    && n.span().end() <= root.span.end() =>
                {
                    Some((n.span(), *value))
                }
                _ => None,
            }),
        "conditional operand source Return",
    )?;
    let (literal, span) = f.expression(literal)?;
    need(
        matches!(literal, Expression::Literal(_)) && f.sources.slice(span) == Ok("7"),
        "conditional operand actual source seven",
        (literal, span),
    )?;
    let constant = one(
        graph.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span)
                && i.operation == Operation::Constant(ScalarConstant::Integer(7))
        }),
        "conditional operand true seven result",
    )?;
    let returned = value(constant)?;
    let ty = *one(
        operand.return_types.iter(),
        "conditional operand one return type",
    )?;
    need(
        module.type_kind(ty)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            && operand
                .entity(result(constant)?)
                .is_some_and(|d| d.ty == EntityType::Value(ty))
            && call.block == constant.block
            && graph.position(call)? < graph.position(constant)?
            && graph.term(constant.block)?.origin == Origin::Source(transfer)
            && graph.term(constant.block)?.kind
                == TerminatorKind::Return {
                    values: vec![returned],
                },
        "conditional operand returns exact source Int seven",
        (call, constant, graph.term(constant.block)?),
    )?;
    uses.protect(result(constant)?);
    uses.check(&graph, Some((constant.block, returned)))
}
