use super::*;

#[test]
fn first_loop_forms_two_instances_before_outer_capture() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureSource,
        IterationCleanupAction as Action, IterationPhiBoundary, IterationPhiCaptureSlot,
        IterationPhiIncoming, IterationPhiIncomingKind,
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
                selected(table, branches[choices[selector]], choices)
            }
        }
    }

    fn source_of(
        incoming: &IterationPhiIncoming,
        target: CleanupOwnerValueId,
    ) -> CleanupOwnerValueId {
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        let [source] = binding.values() else {
            panic!("one checked root source")
        };
        source.source()
    }

    fn copy_roots(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        targets: [CleanupOwnerValueId; 2],
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let mut selector_writes = Vec::new();
        let mut owner_writes = Vec::new();
        for target in targets {
            let binding = incoming
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap();
            let available = selected(table, binding.available_when(), &before_choices);
            selector_writes.push((binding.availability_selector(), usize::from(available)));
            let active = binding
                .values()
                .iter()
                .filter(|source| selected(table, source.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(active.len(), usize::from(available));
            let active_roots = binding
                .root_sources()
                .iter()
                .filter(|source| selected(table, source.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(active_roots.len(), active.len());
            if let [source] = active.as_slice() {
                assert_eq!(active_roots[0].source(), source.source());
            }
            for write in binding.selector_writes() {
                selector_writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            if let Some(source) = active.first() {
                owner_writes.push((target, source.source(), before_owners[&source.source()]));
            }
        }
        choices.extend(selector_writes);
        for (_, source, _) in &owner_writes {
            owners
                .remove(source)
                .expect("phi consumes each root source");
        }
        for (target, _, handle) in owner_writes {
            owners.insert(target, handle);
        }
    }

    fn assert_capture_transport(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        expected: (CleanupOwnerValueId, u32),
        layout: &[IterationPhiCaptureSlot],
        choices: &BTreeMap<CleanupSelectorId, usize>,
        owners: &BTreeMap<CleanupOwnerValueId, u32>,
        captures: &BTreeMap<(u32, usize), u32>,
    ) {
        let (target, expected_source) = expected;
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        let origins = binding
            .origins()
            .iter()
            .filter(|origin| selected(table, origin.condition(), choices))
            .collect::<Vec<_>>();
        let [origin] = origins.as_slice() else {
            panic!("one selected lambda origin")
        };
        let environments = origin
            .environments()
            .iter()
            .filter(|environment| selected(table, environment.condition(), choices))
            .collect::<Vec<_>>();
        let [environment] = environments.as_slice() else {
            panic!("one selected environment instance")
        };
        let sources = environment
            .sources()
            .iter()
            .filter(|source| selected(table, source.input().condition(), choices))
            .collect::<Vec<_>>();
        let [source] = sources.as_slice() else {
            panic!("one captured source transported on this edge")
        };
        let CleanupCaptureValue::Environment { owner, slot, .. } = source
            .transport_value()
            .expect("formed capture slot is published")
        else {
            panic!("transport reads the formed environment")
        };
        let handle = owners[&environment.owner()];
        assert_eq!(owners[&owner], handle);
        let position = table.capture_slot_value(slot).unwrap().position();
        assert_eq!(captures[&(handle, position)], expected_source);
        let target_slot = source.capture_slot().expect("phi capture target exists");
        assert!(source.target().is_some());
        assert!(layout.iter().any(|slot| {
            slot.node() == origin.node()
                && slot.position() == position
                && slot.slot() == target_slot
        }));
        assert!(binding.capture_slots_to_clear().contains(&target_slot));
    }

    fn save_snapshot(
        table: &CleanupConditions,
        owner: CleanupOwnerValueId,
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
    ) -> u32 {
        let snapshot = table.owner_snapshot(owner).unwrap();
        let before = choices.clone();
        let writes = snapshot
            .copies()
            .iter()
            .filter(|copy| selected(table, copy.when(), &before))
            .map(|copy| (copy.target(), before[&copy.source()]))
            .collect::<Vec<_>>();
        choices.extend(writes);
        let inputs = snapshot
            .capture_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), choices))
            .collect::<Vec<_>>();
        let [input] = inputs.as_slice() else {
            panic!("one selected source for this owner snapshot: {inputs:?}")
        };
        let handle = owners
            .remove(&input.owner())
            .expect("snapshot moves its selected instance");
        owners.insert(owner, handle);
        handle
    }

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>) {
            var first: move () -> Unit = move {}
            var second: move () -> Unit = move {}
            for (_ in flags) {
                second = first
                val xs = listOf(1)
                { first = move { read(xs) } }
            }
            var outer: move () -> Unit = move { val a = first()\nval b = second() }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert_eq!(owned.iterations().len(), 1);
    let table = owned.cleanup_conditions();
    let closure = |text| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
            .unwrap()
    };
    let inner = closure("move { read(xs) }");
    let outer = closure("move { val a = first()\nval b = second() }");
    let created = |closure| {
        owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::CreateClosureOwner {
                    owner,
                    closure: actual,
                } if *actual == closure => Some(*owner),
                _ => None,
            })
            .unwrap()
    };
    let inner_owner = created(inner);
    let outer_owner = created(outer);
    let saved = |owner| {
        owned
            .cleanup_steps()
            .iter()
            .filter_map(|(_, action)| match action {
                Action::SaveClosureCapture {
                    owner: actual,
                    target,
                    input,
                } if *actual == owner => Some((*target, *input)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let inner_captures = saved(inner_owner);
    let [inner_capture] = inner_captures.as_slice() else {
        panic!("inner owns one source")
    };
    let CleanupCaptureValue::Owner(xs_owner) = inner_capture.1.value() else {
        panic!("inner captures the round-local owner")
    };
    assert!(
        matches!(table.owner_value(xs_owner), Some(CleanupOwnerValue::Expression { expression, .. })
        if sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()) == Ok("listOf(1)"))
    );
    let outer_captures = saved(outer_owner);
    assert_eq!(outer_captures.len(), 2);
    let [first_source, second_source] = outer_captures.as_slice() else {
        unreachable!()
    };
    let (ClosureCaptureSource::Symbol(first), ClosureCaptureSource::Symbol(second)) =
        (first_source.1.source(), second_source.1.source())
    else {
        panic!("outer captures two bindings")
    };
    let plan = &owned.iterations()[0];
    for incoming in plan.closure_phi_incomings() {
        for binding in incoming.bindings() {
            let layout = plan
                .closure_phis()
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .unwrap();
            assert!(
                binding
                    .root_sources()
                    .iter()
                    .all(|source| layout.root_nodes().contains(&source.node()))
            );
        }
    }
    let phi = |boundary, symbol| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == symbol)
            .unwrap()
            .owner()
    };
    let (header_first, header_second) = (
        phi(IterationPhiBoundary::Header, first),
        phi(IterationPhiBoundary::Header, second),
    );
    let (exit_first, exit_second) = (
        phi(IterationPhiBoundary::Exit, first),
        phi(IterationPhiBoundary::Exit, second),
    );
    let layout = |owner| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.owner() == owner)
            .unwrap()
            .capture_layout()
    };
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let (entry, backedge, exhaustion) = (
        edge(IterationPhiIncomingKind::Entry),
        edge(IterationPhiIncomingKind::Fallthrough),
        edge(IterationPhiIncomingKind::Exhaustion),
    );
    let mut owners = BTreeMap::new();
    let mut choices = BTreeMap::new();
    let mut next_instance = 0_u32;
    for (_, action) in owned.cleanup_steps() {
        if let Action::CreateClosureOwner { owner, .. } = action
            && [
                source_of(entry, header_first),
                source_of(entry, header_second),
            ]
            .contains(owner)
        {
            owners.insert(*owner, next_instance);
            next_instance += 1;
        }
    }
    assert_eq!(
        owners.len(),
        2,
        "both entry instances come from CreateClosureOwner"
    );
    copy_roots(
        table,
        entry,
        [header_first, header_second],
        &mut choices,
        &mut owners,
    );
    let mut captures = BTreeMap::new();
    let mut inner_instances = Vec::new();
    let mut source_instances = Vec::new();
    for round in 0..2 {
        let second_snapshot = source_of(backedge, header_second);
        let old_first = owners[&header_first];
        assert_eq!(
            save_snapshot(table, second_snapshot, &mut choices, &mut owners),
            old_first
        );
        let source_instance = next_instance;
        next_instance += 1;
        owners.insert(xs_owner, source_instance);
        source_instances.push(source_instance);
        let inner_instance = next_instance;
        next_instance += 1;
        owners.insert(inner_owner, inner_instance);
        inner_instances.push(inner_instance);
        assert!(selected(table, inner_capture.1.condition(), &choices));
        let moved_source = owners.remove(&xs_owner).unwrap();
        let inner_position = table
            .capture_slot_value(inner_capture.0)
            .unwrap()
            .position();
        assert!(
            captures
                .insert((inner_instance, inner_position), moved_source)
                .is_none()
        );
        let first_snapshot = source_of(backedge, header_first);
        assert_eq!(
            save_snapshot(table, first_snapshot, &mut choices, &mut owners),
            inner_instance
        );
        assert_capture_transport(
            table,
            backedge,
            (header_first, source_instances[round]),
            layout(header_first),
            &choices,
            &owners,
            &captures,
        );
        if round > 0 {
            assert_capture_transport(
                table,
                backedge,
                (header_second, source_instances[round - 1]),
                layout(header_second),
                &choices,
                &owners,
                &captures,
            );
        }
        copy_roots(
            table,
            backedge,
            [header_first, header_second],
            &mut choices,
            &mut owners,
        );
    }
    assert_eq!(owners[&header_first], inner_instances[1]);
    assert_eq!(owners[&header_second], inner_instances[0]);
    assert_capture_transport(
        table,
        exhaustion,
        (exit_first, source_instances[1]),
        layout(exit_first),
        &choices,
        &owners,
        &captures,
    );
    assert_capture_transport(
        table,
        exhaustion,
        (exit_second, source_instances[0]),
        layout(exit_second),
        &choices,
        &owners,
        &captures,
    );
    copy_roots(
        table,
        exhaustion,
        [exit_first, exit_second],
        &mut choices,
        &mut owners,
    );
    let outer_instance = next_instance;
    owners.insert(outer_owner, outer_instance);
    let outer_positions = outer_captures
        .iter()
        .map(|(target, _)| table.capture_slot_value(*target).unwrap().position())
        .collect::<Vec<_>>();
    for (target, input) in outer_captures {
        assert!(selected(table, input.condition(), &choices));
        let CleanupCaptureValue::Owner(source) = input.value() else {
            panic!("outer reads an exit root handle")
        };
        let handle = owners.remove(&source).unwrap();
        let position = table.capture_slot_value(target).unwrap().position();
        assert!(
            captures
                .insert((outer_instance, position), handle)
                .is_none()
        );
    }
    assert_eq!(captures[&(outer_instance, 0)], inner_instances[1]);
    assert_eq!(captures[&(outer_instance, 1)], inner_instances[0]);
    assert_ne!(inner_instances[0], inner_instances[1]);
    assert_eq!(captures[&(inner_instances[0], 0)], source_instances[0]);
    assert_eq!(captures[&(inner_instances[1], 0)], source_instances[1]);
    assert_ne!(source_instances[0], source_instances[1]);

    let outer_snapshot = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, value, .. } if *value == outer => Some(*owner),
            _ => None,
        })
        .expect("the outer binding saves its formed environment");
    assert_eq!(
        save_snapshot(table, outer_snapshot, &mut choices, &mut owners),
        outer_instance
    );
    assert!(owned.cleanup_steps().iter().any(|(_, action)| {
        matches!(action, Action::Drop(fact) if fact.owner() == Some(outer_snapshot))
    }));
    let root = owners
        .remove(&outer_snapshot)
        .expect("drop consumes the saved root");
    let captured_drops = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            Action::Drop(fact) => fact.instance_address().and_then(|address| {
                let address = table.instance_address(address)?;
                (address.root() == outer_snapshot).then_some((address, fact.capture_slot()?))
            }),
            _ => None,
        })
        .map(|(address, slot)| {
            let parent = address
                .capture_path()
                .iter()
                .fold(root, |instance, position| captures[&(instance, *position)]);
            captures[&(parent, table.capture_slot_value(slot).unwrap().position())]
        })
        .collect::<Vec<_>>();
    let mut pending = vec![(root, false)];
    let mut released = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    while let Some((instance, children_done)) = pending.pop() {
        if children_done {
            released.push(instance);
            continue;
        }
        assert!(
            visited.insert(instance),
            "one owned instance cannot be released twice"
        );
        pending.push((instance, true));
        let positions = if instance == outer_instance {
            outer_positions.clone()
        } else {
            captures
                .range((instance, 0)..=(instance, usize::MAX))
                .map(|(&(parent, position), _)| {
                    assert_eq!(parent, instance);
                    position
                })
                .collect()
        };
        for position in positions {
            let child = captures
                .remove(&(instance, position))
                .expect("release consumes the formed edge");
            pending.push((child, false));
        }
    }
    assert!(
        captures.is_empty(),
        "all formed capture edges were consumed"
    );
    assert_eq!(
        captured_drops,
        released[..4],
        "drop addresses select actual instances"
    );
    assert_eq!(
        released,
        [
            source_instances[0],
            inner_instances[0],
            source_instances[1],
            inner_instances[1],
            outer_instance,
        ],
        "the final drop releases each actual child before its parent, in reverse capture order"
    );
}

#[test]
fn nested_phi_instance_path_uses_original_capture_position() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, IterationCleanupAction as Action, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>) {
            var f: move () -> Unit = move {}
            for (_ in flags) {
                val marker = 1
                val xs = listOf(1)
                val g: () -> Unit = { read(xs) }
                f = move { val keep = marker\nval ignored = g() }
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let plan = &owned.iterations()[0];
    let body = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .unwrap();
    let environment = body
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .find(|environment| {
            environment
                .sources()
                .iter()
                .any(|source| !source.captured().is_empty())
        })
        .unwrap();
    assert!(environment.capture_path().is_empty());
    let nested = environment
        .sources()
        .iter()
        .flat_map(|source| source.captured())
        .flat_map(|origin| origin.environments())
        .next()
        .unwrap();
    assert_eq!(nested.instance_root(), environment.owner());
    assert_eq!(
        nested.capture_path(),
        &[1],
        "marker occupies capture position zero"
    );
    assert_eq!(nested.sources()[0].transport_value(), None);
    let (read_address, read_slot) = nested.sources()[0]
        .transport_read()
        .expect("nested phi source reads a formed environment slot");
    assert_eq!(Some(read_slot), nested.sources()[0].source_capture_slot());
    let read_address = owned
        .cleanup_conditions()
        .instance_address(read_address)
        .unwrap();
    assert_eq!(read_address.root(), nested.instance_root());
    assert_eq!(read_address.capture_path(), nested.capture_path());
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        .unwrap();
    let final_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let addressed_ends = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            Action::EndCaptureLoan {
                instance_address,
                capture_slot,
                ..
            } if *point == DropPoint::CallReturn(final_call) => {
                Some((*instance_address, *capture_slot))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(addressed_ends.len(), 1);
    let address = owned
        .cleanup_conditions()
        .instance_address(addressed_ends[0].0)
        .unwrap();
    assert_eq!(address.root(), exit.owner());
    assert_eq!(address.capture_path(), &[1]);
    let (last_loan_address, last_loan_slot) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(point, action)| match action {
            Action::TestLastCaptureLoan {
                instance_address,
                capture_slot,
                ..
            } if *point == DropPoint::CallReturn(final_call) => {
                Some((*instance_address, *capture_slot))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!((last_loan_address, Some(last_loan_slot)), addressed_ends[0]);
    assert_eq!(
        owned
            .cleanup_conditions()
            .capture_slot_value(last_loan_slot)
            .unwrap()
            .position(),
        0
    );
    let retained_drop = owned
        .drops()
        .iter()
        .find(|fact| {
            fact.point() == DropPoint::CallReturn(final_call)
                && matches!(fact.target(), DropTarget::RetainedSource(_))
        })
        .unwrap();
    assert_eq!(retained_drop.instance_address(), Some(last_loan_address));
    assert_eq!(retained_drop.capture_slot(), Some(last_loan_slot));
    let captured_drop = owned
        .cleanup_steps()
        .iter()
        .find_map(|(point, action)| match action {
            Action::Drop(fact)
                if *point == DropPoint::CallReturn(final_call)
                    && matches!(fact.target(), DropTarget::Captured { owner, .. } if owner == exit.owner()) =>
            {
                Some(*fact)
            }
            _ => None,
        })
        .unwrap();
    let parent = owned
        .cleanup_conditions()
        .instance_address(captured_drop.instance_address().unwrap())
        .unwrap();
    assert_eq!(parent.root(), exit.owner());
    assert!(parent.capture_path().is_empty());
    assert_eq!(
        owned
            .cleanup_conditions()
            .capture_slot_value(captured_drop.capture_slot().unwrap())
            .unwrap()
            .position(),
        1
    );
}

#[test]
fn nested_single_lambda_capture_versions_keep_the_formation_choice() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelectorId,
        IterationCleanupAction, IterationPhiIncomingKind,
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
                selected(table, branches[choices[selector]], choices)
            }
        }
    }

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                var xs = listOf(0)
                if (flag) { xs = listOf(1) } else { xs = listOf(2) }
                val g: move () -> Unit = move { read(xs) }
                val next: move () -> Unit = move { val used = g() }
                f = next
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let table = owned.cleanup_conditions();
    let snapshots = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => {
                Some(table.owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        snapshots.len(),
        3,
        "g, next and f each save the capture choice"
    );
    let copies = snapshots
        .iter()
        .map(|snapshot| *snapshot.copies().first().expect("saved capture choice"))
        .collect::<Vec<_>>();
    assert!(
        snapshots
            .iter()
            .all(|snapshot| snapshot.copies().len() == 1)
    );
    assert_eq!(copies[0].target(), copies[1].source());
    assert_eq!(copies[1].target(), copies[2].source());
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .expect("fallthrough transports f to the next header");
    let inner = incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .flat_map(|origin| origin.environments())
        .find(|environment| environment.capture_path() == [0])
        .expect("f captures the formed g instance");
    let versions = inner.sources();
    assert!(
        table
            .owner_snapshot(inner.instance_root())
            .is_some_and(|snapshot| snapshot
                .copies()
                .iter()
                .any(|copy| copy.target() == copies[2].target())),
        "nested condition must be saved on its carried root instance"
    );
    assert_eq!(versions.len(), 2);
    assert!(versions[0].source_capture_slot().is_some());
    assert_eq!(
        versions[0].source_capture_slot(),
        versions[1].source_capture_slot()
    );
    assert_ne!(versions[0].value(), versions[1].value());
    for version in versions {
        let Some(CleanupCondition::Choice { selector, .. }) =
            table.get(version.input().condition())
        else {
            panic!("nested source version needs a saved choice")
        };
        assert_eq!(*selector, copies[2].target());
    }
    for arm in 0..2 {
        let mut choices = BTreeMap::from([(copies[0].source(), arm)]);
        for copy in &copies {
            let before = choices.clone();
            if selected(table, copy.when(), &before) {
                choices.insert(copy.target(), before[&copy.source()]);
            }
        }
        let initial = versions
            .iter()
            .filter(|version| selected(table, version.input().condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(initial.len(), 1);
        choices.insert(copies[0].source(), 1 - arm);
        let active = versions
            .iter()
            .filter(|version| selected(table, version.input().condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(
            active.len(),
            1,
            "later flag changes cannot replace g's source"
        );
        assert_eq!(active[0].value(), initial[0].value());
    }
}
