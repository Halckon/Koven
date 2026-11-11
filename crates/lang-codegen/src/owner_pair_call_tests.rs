//! Finite normal source tests for selected owner and Copy Value operand mapping.
use super::UnitSource;
use crate::ssa::{model::Operation, verify::verify_program};
fn cases() -> [(&'static str, &'static str, usize); 4] {
    [
        ("own item: String, own count: Int", "held, 7", 0),
        ("own count: Int, own item: String", "7, held", 1),
        (
            "own item: String, own count: Int",
            "count = 7, item = held",
            0,
        ),
        (
            "own count: Int, own item: String",
            "item = held, count = 7",
            1,
        ),
    ]
}
fn source(parameters: &str, arguments: &str, output: bool) -> String {
    let body = if output {
        r#"println(item); if (count == 7) { println("7") } else { println("wrong") }"#
    } else {
        ""
    };
    format!(
        r#"fun sink({parameters}): Unit {{ {body} }}
fun run(flag: Boolean) {{ var held = "outer"; while(flag) {{ val previous = replace(&held, "next"); break }}; sink({arguments}) }}
fun entry(): Unit {{ run(false); run(true) }}"#
    )
}
#[test]
fn pair_call_both_units_preserve_selected_slot_and_cleanup() {
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
    for (parameters, arguments, selected_index) in cases() {
        let source_text = source(parameters, arguments, false);
        for constant in [false, true] {
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    source_text.replace("run(true)", "run(ON)")
                )
            } else {
                source_text.clone()
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
            assert_eq!(receipt.2.len(), 2);
            assert_eq!(receipt.2[selected_index], selected_slot);
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
            assert_eq!(arguments.len(), 2);
            assert_eq!(arguments[selected_index], exit.parameters[0]);
            assert_ne!(arguments[1 - selected_index], exit.parameters[0]);
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
                sink.block(sink.entry_block().unwrap()).unwrap().parameters[selected_index]
            );
        }
    }
}

#[test]
fn pair_call_native_preserves_values_and_parameter_order() {
    for (parameters, arguments, _) in cases() {
        for constant in [false, true] {
            let mut text = source(parameters, arguments, true);
            if constant {
                text = format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                );
            }
            let source = UnitSource::analyze("pair-native.ko", &text).unwrap();
            source.assert_frontend_clean();
            super::assert_native_output(&source, b"outer\n7\nnext\n7\n");
        }
    }
}

// Deliberate mutations are compiled locally; execution belongs to official CI only.
#[test]
fn ci_only_pair_call_rejects_wrong_selected_slot_and_replayed_call() {
    use crate::ssa::OwnerFault::{DuplicateOwnCall, MissingOwnCall, WrongOwnCallSlot};
    for (parameters, arguments, _) in cases() {
        for fault in [DuplicateOwnCall, MissingOwnCall, WrongOwnCallSlot] {
            let text = source(parameters, arguments, false);
            let source = UnitSource::analyze("pair-ci.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert!(
                crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry())).is_err()
            );
        }
    }
}
