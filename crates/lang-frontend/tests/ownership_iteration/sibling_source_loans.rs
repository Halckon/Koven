use super::*;

#[test]
fn loop_carried_sibling_closures_gate_shared_source_drop_on_last_loan() {
    assert_loop_carried_sibling_release(["f", "g"]);
}

#[test]
fn loop_carried_sibling_closures_release_in_reverse_order() {
    assert_loop_carried_sibling_release(["g", "f"]);
}

fn assert_loop_carried_sibling_release(call_order: [&str; 2]) {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupSelection, CleanupSelectorId, CleanupSelectorSource, ClosureCaptureSource,
        DropPoint, DropTarget, IterationCleanupAction, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(&format!(
        "fun read(xs: List<Int>) {{}}\nfun run(flags: List<Boolean>) {{
            var f: () -> Unit = {{}}
            var g: () -> Unit = {{}}
            for (_ in flags) {{
                val xs = listOf(1)
                {{ f = ({{ read(xs) }}) }}
                {{ g = ({{ read(xs) }}) }}
                break
            }}
            val first = {}()
            val second = {}()
        }}",
        call_order[0], call_order[1]
    ));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let break_incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| matches!(incoming.kind(), IterationPhiIncomingKind::Break(_)))
        .unwrap();
    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &std::collections::BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
        }
    }
    let table = owned.cleanup_conditions();
    let mut choices = std::collections::BTreeMap::new();
    assert!(selected(table, break_incoming.condition(), &choices));
    let before = choices.clone();
    let mut writes = Vec::new();
    for binding in break_incoming.bindings() {
        let available = selected(table, binding.available_when(), &before);
        writes.push((binding.availability_selector(), usize::from(available)));
        for write in binding.selector_writes() {
            writes.push((
                write.target(),
                usize::from(selected(table, write.condition(), &before)),
            ));
        }
    }
    choices.extend(writes);
    let sources_by_slot = break_incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter_map(|source| match (source.target(), source.value()) {
            (Some(slot), CleanupCaptureValue::Owner(actual)) => Some((slot, actual)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(sources_by_slot.len(), 2, "f/g need distinct source slots");
    let mut live_loans = std::collections::BTreeMap::new();
    for actual in sources_by_slot.values() {
        *live_loans.entry(*actual).or_insert(0_usize) += 1;
    }
    assert_eq!(live_loans.len(), 1, "f/g borrow one actual xs instance");
    let actual = *live_loans.keys().next().unwrap();
    let early = owned
        .drops()
        .iter()
        .filter(|fact| fact.owner() == Some(actual))
        .collect::<Vec<_>>();
    assert!(
        early.is_empty(),
        "both closures must keep xs live: {early:?}"
    );
    let premature = owned
        .drops()
        .iter()
        .filter(|fact| {
            matches!(fact.target(), DropTarget::RetainedSource(_))
                && !matches!(fact.point(), DropPoint::CallReturn(_))
        })
        .collect::<Vec<_>>();
    assert!(
        premature.is_empty(),
        "xs must survive the break: {premature:?}"
    );
    let call = |name| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, expression)| {
                (sources.slice(expression.span()).unwrap() == name).then_some(id)
            })
            .unwrap()
    };
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = |name| {
        names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id()
    };
    let source_drops = |call| {
        owned
            .drops()
            .iter()
            .filter(|fact| {
                fact.point() == DropPoint::CallReturn(call)
                    && matches!(
                        fact.target(),
                        DropTarget::RetainedSource(ClosureCaptureSource::Symbol(_))
                    )
            })
            .collect::<Vec<_>>()
    };
    fn contains_selector(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        target: lang_frontend::ownership_checking::CleanupSelectorId,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always | CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                *selector == target
                    || branches
                        .iter()
                        .any(|branch| contains_selector(table, *branch, target))
            }
        }
    }
    let mut executed_source_drops = 0;
    for (index, binding_name) in call_order.into_iter().enumerate() {
        let name = if binding_name == "f" { "f()" } else { "g()" };
        let call = call(name);
        let drops = source_drops(call);
        assert_eq!(drops.len(), 1, "{name}: one retained source obligation");
        let actions = owned
            .cleanup_steps()
            .iter()
            .filter(|(point, _)| *point == DropPoint::CallReturn(call))
            .map(|(_, action)| action)
            .collect::<Vec<_>>();
        let drop = drops[0];
        let owner = drop.owner().unwrap();
        let phi = owned.iterations()[0]
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol(binding_name)
            })
            .unwrap();
        let (origin, source) = phi
            .origins()
            .iter()
            .flat_map(|origin| origin.sources().iter().map(move |source| (origin, source)))
            .find(|(_, source)| source.owner() == owner)
            .expect("call must release this binding's exit source slot");
        assert_eq!(
            choices[&origin.selector()],
            1,
            "{name}: selected exit origin"
        );
        let (test_index, selector) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                IterationCleanupAction::TestLastCaptureLoan {
                    owner: actual,
                    selector,
                    ..
                } if *actual == owner => Some((index, *selector)),
                _ => None,
            })
            .expect("retained source must query the last active loan");
        let ended = actions
            .iter()
            .enumerate()
            .filter(|(_, action)| {
                matches!(action, IterationCleanupAction::EndCaptureLoan {
                    owner: environment_owner,
                    closure,
                    source: capture_source,
                    value: CleanupCaptureValue::Owner(actual),
                    ..
                } if *environment_owner == phi.owner()
                    && *closure == origin.closure()
                    && *capture_source == source.source()
                    && *actual == owner)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            ended.len(),
            1,
            "{name}: end exactly this environment's loan"
        );
        assert_eq!(
            actions.iter().filter(|action| matches!(action,
                IterationCleanupAction::EndCaptureLoan { value: CleanupCaptureValue::Owner(actual), .. }
                    if *actual == owner)).count(),
            1,
            "{name}: no other environment may consume this source slot"
        );
        assert!(ended.iter().all(|index| *index < test_index));
        for index in &ended {
            let IterationCleanupAction::EndCaptureLoan { condition, .. } = actions[*index] else {
                unreachable!();
            };
            assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
        }
        let IterationCleanupAction::TestLastCaptureLoan { condition, .. } = actions[test_index]
        else {
            unreachable!();
        };
        assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
        let drop_index = actions
            .iter()
            .position(
                |action| matches!(action, IterationCleanupAction::Drop(actual) if actual == drop),
            )
            .unwrap();
        assert!(
            test_index < drop_index,
            "query must precede the physical drop"
        );
        let selector_info = owned.cleanup_conditions().selector(selector).unwrap();
        assert_eq!(selector_info.selection(), CleanupSelection::LastCaptureLoan);
        assert_eq!(
            selector_info.source(),
            CleanupSelectorSource::CaptureLoan { owner }
        );
        let actual = sources_by_slot[&owner];
        let remaining = live_loans.get_mut(&actual).unwrap();
        *remaining -= ended.len();
        assert_eq!(*remaining == 0, index == 1, "{name}: last-loan choice");
        choices.insert(selector, usize::from(*remaining == 0));
        assert!(
            contains_selector(
                owned.cleanup_conditions(),
                drop.condition().unwrap(),
                selector
            ),
            "{name}: release is conditional on the actual source instance's last loan"
        );
        let executes = selected(table, drop.condition().unwrap(), &choices);
        assert_eq!(executes, index == 1, "{name}: guarded source drop");
        executed_source_drops += usize::from(executes);
    }
    assert_eq!(executed_source_drops, 1, "shared xs drops exactly once");
}

#[test]
fn nested_capture_keeps_branch_local_source_until_outer_call_returns() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureMode, ClosureCaptureSource, DropPoint, DropTarget,
        IterationCleanupAction as Action,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flag: Boolean) {\nval f: move () -> Unit = if (flag) {\nval xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nmove { g() }\n} else move {}\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .and_then(|capture| match capture.source() {
            ClosureCaptureSource::Symbol(symbol) => Some(symbol),
            ClosureCaptureSource::This => None,
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::CallReturn(call)).then_some(action))
        .collect::<Vec<_>>();
    let loan_end = actions
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { source: actual, .. } if *actual == ClosureCaptureSource::Symbol(source)))
        .unwrap();
    let source_drop = actions
        .iter()
        .position(|action| matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(source)))
        .unwrap();
    assert!(loan_end < source_drop, "{actions:?}");
    assert!(
        owned.drops().iter().all(|fact| {
            fact.target() != DropTarget::Named(source)
                || fact.point() == DropPoint::CallReturn(call)
        }),
        "the branch-local source must remain owned until f returns: {:?}",
        owned.drops()
    );
}

#[test]
fn sibling_nested_captures_release_shared_source_after_both_loans() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureMode, ClosureCaptureSource, DropPoint, DropTarget,
        IterationCleanupAction as Action,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() { val xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nval h: () -> Unit = { read(xs) }\nval f: move () -> Unit = move { val first = g()\nval second = h() }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .unwrap()
        .source();
    let ClosureCaptureSource::Symbol(symbol) = source else {
        unreachable!()
    };
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::CallReturn(call)).then_some(action))
        .collect::<Vec<_>>();
    let endings = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            matches!(action, Action::EndCaptureLoan { source: actual, .. } if *actual == source)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let drops = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(symbol))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(endings.len(), 2, "{actions:?}");
    assert_eq!(drops.len(), 1, "{actions:?}");
    assert!(endings[1] < drops[0], "{actions:?}");
}

#[test]
fn sibling_phi_sources_test_last_loan_before_the_next_alias_ends() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupSelectorId, ClosureCaptureMode, DropPoint, DropTarget,
        IterationCleanupAction as Action, IterationPhiIncomingKind,
    };
    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &std::collections::BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
        }
    }
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {\nvar g: () -> Unit = {}\nvar h: () -> Unit = {}\nfor (_ in flags) { val xs = listOf(1)\n{ g = ({ read(xs) }) }\n{ h = ({ read(xs) }) }\nbreak }\nval f: move () -> Unit = move { val first = g()\nval second = h() }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| matches!(incoming.kind(), IterationPhiIncomingKind::Break(_)))
        .unwrap();
    let slots = incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter_map(|source| match (source.target(), source.value()) {
            (Some(slot), CleanupCaptureValue::Owner(actual)) => Some((slot, actual)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(slots.len(), 2, "two distinct phi slots");
    assert_eq!(
        slots
            .values()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1,
        "both slots refer to the same actual xs"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::CallReturn(call)).then_some(action))
        .collect::<Vec<_>>();
    let endings = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| match action {
            Action::EndCaptureLoan {
                value: CleanupCaptureValue::Owner(owner),
                ..
            } if slots.contains_key(owner) => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    let tests = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| match action {
            Action::TestLastCaptureLoan { owner, .. } if slots.contains_key(owner) => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    let drops = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::RetainedSource(_)))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(endings.len(), 2, "{actions:?}");
    assert_eq!(tests.len(), 2, "{actions:?}");
    assert_eq!(drops.len(), 2, "two guarded candidates: {actions:?}");
    let table = owned.cleanup_conditions();
    let mut locations = actions
        .iter()
        .filter_map(|action| match action {
            Action::TestLastCaptureLoan {
                instance_address,
                capture_slot,
                ..
            } => {
                let address = table.instance_address(*instance_address)?;
                let slot = table.capture_slot_value(*capture_slot)?;
                Some((
                    address.root(),
                    address.capture_path().to_vec(),
                    slot.position(),
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    locations.sort_by(|left, right| left.1.cmp(&right.1));
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].0, locations[1].0);
    assert_eq!(locations[0].1, [0]);
    assert_eq!(locations[1].1, [1]);
    assert_eq!(locations[0].2, 0);
    assert_eq!(locations[1].2, 0);
    let retained_locations = actions
        .iter()
        .filter_map(|action| match action {
            Action::Drop(fact) if matches!(fact.target(), DropTarget::RetainedSource(_)) => {
                Some((fact.instance_address()?, fact.capture_slot()?))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(retained_locations.len(), 2);
    for (address, slot) in retained_locations {
        let instance = table.instance_address(address).unwrap();
        let capture = table.capture_slot_value(slot).unwrap();
        assert!(locations.iter().any(|location| {
            location.0 == instance.root()
                && location.1 == instance.capture_path()
                && location.2 == capture.position()
        }));
    }
    assert!(
        endings[0] < tests[0] && tests[0] < endings[1],
        "{actions:?}"
    );
    assert!(endings[1] < tests[1] && tests[1] < drops[0], "{actions:?}");

    // Replay only instances produced by Create/Save and selected by the break phi.
    let source_owner = *slots.values().next().unwrap();
    assert!(!slots.contains_key(&source_owner));
    let source_instance = source_owner.index() + 1;
    let mut values = std::collections::BTreeMap::from([(source_owner, source_instance)]);
    for (_, action) in owned.cleanup_steps() {
        if let Action::CreateClosureOwner { owner, .. } = action {
            values.insert(*owner, owner.index() + 1);
        }
    }
    for (_, action) in owned.cleanup_steps() {
        if let Action::SaveOwnerSnapshot { owner, value, .. } = action {
            let snapshot = table.owner_snapshot(*owner).unwrap();
            assert_eq!(snapshot.value(), *value);
            if let [capture] = snapshot.capture_inputs()
                && let Some(&instance) = values.get(&capture.owner())
            {
                values.insert(*owner, instance);
            }
        }
    }
    for binding in incoming.bindings() {
        let created_slot = binding
            .origins()
            .iter()
            .flat_map(|origin| origin.environments())
            .flat_map(|environment| environment.sources())
            .find_map(|source| source.source_capture_slot())
            .unwrap();
        let created_owner = table
            .capture_slot_value(created_slot)
            .unwrap()
            .environment();
        let incoming_value = binding.values().first().unwrap();
        assert_eq!(values[&incoming_value.source()], values[&created_owner]);
        values.insert(binding.target(), values[&incoming_value.source()]);
    }
    let f_owner = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture { owner, input, .. }
                if input.mode() == ClosureCaptureMode::Owned =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let entry = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let mut choices = std::collections::BTreeMap::new();
    let apply_incoming =
        |edge: &lang_frontend::ownership_checking::IterationPhiIncoming,
         choices: &mut std::collections::BTreeMap<CleanupSelectorId, usize>| {
            assert!(selected(table, edge.condition(), choices));
            let before = choices.clone();
            let mut writes = Vec::new();
            for binding in edge.bindings() {
                writes.push((
                    binding.availability_selector(),
                    usize::from(selected(table, binding.available_when(), &before)),
                ));
                for write in binding.selector_writes() {
                    writes.push((
                        write.target(),
                        usize::from(selected(table, write.condition(), &before)),
                    ));
                }
            }
            choices.extend(writes);
        };
    apply_incoming(entry, &mut choices);
    for after_break in [false, true] {
        if after_break {
            apply_incoming(incoming, &mut choices);
        }
        for (_, action) in owned.cleanup_steps() {
            if let Action::SaveOwnerSnapshot {
                owner, condition, ..
            } = action
            {
                let snapshot = table.owner_snapshot(*owner).unwrap();
                let is_f = snapshot
                    .capture_inputs()
                    .first()
                    .is_some_and(|input| input.owner() == f_owner);
                if is_f != after_break
                    || condition.is_some_and(|guard| !selected(table, guard, &choices))
                {
                    continue;
                }
                let before = choices.clone();
                for copy in snapshot.copies() {
                    if selected(table, copy.when(), &before) {
                        choices.insert(copy.target(), before[&copy.source()]);
                    }
                }
            }
        }
    }
    let mut captures = std::collections::BTreeMap::new();
    let mut formed_loans = 0;
    for (_, action) in owned.cleanup_steps() {
        if let Action::SaveClosureCapture {
            owner,
            target,
            input,
        } = action
            && selected(table, input.condition(), &choices)
            && let CleanupCaptureValue::Owner(source) = input.value()
        {
            let position = table.capture_slot_value(*target).unwrap().position();
            captures.insert((values[owner], position), values[&source]);
            if input.mode() == ClosureCaptureMode::Shared {
                assert_eq!(source, source_owner);
                formed_loans += 1;
            }
        }
    }
    assert_eq!(captures.len(), 4, "two source and two owned capture edges");
    assert_eq!(formed_loans, 2);
    let source_at = |address, slot| {
        let address = table.instance_address(address).unwrap();
        let mut instance = values[&address.root()];
        for &position in address.capture_path() {
            instance = captures[&(instance, position)];
        }
        let position = table.capture_slot_value(slot).unwrap().position();
        captures[&(instance, position)]
    };
    for (point, action) in owned.cleanup_steps() {
        if *point == DropPoint::CallReturn(call) {
            continue;
        }
        if let Action::EndCaptureLoan {
            value: CleanupCaptureValue::Owner(owner),
            instance_address,
            capture_slot,
            condition,
            ..
        } = action
            && (*owner == source_owner || slots.contains_key(owner))
            && condition.is_none_or(|guard| selected(table, guard, &choices))
        {
            let slot = capture_slot.expect("the tracked source loan must have a capture slot");
            assert_ne!(
                source_at(*instance_address, slot),
                source_instance,
                "the selected source loan ended outside f() return: {point:?} {action:?}"
            );
        }
    }
    let mut loans = formed_loans;
    let mut last_choices = Vec::new();
    let mut last_by_owner = std::collections::BTreeMap::new();
    let mut released = Vec::new();
    for action in actions {
        match action {
            Action::EndCaptureLoan {
                value: CleanupCaptureValue::Owner(owner),
                instance_address,
                capture_slot: Some(slot),
                condition,
                ..
            } if slots.contains_key(owner)
                && condition.is_none_or(|guard| selected(table, guard, &choices)) =>
            {
                assert_eq!(source_at(*instance_address, *slot), source_instance);
                loans -= 1;
            }
            Action::TestLastCaptureLoan {
                owner,
                instance_address,
                capture_slot,
                selector,
                condition,
                ..
            } if slots.contains_key(owner)
                && condition.is_none_or(|guard| selected(table, guard, &choices)) =>
            {
                let actual = source_at(*instance_address, *capture_slot);
                assert_eq!(actual, source_instance);
                let last = loans == 0;
                choices.insert(*selector, usize::from(last));
                last_choices.push(last);
                last_by_owner.insert(*owner, (*selector, last));
            }
            Action::Drop(fact) if matches!(fact.target(), DropTarget::RetainedSource(_)) => {
                let owner = fact.owner().unwrap();
                let (selector, last) = last_by_owner[&owner];
                let actual = source_at(
                    fact.instance_address().unwrap(),
                    fact.capture_slot().unwrap(),
                );
                assert_eq!(actual, source_instance);
                let guard = fact.condition().unwrap();
                let mut without_last_loan = choices.clone();
                without_last_loan.insert(selector, 0);
                assert!(!selected(table, guard, &without_last_loan));
                let mut with_last_loan = choices.clone();
                with_last_loan.insert(selector, 1);
                assert!(selected(table, guard, &with_last_loan));
                if selected(table, guard, &choices) {
                    assert!(last);
                    released.push(actual);
                }
            }
            _ => {}
        }
    }
    assert_eq!(loans, 0);
    assert_eq!(last_choices, [false, true]);
    assert_eq!(released, [source_instance]);
}
