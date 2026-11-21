//! Ordinary Unit nullable lowering and source-boundary contracts.
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey, SymbolKind},
    source::SourceMap,
    type_checking::standard_environments,
};

use super::model::{EntityId, LoanKind, Operation, TerminatorKind};

#[path = "unit_lower_nullable_tests/x01.rs"]
mod x01;

#[test]
fn original_nullable_class_borrow_nested_if_reaches_verified_native() {
    x01::run_original();
}

#[test]
fn derived_nullable_class_null_path_skips_read_and_physical_release() {
    x01::run_null_control();
}

// Ten unchanged lowering.ko sources retain the historical compiler-success oracle
// through sixteen independent Unit roots, including unreachable helpers.
// This does not assert whole-file/native or exact CFG equivalence.
fn verify_original_assertion_roots(case: &str, text: &str, roots: &[&str], callees: &[&str]) {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

    let source = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("original assertion source admission: {error:?}\n{text}"));
    source.assert_frontend_clean();
    assert!(source.ownership().deferred().is_empty());
    let [document] = source.names().names().index().source_units() else {
        panic!("one original MemoryDocument for {case}");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), source.parsed().source_id());
    assert_eq!(
        source.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(source.sources().source_text(document.source_id()), Ok(text));
    for &root in roots {
        let declaration = source.declaration(root, SymbolKind::Function);
        let span = source.declaration_span(declaration);
        let (program, entry) = source.lower_entry(declaration).unwrap_or_else(|error| {
            panic!("original assertion root {root} lowering: {error:?}\n{text}")
        });
        let module = program.module(entry.module()).expect("entry's own Program");
        let function = module.function(entry).expect("real root function");
        assert_eq!(function.origin.span(), span, "original root {root}");
        if root == "inspect" {
            for &callee in callees {
                let declaration = source.declaration(callee, SymbolKind::Function);
                let span = source.declaration_span(declaration);
                let emitted = program
                    .modules
                    .iter()
                    .flat_map(|module| &module.functions)
                    .filter(|function| function.origin.span() == span)
                    .collect::<Vec<_>>();
                let [target] = emitted.as_slice() else {
                    panic!("original callee {callee} must be emitted exactly once: {text}");
                };
                let calls = function.instructions.iter().filter(|instruction| matches!(
                    instruction.operation,
                    Operation::DirectCall { callee: id, receiver: None, .. } if id == target.id
                )).collect::<Vec<_>>();
                let [call] = calls.as_slice() else {
                    panic!("{case}: inspect calls original {callee} exactly once");
                };
                assert_eq!(call.origin.span().source_id(), document.source_id());
                assert!(
                    source
                        .sources()
                        .slice(call.origin.span())
                        .unwrap()
                        .starts_with(&format!("{callee}("))
                );
                assert_eq!(
                    source
                        .sources()
                        .slice(target.origin.span())
                        .unwrap()
                        .split('(')
                        .next(),
                    Some(format!("fun {callee}").as_str())
                );
            }
        }
        if root == "inspect" {
            assert_original_assertion_semantics(case, text, function);
        }
        super::verify::verify_program(&program).unwrap_or_else(|error| {
            panic!("original assertion root {root} SSA verification: {error:?}\n{text}")
        });
        // Parameterized roots remain real functions; no fabricated no-argument native entry.
        crate::llvm::render_verified_program(&program).unwrap_or_else(|error| {
            panic!("original assertion root {root} complete LLVM: {error:?}\n{text}")
        });
    }
}

#[test]
fn non_null_assertion_preserves_pending_value_arguments_across_its_branch() {
    // The first owner has been evaluated, but only the eventual call consumes it.
    let analysis = String::from(
        "class Node()\nfun take(own first: Node, own second: Node): Node = first\nfun inspect(own first: Node, own second: Node?): Node = take(first, second!!)",
    );
    verify_original_assertion_roots("pending", &analysis, &["inspect"], &["take"]);
}

#[test]
fn non_null_assertion_temporary_group_and_borrowed_call_results_keep_one_owner() {
    for (case, body) in [
        ("temporary", "fun inspect(): Node = make()!!"),
        ("grouped", "fun inspect(own node: Node?): Node = ((node))!!"),
        (
            "borrowed",
            "fun inspect(own node: Node?): Unit = observe(Node(), node!!)",
        ),
        (
            "boolean",
            "fun inspect(own node: Node?, flag: Boolean): Node = (if (flag) { node } else { make() })!!",
        ),
        (
            "nullable",
            "fun inspect(own probe: Node?, own node: Node?): Node = (if (probe == null) { node } else { make() })!!",
        ),
    ] {
        let analysis = format!(
            "class Node()\nfun make(): Node? {{ val result: Node? = Node()\nreturn result }}\nfun observe(first: Node, second: Node): Unit {{}}\n{body}"
        );
        // The old whole-file planner also emitted helpers not called by inspect.
        let (roots, callees): (&[&str], &[&str]) = match case {
            "temporary" | "boolean" | "nullable" => (&["inspect", "observe"], &["make"]),
            "grouped" => (&["inspect", "make", "observe"], &[]),
            "borrowed" => (&["inspect", "make"], &["observe"]),
            _ => unreachable!("five original bodies"),
        };
        verify_original_assertion_roots(case, &analysis, roots, callees);
    }
}

#[test]
fn non_null_assertion_later_transfer_cleans_pending_value_owner() {
    for (case, body) in [
        (
            "return",
            "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node = take(first, (if (flag) { return Node() } else { second })!!)",
        ),
        (
            "break",
            "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node { loop { val result = take(first, (if (flag) { break } else { second })!!)\nbreak }\nreturn Node() }",
        ),
        (
            "continue",
            "fun inspect(flag: Boolean): Node { loop { val first = Node()\nval second: Node? = Node()\nval result = take(first, (if (flag) { continue } else { second })!!)\nbreak }\nreturn Node() }",
        ),
        (
            "nested-break",
            "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node = take(first, (if (flag) { loop { break }\nsecond } else { second })!!)",
        ),
    ] {
        let analysis = format!(
            "class Node()\nfun take(own first: Node, own second: Node): Node = first\n{body}"
        );
        verify_original_assertion_roots(case, &analysis, &["inspect"], &["take"]);
    }
}

// Follow real CFG argument columns, never assume arena IDs or block numbering.
fn transported_entities(
    function: &super::model::Function,
    seed: EntityId,
) -> std::collections::BTreeSet<EntityId> {
    use super::model::Edge;
    let mut entities = std::collections::BTreeSet::from([seed]);
    loop {
        let previous = entities.len();
        for block in &function.blocks {
            let edges: Vec<&Edge> = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                } => vec![when_null, when_non_null],
                _ => vec![],
            };
            for edge in edges {
                for (argument, parameter) in edge
                    .arguments
                    .iter()
                    .zip(&function.block(edge.target).unwrap().parameters)
                {
                    if entities.contains(argument) {
                        entities.insert(*parameter);
                    }
                }
            }
        }
        if entities.len() == previous {
            return entities;
        }
    }
}

fn assert_original_assertion_semantics(case: &str, text: &str, function: &super::model::Function) {
    use super::model::{AbortReason, Origin};
    let branches = function
        .blocks
        .iter()
        .filter_map(|block| {
            let terminator = block.terminator.as_ref().unwrap();
            let span = terminator.origin.span();
            if text[span.start()..span.end()].ends_with("!!")
                && let TerminatorKind::NullableBranch {
                    owner,
                    when_null,
                    when_non_null,
                    view,
                } = &terminator.kind
            {
                return Some((span, *owner, when_null, when_non_null, *view));
            }
            None
        })
        .collect::<Vec<_>>();
    let [(span, owner, null, success, proof)] = branches.as_slice() else {
        panic!("{case}: exactly one source-qualified assertion branch");
    };
    assert_eq!(span.source_id(), function.origin.span().source_id());
    if matches!(case, "temporary" | "boolean" | "nullable") {
        let calls = function
            .instructions
            .iter()
            .filter(|item| matches!(item.operation, Operation::DirectCall { .. }))
            .collect::<Vec<_>>();
        let [make] = calls.as_slice() else {
            panic!("original make() evaluates once");
        };
        assert!(transported_entities(function, make.results[0]).contains(&EntityId::Value(*owner)));
    }
    if matches!(case, "grouped" | "boolean" | "nullable") {
        let column = usize::from(case == "nullable");
        assert!(
            transported_entities(function, function.blocks[0].parameters[column])
                .contains(&EntityId::Value(*owner)),
            "original node reaches assertion, not an unrelated owner"
        );
    }
    let failure = function.block(null.target).unwrap();
    assert!(
        failure.instructions.is_empty(),
        "abort does not unwind owners"
    );
    let abort = failure.terminator.as_ref().unwrap();
    assert_eq!(abort.kind, TerminatorKind::Abort);
    assert_eq!(abort.origin, Origin::Source(*span));
    assert_eq!(
        abort.abort,
        Some(AbortReason::Text("null assertion failed".into()))
    );
    let success_block = function.block(success.target).unwrap();
    assert_eq!(
        success_block.parameters.last(),
        Some(&EntityId::Loan(*proof))
    );
    let owner_columns = success
        .arguments
        .iter()
        .enumerate()
        .filter(|(_, value)| **value == EntityId::Value(*owner))
        .collect::<Vec<_>>();
    let [(column, _)] = owner_columns.as_slice() else {
        panic!("{case}: nullable owner is carried exactly once");
    };
    assert_eq!(null.arguments, success.arguments);
    let EntityId::Value(rebound) = success_block.parameters[*column] else {
        panic!("rebound nullable owner");
    };
    let takes = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }))
        .collect::<Vec<_>>();
    let [take] = takes.as_slice() else {
        panic!("{case}: one consuming extraction");
    };
    assert_eq!(take.block, success.target);
    assert_eq!(take.origin, Origin::Source(*span));
    assert_eq!(
        take.operation,
        Operation::NullableTake {
            owner: rebound,
            proof: *proof
        }
    );
    let [result @ EntityId::Value(_)] = take.results.as_slice() else {
        panic!("one asserted owner");
    };
    let operations = success_block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let take_at = operations
        .iter()
        .position(|item| item.id == take.id)
        .unwrap();
    assert!(
        !operations[..take_at]
            .iter()
            .any(|item| matches!(item.operation, Operation::Drop { owner } if owner == rebound))
    );

    if matches!(
        case,
        "pending" | "return" | "break" | "continue" | "nested-break"
    ) {
        let first = if case == "continue" {
            let start = text.find("val first = Node()").unwrap() + "val first = ".len();
            let allocations = function
                .instructions
                .iter()
                .filter(|item| {
                    item.origin.span().start() == start
                        && matches!(item.operation, Operation::HeapAllocate { .. })
                })
                .collect::<Vec<_>>();
            let [allocation] = allocations.as_slice() else {
                panic!("original pending Node() evaluated once");
            };
            allocation.results[0]
        } else {
            function.blocks[0].parameters[0]
        };
        assert!(matches!(first, EntityId::Value(_)));
        let carried = transported_entities(function, first);
        let pending_columns = success
            .arguments
            .iter()
            .enumerate()
            .filter(|(_, entity)| carried.contains(entity))
            .collect::<Vec<_>>();
        let [(pending_column, _)] = pending_columns.as_slice() else {
            panic!("{case}: one pending owner column");
        };
        assert_ne!(pending_column, column);
        let calls = operations
            .iter()
            .enumerate()
            .filter(|(_, item)| matches!(item.operation, Operation::DirectCall { .. }))
            .collect::<Vec<_>>();
        let [(call_at, call)] = calls.as_slice() else {
            panic!("one take call after extraction");
        };
        assert!(*call_at > take_at);
        let Operation::DirectCall {
            receiver,
            arguments,
            ..
        } = &call.operation
        else {
            unreachable!()
        };
        assert_eq!(*receiver, None);
        assert_eq!(
            arguments,
            &[success_block.parameters[*pending_column], *result]
        );
        assert_ne!(
            arguments[0], first,
            "pending value must use successor identity"
        );
        let drops = function.instructions.iter().filter(|item| matches!(item.operation, Operation::Drop { owner } if carried.contains(&EntityId::Value(owner)))).collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            usize::from(matches!(case, "return" | "break" | "continue")),
            "{case}: expected abandoned-owner count for this original source"
        );
        // This is a source-case count/identity oracle, not a separate proof of
        // which return/break/continue edge executed. The verifier remains required.
        for drop in drops {
            assert_ne!(
                drop.block, success.target,
                "successful delivery never drops pending owner"
            );
            let block = function.block(drop.block).unwrap();
            assert!(!block.instructions.iter().any(|id| matches!(
                function.instruction(*id).unwrap().operation,
                Operation::DirectCall { .. }
            )));
        }
    } else if case == "borrowed" {
        let calls = operations
            .iter()
            .enumerate()
            .filter(|(_, item)| matches!(item.operation, Operation::DirectCall { .. }))
            .collect::<Vec<_>>();
        let [(call_at, call)] = calls.as_slice() else {
            panic!("one observe call");
        };
        assert!(*call_at > take_at);
        let Operation::DirectCall {
            arguments,
            receiver: None,
            ..
        } = &call.operation
        else {
            panic!("ordinary observe call");
        };
        let [EntityId::Loan(first), EntityId::Loan(second)] = arguments.as_slice() else {
            panic!("both observe arguments are borrowed");
        };
        assert_ne!(first, second);
        let allocations = function
            .instructions
            .iter()
            .filter(|item| matches!(item.operation, Operation::HeapAllocate { .. }))
            .collect::<Vec<_>>();
        let [allocation] = allocations.as_slice() else {
            panic!("one pending Node() temporary");
        };
        assert_eq!(
            &text[allocation.origin.span().start()..allocation.origin.span().end()],
            "Node()"
        );
        let EntityId::Value(temporary) = allocation.results[0] else {
            panic!("temporary owner");
        };
        let places = function
            .instructions
            .iter()
            .filter(|item| item.operation == Operation::RootPlace { owner: temporary })
            .collect::<Vec<_>>();
        let [place] = places.as_slice() else {
            panic!("temporary root place");
        };
        let EntityId::Place(place) = place.results[0] else {
            unreachable!()
        };
        let loans = function
            .instructions
            .iter()
            .filter(|item| {
                item.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            })
            .collect::<Vec<_>>();
        let [loan] = loans.as_slice() else {
            panic!("pending temporary borrow begins once");
        };
        let loans = transported_entities(function, loan.results[0]);
        assert!(
            loans.contains(&EntityId::Loan(*first)),
            "observe borrows the pending temporary, not the extracted second owner"
        );
        for (seed, loan) in [(EntityId::Value(temporary), first), (*result, second)] {
            let owners = transported_entities(function, seed);
            let drops = operations.iter().enumerate().filter(|(_, item)| matches!(item.operation, Operation::Drop { owner } if owners.contains(&EntityId::Value(owner)))).collect::<Vec<_>>();
            let [(drop_at, _)] = drops.as_slice() else {
                panic!("each borrowed temporary owner drops once after the call");
            };
            let ends = operations
                .iter()
                .enumerate()
                .filter(|(_, item)| item.operation == Operation::BorrowEnd { loan: *loan })
                .collect::<Vec<_>>();
            let [(end_at, _)] = ends.as_slice() else {
                panic!("each exact call loan ends once");
            };
            assert!(
                *call_at < *end_at && *end_at < *drop_at,
                "call, exact loan end, then its owner drop"
            );
        }
        let borrow = operations
            .iter()
            .find(|item| item.results == [EntityId::Loan(*second)])
            .unwrap();
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = borrow.operation
        else {
            panic!("borrow extracted owner");
        };
        assert!(operations.iter().any(|item| item.results == [EntityId::Place(place)] && matches!(item.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == *result)));
    } else {
        assert!(matches!(
            case,
            "temporary" | "grouped" | "boolean" | "nullable"
        ));
        let TerminatorKind::Return { values } = &success_block.terminator.as_ref().unwrap().kind
        else {
            panic!("asserted owner returns from success");
        };
        let EntityId::Value(result) = result else {
            unreachable!()
        };
        assert_eq!(values, &[*result]);
        assert!(
            !operations.iter().any(
                |item| matches!(item.operation, Operation::Drop { owner } if owner == *result)
            )
        );
    }
}

#[test]
fn original_nullable_node_tail_move_stops_at_unsupported_result() {
    let original = include_str!(
        "../../../lang-frontend/tests/ownership_nullable_when/fixtures/move-node-root.ko"
    );
    assert_original_nullable_boundary(
        original,
        "nullable-node-move.ko",
        "extract(item)",
        "when (x) { null -> Node(); else -> x }",
    );
}

#[test]
fn original_nullable_named_borrow_stops_at_unsupported_subject() {
    let original = include_str!(
        "../../../lang-frontend/tests/ownership_nullable_when/fixtures/named-borrow-read.ko"
    );
    assert_original_nullable_boundary(
        original,
        "named-borrow-read.ko",
        "test(item)",
        "when (x) { null -> {}; else -> read(x) }",
    );
}

fn assert_original_nullable_boundary(
    original: &str,
    path: &str,
    invocation: &str,
    expected_expression: &str,
) {
    use super::{
        lowering_support::LoweringErrorKind,
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, parsed},
    };
    use lang_frontend::name_resolution::SourceUnitKey;

    // Keep the accepted original declaration untouched. The wrapper only makes it reachable.
    for rooted in [false, true] {
        for initializer in ["null", "Node()"] {
            let text = format!(
                "{original}\nfun entry(): Unit {{ val item: Node? = {initializer}; val result = {invocation} }}"
            );
            let mut sources = SourceMap::new();
            let (source, file) = parsed(&mut sources, path, &text);
            assert_eq!(
                sources.source_text(source).unwrap().get(..original.len()),
                Some(original)
            );
            let inputs = [if rooted {
                SourceUnitInput::new("root", path, source, &file)
            } else {
                SourceUnitInput::memory_document(source, &file)
            }];
            let (name_environment, environment) = standard_environments();
            let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
            let index = names.names().index();
            let [source_entry] = index.source_units() else {
                panic!("one original source");
            };
            assert_eq!(source_entry.source_id(), source);
            assert_eq!(
                matches!(source_entry.key(), SourceUnitKey::MemoryDocument),
                !rooted
            );
            let entry = index
                .declarations()
                .iter()
                .find(|declaration| declaration.name() == "entry")
                .expect("real reachable entry")
                .id();
            let error = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
            )
            .err()
            .expect("boxed nullable When shape is not admitted to Unit SSA");
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            let span = error.span.expect("the exact original expression");
            assert_eq!(span.source_id(), source);
            assert_eq!(span.start(), original.find("when").unwrap());
            assert_eq!(span.end(), original.len());
            assert_eq!(sources.slice(span), Ok(expected_expression));
        }
    }
}

#[path = "unit_lower_nullable_tests/partial_owned_call.rs"]
mod partial_owned_call;

#[test]
fn original_nullable_field_projection_stops_at_unsupported_subject() {
    let text = include_str!(
        "../../../lang-frontend/tests/ownership_nullable_when/fixtures/unstable-field.ko"
    );
    assert_projected_when_boundary(text, "unstable-field.ko", "test(Holder(null))");
}

#[test]
fn original_nullable_index_projection_stops_at_unsupported_subject() {
    let text = include_str!(
        "../../../lang-frontend/tests/ownership_nullable_when/fixtures/unstable-index.ko"
    );
    assert_projected_when_boundary(text, "unstable-index.ko", "test(listOf<Node?>(null))");
}

fn assert_projected_when_boundary(original: &str, path: &str, invoke: &str) {
    use super::{
        LoweringErrorKind,
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitKey, type_checking::complete_compilation_unit_types,
    };

    let text = format!("{original}\nfun entry(): Unit {{ {invoke} }}");
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, path, &text);
    assert_eq!(
        sources.source_text(source).unwrap().get(..original.len()),
        Some(original)
    );
    let (name_environment, environment) = standard_environments();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", path, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        complete_compilation_unit_types(&sources, &inputs, &names, &environment, typed.types())
            .unwrap();
        let index = names.names().index();
        let [unit] = index.source_units() else {
            panic!("one real source")
        };
        assert_eq!(unit.source_id(), source);
        assert_eq!(matches!(unit.key(), SourceUnitKey::MemoryDocument), !rooted);
        let entry = index
            .declarations()
            .iter()
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let error = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .err()
        .unwrap_or_else(|| panic!("requires an actual Unit witness for {path}, rooted={rooted}"));
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let span = error.span.unwrap();
        let start = original.find("when (").unwrap();
        assert_eq!(span.source_id(), source);
        assert_eq!(sources.slice(span), Ok(&original[start..]));
    }
}
