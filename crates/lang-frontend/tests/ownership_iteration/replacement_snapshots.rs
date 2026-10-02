use super::*;

#[test]
fn closure_reassignment_preserves_new_capture_and_releases_old_capture() {
    for tail in [
        "val releasedOld = consume(xs)\nval used = f()\nval releasedNew = consume(ys)",
        "val used = f()\nval releasedOld = consume(xs)\nval releasedNew = consume(ys)",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>) {{ var f: () -> Unit = {{ read(xs) }}\nf = ({{ read(ys) }})\n{tail} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{tail}: {:?}",
            owned.diagnostics()
        );
    }
    for rhs in [
        "({ read(xs) })",
        "if (flag) ({ read(xs) }) else ({ read(ys) })",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(xs: List<Int>, ys: List<Int>, flag: Boolean): () -> Unit {{ var f: () -> Unit = {{}}\nf = {rhs}\nreturn f }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
            "assigned closure cannot hide its borrowed origin: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn assigned_closure_retains_its_source_until_the_new_value_is_dropped() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) { var f: () -> Unit = {}\nf = ({ read(xs) })\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find_map(|capture| match capture.source() {
            ClosureCaptureSource::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(source))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "{drops:?}");
    assert_eq!(
        drops[0].point(),
        DropPoint::CallReturn(call),
        "assignment must transfer captures to the new binding"
    );
}

#[test]
fn closure_reassignment_tracks_aliases_unused_values_and_field_escape() {
    for update in [
        "f = g\nval used = f()",
        "f = if (flag) g else g\nval used = f()",
        "f = g",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ var f: () -> Unit = {{}}\nval g: () -> Unit = {{ read(xs) }}\n{update}\nval released = consume(xs) }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{update}: {:?}",
            owned.diagnostics()
        );
    }
    let (_, _, owned) = checked(
        "class Sink(var callback: () -> Unit) {}\nfun read(xs: List<Int>) {}\nfun store(sink: Sink, xs: List<Int>, flag: Boolean) { sink.callback = if (flag) ({ read(xs) }) else ({ read(xs) }) }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
        "conditional field RHS must not hide borrowed captures: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn closure_reassignment_keeps_old_capture_until_the_entire_rhs_finishes() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { val old = f()\nval invalid = consume(xs)\nval replacement: () -> Unit = { read(ys) }\nreplacement } else ({ read(ys) })\nval used = f() }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
        "RHS has not delivered a replacement, so the old environment still borrows xs: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn replacement_cleanup_waits_for_rhs_completion_or_leaving_scope() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for tail in [
        "val replacement: () -> Unit = { read(ys) }\nreplacement",
        "return",
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) {{ var f: () -> Unit = {{ read(xs) }}\nf = if (flag) {{ val observed = f()\n{tail} }} else ({{ read(ys) }})\nval used = f() }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{tail}: {:?}",
            owned.diagnostics()
        );
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol)
                    if sources.slice(capture.reference_span()).unwrap() == "xs" =>
                {
                    Some(symbol)
                }
                _ => None,
            })
            .unwrap();
        let rhs = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(node.payload(), lang_frontend::parser::Expression::If { .. }).then_some(id)
            })
            .unwrap();
        let old_call = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()).unwrap() == "f()")
            .min_by_key(|(_, node)| node.span().start())
            .unwrap()
            .0;
        let old = owned
            .drops()
            .iter()
            .filter(|fact| {
                sources.slice(fact.value_origin()).unwrap() == "f"
                    && matches!(fact.target(), DropTarget::Named(_))
            })
            .collect::<Vec<_>>();
        assert!(
            old.iter()
                .all(|fact| fact.point() != DropPoint::CallReturn(old_call)),
            "old environment is still a replacement obligation: {old:?}"
        );
        assert!(
            old.iter()
                .any(|fact| fact.point() == DropPoint::AfterExpression(rhs)),
            "normal RHS completion must dispose the replaced environment: {old:?}"
        );
        assert!(
            owned
                .drops()
                .iter()
                .any(|fact| fact.target() == DropTarget::Named(source)
                    && fact.point() == DropPoint::AfterExpression(rhs))
        );
        if tail == "return" {
            let exit = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| {
                    matches!(
                        node.payload(),
                        lang_frontend::parser::Expression::Return { .. }
                    )
                    .then_some(id)
                })
                .unwrap();
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.point() == DropPoint::ControlTransfer(exit))
                .collect::<Vec<_>>();
            let source_drop = drops
                .iter()
                .position(|fact| fact.target() == DropTarget::Named(source))
                .unwrap();
            assert!(
                drops[..source_drop].iter().any(|fact| old.contains(fact)),
                "old environment must release before source on return: {drops:?}"
            );
        }
    }
}

#[test]
fn replacement_self_transfer_never_drops_the_transferred_old_value_twice() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for rhs in ["f", "if (flag) f else f"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ var f: () -> Unit = {{ read(xs) }}\nf = {rhs}\nval used = f() }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{rhs}: {:?}",
            owned.diagnostics()
        );
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                _ => None,
            })
            .unwrap();
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
            .unwrap();
        let named = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), DropTarget::Named(_)))
            .collect::<Vec<_>>();
        assert_eq!(
            named.len(),
            2,
            "one transferred environment and one source: {named:?}"
        );
        assert!(
            named
                .iter()
                .all(|fact| fact.point() == DropPoint::CallReturn(call))
        );
        assert_eq!(named[1].target(), DropTarget::Named(source));
    }
}

#[test]
fn inner_loop_exit_does_not_end_an_outer_replacement_capture() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { loop { val observed = f()\nbreak }\nval invalid = consume(xs)\nval replacement: () -> Unit = { read(ys) }\nreplacement } else ({ read(ys) })\nval used = f() }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
        "inner break does not finish the outer RHS: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn replacement_loop_exit_releases_the_environment_that_leaves_the_loop() {
    for (header, jump) in [
        ("loop", "break"),
        ("while (flag)", "continue"),
        ("for (_ in listOf(0))", "continue"),
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ {header} {{ var f: () -> Unit = {{ read(xs) }}\nf = if (flag) {{ val old = f()\n{jump} }} else ({{}})\nval used = f()\nbreak }}\nval released = consume(xs) }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{header}/{jump}: exited local environment cannot keep borrowing xs: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn nested_replacement_can_dispose_the_old_value_before_outer_rhs_finishes() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { f = ({ read(ys) })\nval released = consume(xs)\nval observed = f()\nval replacement: () -> Unit = { read(ys) }\nreplacement } else f\nval used = f() }",
    );
    assert!(
        owned.diagnostics().is_empty(),
        "an explicit nested replacement ended the old xs capture: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn loop_exit_preserves_a_closure_held_by_another_live_environment() {
    for boundary in ["", "loop { break }\n"] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>) {{ val f: () -> Unit = {{ read(xs) }}\nval g: () -> Unit = {{ val used = f() }}\n{boundary}val invalid = consume(xs)\nval used = g() }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
            "{boundary:?}: g still borrows f and f still borrows xs: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn loop_exit_releases_a_dead_capture_chain_from_the_outer_environment_inward() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { loop { val f: () -> Unit = { read(xs) }\nvar g: () -> Unit = { val used = f() }\ng = if (flag) { val observed = g()\nbreak } else ({})\nval used = g()\nbreak }\nval released = consume(xs) }",
    );
    assert!(
        owned.diagnostics().is_empty(),
        "both g and its captured f leave scope, so xs must become available: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn replacement_saves_new_snapshot_before_old_cleanup_and_commits_it_afterward() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction as Action};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, own zs: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { val observed = f()\nval replacement: () -> Unit = { read(ys) }\nreplacement } else { val observed = f()\nval replacement: () -> Unit = { read(zs) }\nreplacement }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let rhs = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            sources
                .slice(node.span())
                .unwrap()
                .starts_with("if (flag)")
                .then_some(id)
        })
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::AfterExpression(rhs)).then_some(action))
        .collect::<Vec<_>>();
    let save = actions
        .iter()
        .position(|action| matches!(action, Action::SaveOwnerSnapshot { .. }))
        .unwrap();
    let drop_old = actions
        .iter()
        .position(|action| matches!(action, Action::Drop(_)))
        .unwrap();
    let end_capture = actions
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { .. }))
        .unwrap();
    let commit = actions
        .iter()
        .position(|action| matches!(action, Action::CommitOwnerSnapshot { .. }))
        .unwrap();
    assert!(
        save < drop_old && drop_old < end_capture && end_capture < commit,
        "{actions:?}"
    );
    let Action::SaveOwnerSnapshot { owner: saved, .. } = actions[save] else {
        unreachable!()
    };
    let Action::CommitOwnerSnapshot {
        owner: committed, ..
    } = actions[commit]
    else {
        unreachable!()
    };
    assert_eq!(saved, committed);
}

#[test]
fn moved_conditional_environment_slots_refer_to_the_destination_owner_value() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: move () -> Unit = if (flag) (move { read(xs) }) else (move { read(ys) })\nval g = f\nval used = g() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "g()").then_some(id))
        .unwrap();
    let slots = owned
        .drops()
        .iter()
        .filter_map(|fact| match fact.target() {
            DropTarget::Captured { owner, closure, .. }
                if fact.point() == DropPoint::CallReturn(call) =>
            {
                Some((owner, closure))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(slots.len(), 2);
    assert!(
        owned
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), DropTarget::Captured { .. })
                    && fact.point() == DropPoint::CallReturn(call)
            })
            .all(|fact| {
                let DropTarget::Captured { closure, source, .. } = fact.target() else {
                    unreachable!();
                };
                fact.capture_slot()
                    .and_then(|slot| owned.cleanup_conditions().capture_slot_value(slot))
                    .is_some_and(|layout| {
                        layout.source() == source
                            && matches!(
                                owned.cleanup_conditions().owner_value(layout.environment()),
                                Some(CleanupOwnerValue::Closure { expression, .. }) if *expression == closure
                            )
                    })
            }),
        "a saved environment retains its original slot layout"
    );
    assert_eq!(
        slots[0].0, slots[1].0,
        "both alternatives belong to the moved value"
    );
    let CleanupOwnerValue::Snapshot(snapshot) =
        owned.cleanup_conditions().owner_value(slots[0].0).unwrap()
    else {
        panic!("destination snapshot")
    };
    let source_owner = snapshot.capture_inputs()[0].owner();
    assert_ne!(
        snapshot.owner(),
        source_owner,
        "moving the saved value has an explicit source relation"
    );
    assert!(
        snapshot
            .capture_inputs()
            .iter()
            .all(|input| input.owner() == source_owner)
    );
    let source = owned
        .cleanup_conditions()
        .owner_snapshot(source_owner)
        .unwrap();
    for input in source.capture_inputs() {
        let CleanupOwnerValue::Closure { expression, .. } = owned
            .cleanup_conditions()
            .owner_value(input.owner())
            .unwrap()
        else {
            panic!("original environment")
        };
        assert!(slots.iter().any(|(_, closure)| closure == expression));
    }
    assert_eq!(source.capture_inputs().len(), 2);
}

#[test]
fn snapshot_binds_the_full_rhs_even_when_its_capture_origin_is_opaque() {
    use lang_frontend::ownership_checking::{CleanupCondition, IterationCleanupAction as Action};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own cb: move () -> Unit, flag: Boolean) { val f: move () -> Unit = if (flag) (move { read(xs) }) else cb\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let (
        _,
        Action::SaveOwnerSnapshot {
            owner,
            value,
            condition,
        },
    ) = owned
        .cleanup_steps()
        .iter()
        .find(|(_, action)| matches!(action, Action::SaveOwnerSnapshot { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    assert!(condition.is_none(), "both arms reach the save");
    let snapshot = owned.cleanup_conditions().owner_snapshot(*owner).unwrap();
    assert_eq!(snapshot.value(), *value);
    assert_eq!(
        sources
            .slice(parsed.ast().expressions().get(*value).unwrap().span())
            .unwrap(),
        "if (flag) (move { read(xs) }) else cb"
    );
    assert_eq!(snapshot.capture_inputs().len(), 1);
    let CleanupCondition::Choice { branches, .. } = owned
        .cleanup_conditions()
        .get(snapshot.capture_inputs()[0].condition())
        .unwrap()
    else {
        panic!("conditional provenance")
    };
    for (arm, expected) in [CleanupCondition::Always, CleanupCondition::Never]
        .iter()
        .enumerate()
    {
        assert_eq!(
            owned.cleanup_conditions().get(branches[arm]).unwrap(),
            expected
        );
        // The opaque arm has no local capture provenance, but still binds the complete RHS value.
        assert_eq!(snapshot.value(), *value);
    }
}
