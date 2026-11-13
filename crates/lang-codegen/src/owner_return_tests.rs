//! Ordinary finite original sources; no mutation, Abort, or resource-counter execution.
use super::UnitSource;
use crate::ssa::{model::TerminatorKind, verify::verify_program};
const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(flag) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
#[test]
fn selected_owner_actual_return_requires_product_authorization() {
    let source = UnitSource::analyze("owner-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, returns) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .is_ok_and(|s| s.starts_with("fun run("))
        })
        .unwrap();
    assert_eq!(run.blocks.iter().filter(|b| matches!(b.terminator.as_ref().map(|t| &t.kind), Some(TerminatorKind::Return { values }) if values.len() == 1)).count(), 1);
    let [event] = returns.as_slice() else {
        panic!("one actual explicit Return")
    };
    assert!(event.1.is_some());
    assert_eq!(
        event.2, event.1,
        "actual Return must consume the P/Q payload authorization"
    );
}

#[test]
fn selected_owner_return_both_units_deliver_path_value_and_caller_cleans_up() {
    assert_return_transport(SOURCE);
}

pub(super) fn assert_return_transport(input: &str) {
    use super::OwnedProducts;
    use crate::ssa::model::{EntityId, Function, Operation};
    fn edge(f: &Function, b: crate::ssa::model::BlockId) -> &crate::ssa::model::Edge {
        let TerminatorKind::Branch(edge) = &f.block(b).unwrap().terminator.as_ref().unwrap().kind
        else {
            panic!("exit branch")
        };
        edge
    }
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                input.replace("run(true)", "run(ON)")
            )
        } else {
            input.to_owned()
        };
        let source = UnitSource::analyze("owner-return-dual.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let owned = match &source.products {
            OwnedProducts::Basic(_, o) => o.ownership(),
            OwnedProducts::Constant(_, o) => o.ownership(),
        };
        let boundaries = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.replace_break_observation_v1())
            .collect::<Vec<_>>();
        let [boundary] = boundaries.as_slice() else {
            panic!("one return product")
        };
        assert!(boundary.suffix_consumer_v1().is_none());
        assert!(boundary.suffix_call_consumer_v1().is_none());
        let consumer = boundary.suffix_return_consumer_v1().unwrap();
        let ((lowered, returns), actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()))
        });
        let (program, entry) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let [receipt] = returns.as_slice() else {
            panic!("one actual Return")
        };
        assert_eq!(receipt.0, consumer.return_expression());
        let selected_slot = receipt.2.unwrap();
        assert_eq!(receipt.1, Some(selected_slot));
        assert_eq!(actions, [("root", selected_slot), ("take", selected_slot)]);
        let functions = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .collect::<Vec<_>>();
        let named = |prefix: &str| {
            *functions
                .iter()
                .find(|f| {
                    source
                        .sources
                        .slice(f.origin.span())
                        .is_ok_and(|s| s.starts_with(prefix))
                })
                .unwrap()
        };
        let run = named("fun run(");
        let caller = named("fun entry(");
        let literal = |bytes: &[u8]| {
            run.instructions.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes: actual, .. } if actual == bytes)).unwrap()
        };
        let initial = literal(b"outer");
        let replacement = literal(b"next");
        let branches = run
            .blocks
            .iter()
            .filter_map(|b| match &b.terminator.as_ref()?.kind {
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => Some((when_true, when_false)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(first, second)] = branches.as_slice() else {
            panic!("two exact runtime alternatives")
        };
        let exchange = run
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::RootReplace { .. }))
            .unwrap();
        // condition_internal lowers Not by swapping CFG targets. Locate the body by
        // the actual Replace block, never by final branch truth or catalog order.
        let (body, no_body) = match (
            first.target == exchange.block,
            second.target == exchange.block,
        ) {
            (true, false) => (*first, *second),
            (false, true) => (*second, *first),
            _ => panic!("exactly one condition edge enters the actual Replace body"),
        };
        assert_eq!(body.arguments, initial.results);
        assert_eq!(no_body.arguments, initial.results);
        let Operation::RootReplace {
            owner,
            replacement: rhs,
            ..
        } = exchange.operation
        else {
            unreachable!()
        };
        assert_eq!(
            EntityId::Value(owner),
            run.block(body.target).unwrap().parameters[0]
        );
        assert_eq!(EntityId::Value(rhs), replacement.results[0]);
        let from_break = edge(run, exchange.block);
        let from_false = edge(run, no_body.target);
        assert_eq!(from_break.target, from_false.target);
        assert_eq!(from_break.arguments, [exchange.results[0]]);
        assert_eq!(
            from_false.arguments,
            run.block(no_body.target).unwrap().parameters
        );
        let exit = run.block(from_break.target).unwrap();
        let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
            panic!("actual payload terminator")
        };
        assert_eq!(values.len(), 1);
        assert_eq!(exit.parameters, [EntityId::Value(values[0])]);
        let local_drops = run
            .instructions
            .iter()
            .filter_map(|i| match i.operation {
                Operation::Drop { owner } => Some((i, owner)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(old_drop, old_owner)] = local_drops.as_slice() else {
            panic!("returning function only drops detached P")
        };
        assert_eq!(EntityId::Value(*old_owner), exchange.results[1]);
        assert_eq!(old_drop.block, exchange.block);
        let received = caller.instructions.iter().filter(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == run.id())).collect::<Vec<_>>();
        assert_eq!(received.len(), 2);
        let caller_drops = caller
            .instructions
            .iter()
            .filter_map(|i| match i.operation {
                Operation::Drop { owner } => Some(EntityId::Value(owner)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(caller_drops.len(), 2);
        for call in received {
            assert_eq!(call.results.len(), 1);
            assert_eq!(
                caller_drops
                    .iter()
                    .filter(|v| **v == call.results[0])
                    .count(),
                1
            );
        }
    }
}

#[test]
fn selected_owner_return_runs_both_edges_with_existing_caller_cleanup() {
    use super::OwnedProducts;
    let text = SOURCE.replace(
        "val first = run(false); val second = run(true)",
        "val first = run(false); println(first); val second = run(true); println(second)",
    );
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("run(true)", "run(ON)")
            )
        } else {
            text.clone()
        };
        let source = UnitSource::analyze("owner-return-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::assert_native_output(&source, b"outer\nnext\n");
    }
}

// Mutated actions are compile-only locally, reserved for official CI.
#[test]
fn ci_only_owner_return_rejects_replay_wrong_payload_and_missing_commit() {
    use crate::ssa::OwnerFault::{DuplicateOwnerReturn, MissingOwnerReturn, WrongReturnPayload};
    for fault in [DuplicateOwnerReturn, WrongReturnPayload, MissingOwnerReturn] {
        let source = UnitSource::analyze("ci-owner-return.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        assert!(
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry())).is_err()
        );
    }
}

pub(crate) mod owner_replace_break_ci_tests {
    //! CI-only identity, edge corruption and physical String Drop counter proof.
    //! Compile locally; never execute these tests or their subprocesses locally.
    use super::super::{OwnedProducts, UnitSource, owner_replace_break_downstream_tests::SOURCE};
    use crate::ssa::{model::SsaTypeKind, verify::verify_program};
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::{const_owned_compilation_unit_view, owned_compilation_unit_view},
    };
    use std::{fs, os::unix::process::ExitStatusExt, process::Command};

    #[test]
    fn ci_only_bounded_owner_foreign_analysis_products_cannot_construct_unit() {
        for constant in [false, true] {
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    SOURCE.replace("run(true)", "run(ON)")
                )
            } else {
                SOURCE.to_owned()
            };
            let source = UnitSource::analyze("local-owner.ko", &text).unwrap();
            for name in ["local-owner.ko", "foreign-owner.ko"] {
                let foreign = UnitSource::analyze(name, &text).unwrap();
                let inputs = [SourceUnitInput::memory_document(
                    source.parsed.source_id(),
                    &source.parsed,
                )];
                match (&source.products, &foreign.products) {
                    (OwnedProducts::Basic(typed, own), OwnedProducts::Basic(_, foreign_owned)) => {
                        assert!(own.ownership().owner_observations().iter().any(|o| {
                            o.replace_break_observation_v1()
                                .and_then(|b| b.suffix_consumer_v1())
                                .is_some()
                        }));
                        assert!(
                            owned_compilation_unit_view(
                                &source.sources,
                                &inputs,
                                &source.names,
                                &source.environment,
                                typed,
                                foreign_owned
                            )
                            .is_err()
                        );
                    }
                    (
                        OwnedProducts::Constant(typed, own),
                        OwnedProducts::Constant(_, foreign_owned),
                    ) => {
                        assert!(own.ownership().owner_observations().iter().any(|o| {
                            o.replace_break_observation_v1()
                                .and_then(|b| b.suffix_consumer_v1())
                                .is_some()
                        }));
                        assert!(
                            const_owned_compilation_unit_view(
                                &source.sources,
                                &inputs,
                                &source.names,
                                &source.environment,
                                typed,
                                foreign_owned
                            )
                            .is_err()
                        );
                    }
                    _ => panic!("matching actual capabilities"),
                }
            }
        }
    }

    #[test]
    fn ci_only_bounded_owner_missing_foreign_and_reversed_incoming_roles_fail_closed() {
        use crate::ssa::OwnerFault::{ForeignIncoming, MissingIncoming, ReversedIncomingRoles};
        for fault in [MissingIncoming, ForeignIncoming, ReversedIncomingRoles] {
            let source = UnitSource::analyze("broken-owner-edge.ko", SOURCE).unwrap();
            let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
            assert!(
                result.is_err(),
                "selected plan must reject a missing, unrelated or wrongly assigned predecessor"
            );
        }
    }

    #[derive(Clone, Copy)]
    enum Mutation {
        None,
        DuplicateDrop,
        OmitDrop,
    }

    #[test]
    fn ci_only_bounded_owner_literal_drop_counts_cover_both_edges_and_units() {
        for constant in [false, true] {
            for flag in [false, true] {
                let llvm = llvm_for_edge(constant, flag);
                let output = counted(&llvm.0, llvm.1, flag, Mutation::None);
                assert!(output.status.success(), "{output:?}");
                assert!(
                    output.stdout.is_empty() && output.stderr.is_empty(),
                    "{output:?}"
                );
            }
        }
    }

    #[test]
    fn ci_only_bounded_owner_counter_rejects_actual_duplicate_and_missing_drop_calls() {
        for flag in [false, true] {
            let llvm = llvm_for_edge(false, flag);
            for mutation in [Mutation::DuplicateDrop, Mutation::OmitDrop] {
                let output = counted(&llvm.0, llvm.1, flag, mutation);
                assert_eq!(
                    output.status.signal(),
                    Some(6),
                    "counter must intentionally abort: {output:?}"
                );
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("bounded-owner-count-mismatch"),
                    "failure must come from the count oracle: {output:?}"
                );
            }
        }
    }

    fn llvm_for_edge(constant: bool, flag: bool) -> (String, usize) {
        // Change only the caller. The strictly bounded run function remains byte-for-byte unchanged.
        let (run, _) = SOURCE.split_once("\nfun entry").unwrap();
        let text = if constant {
            format!("const val FLAG: Boolean = {flag}\n{run}\nfun entry(): Unit {{ run(FLAG) }}")
        } else {
            format!("{run}\nfun entry(): Unit {{ run({flag}) }}")
        };
        let source = UnitSource::analyze("counted-b-owner.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let ((program, entry), drops) =
            crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()).unwrap());
        assert_eq!(
            drops.iter().filter(|drop| drop.3.is_some()).count(),
            1,
            "the counted program must use the real B consumer"
        );
        verify_program(&program).unwrap();
        let string = program.modules[0]
            .types
            .iter()
            .position(|ty| *ty == SsaTypeKind::StringOwner)
            .unwrap();
        (
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap(),
            string,
        )
    }

    fn counted(llvm: &str, string: usize, flag: bool, mutation: Mutation) -> std::process::Output {
        let mut instrumented = String::new();
        let mut parameter = None;
        let mut destructors = 0;
        let mut sites = 0;
        for line in llvm.lines() {
            let line = line
                .replace("@malloc(", "@counted_malloc(")
                .replace("@free(", "@counted_free(");
            let drop_call = line.contains(&format!("call void @koven.drop.t{string}("));
            if drop_call {
                sites += 1;
            }
            if !(drop_call && matches!(mutation, Mutation::OmitDrop)) {
                instrumented.push_str(&line);
                instrumented.push('\n');
            }
            if drop_call && matches!(mutation, Mutation::DuplicateDrop) {
                instrumented.push_str(&line);
                instrumented.push('\n');
            }
            if line.starts_with("define ") && line.contains(&format!("@koven.drop.t{string}(")) {
                parameter = Some(
                    line.split_once('(')
                        .unwrap()
                        .1
                        .split_once(')')
                        .unwrap()
                        .0
                        .to_owned(),
                );
            } else if parameter.is_some() && line.ends_with(':') {
                let owner = parameter.take().unwrap();
                let bits = usize::BITS;
                instrumented.push_str(&format!("  %count.bytes = extractvalue {owner}, 0\n  %count.length = extractvalue {owner}, 1\n  %count.capacity = extractvalue {owner}, 2\n  call void @counted_owner_drop(ptr %count.bytes, i{bits} %count.length, i{bits} %count.capacity)\n"));
                destructors += 1;
            }
        }
        assert_eq!(
            destructors, 1,
            "instrument the real String destructor exactly once"
        );
        assert_eq!(
            sites, 2,
            "exactly detached P and selected-owner physical sites"
        );
        let bits = usize::BITS;
        instrumented.push_str(&format!(
            "declare void @counted_owner_drop(ptr, i{bits}, i{bits})\n"
        ));
        let directory = crate::native_tests::TestDirectory::create();
        let ir = directory.join("bounded-owner.ll");
        let counter = directory.join("counter.c");
        let executable = directory.join("bounded-owner");
        fs::write(&ir, instrumented).unwrap();
        fs::write(
            &counter,
            format!("#define EXPECT_Q {}\n{COUNTER}", u8::from(flag)),
        )
        .unwrap();
        let linked = Command::new(crate::test_support::ir_clang())
            .arg(&ir)
            .arg(&counter)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            linked.status.success(),
            "LLVM/C counter must link before execution: {linked:?}"
        );
        Command::new(executable).output().unwrap()
    }

    const COUNTER: &str = r#"
#include <assert.h>
#include <stddef.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
static unsigned p_drops, q_drops, allocations, releases;
void counted_owner_drop(const void *bytes, size_t length, size_t capacity) {
    /* The bounded shape has literals, not heap-allocated Strings or Resource. */
    assert(capacity == 0);
    if (length == 5 && memcmp(bytes, "outer", 5) == 0) ++p_drops;
    else if (length == 4 && memcmp(bytes, "next", 4) == 0) ++q_drops;
    else abort();
}
void *counted_malloc(size_t size) { ++allocations; return malloc(size); }
void counted_free(void *value) { ++releases; free(value); }
__attribute__((destructor)) static void verify_counts(void) {
    if (p_drops != 1 || q_drops != EXPECT_Q) {
        fputs("bounded-owner-count-mismatch\n", stderr);
        abort();
    }
    assert(allocations == 0 && releases == 0);
}
"#;
}

pub(crate) mod owner_alias_return_tests {
    //! One immutable alias carries the same selected P/Q payload through a normal Return.
    use super::super::UnitSource;
    use crate::ssa::{model::TerminatorKind, verify::verify_program};

    const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    val forwarded = held
    return forwarded
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

    #[test]
    fn alias_return_actual_payload_requires_product_authorization() {
        let source = UnitSource::analyze("owner-alias-return.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        let (lowered, returns) =
            crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
        let (program, entry) =
            lowered.expect("actual selected alias payload reaches old Return emitter");
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let [receipt] = returns.as_slice() else {
            panic!("one explicit Return");
        };
        assert!(receipt.1.is_some());
        assert_eq!(receipt.1, receipt.2);
    }

    #[test]
    fn alias_return_both_units_preserve_cfg_alias_and_cleanup() {
        let input = SOURCE;
        use super::super::OwnedProducts;
        use crate::ssa::model::{EntityId, Function, Operation};
        fn edge(f: &Function, b: crate::ssa::model::BlockId) -> &crate::ssa::model::Edge {
            let TerminatorKind::Branch(edge) =
                &f.block(b).unwrap().terminator.as_ref().unwrap().kind
            else {
                panic!("exit branch")
            };
            edge
        }
        for constant in [false, true] {
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    input.replace("run(true)", "run(ON)")
                )
            } else {
                input.to_owned()
            };
            let source = UnitSource::analyze("owner-return-dual.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            let owned = match &source.products {
                OwnedProducts::Basic(_, o) => o.ownership(),
                OwnedProducts::Constant(_, o) => o.ownership(),
            };
            let boundaries = owned
                .owner_observations()
                .iter()
                .filter_map(|o| o.replace_break_observation_v1())
                .collect::<Vec<_>>();
            let [boundary] = boundaries.as_slice() else {
                panic!("one return product")
            };
            assert!(boundary.suffix_consumer_v1().is_none());
            assert!(boundary.suffix_call_consumer_v1().is_none());
            let consumer = boundary.suffix_return_consumer_v1().unwrap();
            let ((lowered, returns), actions) = crate::ssa::observe_owner_actions(|| {
                crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()))
            });
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let [receipt] = returns.as_slice() else {
                panic!("one actual Return")
            };
            assert_eq!(receipt.0, consumer.return_expression());
            let selected_slot = receipt.2.unwrap();
            assert_eq!(receipt.1, Some(selected_slot));
            let root_slot = actions.first().unwrap().1;
            assert_ne!(
                root_slot, selected_slot,
                "actual Alias creates a distinct refine slot"
            );
            assert_eq!(
                actions,
                [
                    ("root", root_slot),
                    ("take", root_slot),
                    ("alias_bind", selected_slot),
                    ("alias_take", selected_slot)
                ]
            );
            assert_eq!(consumer.actions().len(), 4);
            let functions = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .collect::<Vec<_>>();
            let named = |prefix: &str| {
                *functions
                    .iter()
                    .find(|f| {
                        source
                            .sources
                            .slice(f.origin.span())
                            .is_ok_and(|s| s.starts_with(prefix))
                    })
                    .unwrap()
            };
            let run = named("fun run(");
            let caller = named("fun entry(");
            let literal = |bytes: &[u8]| {
                run.instructions.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes: actual, .. } if actual == bytes)).unwrap()
            };
            let initial = literal(b"outer");
            let replacement = literal(b"next");
            let branches = run
                .blocks
                .iter()
                .filter_map(|b| match &b.terminator.as_ref()?.kind {
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => Some((when_true, when_false)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(first, second)] = branches.as_slice() else {
                panic!("two exact runtime alternatives")
            };
            let exchange = run
                .instructions
                .iter()
                .find(|i| matches!(i.operation, Operation::RootReplace { .. }))
                .unwrap();
            // condition_internal lowers Not by swapping CFG targets. Locate the body by
            // the actual Replace block, never by final branch truth or catalog order.
            let (body, no_body) = match (
                first.target == exchange.block,
                second.target == exchange.block,
            ) {
                (true, false) => (*first, *second),
                (false, true) => (*second, *first),
                _ => panic!("exactly one condition edge enters the actual Replace body"),
            };
            assert_eq!(body.arguments, initial.results);
            assert_eq!(no_body.arguments, initial.results);
            let Operation::RootReplace {
                owner,
                replacement: rhs,
                ..
            } = exchange.operation
            else {
                unreachable!()
            };
            assert_eq!(
                EntityId::Value(owner),
                run.block(body.target).unwrap().parameters[0]
            );
            assert_eq!(EntityId::Value(rhs), replacement.results[0]);
            let from_break = edge(run, exchange.block);
            let from_false = edge(run, no_body.target);
            assert_eq!(from_break.target, from_false.target);
            assert_eq!(from_break.arguments, [exchange.results[0]]);
            assert_eq!(
                from_false.arguments,
                run.block(no_body.target).unwrap().parameters
            );
            let exit = run.block(from_break.target).unwrap();
            let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
                panic!("actual payload terminator")
            };
            assert_eq!(values.len(), 1);
            assert_eq!(exit.parameters, [EntityId::Value(values[0])]);
            let local_drops = run
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some((i, owner)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(old_drop, old_owner)] = local_drops.as_slice() else {
                panic!("returning function only drops detached P")
            };
            assert_eq!(EntityId::Value(*old_owner), exchange.results[1]);
            assert_eq!(old_drop.block, exchange.block);
            let received = caller.instructions.iter().filter(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == run.id())).collect::<Vec<_>>();
            assert_eq!(received.len(), 2);
            let caller_drops = caller
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some(EntityId::Value(owner)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(caller_drops.len(), 2);
            for call in received {
                assert_eq!(call.results.len(), 1);
                assert_eq!(
                    caller_drops
                        .iter()
                        .filter(|v| **v == call.results[0])
                        .count(),
                    1
                );
            }
        }
    }

    #[test]
    fn alias_return_native_delivers_both_edges_once() {
        use super::super::OwnedProducts;
        let text = SOURCE.replace(
            "val first = run(false); val second = run(true)",
            "val first = run(false); println(first); val second = run(true); println(second)",
        );
        for constant in [false, true] {
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                )
            } else {
                text.clone()
            };
            let source = UnitSource::analyze("owner-return-native.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            super::super::assert_native_output(&source, b"outer\nnext\n");
        }
    }

    // Only compiled locally; explicit action corruption belongs to official CI.
    #[test]
    fn ci_only_alias_return_rejects_repeated_binding_take_and_return() {
        use crate::ssa::OwnerFault::{
            DuplicateBind, DuplicateOwnerReturn, DuplicateTake, MissingOwnerReturn,
            WrongReturnPayload,
        };
        for fault in [
            DuplicateTake,
            DuplicateBind,
            DuplicateOwnerReturn,
            MissingOwnerReturn,
            WrongReturnPayload,
        ] {
            let source = UnitSource::analyze("alias-return-ci.ko", SOURCE).unwrap();
            source.assert_frontend_clean();
            assert!(
                crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry())).is_err()
            );
        }
    }
}
