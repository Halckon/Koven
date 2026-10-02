use super::*;

#[test]
fn loop_carried_nullable_closure_can_be_taken_called_and_replaced() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    // flags 为 [true, false] 时：第一轮保存环境；第二轮先形成新环境，
    // 再调用旧环境并补回 saved。
    // 不能因为同一静态 lambda 同时存在多个实例而拒绝合法源码。
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}
fun <T> none(): T? { return null }
fun run(flags: List<Boolean>, xs: List<Int>, ys: List<Int>) {
    var saved = none<move () -> Unit>()
    for (flag in flags) {
        val chosen: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })
        val current: move () -> Unit = move {
            val inner: move () -> Unit = move { val used = chosen() }
            val used = inner()
        }
        if (flag) { saved = current } else {
            val old = saved!!
            { val used = old() }
            saved = current
        }
    }
}",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let old_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("old()")).then_some(id))
        .unwrap();
    let old_owner = owned
        .drops()
        .iter()
        .find(|fact| {
            fact.point() == DropPoint::CallReturn(old_call)
                && fact.owner().is_some()
                && sources.slice(fact.value_origin()) == Ok("old")
        })
        .expect("the extracted old value needs a return-point cleanup")
        .owner();
    let old_drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.owner() == old_owner)
        .collect::<Vec<_>>();
    assert_eq!(
        old_drops.len(),
        1,
        "the old owner must not also be released before or after its call: {old_drops:?}"
    );
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(old_call)
            && matches!(action, IterationCleanupAction::PassClosureEnvironment { callee, closure: None }
                if Some(*callee) == old_owner)
    }), "the stable evaluated old value must provide the called environment");
}

#[test]
fn loop_carried_conditional_closure_preserves_its_sources_until_the_exit_call() {
    assert_loop_carried_sources_survive_exit("");
}

#[test]
fn loop_carried_conditional_closure_preserves_sources_on_continue() {
    assert_loop_carried_sources_survive_exit("continue");
}

#[test]
fn loop_carried_conditional_closure_preserves_sources_on_break() {
    assert_loop_carried_sources_survive_exit("break");
}

fn assert_loop_carried_sources_survive_exit(transfer: &str) {
    use lang_frontend::ownership_checking::{
        CleanupOwnerValue, ClosureCaptureSource, DropPoint, DropTarget,
    };

    let (sources, parsed, owned) = checked(&format!(
        "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {{ var f: () -> Unit = {{}}\nfor (flag in flags) {{ f = if (flag) ({{ read(xs) }}) else ({{ read(ys) }})\n{transfer} }}\nval used = f() }}",
    ));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    for name in ["xs", "ys"] {
        let symbol = owned
            .captures()
            .iter()
            .find_map(|capture| {
                if sources.slice(capture.reference_span()).unwrap() != name {
                    return None;
                }
                match capture.source() {
                    ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                    ClosureCaptureSource::This => None,
                }
            })
            .unwrap();
        assert!(
            owned.drops().iter().any(|drop| {
                if drop.point() != DropPoint::CallReturn(call) || drop.condition().is_none() {
                    return false;
                }
                match drop.target() {
                    DropTarget::Named(owner) => owner == symbol,
                    DropTarget::RetainedSource(ClosureCaptureSource::Symbol(owner)) => {
                        owner == symbol
                            && matches!(
                                drop.owner().and_then(|owner| owned
                                    .cleanup_conditions()
                                    .owner_value(owner)),
                                Some(CleanupOwnerValue::IterationPhiSourceOwner { .. })
                            )
                    }
                    _ => false,
                }
            }),
            "{name}: the final selected source must remain conditionally owned until f() returns: {:?}",
            owned.drops()
        );
    }
}

#[test]
fn loop_carried_branch_choices_release_the_previous_source_after_two_rounds() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureSlotId, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
        CleanupConditions, CleanupOwnerValue, CleanupOwnerValueId, CleanupSelection,
        CleanupSelectorId, CleanupSelectorSource, ClosureCaptureSource, DropPoint, DropTarget,
        IterationCleanupAction as Action, IterationClosurePhiBinding, IterationPhiBoundary,
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

    #[derive(Default)]
    struct FormedCaptures {
        slots: BTreeMap<(u32, CleanupCaptureSlotId), u32>,
        layouts: BTreeMap<u32, CleanupOwnerValueId>,
    }

    fn replay(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        layouts: &[IterationClosurePhiBinding],
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
        phi_capture_slots: &mut BTreeMap<CleanupCaptureSlotId, u32>,
        formed: &FormedCaptures,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        let before_phi_capture_slots = phi_capture_slots.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let mut choice_writes = Vec::new();
        let mut owner_writes = Vec::new();
        let mut owner_clears = Vec::new();
        let mut capture_writes = Vec::new();
        let mut capture_clears = Vec::new();
        for binding in incoming.bindings() {
            owner_clears.push(binding.target());
            capture_clears.extend_from_slice(binding.capture_slots_to_clear());
            let layout = layouts
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .expect("each incoming binding has a preallocated layout");
            for origin in layout.origins() {
                for source in origin.sources() {
                    owner_clears.push(source.owner());
                }
            }
            let available = selected(table, binding.available_when(), &before_choices);
            choice_writes.push((binding.availability_selector(), usize::from(available)));
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(values.len(), usize::from(available));
            let roots = binding
                .root_sources()
                .iter()
                .filter(|source| selected(table, source.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert!(
                roots
                    .iter()
                    .all(|source| layout.root_nodes().contains(&source.node()))
            );
            if !layout.root_nodes().is_empty() {
                assert_eq!(roots.len(), values.len());
                if let [value] = values.as_slice() {
                    assert_eq!(roots[0].source(), value.source());
                }
            }
            let actual = values.first().map(|value| before_owners[&value.source()]);
            if let Some(actual) = actual {
                owner_writes.push((binding.target(), actual));
            }
            for write in binding.selector_writes() {
                choice_writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            let mut origins = Vec::new();
            for origin in binding.origins() {
                let present = selected(table, origin.condition(), &before_choices);
                if !present {
                    continue;
                }
                let environments = origin
                    .environments()
                    .iter()
                    .filter(|environment| selected(table, environment.condition(), &before_choices))
                    .collect::<Vec<_>>();
                assert_eq!(environments.len(), 1);
                let environment = environments[0];
                origins.push(before_owners[&environment.owner()]);
                for source in environment.sources() {
                    if !selected(table, source.input().condition(), &before_choices) {
                        continue;
                    }
                    if let Some(target) = source.target() {
                        let CleanupCaptureValue::Owner(value) = source.value() else {
                            panic!("carried source must read an actual owner");
                        };
                        let CleanupCaptureValue::Environment {
                            owner: source_environment,
                            slot: source_slot,
                            ..
                        } = source
                            .transport_value()
                            .expect("tracked source must publish its transport location")
                        else {
                            panic!("phi must read the formed environment capture slot");
                        };
                        assert_eq!(source_environment, environment.owner());
                        let source_layout = table.capture_slot_value(source_slot).unwrap();
                        assert_eq!(source_layout.source(), source.input().source());
                        let captured = match table.owner_value(source_layout.environment()).unwrap()
                        {
                            CleanupOwnerValue::Closure { .. } => {
                                assert_eq!(
                                    source_layout.environment(),
                                    formed.layouts[&before_owners[&source_environment]]
                                );
                                formed.slots[&(before_owners[&source_environment], source_slot)]
                            }
                            CleanupOwnerValue::IterationPhi { .. } => {
                                assert_eq!(source_layout.environment(), source_environment);
                                before_phi_capture_slots[&source_slot]
                            }
                            value => panic!("unexpected transport layout: {value:?}"),
                        };
                        assert_eq!(captured, before_owners[&value]);
                        owner_writes.push((target, captured));
                        capture_writes.push((
                            source
                                .capture_slot()
                                .expect("tracked source has a phi capture slot"),
                            captured,
                        ));
                    }
                }
            }
            if !binding.origins().is_empty() {
                assert_eq!(origins, actual.into_iter().collect::<Vec<_>>());
            }
        }
        choices.extend(choice_writes);
        for owner in owner_clears {
            owners.remove(&owner);
        }
        owners.extend(owner_writes);
        for slot in capture_clears {
            phi_capture_slots.remove(&slot);
        }
        phi_capture_slots.extend(capture_writes);
    }

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) { var f: () -> Unit = {}\nfor (flag in flags) { f = if (flag) ({ read(xs) }) else ({ read(ys) }) }\nval used = f() }",
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
    let captured_symbol = |name| {
        owned
            .captures()
            .iter()
            .find_map(|capture| {
                (sources.slice(capture.reference_span()).unwrap() == name).then(|| {
                    match capture.source() {
                        ClosureCaptureSource::Symbol(symbol) => symbol,
                        ClosureCaptureSource::This => panic!("fixture captures a named source"),
                    }
                })
            })
            .unwrap()
    };
    let xs = captured_symbol("xs");
    let ys = captured_symbol("ys");
    let f = symbol("f");
    let plan = &owned.iterations()[0];
    let phi = |boundary, name| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == name)
            .unwrap()
    };
    let header_f = phi(IterationPhiBoundary::Header, f);
    let exit_f = phi(IterationPhiBoundary::Exit, f);
    let header_xs = phi(IterationPhiBoundary::Header, xs);
    let header_ys = phi(IterationPhiBoundary::Header, ys);
    let header_origin = |name| {
        header_f
            .origins()
            .iter()
            .find(|origin| {
                origin
                    .sources()
                    .iter()
                    .any(|source| source.source() == ClosureCaptureSource::Symbol(name))
            })
            .unwrap()
    };
    let exit_origin = |name| {
        exit_f
            .origins()
            .iter()
            .find(|origin| {
                origin
                    .sources()
                    .iter()
                    .any(|source| source.source() == ClosureCaptureSource::Symbol(name))
            })
            .unwrap()
    };
    let exit_source = |name| {
        exit_origin(name)
            .sources()
            .iter()
            .find(|source| source.source() == ClosureCaptureSource::Symbol(name))
            .unwrap()
            .owner()
    };
    let exit_xs = exit_source(xs);
    let exit_ys = exit_source(ys);
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let entry = edge(IterationPhiIncomingKind::Entry);
    let backedge = edge(IterationPhiIncomingKind::Fallthrough);
    let exhaustion = edge(IterationPhiIncomingKind::Exhaustion);
    let table = owned.cleanup_conditions();
    for (incoming, phi) in [
        (entry, header_f),
        (backedge, header_f),
        (exhaustion, exit_f),
    ] {
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == phi.owner())
            .unwrap();
        let expected = phi
            .origins()
            .iter()
            .flat_map(|origin| {
                origin.sources().iter().map(|source| {
                    table
                        .phi_capture_slot(phi.owner(), origin.closure(), source.source())
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), 2, "both alternative captures need clearing");
        assert_eq!(binding.capture_slots_to_clear(), expected);
    }
    let (save_point, save_condition, snapshot) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(point, action)| match action {
            Action::SaveOwnerSnapshot {
                owner,
                value,
                condition,
            } => {
                let snapshot = table.owner_snapshot(*owner).unwrap();
                assert_eq!(snapshot.value(), *value);
                assert_eq!(*point, DropPoint::AfterExpression(*value));
                Some((*point, *condition, snapshot))
            }
            _ => None,
        })
        .unwrap();
    let save_index = owned
        .cleanup_steps()
        .iter()
        .position(|(point, action)| {
            *point == save_point
                && matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == snapshot.owner())
        })
        .unwrap();
    let commit_index = owned
        .cleanup_steps()
        .iter()
        .position(|(point, action)| {
            *point == save_point
                && matches!(action, Action::CommitOwnerSnapshot { owner, target } if *owner == snapshot.owner() && *target == f)
        })
        .unwrap();
    assert!(save_index < commit_index);
    for (index, (point, action)) in owned.cleanup_steps().iter().enumerate() {
        if *point == save_point && matches!(action, Action::EndCaptureLoan { .. }) {
            assert!(save_index < index && index < commit_index);
        }
    }
    let control = snapshot
        .copies()
        .iter()
        .find_map(|copy| {
            let selector = table.selector(copy.source()).unwrap();
            (selector.source() == CleanupSelectorSource::Control(snapshot.value())
                && selector.selection() == CleanupSelection::Branch)
                .then_some(copy.source())
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let source_symbol = |target| match target {
        DropTarget::Named(name)
        | DropTarget::RetainedSource(ClosureCaptureSource::Symbol(name))
            if name == xs || name == ys =>
        {
            Some(name)
        }
        DropTarget::Captured {
            source: ClosureCaptureSource::Symbol(name),
            ..
        } if name == xs || name == ys => Some(name),
        _ => None,
    };
    let entry_source = |target| {
        entry
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap()
            .values()[0]
            .source()
    };
    let source_slots = [
        entry_source(header_xs.owner()),
        entry_source(header_ys.owner()),
        header_xs.owner(),
        header_ys.owner(),
        header_origin(xs).sources()[0].owner(),
        header_origin(ys).sources()[0].owner(),
        exit_xs,
        exit_ys,
    ];
    for (point, action) in owned.cleanup_steps() {
        match action {
            Action::Drop(fact)
                if source_symbol(fact.target()).is_some()
                    || fact
                        .owner()
                        .is_some_and(|owner| source_slots.contains(&owner)) =>
            {
                assert!(
                    *point == exhaustion.point()
                        || *point == DropPoint::CallReturn(call)
                        || (*point == save_point
                            && matches!(fact.target(), DropTarget::RetainedSource(_))),
                    "either source can be captured in a later round: {point:?} {fact:?}"
                );
            }
            Action::EndCaptureLoan { source, value, .. }
                if matches!(source, ClosureCaptureSource::Symbol(name) if *name == xs || *name == ys)
                    || matches!(value, CleanupCaptureValue::Owner(owner) if source_slots.contains(owner)) =>
            {
                assert!(
                    *point == save_point || *point == DropPoint::CallReturn(call),
                    "a carried capture must survive until replacement or call return: {point:?} {action:?}"
                );
            }
            Action::TestLastCaptureLoan { owner, .. } if source_slots.contains(owner) => {
                assert!(*point == save_point || *point == DropPoint::CallReturn(call));
            }
            _ => {}
        }
    }

    for order in [[xs, ys], [ys, xs]] {
        let mut choices = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let mut phi_capture_slots = BTreeMap::new();
        let mut formed = FormedCaptures::default();
        let mut live_loans = BTreeMap::from([(1_u32, 0_usize), (2_u32, 0_usize)]);
        for (target, instance) in [
            (header_f.owner(), 0),
            (header_xs.owner(), 1),
            (header_ys.owner(), 2),
        ] {
            let source = entry
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
                .values()[0]
                .source();
            owners.insert(source, instance);
        }
        replay(
            table,
            entry,
            plan.closure_phis(),
            &mut choices,
            &mut owners,
            &mut phi_capture_slots,
            &formed,
        );
        for (round, chosen) in order.into_iter().enumerate() {
            choices.insert(control, usize::from(chosen == ys));
            let before = choices.clone();
            assert!(
                save_condition.is_none_or(|guard| selected(table, guard, &before)),
                "the selected RHS must save its selectors before replacing f"
            );
            for copy in snapshot.copies() {
                if selected(table, copy.when(), &before) {
                    choices.insert(copy.target(), before[&copy.source()]);
                }
            }
            let active_inputs = snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(table, input.condition(), &before))
                .collect::<Vec<_>>();
            assert_eq!(active_inputs.len(), 1);
            let Some(CleanupOwnerValue::Closure {
                expression, inputs, ..
            }) = table.owner_value(active_inputs[0].owner())
            else {
                panic!("selected RHS must create a closure environment");
            };
            assert_eq!(inputs.len(), 1);
            let CleanupCaptureValue::Owner(source) = inputs[0].value() else {
                panic!("selected closure must capture the source owner");
            };
            assert_eq!(
                source,
                if chosen == xs {
                    header_xs.owner()
                } else {
                    header_ys.owner()
                }
            );
            let source_instance = owners[&source];
            let formed_owner = active_inputs[0].owner();
            let captures = owned
                .cleanup_steps()
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| match action {
                    Action::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == formed_owner => Some((index, *point, *target, *input)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let &[(capture_index, capture_point, formed_slot, formed_input)] = captures.as_slice()
            else {
                panic!("the selected closure saves exactly one capture at formation");
            };
            let creates = owned
                .cleanup_steps()
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| match action {
                    Action::CreateClosureOwner { owner, closure } if *owner == formed_owner => {
                        Some((index, *point, *closure))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let &[(create_index, create_point, created_closure)] = creates.as_slice() else {
                panic!("the selected closure creates exactly one environment");
            };
            assert_eq!(created_closure, *expression);
            assert_eq!(create_point, DropPoint::AfterExpression(*expression));
            assert_eq!(capture_point, create_point);
            assert!(create_index < capture_index && capture_index < save_index);
            assert!(save_index < commit_index);
            assert_eq!(formed_input, inputs[0]);
            assert!(selected(table, formed_input.condition(), &choices));
            assert!(
                formed
                    .slots
                    .insert((10 + round as u32, formed_slot), source_instance)
                    .is_none()
            );
            assert!(
                formed
                    .layouts
                    .insert(10 + round as u32, formed_owner)
                    .is_none()
            );
            owners.insert(formed_owner, 10 + round as u32);
            *live_loans.get_mut(&source_instance).unwrap() += 1;
            let ended = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| match action {
                    Action::EndCaptureLoan {
                        owner,
                        closure,
                        condition,
                        source,
                        value,
                        ..
                    } if *point == save_point
                        && condition.is_none_or(|guard| selected(table, guard, &choices)) =>
                    {
                        let old = order[round.checked_sub(1).expect("entry f has no capture")];
                        assert_eq!(*owner, header_f.owner());
                        assert_eq!(*closure, header_origin(old).closure());
                        assert_eq!(*source, ClosureCaptureSource::Symbol(old));
                        let old_owner = if old == xs {
                            header_xs.owner()
                        } else {
                            header_ys.owner()
                        };
                        assert_eq!(*value, CleanupCaptureValue::Owner(old_owner));
                        let old_instance = owners[&old_owner];
                        let count = live_loans.get_mut(&old_instance).unwrap();
                        assert_eq!(*count, 1);
                        *count -= 1;
                        Some(*source)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                ended,
                if round == 0 {
                    Vec::new()
                } else {
                    vec![ClosureCaptureSource::Symbol(order[round - 1])]
                },
                "replacement releases the preceding round's capture"
            );
            let mut last_loan_true = choices.clone();
            for (point, action) in owned.cleanup_steps() {
                if *point == save_point
                    && let Action::TestLastCaptureLoan {
                        condition,
                        selector,
                        ..
                    } = action
                {
                    assert!(
                        condition.is_some_and(|guard| !selected(table, guard, &choices)),
                        "available named sources must not query retained-source cleanup"
                    );
                    choices.insert(*selector, 0);
                    last_loan_true.insert(*selector, 1);
                }
            }
            for (point, action) in owned.cleanup_steps() {
                if *point == save_point
                    && let Action::Drop(fact) = action
                    && matches!(fact.target(), DropTarget::RetainedSource(_))
                {
                    assert!(
                        fact.condition().is_some_and(|guard| !selected(
                            table,
                            guard,
                            &last_loan_true
                        )),
                        "named availability must forbid retained drop even if last-loan were true"
                    );
                }
            }
            owners.insert(snapshot.owner(), 10 + round as u32);
            replay(
                table,
                backedge,
                plan.closure_phis(),
                &mut choices,
                &mut owners,
                &mut phi_capture_slots,
                &formed,
            );
            assert_eq!(owners[&header_f.owner()], 10 + round as u32);
            let inactive = if chosen == xs { ys } else { xs };
            assert!(
                !owners.contains_key(&header_origin(inactive).sources()[0].owner()),
                "the inactive capture source must not retain a prior phi value"
            );
            let capture_slot = |name| {
                table
                    .phi_capture_slot(
                        header_f.owner(),
                        header_origin(name).closure(),
                        ClosureCaptureSource::Symbol(name),
                    )
                    .unwrap()
            };
            assert_eq!(phi_capture_slots[&capture_slot(chosen)], source_instance);
            assert!(
                !phi_capture_slots.contains_key(&capture_slot(inactive)),
                "the unselected alternative must clear its previous capture slot"
            );
        }
        let last = order[1];
        assert_eq!(
            live_loans[&owners[&header_xs.owner()]],
            usize::from(last == xs)
        );
        assert_eq!(
            live_loans[&owners[&header_ys.owner()]],
            usize::from(last == ys)
        );
        let source_at_exit = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                Action::Drop(fact)
                    if *point == exhaustion.point()
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, &choices))
                        && (source_symbol(fact.target()).is_some()
                            || fact
                                .owner()
                                .is_some_and(|owner| source_slots.contains(&owner))) =>
                {
                    let name = source_symbol(fact.target()).expect("source drop target");
                    let expected_owner = if name == xs {
                        header_xs.owner()
                    } else {
                        header_ys.owner()
                    };
                    assert_eq!(fact.owner(), Some(expected_owner));
                    let instance = owners[&expected_owner];
                    assert_eq!(live_loans[&instance], 0, "cannot drop a borrowed source");
                    Some((name, instance))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            source_at_exit,
            [(order[0], if order[0] == xs { 1 } else { 2 })],
            "the unheld source drops at loop exit"
        );
        replay(
            table,
            exhaustion,
            plan.closure_phis(),
            &mut choices,
            &mut owners,
            &mut phi_capture_slots,
            &formed,
        );
        assert_eq!(owners[&exit_f.owner()], 11);
        assert_eq!(owners[&exit_source(last)], if last == xs { 1 } else { 2 });
        let mut ended = Vec::new();
        let mut tested = Vec::new();
        let mut dropped = Vec::new();
        for (point, action) in owned.cleanup_steps() {
            if *point != DropPoint::CallReturn(call) {
                continue;
            }
            match action {
                Action::EndCaptureLoan {
                    owner,
                    closure,
                    condition,
                    source,
                    value,
                    ..
                } if condition.is_none_or(|guard| selected(table, guard, &choices)) => {
                    assert_eq!(*owner, exit_f.owner());
                    assert_eq!(*closure, exit_origin(last).closure());
                    assert_eq!(*source, ClosureCaptureSource::Symbol(last));
                    assert_eq!(*value, CleanupCaptureValue::Owner(exit_source(last)));
                    let instance = owners[&exit_source(last)];
                    let count = live_loans.get_mut(&instance).unwrap();
                    assert_eq!(*count, 1);
                    *count -= 1;
                    ended.push(*source);
                }
                Action::TestLastCaptureLoan {
                    condition,
                    owner,
                    selector,
                    ..
                } if condition.is_none_or(|guard| selected(table, guard, &choices)) => {
                    assert_eq!(*owner, exit_source(last));
                    assert_eq!(ended.len(), 1);
                    assert_eq!(live_loans[&owners[owner]], 0);
                    choices.insert(*selector, 1);
                    tested.push(*owner);
                }
                Action::Drop(fact)
                    if fact
                        .condition()
                        .is_none_or(|guard| selected(table, guard, &choices))
                        && (source_symbol(fact.target()).is_some()
                            || fact.owner() == Some(exit_xs)
                            || fact.owner() == Some(exit_ys)) =>
                {
                    assert_eq!(fact.owner(), Some(exit_source(last)));
                    assert_eq!(tested.len(), 1);
                    dropped.push(source_symbol(fact.target()));
                }
                _ => {}
            }
        }
        assert_eq!(ended, [ClosureCaptureSource::Symbol(last)]);
        assert_eq!(tested, [exit_source(last)]);
        assert_eq!(dropped, [Some(last)]);
    }
}
