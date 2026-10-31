//! Real ordinary sources only: all incoming identities and preserved prefix seeds.
use super::*;
use crate::ssa::{
    map_provider_exits_pipeline_tests::{OrdinaryGraph as Graph, one_query as one},
    model::{BlockId, EntityType, Origin, ScalarConstant},
};
use lang_frontend::parser::{Expression, Item, NameMarker, ParameterModeMarker};

#[test]
fn paired_zero_and_multiple_normal_exits_preserve_string_return_owners() {
    let mut failures = Vec::new();
    for constants in [false, true] {
        for direct in [true, false] {
            let keyword = if direct { "return " } else { "" };
            let declaration = if constants {
                "const val UNUSED = 17\n"
            } else {
                ""
            };
            let text = format!(
                "package pair\n{declaration}fun entry(own flag: Boolean): String = \
                 if (flag) {{ {keyword}\"left\" }} else {{ {keyword}\"right\" }}"
            );
            observe_products(
                &text,
                constants,
                &mut failures,
                paired_in_order_observed_entry,
                |sources, parsed, program, entry, _, _| {
                    string_returns(sources, parsed, program, entry, direct);
                },
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn paired_multiple_normal_exits_keep_zero_column_merge() {
    let mut failures = Vec::new();
    let text = "package pair\nfun entry(): Unit { if (true) {} else {} }";
    observe_products(
        text,
        false,
        &mut failures,
        paired_in_order_observed_entry,
        |sources, parsed, program, entry, _, _| {
            let function = program.modules[0].function(entry).unwrap();
            assert_eq!(
                function.origin,
                Origin::Source(function_span(sources, parsed, "entry"))
            );
            let graph = Graph::new(function).unwrap();
            let returned = one(
                graph.blocks.iter().copied().filter(|block| {
                    match &graph.term(*block).unwrap().kind {
                        TerminatorKind::Return { values } => values.is_empty(),
                        _ => false,
                    }
                }),
                "empty merge Return",
            )
            .unwrap();
            let (condition, when_true, when_false) = conditional(sources, parsed, &graph);
            assert!(matches!(
                graph.definition(condition).unwrap().operation,
                Operation::Constant(ScalarConstant::Boolean(true))
            ));
            assert_ne!(returned, when_true);
            assert_ne!(returned, when_false);
            graph
                .incoming_from(returned, &[when_true, when_false])
                .unwrap();
            assert!(graph.block(returned).unwrap().parameters.is_empty());
            for (_, edge) in graph.incoming(returned) {
                assert!(edge.arguments.is_empty());
            }
        },
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn paired_pending_value_joins_compare_complete_incoming_signatures() {
    let mut failures = Vec::new();
    for same in [false, true] {
        let tail = if same { "first" } else { "0" };
        let call = format!("take(first, if (flag) {{ first }} else {{ {tail} }})");
        let body = if same {
            format!("{{ val value = {call}; return take(first, value) }}")
        } else {
            format!("= {call}")
        };
        let text = format!(
            "package pair\nfun take(own first: Int, own second: Int): Int = first\n\
             fun entry(own first: Int, own flag: Boolean): Int {body}"
        );
        let kinds = std::cell::RefCell::new(Vec::new());
        let observe = |sources: &SourceMap,
                       parsed: &ParsedFile,
                       program: &Program,
                       entry,
                       kind,
                       modes: Option<&[ParameterMode]>| {
            kinds.borrow_mut().push(kind);
            scalar_join(sources, parsed, program, entry, modes, same);
        };
        // This retirement requires the separate Stage A CI baseline. Both
        // original sources and their Unit relation oracle stay unchanged.
        let rooted_events = observe_products(
            &text,
            false,
            &mut failures,
            paired_in_order_observed_entry,
            observe,
        );
        // Both sources use only Copyable scalar values: the complete observed
        // String/borrow/drop/abort event stream is independently fixed to empty.
        assert_eq!(rooted_events, Vec::<Event>::new());
        assert_eq!(
            *kinds.borrow(),
            [
                EntryKind::RootedUnit,
                EntryKind::Unit,
                EntryKind::Unit,
                EntryKind::RootedUnit,
                EntryKind::Unit,
                EntryKind::Unit,
            ]
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn observe_products(
    text: &str,
    constants: bool,
    failures: &mut Vec<String>,
    produce: fn(&str, bool, bool, &products::Observer<'_>) -> Vec<Event>,
    observe: impl Fn(&SourceMap, &ParsedFile, &Program, FunctionId, EntryKind, Option<&[ParameterMode]>),
) -> Vec<Event> {
    let observations = std::cell::RefCell::new(Vec::new());
    let query = |sources: &SourceMap,
                 parsed: &ParsedFile,
                 program: &Program,
                 entry,
                 kind,
                 modes: Option<&[ParameterMode]>| {
        // Catch only the new read-only query; all real product gates remain direct.
        observations
            .borrow_mut()
            .push(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || {
                    observe(sources, parsed, program, entry, kind, modes);
                },
            )));
    };
    let first = produce(text, constants, false, &query);
    let second = produce(text, constants, true, &query);
    let observations = observations.into_inner();
    assert_eq!(observations.len(), 6, "two baseline and four unit products");
    for (index, result) in observations.into_iter().enumerate() {
        if let Err(payload) = result {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-string query panic");
            failures.push(format!("carry join observation {}: {message}", index + 1));
        }
    }
    assert_eq!(first, second);
    first
}

fn function_span(sources: &SourceMap, parsed: &ParsedFile, name: &str) -> Span {
    one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).unwrap();
            matches!(node.payload(), Item::Function { name: NameMarker::Present(span), .. }
            if sources.slice(*span) == Ok(name))
            .then_some(node.span())
        }),
        "actual AST function declaration",
    )
    .unwrap()
}

fn column(graph: &Graph<'_>, target: BlockId, parameter: EntityId) -> Vec<(BlockId, EntityId)> {
    let Definition::BlockParameter { block, index } =
        graph.function.entity(parameter).unwrap().definition
    else {
        panic!("observed operand must be an actual block parameter");
    };
    assert_eq!(block, target, "operand belongs to the observed merge block");
    assert_eq!(
        graph.block(block).unwrap().parameters.get(index),
        Some(&parameter)
    );
    let incoming = graph.incoming(block);
    assert!(
        !incoming.is_empty(),
        "all incoming must exist for the target parameter"
    );
    incoming
        .into_iter()
        .map(|(from, edge)| (from, graph.argument(edge, parameter).unwrap()))
        .collect()
}

fn conditional(
    sources: &SourceMap,
    parsed: &ParsedFile,
    graph: &Graph<'_>,
) -> (EntityId, BlockId, BlockId) {
    let node = one(
        parsed.ast().expressions().iter().filter_map(|(_, node)| {
            matches!(node.payload(), Expression::If { .. }).then_some(node)
        }),
        "one real If expression",
    )
    .unwrap();
    let Expression::If { condition, .. } = node.payload() else {
        unreachable!()
    };
    let condition_node = parsed.ast().expressions().get(*condition).unwrap();
    assert!(matches!(
        sources.slice(condition_node.span()).unwrap(),
        "flag" | "true"
    ));
    let term = graph.term(graph.function.entry_block().unwrap()).unwrap();
    assert_eq!(term.origin, Origin::Source(node.span()));
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        panic!("the real If header must have both actual branch edges");
    };
    assert_ne!(when_true.target, when_false.target);
    (
        EntityId::Value(*condition),
        when_true.target,
        when_false.target,
    )
}

fn string_returns(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    entry: FunctionId,
    direct: bool,
) {
    let function = program.modules[0].function(entry).unwrap();
    assert_eq!(
        function.origin,
        Origin::Source(function_span(sources, parsed, "entry"))
    );
    let graph = Graph::new(function).unwrap();
    let (_, when_true, when_false) = conditional(sources, parsed, &graph);
    let mut owners = Vec::new();
    for (text, block) in [("left", when_true), ("right", when_false)] {
        let node = one(
            parsed.ast().expressions().iter().filter_map(|(_, node)| {
                (matches!(node.payload(), Expression::String { .. })
                    && sources.slice(node.span()).unwrap() == format!("\"{text}\""))
                .then_some(node)
            }),
            "actual AST String literal",
        )
        .unwrap();
        let literal = one(
            graph.instructions.iter().copied().filter(|instruction| {
                instruction.origin == Origin::Source(node.span())
                    && match &instruction.operation {
                        Operation::StringLiteral { bytes, .. } => bytes == text.as_bytes(),
                        _ => false,
                    }
            }),
            "exact literal owner definition",
        )
        .unwrap();
        assert_eq!(literal.block, block);
        owners.push(one(literal.results.iter().copied(), "literal owner").unwrap());
    }
    let returned = graph
        .blocks
        .iter()
        .filter_map(|block| match &graph.term(*block).unwrap().kind {
            TerminatorKind::Return { values } => Some((*block, values.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if direct {
        assert_direct_string_returns(sources, program, entry, &["left", "right"]);
        assert_eq!(returned.len(), 2);
        assert_eq!(
            graph.edges.len(),
            2,
            "zero normal exits create no merge edges"
        );
    } else {
        let (block, values) = one(returned.into_iter(), "unique merged Return").unwrap();
        let current = EntityId::Value(one(values.into_iter(), "one returned String").unwrap());
        graph
            .incoming_from(block, &[when_true, when_false])
            .unwrap();
        let signature = column(&graph, block, current);
        for (from, argument) in signature {
            assert_eq!(argument, owners[usize::from(from == when_false)]);
        }
        owners.push(current);
    }
    assert!(graph.instructions.iter().all(|instruction| {
        if let Operation::Drop { owner } = instruction.operation {
            !owners.contains(&EntityId::Value(owner))
        } else {
            true
        }
    }));
}

fn scalar_join(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    entry: FunctionId,
    modes: Option<&[ParameterMode]>,
    same: bool,
) {
    let module = &program.modules[0];
    let function = module.function(entry).unwrap();
    assert_eq!(
        function.origin,
        Origin::Source(function_span(sources, parsed, "entry"))
    );
    assert_eq!(
        modes,
        Some([ParameterMode::Value, ParameterMode::Value].as_slice())
    );
    let ast_entry = one(
        parsed
            .roots()
            .iter()
            .map(|id| parsed.ast().items().get(*id).unwrap())
            .filter(|node| node.span() == function.origin.span()),
        "entry AST root",
    )
    .unwrap();
    let Item::Function { parameters, .. } = ast_entry.payload() else {
        unreachable!()
    };
    assert_eq!(parameters.len(), 2);
    for parameter in parameters {
        assert!(matches!(
            parameter.mode_marker,
            Some(ParameterModeMarker::Own(_))
        ));
    }
    let parameters = &graph_entry(function).parameters;
    assert_eq!(parameters.len(), 2);
    assert!(
        parameters
            .iter()
            .all(|entity| matches!(function.entity(*entity).unwrap().ty, EntityType::Value(_)))
    );
    let graph = Graph::new(function).unwrap();
    let (condition, when_true, when_false) = conditional(sources, parsed, &graph);
    assert_eq!(condition, parameters[1]);
    let mut calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| matches!(node.payload(), Expression::Call { .. }).then_some(node))
        .map(|node| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                unreachable!()
            };
            assert_eq!(
                sources.slice(parsed.ast().expressions().get(*callee).unwrap().span()),
                Ok("take")
            );
            assert_eq!(arguments.len(), 2);
            one(
                graph.instructions.iter().copied().filter(|instruction| {
                    instruction.origin == Origin::Source(node.span())
                        && matches!(instruction.operation, Operation::DirectCall { .. })
                }),
                "actual Call AST operation",
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|call| call.origin.span().start());
    assert_eq!(calls.len(), if same { 2 } else { 1 });
    if same {
        assert!(calls[0].origin.span().end() < calls[1].origin.span().start());
    }
    let arguments = calls
        .iter()
        .map(|call| {
            let Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } = &call.operation
            else {
                unreachable!()
            };
            assert!(receiver.is_none());
            assert_eq!(
                module.function(*callee).unwrap().origin,
                Origin::Source(function_span(sources, parsed, "take"))
            );
            assert_eq!(arguments.len(), 2);
            arguments
        })
        .collect::<Vec<_>>();
    let result = one(
        calls.last().unwrap().results.iter().copied(),
        "last call result",
    )
    .unwrap();
    let returned = one(
        graph
            .blocks
            .iter()
            .filter_map(|block| match &graph.term(*block).unwrap().kind {
                TerminatorKind::Return { values } => Some(values),
                _ => None,
            }),
        "actual caller Return",
    )
    .unwrap();
    assert_eq!(
        returned
            .iter()
            .copied()
            .map(EntityId::Value)
            .collect::<Vec<_>>(),
        [result]
    );
    let first = arguments[0][0];
    let result = arguments[0][1];
    if same {
        assert_eq!(
            arguments[1][1],
            one(calls[0].results.iter().copied(), "first call result use").unwrap()
        );
    }
    graph
        .incoming_from(calls[0].block, &[when_true, when_false])
        .unwrap();
    let signature = column(&graph, calls[0].block, result);
    assert_eq!(signature.len(), 2);
    let pending = column(&graph, calls[0].block, first);
    assert_eq!(
        pending.iter().map(|(from, _)| *from).collect::<Vec<_>>(),
        signature.iter().map(|(from, _)| *from).collect::<Vec<_>>()
    );
    for ((from, value), (_, prior)) in signature.iter().zip(&pending) {
        graph.anchored(*prior, &[parameters[0]]).unwrap();
        if same || *from == when_true {
            assert_eq!(value, prior);
        } else {
            let zero = graph.definition(*value).unwrap();
            assert_eq!(zero.block, when_false);
            assert!(matches!(
                zero.operation,
                Operation::Constant(ScalarConstant::Integer(0))
            ));
            assert_eq!(sources.slice(zero.origin.span()), Ok("0"));
            assert_ne!(value, prior);
        }
    }
    if same {
        assert_eq!(calls[1].block, calls[0].block);
        let named = arguments[1][0];
        assert_ne!(
            named, result,
            "same signatures do not merge independent prefix seeds"
        );
        assert_eq!(column(&graph, calls[0].block, named), signature);
        let earliest = graph
            .block(calls[0].block)
            .unwrap()
            .parameters
            .iter()
            .copied()
            .find(|parameter| column(&graph, calls[0].block, *parameter) == signature)
            .unwrap();
        assert_eq!(
            first, earliest,
            "pending associates with the first whole matching column"
        );
    } else {
        assert_ne!(
            first, result,
            "a one-edge alias cannot collapse complete columns"
        );
    }
}

fn graph_entry(function: &Function) -> &crate::ssa::model::Block {
    function.block(function.entry_block().unwrap()).unwrap()
}
