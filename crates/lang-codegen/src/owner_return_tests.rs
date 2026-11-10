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
                SOURCE.replace("run(true)", "run(ON)")
            )
        } else {
            SOURCE.to_owned()
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
