//! Normal no-carry sources, without native execution or manufactured evidence.
use super::*;
use crate::ssa::{refine::fresh_loop_receipt, unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn fresh_body_lease_closes_one_loop_across_source_and_parameter_order() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            check(
                reverse_sources,
                reverse_parameters,
                "consume(item)",
                (0, 0),
                true,
            );
        }
    }
}
#[test]
fn fresh_source_segments_preserve_entry_and_exit_source_consumption() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            check(
                reverse_sources,
                reverse_parameters,
                "consume(item)",
                (1, 1),
                true,
            );
        }
    }
}
#[test]
fn two_ordered_consumers_share_one_fresh_result_until_the_last_end() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            for uses in ["consume(item);other(item)", "other(item);consume(item)"] {
                check(reverse_sources, reverse_parameters, uses, (0, 0), true);
            }
        }
    }
}
#[test]
fn two_body_uses_and_outer_source_segments_keep_distinct_lifetimes() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            for uses in ["consume(item);other(item)", "other(item);consume(item)"] {
                check(reverse_sources, reverse_parameters, uses, (1, 1), true);
            }
        }
    }
}
#[test]
fn single_sided_source_segments_bind_the_exact_entry_or_exit() {
    for segments in [(1, 0), (0, 1)] {
        for reverse_sources in [false, true] {
            for reverse_parameters in [false, true] {
                for uses in [
                    "consume(item)",
                    "consume(item);other(item)",
                    "other(item);consume(item)",
                ] {
                    check(reverse_sources, reverse_parameters, uses, segments, true);
                }
            }
        }
    }
}
#[test]
fn original_longer_use_sets_and_longer_segments_remain_outside_fresh_domain() {
    check(
        false,
        false,
        "consume(item);other(item);consume(item)",
        (0, 0),
        false,
    );
    check(
        false,
        false,
        "consume(item);other(item);consume(item)",
        (1, 1),
        false,
    );
    for segments in [(1, 0), (0, 1)] {
        check(
            false,
            false,
            "consume(item);other(item);consume(item)",
            segments,
            false,
        );
    }
    for segments in [(2, 0), (0, 2), (2, 1), (1, 2)] {
        check(false, false, "consume(item)", segments, false);
        check(false, false, "consume(item);other(item)", segments, false);
    }
}
fn check(
    reverse_sources: bool,
    reverse_parameters: bool,
    uses: &str,
    segments: (usize, usize),
    certified: bool,
) {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "fresh-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}\nfun before(item:String):Unit {}\nfun after(item:String):Unit {}",
    );
    let parameters = if reverse_parameters {
        "flag:Boolean,source:String"
    } else {
        "source:String,flag:Boolean"
    };
    let before = "before(source);".repeat(segments.0);
    let after = "after(source);".repeat(segments.1);
    let text = format!(
        "fun run({parameters}):Unit {{{before}while(flag){{borrow val item=view(source);{uses}}};{after}}}"
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "fresh-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "fresh-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "fresh-caller.ko", caller_id, &caller),
    ];
    if reverse_sources {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    let facts = owned.ownership().no_carry_loops();
    assert_eq!(facts.len(), 1);
    assert!(owned.ownership().owns_no_carry_loop(&facts[0]));
    let ((results, audits), receipts) = fresh_loop_receipt::observe(|| {
        observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        declaration("run"),
                    )
                })
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(audits.len(), 2);
    assert_eq!(
        receipts.len(),
        2,
        "original route must still publish its genuine receipt"
    );
    for ((result, audit), receipt) in results.into_iter().zip(audits).zip(receipts) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        receipt.validate(function).unwrap();
        assert_eq!(receipt.loops.len(), 1);
        assert!(receipt.loops[0].fact.same_identity(&facts[0]));
        assert_eq!(receipt.loops[0].fact, facts[0]);
        assert!(
            function.blocks.len() >= 4,
            "entry, condition, body and zero-iteration exit"
        );
        let edge = (declaration("run"), declaration("view"));
        if certified {
            assert!(audit.borrow_edges.contains(&edge));
            assert!(
                audit
                    .direct_edges
                    .contains(&(declaration("run"), declaration("consume")))
            );
            if facts[0].consumers().len() == 2 {
                use crate::ssa::{model::EntityId, refine::fresh_loop_receipt::Tag};
                use lang_frontend::ownership_checking::UnitDropPoint;
                let local = &receipt.loops[0];
                let instruction = |tag| {
                    let ids: Vec<_> = local
                        .instructions
                        .iter()
                        .filter(|(t, _)| *t == tag)
                        .collect();
                    assert_eq!(ids.len(), 1);
                    function.instruction(ids[0].1).unwrap()
                };
                let acquire = instruction(Tag::Result);
                let [result] = acquire.results.as_slice() else {
                    panic!("one fresh loan");
                };
                let first = instruction(Tag::Consumer(0));
                let second = instruction(Tag::Consumer(1));
                let end = instruction(Tag::ResultEnd);
                assert_eq!(
                    facts[0].end(),
                    UnitDropPoint::AfterStatement(facts[0].consumers()[1].0)
                );
                let body = function.block(acquire.block).unwrap();
                let position = |id| body.instructions.iter().position(|v| *v == id).unwrap();
                assert!(
                    position(first.id) < position(second.id)
                        && position(second.id) < position(end.id)
                );
                for (call, name) in [first, second].into_iter().zip(uses.split(';')) {
                    let Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    } = &call.operation
                    else {
                        panic!("body consumer");
                    };
                    assert_eq!(arguments.as_slice(), [*result]);
                    let target = program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .find(|f| f.id == *callee)
                        .unwrap();
                    let marker = format!(".{}.", name.split('(').next().unwrap());
                    assert!(target.name.contains(&marker));
                }
                assert_eq!(function.instructions.iter().filter(|i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==*result)).count(),1);
                assert!(
                    matches!(end.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==*result)
                );
                assert!(
                    receipt
                        .edges
                        .iter()
                        .all(|(_, edge)| !edge.arguments.contains(result))
                );
            }
            if matches!(segments, (1, 0) | (0, 1)) {
                let side = usize::from(segments.1 == 1);
                assert_eq!(receipt.segments.len(), 2);
                assert!(receipt.segments[1 - side].instructions.is_empty());
                assert!(!audit.direct_edges.contains(&(
                    declaration("run"),
                    declaration(if side == 0 { "after" } else { "before" })
                )));
                let exits: Vec<_> = function
                    .blocks
                    .iter()
                    .filter(|b| {
                        matches!(
                            b.terminator.as_ref().map(|t| &t.kind),
                            Some(TerminatorKind::Return { .. })
                        )
                    })
                    .collect();
                assert_eq!(exits.len(), 1);
                assert_eq!(
                    exits[0].parameters.len(),
                    side,
                    "only suffix consumption keeps source live at exit"
                );

                let [id] = receipt.segments[side].instructions.as_slice() else {
                    panic!("one exact outer call");
                };
                let actual = function.instruction(*id).unwrap();
                assert_eq!(Some(actual.block) == function.entry_block(), side == 0);
                if side == 1 {
                    assert_eq!(actual.block, exits[0].id);
                }

                let Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                } = &actual.operation
                else {
                    panic!("source consumer");
                };
                let target = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.id == *callee)
                    .unwrap();
                assert!(
                    target
                        .name
                        .contains(if side == 0 { ".before." } else { ".after." })
                );
                let [argument] = arguments.as_slice() else {
                    panic!("source argument");
                };
                assert!(
                    function
                        .block(actual.block)
                        .unwrap()
                        .parameters
                        .contains(argument)
                );
                let fresh = receipt.loops[0]
                    .instructions
                    .iter()
                    .find(|(tag, _)| *tag == fresh_loop_receipt::Tag::Result)
                    .unwrap()
                    .1;
                assert!(
                    !function
                        .instruction(fresh)
                        .unwrap()
                        .results
                        .contains(argument)
                );
                let source = function
                    .block(function.entry_block().unwrap())
                    .unwrap()
                    .parameters[usize::from(reverse_parameters)];
                assert_eq!(
                    function.entity(*argument).unwrap().ty,
                    function.entity(source).unwrap().ty
                );
                let loans: Vec<_> = owned
                    .ownership()
                    .loans()
                    .iter()
                    .filter(|l| l.call().source_unit() == facts[0].callable().source_unit())
                    .collect();
                assert_eq!(loans.len(), 2 + facts[0].consumers().len());
                assert_eq!(
                    loans
                        .iter()
                        .map(|l| l.call())
                        .collect::<BTreeSet<_>>()
                        .len(),
                    loans.len()
                );
            }
            if segments == (1, 1) {
                if facts[0].consumers().len() == 2 {
                    let original: Vec<_> = owned
                        .ownership()
                        .loans()
                        .iter()
                        .filter(|l| l.call().source_unit() == facts[0].callable().source_unit())
                        .collect();
                    assert_eq!(
                        original.len(),
                        5,
                        "initializer, two body and two outer consumers"
                    );
                    let calls: BTreeSet<_> = original.iter().map(|l| l.call()).collect();
                    assert_eq!(
                        calls.len(),
                        5,
                        "each original acquisition has its own source call"
                    );
                    assert_eq!(
                        function
                            .instructions
                            .iter()
                            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                            .count(),
                        4
                    );
                }
                assert!(
                    audit
                        .direct_edges
                        .contains(&(declaration("run"), declaration("before")))
                );
                assert!(
                    audit
                        .direct_edges
                        .contains(&(declaration("run"), declaration("after")))
                );
                assert_eq!(receipt.segments.len(), 2);
                assert_eq!(receipt.segments[0].instructions.len(), 1);
                assert_eq!(receipt.segments[1].instructions.len(), 1);
                let first = function
                    .instruction(receipt.segments[0].instructions[0])
                    .unwrap();
                let last = function
                    .instruction(receipt.segments[1].instructions[0])
                    .unwrap();
                assert_eq!(Some(first.block), function.entry_block());
                assert_ne!(first.block, last.block);
                let source_index = usize::from(reverse_parameters);
                let entry_source = function
                    .block(function.entry_block().unwrap())
                    .unwrap()
                    .parameters[source_index];
                for (instruction, name) in [(first, ".before."), (last, ".after.")] {
                    let Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    } = &instruction.operation
                    else {
                        panic!("actual segment call");
                    };
                    let target = program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .find(|f| f.id == *callee)
                        .unwrap();
                    assert!(target.name.contains(name), "{}", target.name);
                    let [argument] = arguments.as_slice() else {
                        panic!("one source argument");
                    };
                    let fresh = receipt.loops[0]
                        .instructions
                        .iter()
                        .find(|(tag, _)| *tag == fresh_loop_receipt::Tag::Result)
                        .unwrap()
                        .1;
                    let [fresh_result] = function.instruction(fresh).unwrap().results.as_slice()
                    else {
                        panic!("one fresh result");
                    };
                    assert_ne!(
                        argument, fresh_result,
                        "outer source loan is never the fresh body result"
                    );
                    assert!(
                        function
                            .block(instruction.block)
                            .unwrap()
                            .parameters
                            .contains(argument)
                    );
                    assert_eq!(
                        function.entity(*argument).unwrap().ty,
                        function.entity(entry_source).unwrap().ty
                    );
                }
            }
            assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        } else {
            assert!(!audit.borrow_edges.contains(&edge));
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.starts_with("fresh no-carry missing/duplicate")),
                "{:?}",
                audit.unsupported
            );
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.contains("unclassified") && g.contains("BorrowCall")),
                "{:?}",
                audit.unsupported
            );
        }
    }
}

#[test]
fn two_sequential_fresh_scopes_are_certified_as_one_complete_caller() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            check_sequence(
                reverse_sources,
                reverse_parameters,
                2,
                [0, 0, 0],
                false,
                true,
            );
        }
    }
}

#[test]
fn longer_or_interrupted_fresh_sequences_keep_the_whole_group_unproven() {
    check_sequence(false, false, 3, [0, 0, 0], false, false);
    for segments in [[2, 0, 0], [0, 2, 0], [0, 0, 2], [1, 2, 1]] {
        check_sequence(false, false, 2, segments, false, false);
    }
    check_sequence(false, false, 2, [0, 0, 0], true, false);
    check_sequence(false, false, 2, [1, 1, 1], true, false);
}

#[test]
fn middle_source_consumer_connects_two_independent_fresh_scopes() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            check_sequence(
                reverse_sources,
                reverse_parameters,
                2,
                [0, 1, 0],
                false,
                true,
            );
        }
    }
}

#[test]
fn three_source_segments_preserve_entry_middle_and_exit_lifetimes() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            for mask in 0..8 {
                let segments = [mask & 1, (mask >> 1) & 1, (mask >> 2) & 1];
                check_sequence(
                    reverse_sources,
                    reverse_parameters,
                    2,
                    segments,
                    false,
                    true,
                );
            }
        }
    }
}

fn check_sequence(
    reverse_sources: bool,
    reverse_parameters: bool,
    count: usize,
    segments: [usize; 3],
    second_longer_use: bool,
    certified: bool,
) {
    let second = if second_longer_use {
        "other,consume,other"
    } else {
        "other"
    };
    check_sequence_uses(
        reverse_sources,
        reverse_parameters,
        count,
        segments,
        ["consume", second],
        certified,
    );
}

#[test]
fn two_scopes_consume_their_own_results_in_independent_orders() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            for uses in [
                ["consume", "other"],
                ["consume", "other,consume"],
                ["other,consume", "other"],
                ["consume,other", "other,consume"],
                ["other,consume", "consume,other"],
                ["consume,consume", "other,other"],
            ] {
                for segments in [[0, 0, 0], [1, 1, 1]] {
                    check_sequence_uses(
                        reverse_sources,
                        reverse_parameters,
                        2,
                        segments,
                        uses,
                        true,
                    );
                }
            }
        }
    }
    for uses in [
        ["consume,other,consume", "other"],
        ["consume", "other,consume,other"],
        ["consume,other,consume", "other,consume"],
    ] {
        check_sequence_uses(false, false, 2, [1, 1, 1], uses, false);
    }
}

fn check_sequence_uses(
    reverse_sources: bool,
    reverse_parameters: bool,
    count: usize,
    segments: [usize; 3],
    uses: [&str; 2],
    certified: bool,
) {
    use crate::ssa::{model::EntityId, refine::fresh_loop_receipt::Tag};
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "sequence-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}\nfun middle(item:String):Unit {}\nfun before(item:String):Unit {}\nfun after(item:String):Unit {}",
    );
    let parameters = if reverse_parameters {
        "flag:Boolean,source:String"
    } else {
        "source:String,flag:Boolean"
    };
    let mut body = "before(source);".repeat(segments[0]);
    body.push_str("while(flag){borrow val item=view(source);");
    for name in uses[0].split(',') {
        body.push_str(&format!("{name}(item);"));
    }
    body.push_str("};");
    body.push_str(&"middle(source);".repeat(segments[1]));
    body.push_str("while(flag){borrow val second=next(source);");
    for name in uses[1].split(',') {
        body.push_str(&format!("{name}(second);"));
    }
    body.push_str("};");
    if count == 3 {
        body.push_str("while(flag){borrow val third=view(source);consume(third)}");
    }
    body.push_str(&"after(source);".repeat(segments[2]));
    let (caller_id, caller) = unit_lower_test_support::parsed(
        &mut sources,
        "sequence-caller.ko",
        &format!("fun run({parameters}):Unit {{{body}}}"),
    );
    let mut inputs = [
        SourceUnitInput::new("root", "sequence-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "sequence-caller.ko", caller_id, &caller),
    ];
    if reverse_sources {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    let facts = owned.ownership().no_carry_loops();
    assert_eq!(facts.len(), count);
    let ((results, audits), receipts) = fresh_loop_receipt::observe(|| {
        observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        declaration("run"),
                    )
                })
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(audits.len(), 2);
    assert_eq!(
        receipts.len(),
        2,
        "all normal sources retain the original multi-loop receipt"
    );
    for ((result, audit), receipt) in results.into_iter().zip(audits).zip(receipts) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        receipt.validate(function).unwrap();
        assert_eq!(receipt.loops.len(), count);
        for local in &receipt.loops {
            let originals: Vec<_> = facts
                .iter()
                .filter(|f| f.loop_statement() == local.fact.loop_statement())
                .collect();
            let [original] = originals.as_slice() else {
                panic!("unique original loop");
            };
            assert!(original.same_identity(&local.fact));
            assert_eq!(**original, local.fact);
        }
        let source_edges = [
            (declaration("run"), declaration("view")),
            (declaration("run"), declaration("next")),
        ];
        if certified {
            assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
            assert!(
                source_edges
                    .iter()
                    .all(|edge| audit.borrow_edges.contains(edge))
            );
            assert_eq!(receipt.segments.len(), 3);
            assert_eq!(
                receipt
                    .segments
                    .iter()
                    .map(|s| s.instructions.len())
                    .collect::<Vec<_>>(),
                segments
            );
            if segments == [0, 0, 0] {
                assert!(receipt.has_empty_segments(3));
            }
            let mut results = Vec::new();
            for (local, (source_name, consumer_name)) in receipt
                .loops
                .iter()
                .zip([("view", uses[0]), ("next", uses[1])])
            {
                let instruction = |tag| {
                    let ids: Vec<_> = local
                        .instructions
                        .iter()
                        .filter(|(t, _)| *t == tag)
                        .map(|(_, id)| *id)
                        .collect();
                    assert_eq!(ids.len(), 1);
                    function.instruction(ids[0]).unwrap()
                };
                let acquire = instruction(Tag::Result);
                let [result] = acquire.results.as_slice() else {
                    panic!("one fresh result");
                };
                let end = instruction(Tag::ResultEnd);
                let Operation::BorrowCall { callee, .. } = acquire.operation else {
                    panic!("source call");
                };
                assert!(
                    program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .find(|f| f.id == callee)
                        .unwrap()
                        .name
                        .contains(&format!(".{source_name}."))
                );
                let block = function.block(acquire.block).unwrap();
                let position = |id| block.instructions.iter().position(|v| *v == id).unwrap();
                let mut previous = position(acquire.id);
                let names: Vec<_> = consumer_name.split(',').collect();
                assert_eq!(local.fact.consumers().len(), names.len());
                for (i, name) in names.iter().enumerate() {
                    let consumer = instruction(Tag::Consumer(i));
                    let Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    } = &consumer.operation
                    else {
                        panic!("consumer call");
                    };
                    assert_eq!(arguments.as_slice(), [*result]);
                    assert_eq!(consumer.block, acquire.block);
                    assert!(
                        program
                            .modules
                            .iter()
                            .flat_map(|m| &m.functions)
                            .find(|f| f.id == *callee)
                            .unwrap()
                            .name
                            .contains(&format!(".{name}."))
                    );
                    assert!(position(consumer.id) > previous);
                    previous = position(consumer.id);
                }
                assert!(position(end.id) > previous);
                assert!(
                    matches!(end.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == *result)
                );
                assert_eq!(function.instructions.iter().filter(|i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==*result)).count(), 1);
                assert!(
                    receipt
                        .edges
                        .iter()
                        .all(|(_, edge)| !edge.arguments.contains(result))
                );
                results.push(*result);
            }
            assert_ne!(results[0], results[1]);
            for (segment_index, name) in [".before.", ".middle.", ".after."].iter().enumerate() {
                if segments[segment_index] == 0 {
                    continue;
                }
                let middle = &receipt.segments[segment_index];
                assert_eq!(middle.instructions.len(), 1);
                let call = function.instruction(middle.instructions[0]).unwrap();
                let Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                } = &call.operation
                else {
                    panic!("middle source call");
                };
                assert!(
                    program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .find(|f| f.id == *callee)
                        .unwrap()
                        .name
                        .contains(name)
                );
                let [source] = arguments.as_slice() else {
                    panic!("one source argument");
                };
                assert!(!results.contains(source));
                assert!(
                    function
                        .block(call.block)
                        .unwrap()
                        .parameters
                        .contains(source)
                );
                assert_eq!(
                    function.entity(*source).unwrap().ty,
                    function
                        .entity(
                            function
                                .block(function.entry_block().unwrap())
                                .unwrap()
                                .parameters[usize::from(reverse_parameters)]
                        )
                        .unwrap()
                        .ty
                );
                let acquisition_calls: BTreeSet<_> = owned
                    .ownership()
                    .loans()
                    .iter()
                    .filter(|loan| loan.call().source_unit() == facts[0].callable().source_unit())
                    .map(|loan| loan.call())
                    .collect();
                assert_eq!(
                    acquisition_calls.len(),
                    2 + uses.iter().map(|u| u.split(',').count()).sum::<usize>()
                        + segments.iter().sum::<usize>()
                );
                if segment_index == 0 {
                    assert_eq!(Some(call.block), function.entry_block());
                }
            }
            let exits: Vec<_> = function
                .blocks
                .iter()
                .filter(|b| {
                    matches!(
                        b.terminator.as_ref().map(|t| &t.kind),
                        Some(crate::ssa::model::TerminatorKind::Return { .. })
                    )
                })
                .collect();
            assert_eq!(exits.len(), 1);
            assert_eq!(exits[0].parameters.len(), segments[2]);
            if segments[2] == 1 {
                let call = function
                    .instruction(receipt.segments[2].instructions[0])
                    .unwrap();
                assert_eq!(call.block, exits[0].id);
            }
            assert_ne!(
                receipt.loops[0].fact.binding(),
                receipt.loops[1].fact.binding()
            );
        } else {
            assert!(
                source_edges
                    .iter()
                    .all(|edge| !audit.borrow_edges.contains(edge)),
                "no partial group publication"
            );
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.contains("sequential caller evidence gap"))
            );
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.contains("unclassified") && g.contains("BorrowCall"))
            );
        }
    }
}
