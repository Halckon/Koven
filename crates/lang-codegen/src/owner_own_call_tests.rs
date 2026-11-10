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
