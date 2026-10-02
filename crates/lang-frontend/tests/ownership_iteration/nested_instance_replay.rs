use super::*;

#[test]
fn nested_loop_phi_replays_distinct_source_instances_across_zero_and_two_rounds() {
    assert_nested_loop_phi_replays_distinct_source_instances("");
}

#[test]
fn nested_loop_phi_continue_replays_distinct_source_instances() {
    assert_nested_loop_phi_replays_distinct_source_instances("continue");
}

#[test]
fn nested_loop_phi_break_preserves_the_new_source_instance() {
    assert_nested_loop_phi_replays_distinct_source_instances("break");
}

#[test]
fn nested_conditional_capture_defers_until_instance_presence_is_saved() {
    assert_nested_conditional_capture_defers("{ read(xs) }");
}

#[test]
fn nested_optional_capture_defers_until_instance_presence_is_saved() {
    assert_nested_conditional_capture_defers("{}");
}

fn assert_nested_conditional_capture_defers(initial: &str) {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let source = "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                val xs = listOf(1)
                val ys = listOf(2)
                var g: () -> Unit = __INITIAL__
                if (flag) { g = { read(ys) } }
                f = move { val used = g() }
            }
            val used = f()
        }"
    .replace("__INITIAL__", initial);
    let (_, _, owned) = checked(&source);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn nested_known_and_opaque_capture_defers_until_full_origin_is_saved() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own cb: move () -> Unit, flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                val xs = listOf(1)
                var g: move () -> Unit = cb
                if (flag) { g = move { read(xs) } }
                f = move { val used = g() }
                break
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn nested_known_and_call_result_capture_defers_until_full_origin_is_saved() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun make(): move () -> Unit = move {}\nfun run(flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                val xs = listOf(1)
                val g: move () -> Unit = if (flag) (move { read(xs) }) else (make())
                val next: move () -> Unit = move { val used = g() }
                f = next
                break
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

fn assert_nested_loop_phi_replays_distinct_source_instances(transfer: &str) {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId,
        IterationCleanupAction as Action, IterationPhiBoundary, IterationPhiIncoming,
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
                    panic!(
                        "selector {selector:?} is uninitialized: {:?}; initialized: {choices:?}",
                        table.selector(*selector),
                    )
                });
                selected(table, branches[*branch], choices)
            }
        }
    }

    fn replay_origin(
        table: &CleanupConditions,
        origin: &IterationPhiIncomingOrigin,
        choices: &BTreeMap<CleanupSelectorId, usize>,
        owners: &BTreeMap<CleanupOwnerValueId, u32>,
        owner_writes: &mut Vec<(CleanupOwnerValueId, u32)>,
    ) -> Option<u32> {
        let present = selected(table, origin.condition(), choices);
        if !present {
            return None;
        }
        let environments = origin
            .environments()
            .iter()
            .filter(|environment| selected(table, environment.condition(), choices))
            .collect::<Vec<_>>();
        assert_eq!(environments.len(), 1, "one actual captured environment");
        let environment = environments[0];
        for source in environment.sources() {
            if !selected(table, source.input().condition(), choices) {
                continue;
            }
            if let Some(target) = source.target() {
                let CleanupCaptureValue::Owner(actual) = source.value() else {
                    panic!("owned source slot must read an actual owner");
                };
                owner_writes.push((target, owners[&actual]));
            }
            for nested in source.captured() {
                replay_origin(table, nested, choices, owners, owner_writes);
            }
        }
        Some(owners[&environment.owner()])
    }

    fn replay(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        target: CleanupOwnerValueId,
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        let available = selected(table, binding.available_when(), &before_choices);
        let values = binding
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), &before_choices))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), usize::from(available));
        let mut choice_writes = vec![(binding.availability_selector(), usize::from(available))];
        for write in binding.selector_writes() {
            choice_writes.push((
                write.target(),
                usize::from(selected(table, write.condition(), &before_choices)),
            ));
        }
        let mut owner_writes = Vec::new();
        let selected_origins = binding
            .origins()
            .iter()
            .filter_map(|origin| {
                replay_origin(
                    table,
                    origin,
                    &before_choices,
                    &before_owners,
                    &mut owner_writes,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            selected_origins,
            values
                .iter()
                .map(|value| before_owners[&value.source()])
                .collect::<Vec<_>>()
        );
        if let Some(value) = values.first() {
            owner_writes.push((target, before_owners[&value.source()]));
        }
        choices.extend(choice_writes);
        owners.extend(owner_writes);
    }

    let (sources, parsed, owned) = checked(&
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) { val xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nf = move { g() }\n__TRANSFER__ }\nval used = f() }"
            .replace("__TRANSFER__", transfer),
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let phi = |boundary| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == f)
            .unwrap()
    };
    let header = phi(IterationPhiBoundary::Header);
    let exit = phi(IterationPhiBoundary::Exit);
    let new_origin = |binding: &lang_frontend::ownership_checking::IterationClosurePhiBinding| {
        binding.origins().iter().position(|origin| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(origin.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                == "move { g() }"
        })
    };
    let header_new = new_origin(header).map(|index| &header.origins()[index]);
    let exit_new = &exit.origins()[new_origin(exit).unwrap()];
    let exit_g = &exit.origins()[exit_new.sources()[0].captured()[0]];
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let entry = edge(IterationPhiIncomingKind::Entry);
    let body_edge = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| match incoming.kind() {
            IterationPhiIncomingKind::Fallthrough => transfer.is_empty(),
            IterationPhiIncomingKind::Continue(_) => transfer == "continue",
            IterationPhiIncomingKind::Break(_) => transfer == "break",
            _ => false,
        })
        .unwrap();
    let exhaustion = edge(IterationPhiIncomingKind::Exhaustion);
    let body_target = if transfer == "break" {
        exit.owner()
    } else {
        header.owner()
    };
    let body_origin = if transfer == "break" {
        exit_new
    } else {
        header_new.expect("a continuing edge carries the body lambda into the header")
    };
    let body_binding = if transfer == "break" { exit } else { header };
    let body_edge_new = body_edge
        .bindings()
        .iter()
        .find(|binding| binding.target() == body_target)
        .unwrap()
        .origins()
        .iter()
        .find(|origin| origin.target() == body_origin.selector())
        .unwrap();
    let created = |closure| {
        owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::CreateClosureOwner { owner, closure: id } if *id == closure => Some(*owner),
                _ => None,
            })
            .expect("the body must create this closure environment")
    };
    let fresh_f = body_edge_new.environments()[0].owner();
    assert_eq!(body_edge_new.environments()[0].instance_root(), fresh_f);
    assert!(body_edge_new.environments()[0].capture_path().is_empty());
    let direct_source = &body_edge_new.environments()[0].sources()[0];
    let fresh_g = match direct_source.value() {
        CleanupCaptureValue::Owner(owner) => owner,
        other => panic!("g must have an actual environment owner: {other:?}"),
    };
    assert_ne!(fresh_f, body_target);
    assert_ne!(direct_source.target(), Some(fresh_g));
    let nested = &body_edge_new.environments()[0].sources()[0].captured()[0];
    assert_eq!(nested.environments()[0].owner(), fresh_g);
    assert_eq!(nested.environments()[0].instance_root(), fresh_f);
    assert_eq!(nested.environments()[0].capture_path(), &[0]);
    assert_eq!(
        nested.environments()[0].sources()[0].transport_value(),
        None
    );
    if transfer != "break" {
        let forwarded = exhaustion
            .bindings()
            .iter()
            .find(|binding| binding.target() == exit.owner())
            .unwrap()
            .origins()
            .iter()
            .find(|origin| origin.target() == exit_new.selector())
            .unwrap();
        let environment = &forwarded.environments()[0];
        assert_eq!(environment.instance_root(), header.owner());
        assert!(environment.capture_path().is_empty());
        let nested = &environment.sources()[0].captured()[0].environments()[0];
        assert_eq!(nested.instance_root(), header.owner());
        assert_eq!(nested.capture_path(), &[0]);
        assert_eq!(nested.sources()[0].transport_value(), None);
    }
    let table = owned.cleanup_conditions();
    let created_g =
        created(body_binding.origins()[body_origin.sources()[0].captured()[0]].closure());
    let fresh_xs = match table.owner_value(created_g) {
        Some(CleanupOwnerValue::Closure { inputs, .. }) => match inputs[0].value() {
            CleanupCaptureValue::Owner(owner) => owner,
            other => panic!("g must capture this round's xs owner: {other:?}"),
        },
        other => panic!("g must be a created closure owner: {other:?}"),
    };
    let xs_incoming = &nested.environments()[0].sources()[0];
    assert_eq!(xs_incoming.value(), CleanupCaptureValue::Owner(fresh_xs));
    assert_ne!(xs_incoming.target(), Some(fresh_xs));
    let initial = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == header.owner())
        .unwrap()
        .values()[0]
        .source();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let xs_capture = exit_g.sources()[0].source();
    let copy_snapshot = |owner: CleanupOwnerValueId,
                         value: lang_frontend::ast::ExpressionId,
                         point: lang_frontend::ownership_checking::DropPoint,
                         choices: &mut BTreeMap<CleanupSelectorId, usize>| {
        assert_eq!(
            point,
            lang_frontend::ownership_checking::DropPoint::AfterExpression(value)
        );
        let snapshot = table.owner_snapshot(owner).unwrap();
        assert_eq!(snapshot.value(), value);
        let before = choices.clone();
        for copy in snapshot.copies() {
            if selected(table, copy.when(), &before) {
                let value = *before.get(&copy.source()).unwrap_or_else(|| {
                    panic!(
                        "snapshot copy {copy:?} reads absent source {:?}; choices {before:?}",
                        table.selector(copy.source())
                    )
                });
                choices.insert(copy.target(), value);
            }
        }
    };
    let release = |point: lang_frontend::ownership_checking::DropPoint,
                   environment_slot: CleanupOwnerValueId,
                   source_slot: CleanupOwnerValueId,
                   choices: &mut BTreeMap<CleanupSelectorId, usize>,
                   owners: &BTreeMap<CleanupOwnerValueId, u32>,
                   captures: &BTreeMap<(u32, usize), u32>,
                   expected: Option<u32>| {
        let mut ended = 0;
        let mut tested = 0;
        let mut dropped = Vec::new();
        let mut loan_location = None;
        for (at, action) in owned.cleanup_steps() {
            if *at != point {
                continue;
            }
            match action {
                Action::SaveOwnerSnapshot {
                    owner,
                    value,
                    condition,
                } if condition.is_none_or(|guard| selected(table, guard, choices)) => {
                    assert_eq!(*value, body_origin.closure());
                    copy_snapshot(*owner, *value, point, choices);
                }
                Action::EndCaptureLoan {
                    owner,
                    instance_address,
                    capture_slot,
                    condition,
                    closure,
                    source,
                    value,
                    ..
                } if *source == xs_capture
                    && condition.is_none_or(|guard| selected(table, guard, choices)) =>
                {
                    assert_eq!(*owner, environment_slot);
                    assert_eq!(*closure, exit_g.closure());
                    assert_eq!(*value, CleanupCaptureValue::Owner(source_slot));
                    let address = table.instance_address(*instance_address).unwrap();
                    let slot = table.capture_slot_value(capture_slot.unwrap()).unwrap();
                    assert_eq!(address.capture_path(), &[0]);
                    assert_eq!(slot.position(), 0);
                    let mut instance = owners[&address.root()];
                    for &position in address.capture_path() {
                        instance = captures[&(instance, position)];
                    }
                    assert_eq!(Some(captures[&(instance, slot.position())]), expected);
                    loan_location = Some((*instance_address, capture_slot.unwrap()));
                    ended += 1;
                }
                Action::TestLastCaptureLoan {
                    owner,
                    instance_address,
                    capture_slot,
                    selector,
                    condition,
                    ..
                } if *owner == source_slot
                    && condition.is_none_or(|guard| selected(table, guard, choices)) =>
                {
                    assert_eq!(ended, 1, "test only after ending this capture loan");
                    assert_eq!(loan_location, Some((*instance_address, *capture_slot)));
                    choices.insert(*selector, 1);
                    tested += 1;
                }
                Action::Drop(fact)
                    if fact.owner() == Some(source_slot)
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, choices)) =>
                {
                    assert_eq!(tested, 1, "drop only after the last-loan test");
                    assert_eq!(
                        Some((
                            fact.instance_address().unwrap(),
                            fact.capture_slot().unwrap()
                        )),
                        loan_location
                    );
                    dropped.push(owners[&source_slot]);
                }
                _ => {}
            }
        }
        assert_eq!(ended, usize::from(expected.is_some()));
        assert_eq!(tested, usize::from(expected.is_some()));
        assert_eq!(dropped, expected.into_iter().collect::<Vec<_>>());
    };
    let mut choices = BTreeMap::new();
    let mut owners = BTreeMap::from([(initial, 0_u32)]);
    let mut captures = BTreeMap::new();
    replay(table, entry, header.owner(), &mut choices, &mut owners);
    let zero_choices = choices.clone();
    let zero_owners = owners.clone();
    replay(table, exhaustion, exit.owner(), &mut choices, &mut owners);
    assert_eq!(owners[&exit.owner()], 0, "zero rounds keep the entry f");
    assert_eq!(choices[&exit_g.selector()], 0, "zero rounds clear absent g");
    release(
        lang_frontend::ownership_checking::DropPoint::CallReturn(call),
        exit_new.sources()[0].owner(),
        exit_g.sources()[0].owner(),
        &mut choices,
        &owners,
        &captures,
        None,
    );
    choices = zero_choices;
    owners = zero_owners;
    let rounds = if transfer == "break" { 1 } else { 2 };
    for round in 1..=rounds {
        owners.insert(fresh_f, round * 10);
        owners.insert(fresh_g, round * 10 + 1);
        owners.insert(fresh_xs, round * 10 + 2);
        captures.insert((round * 10, 0), round * 10 + 1);
        captures.insert((round * 10 + 1, 0), round * 10 + 2);
        for (at, action) in owned.cleanup_steps() {
            if *at
                == lang_frontend::ownership_checking::DropPoint::AfterExpression(exit_g.closure())
                && let Action::SaveOwnerSnapshot {
                    owner,
                    value,
                    condition,
                } = action
                && condition.is_none_or(|guard| selected(table, guard, &choices))
            {
                assert_eq!(*value, exit_g.closure());
                copy_snapshot(*owner, *value, *at, &mut choices);
            }
        }
        if let Some(header_new) = header_new {
            release(
                lang_frontend::ownership_checking::DropPoint::AfterExpression(
                    body_origin.closure(),
                ),
                header_new.sources()[0].owner(),
                header.origins()[header_new.sources()[0].captured()[0]].sources()[0].owner(),
                &mut choices,
                &owners,
                &captures,
                (round > 1).then_some((round - 1) * 10 + 2),
            );
        }
        replay(table, body_edge, body_target, &mut choices, &mut owners);
        assert_eq!(owners[&body_target], round * 10);
        assert_eq!(owners[&body_origin.sources()[0].owner()], round * 10 + 1);
        assert_eq!(
            owners[&body_binding.origins()[body_origin.sources()[0].captured()[0]].sources()[0]
                .owner()],
            round * 10 + 2
        );
    }
    if transfer != "break" {
        replay(table, exhaustion, exit.owner(), &mut choices, &mut owners);
    }
    assert_eq!(owners[&exit.owner()], rounds * 10);
    assert_eq!(owners[&exit_new.sources()[0].owner()], rounds * 10 + 1);
    assert_eq!(owners[&exit_g.sources()[0].owner()], rounds * 10 + 2);
    release(
        lang_frontend::ownership_checking::DropPoint::CallReturn(call),
        exit_new.sources()[0].owner(),
        exit_g.sources()[0].owner(),
        &mut choices,
        &owners,
        &captures,
        Some(rounds * 10 + 2),
    );
}
