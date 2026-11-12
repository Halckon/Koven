//! Ordinary original-source owner handoff; no damaged inputs or resource counters.
use super::UnitSource;
use crate::ssa::{model::Operation, verify::verify_program};

const SOURCE: &str = "fun sink(own item: String): Unit {}\nfun run(flag: Boolean) { var held = \"outer\"; while(flag) { val previous = replace(&held, \"next\"); break }; sink(held) }\nfun entry(): Unit { run(false); run(true) }";

#[test]
fn selected_owner_actual_call_requires_product_authorization() {
    let source = UnitSource::analyze("owner-own-call.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, calls) = crate::ssa::observe_owner_calls(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let functions = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .collect::<Vec<_>>();
    let sink = functions
        .iter()
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .is_ok_and(|s| s.starts_with("fun sink("))
        })
        .unwrap();
    let run = functions
        .iter()
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .is_ok_and(|s| s.starts_with("fun run("))
        })
        .unwrap();
    let physical = run
        .instructions
        .iter()
        .filter(
            |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == sink.id()),
        )
        .collect::<Vec<_>>();
    assert_eq!(
        physical.len(),
        1,
        "the existing emitter makes the actual call"
    );
    let events = calls
        .iter()
        .filter(|c| c.1 == sink.id())
        .collect::<Vec<_>>();
    let [event] = events.as_slice() else {
        panic!("one actual sink call")
    };
    assert_eq!(event.2.len(), 1);
    assert_eq!(
        event.3,
        Some(event.2[0]),
        "actual own call must consume the selected P/Q product authorization"
    );
}

#[test]
fn selected_owner_call_both_units_preserve_path_values_and_callee_cleanup() {
    use super::OwnedProducts;
    use crate::ssa::model::{EntityId, Function, TerminatorKind};
    fn edge(function: &Function, block: crate::ssa::model::BlockId) -> &crate::ssa::model::Edge {
        let TerminatorKind::Branch(edge) = &function
            .block(block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            panic!("exit branch")
        };
        edge
    }
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                SOURCE.replace("run(true)", "run(ON)")
            )
        } else {
            SOURCE.to_owned()
        };
        let source = UnitSource::analyze("owner-own-call-dual.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let owned = match &source.products {
            OwnedProducts::Basic(_, owned) => owned.ownership(),
            OwnedProducts::Constant(_, owned) => owned.ownership(),
        };
        let boundaries = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.replace_break_observation_v1())
            .collect::<Vec<_>>();
        let [boundary] = boundaries.as_slice() else {
            panic!("one selected-owner boundary")
        };
        assert!(boundary.suffix_consumer_v1().is_none());
        let consumer = boundary.suffix_call_consumer_v1().unwrap();
        assert_ne!(
            consumer.incoming()[0].owner(),
            consumer.incoming()[1].owner()
        );
        let ((lowered, calls), actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::observe_owner_calls(|| source.lower_entry(source.entry()))
        });
        let (program, entry) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
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
        let sink = named("fun sink(");
        let selected_calls = calls
            .iter()
            .filter(|c| c.0 == consumer.call())
            .collect::<Vec<_>>();
        let [receipt] = selected_calls.as_slice() else {
            panic!("one matching call receipt")
        };
        assert_eq!(receipt.1, sink.id());
        let selected_slot = receipt.3.unwrap();
        assert_eq!(receipt.2, [selected_slot]);
        assert_eq!(actions, [("root", selected_slot), ("take", selected_slot)]);
        let initial = run.instructions.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"outer")).unwrap();
        let replacement = run.instructions.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"next")).unwrap();
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
        let [(body, no_body)] = branches.as_slice() else {
            panic!("two exact runtime alternatives")
        };
        assert_eq!(body.arguments, initial.results);
        assert_eq!(no_body.arguments, initial.results);
        let exchange = run
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::RootReplace { .. }))
            .unwrap();
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
        assert_eq!(exit.parameters.len(), 1);
        let call = run.instructions.iter().find(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == sink.id())).unwrap();
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        assert_eq!(arguments, &exit.parameters);
        assert_eq!(call.block, exit.id);
        let caller_drops = run
            .instructions
            .iter()
            .filter_map(|i| match i.operation {
                Operation::Drop { owner } => Some((i, owner)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(old_drop, old_owner)] = caller_drops.as_slice() else {
            panic!("only detached P is dropped by caller")
        };
        assert_eq!(EntityId::Value(*old_owner), exchange.results[1]);
        assert_eq!(old_drop.block, exchange.block);
        let callee_drops = sink
            .instructions
            .iter()
            .filter_map(|i| match i.operation {
                Operation::Drop { owner } => Some(owner),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(callee_drops.len(), 1);
        assert_eq!(
            EntityId::Value(callee_drops[0]),
            sink.block(sink.entry_block().unwrap()).unwrap().parameters[0]
        );
    }
}

#[test]
fn selected_owner_call_runs_both_edges_with_existing_own_cleanup() {
    use super::OwnedProducts;
    let text = SOURCE.replace(
        "fun sink(own item: String): Unit {}",
        "fun sink(own item: String): Unit { println(item) }",
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
        let source = UnitSource::analyze("owner-own-call-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::assert_native_output(&source, b"outer\nnext\n");
    }
}

// Deliberate action mutations: compile locally, run only in official CI.
#[test]
fn ci_only_selected_owner_call_rejects_replay_wrong_slot_and_missing_commit() {
    use crate::ssa::OwnerFault::{DuplicateOwnCall, MissingOwnCall, WrongOwnCallSlot};
    for fault in [DuplicateOwnCall, WrongOwnCallSlot, MissingOwnCall] {
        let source = UnitSource::analyze("ci-owner-call.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        assert!(
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry())).is_err()
        );
    }
}

pub(crate) mod owner_short_circuit_tests {
//! Ordinary finite sources and real SSA edge tracing; no mutation or foreign products.
use super::super::{OwnedProducts, UnitSource};
use crate::ssa::model::{Definition, Edge, EntityId, Function, Operation, TerminatorKind};
use std::collections::BTreeSet;
pub(crate) const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(flag && flag) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
fn edges(term: &TerminatorKind) -> Vec<&Edge> {
    match term {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::Return { .. } => vec![],
        _ => panic!("only normal finite Boolean CFG expected"),
    }
}
// Follow actual phi arguments backwards. No expected AST edge numbering is involved.
fn origins(
    f: &Function,
    value: EntityId,
    p: EntityId,
    q: EntityId,
    visiting: &mut BTreeSet<EntityId>,
) -> u8 {
    if value == p {
        return 1;
    }
    if value == q {
        return 2;
    }
    assert!(
        visiting.insert(value),
        "no loop backedge in the bare-Break source"
    );
    let Definition::BlockParameter { block, index } = f.entity(value).unwrap().definition else {
        panic!("owner transport must be the actual producer or an actual block parameter")
    };
    let mut incoming = 0;
    let mut count = 0;
    for predecessor in &f.blocks {
        for edge in edges(&predecessor.terminator.as_ref().unwrap().kind) {
            if edge.target == block {
                count += 1;
                incoming |= origins(f, edge.arguments[index], p, q, visiting);
            }
        }
    }
    assert!(count > 0);
    visiting.remove(&value);
    incoming
}
#[test]
fn short_circuit_actual_return_requires_owner_receipt() {
    let source = UnitSource::analyze("short-circuit-owner.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, events) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [event] = events.as_slice() else {
        panic!("one actual Return")
    };
    assert!(event.1.is_some());
    assert_eq!(
        event.2, event.1,
        "actual short-circuit Return needs P/Q authorization"
    );
}
#[test]
fn short_circuit_real_cfg_preserves_p_before_two_owner_exits() {
    check_cfg(&[
        "flag && flag",
        "flag || flag",
        "!((flag) && (flag))",
        "!((flag) || (flag))",
    ]);
}
pub(crate) fn check_cfg(conditions: &[&str]) {
    for condition in conditions {
        for constant in [false, true] {
            let text = SOURCE.replace("flag && flag", condition);
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                )
            } else {
                text
            };
            let source = UnitSource::analyze("short-circuit-owner-cfg.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            let ((lowered, events), actions) = crate::ssa::observe_owner_actions(|| {
                crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()))
            });
            let (program, entry) = lowered.unwrap();
            crate::ssa::verify::verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let [event] = events.as_slice() else {
                panic!("one actual Return")
            };
            let slot = event.2.unwrap();
            assert_eq!(event.1, Some(slot));
            assert_eq!(actions, [("root", slot), ("take", slot)]);
            let functions: Vec<_> = program.modules.iter().flat_map(|m| &m.functions).collect();
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
            assert_eq!(
                run.blocks
                    .iter()
                    .filter(|b| matches!(
                        b.terminator.as_ref().unwrap().kind,
                        TerminatorKind::Conditional { .. }
                    ))
                    .count(),
                2,
                "both real Boolean reads remain in the short-circuit CFG"
            );
            let literals: Vec<_> = run
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
                .collect();
            assert_eq!(literals.len(), 2);
            let p = literals.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"outer")).unwrap().results[0];
            let literal_q = literals.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"next")).unwrap().results[0];
            let replacements: Vec<_> = run
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
                .collect();
            let [replacement] = replacements.as_slice() else {
                panic!("one actual Replace")
            };
            let Operation::RootReplace {
                owner,
                replacement: rhs,
                ..
            } = replacement.operation
            else {
                unreachable!()
            };
            let q = replacement.results[0];
            let trace = |v| origins(run, v, p, q, &mut BTreeSet::new());
            assert_eq!(trace(EntityId::Value(owner)), 1);
            assert_eq!(EntityId::Value(rhs), literal_q);
            let returns: Vec<_> = run
                .blocks
                .iter()
                .filter_map(|b| match &b.terminator.as_ref()?.kind {
                    TerminatorKind::Return { values } => Some((b, values)),
                    _ => None,
                })
                .collect();
            let [(exit, values)] = returns.as_slice() else {
                panic!("one owner exit")
            };
            assert_eq!(values.len(), 1);
            assert_eq!(trace(EntityId::Value(values[0])), 3);
            let incoming: Vec<_> = run
                .blocks
                .iter()
                .flat_map(|b| {
                    edges(&b.terminator.as_ref().unwrap().kind)
                        .into_iter()
                        .filter(move |e| e.target == exit.id)
                        .map(move |e| (b, e))
                })
                .collect();
            assert_eq!(
                incoming.len(),
                2,
                "internal condition edges must converge before the owner exit"
            );
            let mut roles = Vec::new();
            for (block, edge) in incoming {
                assert!(matches!(
                    block.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Branch(_)
                ));
                assert_eq!(edge.arguments.len(), 1);
                roles.push(trace(edge.arguments[0]));
            }
            roles.sort_unstable();
            assert_eq!(roles, [1, 2]);
            let p_type = run.entity(p).unwrap().ty;
            for block in &run.blocks {
                for parameter in &block.parameters {
                    if run.entity(*parameter).unwrap().ty == p_type && block.id != exit.id {
                        assert_eq!(
                            trace(*parameter),
                            1,
                            "all pre-exit owner phi inputs remain actual P"
                        );
                    }
                }
            }
            let drops: Vec<_> = run
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some((i.block, EntityId::Value(owner))),
                    _ => None,
                })
                .collect();
            assert_eq!(drops, [(replacement.block, replacement.results[1])]);
            let calls: Vec<_> = caller.instructions.iter().filter(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == run.id())).collect();
            let drops: Vec<_> = caller
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some(EntityId::Value(owner)),
                    _ => None,
                })
                .collect();
            assert_eq!(calls.len(), 2);
            assert_eq!(drops.len(), 2);
            for call in calls {
                assert_eq!(call.results.len(), 1);
                assert_eq!(drops.iter().filter(|v| **v == call.results[0]).count(), 1);
            }
        }
    }
}
#[test]
fn short_circuit_native_preserves_skip_and_evaluated_results() {
    check_native(&[
        ("flag && flag", b"outer\nnext\n".as_slice()),
        ("flag || flag", b"outer\nnext\n".as_slice()),
        ("!((flag) && (flag))", b"next\nouter\n".as_slice()),
        ("!((flag) || (flag))", b"next\nouter\n".as_slice()),
    ]);
}
pub(crate) fn check_native(cases: &[(&str, &[u8])]) {
    for (condition, expected) in cases {
        for constant in [false, true] {
            let text = SOURCE.replace("flag && flag", condition).replace(
                "val first = run(false); val second = run(true)",
                "val first = run(false); println(first); val second = run(true); println(second)",
            );
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                )
            } else {
                text
            };
            let source = UnitSource::analyze("short-circuit-owner-native.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            super::super::assert_native_output(&source, expected);
        }
    }
}
}

pub(crate) mod owner_replace_return_tests {
//! Ordinary finite suffix Replace probe through original admitted products.
use super::super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    return replace(&held, "last")
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn suffix_replace_return_original_source_probe() {
    let source = UnitSource::analyze("suffix-replace-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    assert_suffix_shape(&source);
}

fn assert_suffix_shape(source: &UnitSource) {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    use lang_frontend::ownership_checking::UnitDropPoint;
    let suffix = "replace(&held, \"last\")";
    let drops = source
        .ownership()
        .drops()
        .iter()
        .filter(|drop| source.sources.slice(drop.value_origin()).unwrap() == suffix)
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one actual root cleanup");
    };
    let UnitDropPoint::CallReturn(call) = drop.point() else {
        panic!("R retires at CallReturn");
    };
    assert_eq!(
        source
            .sources
            .slice(
                source
                    .parsed
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .unwrap()
                    .span()
            )
            .unwrap(),
        suffix
    );
    let (program, entry) = source.lower_entry(source.entry()).unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .unwrap()
                .starts_with("fun run(")
        })
        .unwrap();
    let exchanges = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    assert_eq!(exchanges.len(), 2);
    let exchange = exchanges
        .iter()
        .find(|i| source.sources.slice(i.origin.span()).unwrap() == suffix)
        .unwrap();
    let exit = run.block(exchange.block).unwrap();
    let Operation::RootReplace { replacement, .. } = exchange.operation else {
        unreachable!()
    };
    let literal = run
        .instructions
        .iter()
        .find(
            |i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"last"),
        )
        .unwrap();
    assert_eq!(literal.results, [EntityId::Value(replacement)]);
    let drops = run
        .instructions
        .iter()
        .filter(|i| i.block == exit.id && matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one installed R drop at exit");
    };
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    assert_eq!(EntityId::Value(owner), exchange.results[0]);
    let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
        panic!("old payload Return");
    };
    assert_eq!(values.len(), 1);
    assert_eq!(EntityId::Value(values[0]), exchange.results[1]);
    let position = |id| exit.instructions.iter().position(|i| *i == id).unwrap();
    assert!(position(exchange.id) < position(drop.id));
}

#[test]
fn suffix_replace_return_actual_old_slot_requires_authorization() {
    let source = UnitSource::analyze("suffix-replace-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, returns) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("actual detached old slot requires authorized Return");
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = returns.as_slice() else {
        panic!("one explicit Return");
    };
    assert!(receipt.2.is_some());
    assert_eq!(receipt.1, receipt.2);
}

#[test]
fn suffix_replace_return_native_delivers_selected_old_value() {
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
        let source = UnitSource::analyze("suffix-replace-return-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::super::assert_native_output(&source, b"outer\nnext\n");
    }
}

#[test]
fn suffix_replace_return_both_units_authorize_distinct_old_and_installed_slots() {
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                SOURCE.replace("run(true)", "run(ON)")
            )
        } else {
            SOURCE.to_owned()
        };
        let source = UnitSource::analyze("suffix-replace-return-dual.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        assert_suffix_shape(&source);
        let ((lowered, returns), actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()))
        });
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let [receipt] = returns.as_slice() else {
            panic!("one explicit Return");
        };
        let old = receipt.2.unwrap();
        assert_eq!(receipt.1, Some(old));
        let root = actions[0].1;
        assert_ne!(root, old);
        assert_eq!(
            actions,
            [
                ("root", root),
                ("replace_result", old),
                ("replace_installed_drop", root)
            ]
        );
    }
}

// Explicit protocol faults are compiled locally and executed only by official CI.
#[test]
fn ci_only_suffix_replace_return_rejects_incomplete_or_wrong_handoffs() {
    use crate::ssa::OwnerFault::{
        DuplicateOwnerReturn, MissingFinish, MissingOwnerReturn, WrongReturnPayload,
    };
    for fault in [
        DuplicateOwnerReturn,
        MissingOwnerReturn,
        WrongReturnPayload,
        MissingFinish,
    ] {
        let source = UnitSource::analyze("suffix-replace-return-ci.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        assert!(
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry())).is_err()
        );
    }
}

#[test]
fn suffix_replace_hooks_preserve_existing_ordinary_consumers() {
    let texts = [
        SOURCE.replace("return replace(&held, \"last\")", "return held"),
        SOURCE.replace(
            "return replace(&held, \"last\")",
            "val forwarded = held; return forwarded",
        ),
        SOURCE
            .replace(
                "fun run(flag: Boolean): String",
                "fun run(flag: Boolean): Unit",
            )
            .replace("return replace(&held, \"last\")", "val retired = held"),
        format!(
            "fun sink(own value: String, own count: Int): Unit {{}}\n{}",
            SOURCE
                .replace(
                    "fun run(flag: Boolean): String",
                    "fun run(flag: Boolean): Unit"
                )
                .replace("return replace(&held, \"last\")", "sink(held, 7)")
        ),
    ];
    for (index, text) in texts.iter().enumerate() {
        let source = UnitSource::analyze("existing-owner-consumer.ko", text).unwrap();
        source.assert_frontend_clean();
        let boundaries = source
            .ownership()
            .owner_observations()
            .iter()
            .filter_map(|o| o.replace_break_observation_v1())
            .collect::<Vec<_>>();
        let [boundary] = boundaries.as_slice() else {
            panic!("one original closed consumer");
        };
        match index {
            0 | 1 => assert!(boundary.suffix_return_consumer_v1().is_some()),
            2 => assert!(boundary.suffix_consumer_v1().is_some()),
            3 => assert!(boundary.suffix_call_consumer_v1().is_some()),
            _ => unreachable!(),
        }
        let (program, entry) = source.lower_entry(source.entry()).unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    }
}

// CI-only: never include these mutations in a local execution selection.
fn reject_suffix_receipt_fault(fault: crate::ssa::OwnerFault) {
    let source = UnitSource::analyze("suffix-receipt-ci.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
    let Err(error) = result else {
        panic!("missing or invalid protocol receipt must fail loud");
    };
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::InvalidModel);
}

#[test]
fn ci_only_suffix_return_rejects_missing_replace_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::MissingReturnReplaceReceipt);
}

#[test]
fn ci_only_suffix_return_rejects_duplicate_replace_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::DuplicateReturnReplaceReceipt);
}

#[test]
fn ci_only_suffix_return_rejects_old_is_root() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::ReturnReplaceOldIsRoot);
}

#[test]
fn ci_only_suffix_return_rejects_drop_is_old() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::ReturnReplaceDropIsOld);
}

#[test]
fn ci_only_suffix_return_rejects_missing_drop_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::MissingReturnReplaceDropReceipt);
}

#[test]
fn ci_only_suffix_return_rejects_duplicate_drop_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::DuplicateReturnReplaceDropReceipt);
}
}
