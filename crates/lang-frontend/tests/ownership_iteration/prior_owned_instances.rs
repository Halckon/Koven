use super::*;

#[test]
fn owned_capture_reads_the_prior_phi_environment_before_replacement() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupOwnerValue, ClosureCaptureSource, DropPoint,
        IterationCleanupAction as Action, IterationPhiBoundary,
    };

    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Int>) { var g: move () -> Unit = move {}\nvar f: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move {} }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let captured_g = owned
        .captures()
        .iter()
        .find_map(|capture| {
            (sources.slice(capture.reference_span()).unwrap() == "g").then_some(capture.source())
        })
        .unwrap();
    let ClosureCaptureSource::Symbol(g_symbol) = captured_g else {
        panic!("g capture must resolve to a binding")
    };
    let plan = &owned.iterations()[0];
    let header_g = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == g_symbol)
        .unwrap();
    let f_lambda = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()).unwrap() == "move { g() }").then_some(id)
        })
        .unwrap();
    let (formation_index, created_f) = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (point, action))| match action {
            Action::CreateClosureOwner { owner, closure }
                if *point == DropPoint::AfterExpression(f_lambda) && *closure == f_lambda =>
            {
                Some((index, *owner))
            }
            _ => None,
        })
        .unwrap();
    let Some(CleanupOwnerValue::Closure { inputs, .. }) =
        owned.cleanup_conditions().owner_value(created_f)
    else {
        panic!("f formation must define an environment")
    };
    assert!(inputs.iter().any(|input| {
        input.source() == captured_g
            && input.value() == CleanupCaptureValue::Owner(header_g.owner())
    }));
    let (replacement_index, replacement_owner) = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            Action::CommitOwnerSnapshot { owner, target }
                if index > formation_index && *target == g_symbol =>
            {
                Some((index, *owner))
            }
            _ => None,
        })
        .unwrap();
    assert_ne!(replacement_owner, header_g.owner());
    assert!(
        formation_index < replacement_index,
        "f must capture the old g instance before g is replaced"
    );
}

#[test]
fn two_round_owned_capture_keeps_each_prior_environment_instance() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureSource, DropPoint,
        DropTarget, IterationCleanupAction as Action, IterationClosurePhiBinding,
        IterationClosurePhiOrigin, IterationPhiBoundary, IterationPhiIncoming,
        IterationPhiIncomingKind, IterationPhiIncomingOrigin,
    };

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let branch = choices.get(selector).unwrap_or_else(|| {
                    panic!("condition {condition:?} reads uninitialized selector {selector:?}: {:?}; choices: {choices:?}", table.selector(*selector))
                });
                selected(table, branches[*branch], choices)
            }
        }
    }

    fn source_of(edge: &IterationPhiIncoming, target: CleanupOwnerValueId) -> CleanupOwnerValueId {
        let binding = edge
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        assert_eq!(binding.values().len(), 1, "this edge has one actual value");
        binding.values()[0].source()
    }

    struct ReplayBefore<'a> {
        choices: &'a BTreeMap<CleanupSelectorId, usize>,
        owners: &'a BTreeMap<CleanupOwnerValueId, u32>,
        captured: &'a BTreeMap<(u32, usize), (CleanupOwnerValueId, u32)>,
    }

    #[derive(Default)]
    struct ReplayWrites {
        choices: Vec<(CleanupSelectorId, usize)>,
        owners: Vec<(CleanupOwnerValueId, u32)>,
        consumed: Vec<CleanupOwnerValueId>,
    }

    fn copy_origin(
        table: &CleanupConditions,
        root: CleanupOwnerValueId,
        origins: &[IterationClosurePhiOrigin],
        layout: &IterationClosurePhiOrigin,
        origin: &IterationPhiIncomingOrigin,
        before: &ReplayBefore<'_>,
        writes: &mut ReplayWrites,
    ) {
        assert_eq!(origin.target(), layout.selector());
        let present = selected(table, origin.condition(), before.choices);
        if !present {
            return;
        }
        let environments = origin
            .environments()
            .iter()
            .filter(|environment| selected(table, environment.condition(), before.choices))
            .collect::<Vec<_>>();
        assert_eq!(environments.len(), 1);
        let environment = environments[0];
        for source in environment.sources() {
            if !selected(table, source.input().condition(), before.choices) {
                continue;
            }
            let slot = layout
                .sources()
                .iter()
                .find(|slot| slot.source() == source.input().source())
                .unwrap();
            if let Some(target) = source.target() {
                assert_eq!(target, slot.owner());
                let CleanupCaptureValue::Owner(actual) = source.value() else {
                    panic!("owned capture must read an actual source owner")
                };
                let static_slot = table
                    .phi_capture_slot(root, layout.closure(), slot.source())
                    .unwrap();
                assert_eq!(source.capture_slot(), Some(static_slot));
                let (read_address, source_slot) = source
                    .transport_read()
                    .expect("owned incoming reads a published instance and source slot");
                let read_address = table.instance_address(read_address).unwrap();
                assert_eq!(read_address.root(), environment.instance_root());
                assert_eq!(read_address.capture_path(), environment.capture_path());
                if environment.capture_path().is_empty() {
                    assert!(matches!(
                        source.transport_value(),
                        Some(CleanupCaptureValue::Environment { owner, slot, .. })
                            if owner == environment.owner() && slot == source_slot
                    ));
                } else {
                    assert_eq!(source.transport_value(), None);
                }
                let source_layout = table.capture_slot_value(source_slot).unwrap();
                assert_eq!(source_layout.source(), slot.source());
                assert_eq!(source_layout.closure(), layout.closure());
                assert_ne!(source_slot, static_slot);
                let position = source_layout.position();
                let mut instance = before.owners[&read_address.root()];
                for &capture in read_address.capture_path() {
                    instance = before.captured[&(instance, capture)].1;
                }
                let (saved_source, captured) = before.captured[&(instance, position)];
                if let Some(source_instance) = before.owners.get(&actual) {
                    assert_eq!(*source_instance, captured);
                } else {
                    assert_eq!(actual, saved_source);
                }
                writes.owners.push((target, captured));
            }
            for nested in source.captured() {
                let nested_layout = slot
                    .captured()
                    .iter()
                    .map(|&index| &origins[index])
                    .find(|layout| layout.selector() == nested.target())
                    .unwrap();
                copy_origin(table, root, origins, nested_layout, nested, before, writes);
            }
        }
    }

    fn copy(
        table: &CleanupConditions,
        edge: &IterationPhiIncoming,
        targets: &[&IterationClosurePhiBinding],
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
        captured: &BTreeMap<(u32, usize), (CleanupOwnerValueId, u32)>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        assert!(selected(table, edge.condition(), &before_choices));
        let before = ReplayBefore {
            choices: &before_choices,
            owners: &before_owners,
            captured,
        };
        let mut writes = ReplayWrites::default();
        for target in targets {
            let binding = edge
                .bindings()
                .iter()
                .find(|binding| binding.target() == target.owner())
                .unwrap();
            let available = selected(table, binding.available_when(), &before_choices);
            writes
                .choices
                .push((binding.availability_selector(), usize::from(available)));
            for write in binding.selector_writes() {
                writes.choices.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(values.len(), usize::from(available));
            assert_eq!(target.root_origins().count(), binding.origins().len());
            let selected_origins = binding
                .origins()
                .iter()
                .filter(|origin| selected(table, origin.condition(), &before_choices))
                .flat_map(|origin| origin.environments())
                .filter(|environment| selected(table, environment.condition(), &before_choices))
                .map(|environment| before_owners[&environment.owner()])
                .collect::<Vec<_>>();
            assert_eq!(
                selected_origins,
                values
                    .iter()
                    .map(|value| before_owners[&value.source()])
                    .collect::<Vec<_>>()
            );
            if let Some(value) = values.first() {
                writes.consumed.push(value.source());
                writes
                    .owners
                    .push((target.owner(), before_owners[&value.source()]));
            }
            for origin in binding.origins() {
                let layout = target
                    .origins()
                    .iter()
                    .find(|layout| layout.selector() == origin.target())
                    .unwrap();
                copy_origin(
                    table,
                    target.owner(),
                    target.origins(),
                    layout,
                    origin,
                    &before,
                    &mut writes,
                );
            }
        }
        choices.extend(writes.choices);
        for source in writes.consumed {
            assert!(
                owners.remove(&source).is_some(),
                "phi move consumes its source"
            );
        }
        owners.extend(writes.owners);
    }

    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Int>) { var g: move () -> Unit = move {}\nvar f: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move {} }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = |name| {
        names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id()
    };
    let (f, g) = (symbol("f"), symbol("g"));
    let capture = ClosureCaptureSource::Symbol(g);
    let plan = &owned.iterations()[0];
    let phi = |boundary, name| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == name)
            .unwrap()
    };
    let (header_f, header_g) = (
        phi(IterationPhiBoundary::Header, f),
        phi(IterationPhiBoundary::Header, g),
    );
    let exit_f = phi(IterationPhiBoundary::Exit, f);
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let entry = edge(IterationPhiIncomingKind::Entry);
    let backedge = edge(IterationPhiIncomingKind::Fallthrough);
    let exhaustion = edge(IterationPhiIncomingKind::Exhaustion);
    let f_lambda = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()).unwrap() == "move { g() }").then_some(id)
        })
        .unwrap();
    let created_f = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::CreateClosureOwner { owner, closure } if *closure == f_lambda => Some(*owner),
            _ => None,
        })
        .unwrap();
    let Some(CleanupOwnerValue::Closure { inputs, .. }) =
        owned.cleanup_conditions().owner_value(created_f)
    else {
        panic!("body lambda forms a concrete environment")
    };
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].source(), capture);
    assert_eq!(
        inputs[0].value(),
        CleanupCaptureValue::Owner(header_g.owner())
    );
    let table = owned.cleanup_conditions();
    let capture_edges = table.closure_capture_edges(created_f).unwrap();
    assert_eq!(capture_edges.len(), 1);
    assert_eq!(capture_edges[0].input(), inputs[0]);
    let slot = capture_edges[0].target();
    let position = table.capture_slot_value(slot).unwrap().position();
    for (edge, target, expected) in [
        (backedge, header_f.owner(), slot),
        (
            exhaustion,
            exit_f.owner(),
            table
                .phi_capture_slot(header_f.owner(), f_lambda, capture)
                .unwrap(),
        ),
    ] {
        let source_slots = edge
            .bindings()
            .iter()
            .filter(|binding| binding.target() == target)
            .flat_map(|binding| binding.origins())
            .flat_map(|origin| origin.environments())
            .flat_map(|environment| environment.sources())
            .filter(|source| source.input().source() == capture)
            .map(|source| source.source_capture_slot())
            .collect::<Vec<_>>();
        assert_eq!(source_slots, [Some(expected)]);
    }
    let committed = |target| {
        owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::CommitOwnerSnapshot {
                    owner,
                    target: actual,
                } if *actual == target => Some(*owner),
                _ => None,
            })
            .unwrap()
    };
    let (next_f, next_g) = (committed(f), committed(g));
    let snapshot = table.owner_snapshot(next_f).unwrap();
    let g_snapshot = table.owner_snapshot(next_g).unwrap();
    assert_eq!(snapshot.value(), f_lambda);
    assert_eq!(snapshot.capture_inputs().len(), 1);
    assert_eq!(snapshot.capture_inputs()[0].owner(), created_f);
    let action_index = |predicate: &dyn Fn(&Action) -> bool| {
        owned
            .cleanup_steps()
            .iter()
            .position(|(_, action)| predicate(action))
            .unwrap()
    };
    let formation = action_index(
        &|action| matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == created_f),
    );
    let capture_write = action_index(&|action| {
        matches!(action, Action::SaveClosureCapture { owner, target, input }
            if *owner == created_f && *target == slot && *input == capture_edges[0].input())
    });
    let f_save = action_index(
        &|action| matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == next_f),
    );
    let old_f_drop = action_index(&|action| {
        matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::Captured { owner, closure, source, .. }
            if owner == header_f.owner() && closure == f_lambda && source == capture))
    });
    let f_commit = action_index(
        &|action| matches!(action, Action::CommitOwnerSnapshot { owner, .. } if *owner == next_f),
    );
    let g_save = action_index(
        &|action| matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == next_g),
    );
    assert!(
        formation < capture_write
            && capture_write < f_save
            && f_save < old_f_drop
            && old_f_drop < f_commit
    );
    assert!(f_commit < g_save);
    let save_guard = |index, expected_owner, expected_value| {
        let (
            point,
            Action::SaveOwnerSnapshot {
                owner,
                value,
                condition,
            },
        ) = &owned.cleanup_steps()[index]
        else {
            panic!("expected a saved owner action")
        };
        assert_eq!(*owner, expected_owner);
        assert_eq!(*value, expected_value);
        assert_eq!(*point, DropPoint::AfterExpression(*value));
        *condition
    };
    let f_save_guard = save_guard(f_save, next_f, snapshot.value());
    let g_save_guard = save_guard(g_save, next_g, g_snapshot.value());
    assert_eq!(source_of(backedge, header_f.owner()), next_f);
    assert_eq!(source_of(backedge, header_g.owner()), next_g);
    assert_eq!(source_of(exhaustion, exit_f.owner()), header_f.owner());
    for (owner, point) in [
        (header_f.owner(), DropPoint::AfterExpression(f_lambda)),
        (
            exit_f.owner(),
            DropPoint::CallReturn(
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| {
                        (sources.slice(node.span()).unwrap() == "f()").then_some(id)
                    })
                    .unwrap(),
            ),
        ),
    ] {
        let expected = table.phi_capture_slot(owner, f_lambda, capture).unwrap();
        assert_eq!(
            table.capture_slot_value(expected).unwrap().position(),
            position
        );
        assert!(owned.drops().iter().any(|fact| {
            fact.point() == point
                && matches!(fact.target(), DropTarget::Captured { owner: actual, closure, source, .. }
                    if actual == owner && closure == f_lambda && source == capture)
                && fact.capture_slot() == Some(expected)
        }), "both old and final f need a captured-edge release candidate");
    }

    let mut current = BTreeMap::from([
        (source_of(entry, header_f.owner()), 2_u32),
        (source_of(entry, header_g.owner()), 1_u32),
    ]);
    let mut choices = BTreeMap::new();
    let mut captured_instances = BTreeMap::new();
    copy(
        table,
        entry,
        &[header_f, header_g],
        &mut choices,
        &mut current,
        &captured_instances,
    );
    let mut released = Vec::new();
    for round in 0..2_u32 {
        let old_g = current[&header_g.owner()];
        let new_f = 10 + round;
        let before_formation = current.clone();
        let Action::SaveClosureCapture {
            owner,
            target,
            input,
        } = owned.cleanup_steps()[capture_write].1
        else {
            panic!("the formed environment must save its checked capture")
        };
        assert_eq!(owner, created_f);
        assert_eq!(target, slot);
        assert!(selected(table, input.condition(), &choices));
        let CleanupCaptureValue::Owner(source_owner) = input.value() else {
            panic!("this body lambda reads the prior g owner")
        };
        let captured_g = current.remove(&source_owner).unwrap();
        assert_eq!(captured_g, before_formation[&source_owner]);
        assert!(!current.contains_key(&source_owner));
        current.insert(owner, new_f);
        assert!(
            captured_instances
                .insert((current[&owner], position), (source_owner, captured_g))
                .is_none()
        );
        assert_eq!(captured_g, old_g);
        let before_save = choices.clone();
        assert!(f_save_guard.is_none_or(|guard| selected(table, guard, &before_save)));
        for selector in snapshot.copies() {
            if selected(table, selector.when(), &before_save) {
                choices.insert(selector.target(), before_save[&selector.source()]);
            }
        }
        let active_inputs = snapshot
            .capture_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(active_inputs.len(), 1);
        let saved_f = current[&active_inputs[0].owner()];
        current.insert(next_f, saved_f);
        let old_drops = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                Action::Drop(fact)
                    if *point == DropPoint::AfterExpression(f_lambda)
                        && matches!(fact.target(), DropTarget::Captured { owner, closure, source, .. }
                            if owner == header_f.owner() && closure == f_lambda && source == capture)
                        && fact.condition().is_none_or(|guard| selected(table, guard, &choices)) =>
                {
                    let static_slot = fact.capture_slot().unwrap();
                    assert_eq!(static_slot, table.phi_capture_slot(header_f.owner(), f_lambda, capture).unwrap());
                    let instance = current[&header_f.owner()];
                    let source_instance = current[&fact.owner().unwrap()];
                    let (_, captured) = captured_instances.remove(&(instance, table.capture_slot_value(static_slot).unwrap().position())).unwrap();
                    assert_eq!(captured, source_instance);
                    Some(captured)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(old_drops.len(), usize::from(round > 0));
        released.extend(old_drops);
        let before_g_save = choices.clone();
        assert!(g_save_guard.is_none_or(|guard| selected(table, guard, &before_g_save)));
        for selector in g_snapshot.copies() {
            if selected(table, selector.when(), &before_g_save) {
                choices.insert(selector.target(), before_g_save[&selector.source()]);
            }
        }
        current.insert(next_g, 20 + round);
        copy(
            table,
            backedge,
            &[header_f, header_g],
            &mut choices,
            &mut current,
            &captured_instances,
        );
        assert_eq!(
            captured_instances[&(current[&header_f.owner()], position)].1,
            old_g
        );
        assert_ne!(current[&header_g.owner()], old_g);
    }
    copy(
        table,
        exhaustion,
        &[exit_f],
        &mut choices,
        &mut current,
        &captured_instances,
    );
    let final_f = current[&exit_f.owner()];
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let final_drops = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            Action::Drop(fact)
                if *point == DropPoint::CallReturn(call)
                    && matches!(fact.target(), DropTarget::Captured { owner, closure, source, .. }
                        if owner == exit_f.owner() && closure == f_lambda && source == capture)
                    && fact
                        .condition()
                        .is_none_or(|guard| selected(table, guard, &choices)) =>
            {
                let static_slot = fact.capture_slot().unwrap();
                assert_eq!(
                    static_slot,
                    table
                        .phi_capture_slot(exit_f.owner(), f_lambda, capture)
                        .unwrap()
                );
                let (_, captured) = captured_instances
                    .remove(&(
                        final_f,
                        table.capture_slot_value(static_slot).unwrap().position(),
                    ))
                    .unwrap();
                assert_eq!(captured, current[&fact.owner().unwrap()]);
                Some(captured)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(final_drops.len(), 1);
    released.extend(final_drops);
    assert_eq!(
        released,
        [1, 20],
        "old g instances release in capture order"
    );
    assert!(captured_instances.is_empty());
}
