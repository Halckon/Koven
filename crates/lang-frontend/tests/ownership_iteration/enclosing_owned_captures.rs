use super::*;

#[test]
fn inner_loop_phi_preserves_nested_outer_element_capture() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, ClosureCaptureMode, DropPoint, IterationCleanupAction as Action,
        IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(n: Int) {}\nfun run(xs: List<Int>, flags: List<Int>) { for (n in xs) {\nval g: () -> Unit = { read(n) }\nval f: move () -> Unit = move { g() }\nfor (_ in flags) {}\nval used = f()\nbreak } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let is_outer = |closure| {
        sources
            .slice(parsed.ast().expressions().get(closure).unwrap().span())
            .unwrap()
            == "move { g() }"
    };
    let plan = owned
        .iterations()
        .iter()
        .find(|plan| {
            plan.closure_phis().iter().any(|phi| {
                phi.boundary() == IterationPhiBoundary::Header
                    && phi
                        .origins()
                        .iter()
                        .any(|origin| is_outer(origin.closure()))
            })
        })
        .expect("inner loop must carry f");
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| {
            phi.boundary() == IterationPhiBoundary::Header
                && phi
                    .origins()
                    .iter()
                    .any(|origin| is_outer(origin.closure()))
        })
        .unwrap();
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == header.symbol())
        .unwrap();
    let header_outer = header
        .origins()
        .iter()
        .find(|origin| is_outer(origin.closure()))
        .unwrap();
    let exit_outer = exit
        .origins()
        .iter()
        .find(|origin| is_outer(origin.closure()))
        .unwrap();
    let header_source = header_outer
        .sources()
        .iter()
        .find(|source| !source.captured().is_empty())
        .expect("f must carry g's environment");
    let header_nested = &header.origins()[header_source.captured()[0]];
    let exit_source = exit_outer
        .sources()
        .iter()
        .find(|source| source.source() == header_source.source())
        .unwrap();
    let exit_nested = &exit.origins()[exit_source.captured()[0]];
    assert_ne!(header_nested.selector(), exit_nested.selector());
    let incoming = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    fn source_input(
        incoming: &lang_frontend::ownership_checking::IterationPhiIncoming,
        owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
        selector: lang_frontend::ownership_checking::CleanupSelectorId,
        source: lang_frontend::ownership_checking::ClosureCaptureSource,
    ) -> &lang_frontend::ownership_checking::IterationPhiIncomingSource {
        incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == owner)
            .unwrap()
            .origins()
            .iter()
            .find(|origin| origin.target() == selector)
            .unwrap()
            .environments()[0]
            .sources()
            .iter()
            .find(|input| input.input().source() == source)
            .unwrap()
    }
    let entry_source = source_input(
        incoming(IterationPhiIncomingKind::Entry),
        header.owner(),
        header_outer.selector(),
        header_source.source(),
    );
    let entry_nested = &entry_source.captured()[0];
    assert_eq!(entry_nested.target(), header_nested.selector());
    assert_eq!(
        entry_source.value(),
        CleanupCaptureValue::Owner(entry_nested.environments()[0].owner())
    );
    let exhaustion_source = source_input(
        incoming(IterationPhiIncomingKind::Exhaustion),
        exit.owner(),
        exit_outer.selector(),
        header_source.source(),
    );
    let exhaustion_nested = &exhaustion_source.captured()[0];
    assert_eq!(exhaustion_nested.target(), exit_nested.selector());
    assert_eq!(
        exhaustion_source.value(),
        CleanupCaptureValue::Owner(header_source.owner())
    );
    assert_eq!(
        exhaustion_nested.environments()[0].owner(),
        header_source.owner()
    );
    let source = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .unwrap()
        .source();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    assert!(
        owned.cleanup_steps().iter().any(|(point, action)| {
            *point == DropPoint::CallReturn(call)
                && matches!(action, Action::EndCaptureLoan { source: actual, .. } if *actual == source)
        }),
        "the inner phi must preserve g's element loan until f returns: {:?}",
        owned.cleanup_steps()
    );
}

#[test]
fn loop_phi_publishes_enclosing_closure_with_owned_descendant() {
    use lang_frontend::ownership_checking::{CleanupCaptureValue, DropTarget};

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val base: move () -> Unit = move { read(xs) }
            val outer: move () -> Unit = move {
                var f: move () -> Unit = move { base() }
                for (_ in listOf(1)) {}
                val used = f()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(!owned.iterations().is_empty());
    // 静态唯一、owned move 的紧邻后代必须带捕获槽与实例地址释放。
    assert!(
        owned.drops().iter().any(|fact| {
            matches!(
                fact.target(),
                DropTarget::Captured {
                    value: CleanupCaptureValue::Owner(_),
                    ..
                }
            ) && fact.capture_slot().is_some()
                && fact.instance_address().is_some()
        }),
        "owned descendant must be released by instance: {:?}",
        owned.drops()
    );
}

#[test]
fn non_loop_enclosing_capture_keeps_known_and_opaque_drops() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, DropPoint, DropTarget, IterationCleanupAction,
    };

    let (sources, parsed, owned) = checked(
        "fun make(): move () -> Unit = move {}\nfun read(xs: List<Int>) {}\nfun run(flag: Boolean, own xs: List<Int>) {\nval base: move () -> Unit = if (flag) (move { read(xs) }) else (make())\nval outer: move () -> Unit = move {\nval inner: move () -> Unit = move { base() }\nval used = inner() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let inner_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("inner()")).then_some(id))
        .unwrap();
    let parent = owned
        .drops()
        .iter()
        .filter(|fact| {
            fact.point() == DropPoint::CallReturn(inner_call)
                && matches!(
                    fact.target(),
                    DropTarget::Captured {
                        value: CleanupCaptureValue::Environment { .. },
                        ..
                    }
                )
        })
        .collect::<Vec<_>>();
    assert_eq!(parent.len(), 2);
    assert_eq!(
        parent.iter().filter(|fact| fact.owner().is_some()).count(),
        1
    );
    assert_eq!(parent[0].capture_slot(), parent[1].capture_slot());
    assert_eq!(parent[0].instance_address(), parent[1].instance_address());
    let DropTarget::Captured {
        owner: environment,
        closure,
        ..
    } = parent[0].target()
    else {
        unreachable!()
    };
    let snapshot = owned
        .cleanup_conditions()
        .owner_snapshot(environment)
        .unwrap();
    assert_eq!(snapshot.capture_inputs().len(), 1);
    let formed = snapshot.capture_inputs()[0].owner();
    assert!(owned.cleanup_steps().iter().any(|(_, action)| matches!(
        action,
        IterationCleanupAction::CreateClosureOwner { owner, closure: created }
            if *owner == formed && *created == closure
    )));
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::LambdaEntry(closure)
            && matches!(
                action,
                IterationCleanupAction::BindClosureEnvironment {
                    owner,
                    closure: entered,
                } if *owner == formed && *entered == closure
            )
    }));
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(inner_call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { callee, closure: called }
                    if *callee == environment && *called == Some(closure)
            )
    }));
    let base_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("base()")).then_some(id))
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(base_call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn loop_phi_keeps_non_closure_enclosing_capture_without_deferred() {
    use lang_frontend::ownership_checking::CleanupCaptureValue;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() { val xs = listOf(1)\nval outer: move () -> Unit = move { val f: () -> Unit = { read(xs) }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert_eq!(owned.iterations().len(), 1);
    let enclosing_sources = owned
        .iterations()
        .iter()
        .flat_map(|iteration| iteration.closure_phi_incomings())
        .flat_map(|incoming| incoming.bindings())
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter_map(|source| match source.value() {
            CleanupCaptureValue::Environment { slot, .. } => Some((source, slot)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!enclosing_sources.is_empty());
    for (source, outer_slot) in enclosing_sources {
        let inner_slot = source
            .source_capture_slot()
            .expect("phi must read the formed inner environment");
        assert_ne!(inner_slot, outer_slot);
        let slots = owned.cleanup_conditions();
        assert_eq!(
            slots.capture_slot_value(inner_slot).unwrap().source(),
            slots.capture_slot_value(outer_slot).unwrap().source()
        );
    }
}

#[test]
fn loop_phi_reads_formed_owned_capture_after_enclosing_move() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, ClosureCaptureEffect, ClosureCaptureMode,
        IterationCleanupAction as Action, IterationPhiIncomingKind,
    };

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val outer: move () -> Unit = move {
                var f: move () -> Unit = move { read(xs) }
                for (_ in listOf(1)) {}
                val used = f()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let mut checked_sources = 0;
    let mut transport = None;
    for iteration in owned.iterations() {
        for incoming in iteration.closure_phi_incomings() {
            if incoming.kind() != IterationPhiIncomingKind::Entry {
                continue;
            }
            for binding in incoming.bindings() {
                for origin in binding.origins() {
                    for environment in origin.environments() {
                        for source in environment.sources() {
                            let CleanupCaptureValue::Environment {
                                owner: outer,
                                slot: outer_slot,
                                ..
                            } = source.value()
                            else {
                                continue;
                            };
                            let inner_slot = source
                                .source_capture_slot()
                                .expect("phi reads the formed inner environment");
                            assert_eq!(
                                source.transport_value(),
                                Some(CleanupCaptureValue::Environment {
                                    owner: environment.owner(),
                                    source: source.input().source(),
                                    slot: inner_slot,
                                })
                            );
                            assert_ne!(source.transport_value(), Some(source.value()));
                            let slots = owned.cleanup_conditions();
                            assert_eq!(source.input().mode(), ClosureCaptureMode::Owned);
                            assert_eq!(source.input().effect(), ClosureCaptureEffect::Move);
                            assert_ne!(inner_slot, outer_slot);
                            assert_eq!(
                                slots.capture_slot_value(outer_slot).unwrap().environment(),
                                outer
                            );
                            assert_ne!(
                                slots.capture_slot_value(inner_slot).unwrap().environment(),
                                outer
                            );
                            transport.get_or_insert((
                                outer,
                                outer_slot,
                                environment.owner(),
                                inner_slot,
                                source.transport_value().unwrap(),
                                binding.target(),
                                binding.values()[0].source(),
                            ));
                            checked_sources += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(
        checked_sources > 0,
        "owned enclosing capture must reach a phi incoming"
    );
    let (outer, outer_slot, inner, inner_slot, transport_value, header, entry_source) =
        transport.unwrap();
    assert_eq!(entry_source, inner);
    let saves = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            Action::SaveClosureCapture {
                owner,
                target,
                input,
            } if *target == outer_slot || *target == inner_slot => Some((*owner, *target, *input)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(saves.len(), 2);
    let outer_save = *saves
        .iter()
        .find(|(_, target, _)| *target == outer_slot)
        .unwrap();
    let inner_save = *saves
        .iter()
        .find(|(_, target, _)| *target == inner_slot)
        .unwrap();
    assert_eq!(outer_save.0, outer);
    assert_eq!(inner_save.0, inner);
    let CleanupCaptureValue::Owner(xs_owner) = outer_save.2.value() else {
        panic!("outer formation must consume the xs instance")
    };
    assert!(
        matches!(inner_save.2.value(), CleanupCaptureValue::Environment { owner, slot, .. }
        if owner == outer && slot == outer_slot)
    );
    // Lambda body is planned separately; actual execution forms the outer environment first.
    let formation_order = [outer_save, inner_save];
    let exhaustion = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let exit = exhaustion
        .bindings()
        .iter()
        .find(|binding| {
            binding
                .values()
                .iter()
                .any(|value| value.source() == header)
        })
        .unwrap();
    assert_eq!(exit.values().len(), 1);
    let exit_target = exit.target();
    let exit_origin = exit
        .origins()
        .iter()
        .find(|origin| {
            origin
                .environments()
                .iter()
                .flat_map(|environment| environment.sources())
                .any(|source| source.input().source() == inner_save.2.source())
        })
        .unwrap();
    let exit_transport = exit_origin
        .environments()
        .iter()
        .flat_map(|environment| environment.sources())
        .find(|source| source.input().source() == inner_save.2.source())
        .unwrap()
        .transport_value()
        .unwrap();
    let header_capture_slot = owned
        .cleanup_conditions()
        .phi_capture_slot(
            header,
            owned.iterations()[0].capture_graph().nodes()[exit_origin.node()].closure(),
            inner_save.2.source(),
        )
        .unwrap();
    assert!(
        matches!(exit_transport, CleanupCaptureValue::Environment { owner, slot, .. }
        if owner == header && slot == header_capture_slot)
    );
    assert_ne!(header_capture_slot, inner_slot);
    let mut capture_slots = BTreeMap::new();
    for round in 0..2_u32 {
        let xs_instance = 100 + round;
        let outer_instance = 200 + round;
        let inner_instance = 300 + round;
        let mut owners = BTreeMap::from([
            (xs_owner, xs_instance),
            (outer, outer_instance),
            (inner, inner_instance),
        ]);
        for (owner, target, input) in &formation_order {
            let target_position = owned
                .cleanup_conditions()
                .capture_slot_value(*target)
                .unwrap()
                .position();
            let value = match input.value() {
                CleanupCaptureValue::Owner(source) => owners.remove(&source).unwrap(),
                CleanupCaptureValue::Environment { owner, slot, .. } => {
                    let source_position = owned
                        .cleanup_conditions()
                        .capture_slot_value(slot)
                        .unwrap()
                        .position();
                    capture_slots
                        .remove(&(owners[&owner], source_position))
                        .unwrap()
                }
                CleanupCaptureValue::Place(_) => panic!("owned move must read a saved instance"),
            };
            assert!(
                capture_slots
                    .insert((owners[owner], target_position), value)
                    .is_none()
            );
        }
        let outer_position = owned
            .cleanup_conditions()
            .capture_slot_value(outer_slot)
            .unwrap()
            .position();
        assert!(!capture_slots.contains_key(&(outer_instance, outer_position)));
        let CleanupCaptureValue::Environment { owner, slot, .. } = transport_value else {
            panic!("phi must read the formed inner environment")
        };
        let entry_position = owned
            .cleanup_conditions()
            .capture_slot_value(slot)
            .unwrap()
            .position();
        assert_eq!(
            capture_slots[&(owners[&owner], entry_position)],
            xs_instance
        );
        let entry_instance = owners.remove(&entry_source).unwrap();
        assert_eq!(entry_instance, inner_instance);
        assert!(owners.insert(header, entry_instance).is_none());
        let CleanupCaptureValue::Environment { owner, slot, .. } = exit_transport else {
            panic!("exhaustion must read the header environment")
        };
        assert_eq!(owner, header);
        let exit_position = owned
            .cleanup_conditions()
            .capture_slot_value(slot)
            .unwrap()
            .position();
        assert_eq!(capture_slots[&(owners[&owner], exit_position)], xs_instance);
        let header_instance = owners.remove(&header).unwrap();
        assert!(owners.insert(exit_target, header_instance).is_none());
        assert_eq!(owners[&exit_target], inner_instance);
    }
    let inner_position = owned
        .cleanup_conditions()
        .capture_slot_value(inner_slot)
        .unwrap()
        .position();
    assert_eq!(capture_slots[&(300, inner_position)], 100);
    assert_eq!(capture_slots[&(301, inner_position)], 101);
}

#[test]
fn loop_phi_publishes_statically_unique_owned_descendant() {
    use lang_frontend::ownership_checking::{CleanupCaptureValue, DropTarget};

    let (_, _, owned) = checked(
        "fun take(own xs: List<Int>) {}\nfun run(own xs: List<Int>) {\nval base: move () -> Unit = move { take(xs) }\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(!owned.iterations().is_empty());
    assert!(
        owned.drops().iter().any(|fact| {
            matches!(
                fact.target(),
                DropTarget::Captured {
                    value: CleanupCaptureValue::Owner(_),
                    ..
                }
            ) && fact.capture_slot().is_some()
                && fact.instance_address().is_some()
        }),
        "the owned descendant must remain droppable through the phi: {:?}",
        owned.drops()
    );
}
