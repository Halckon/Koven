use super::*;

#[test]
fn loop_phi_backedge_copies_the_prior_environment_source_relation() {
    assert_loop_phi_source_replay("");
}

#[test]
fn loop_phi_continue_copies_the_prior_environment_source_relation() {
    assert_loop_phi_source_replay("continue");
}

fn assert_loop_phi_source_replay(transfer: &str) {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValueId, CleanupSelection, CleanupSelectorId, CleanupSelectorSource,
        ClosureCaptureSource, DropFact, DropPoint, DropTarget, IterationCleanupAction,
        IterationPhiBoundary, IterationPhiIncoming, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(&format!(
        "fun read(xs: List<Int>) {{}}\nfun run(flags: List<Boolean>) {{
            var f: () -> Unit = {{}}
            var g: () -> Unit = {{}}
            for (_ in flags) {{
                val prior = f
                val xs = listOf(1)
                {{ g = prior }}
                {{ f = ({{ read(xs) }}) }}
                {transfer}
            }}
            val first = g()
            val second = f()
        }}"
    ));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = |name| {
        names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id()
    };
    let plan = &owned.iterations()[0];
    let f = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol("f"))
        .unwrap();
    let g = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol("g"))
        .unwrap();
    let lambda = f
        .origins()
        .iter()
        .find(|origin| {
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
                .contains("read(xs)")
        })
        .unwrap();
    let carried = g
        .origins()
        .iter()
        .find(|origin| origin.closure() == lambda.closure())
        .unwrap();
    let backedge = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| match incoming.kind() {
            IterationPhiIncomingKind::Fallthrough => transfer.is_empty(),
            IterationPhiIncomingKind::Continue(_) => !transfer.is_empty(),
            _ => false,
        })
        .unwrap();
    let input = backedge
        .bindings()
        .iter()
        .find(|binding| binding.target() == g.owner())
        .unwrap()
        .origins()
        .iter()
        .find(|input| input.target() == carried.selector())
        .unwrap();
    assert_ne!(
        owned.cleanup_conditions().get(input.condition()),
        Some(&CleanupCondition::Never)
    );
    assert!(input.environments().iter().any(|environment| {
        environment.sources().iter().any(|source| {
            source.target() == Some(carried.sources()[0].owner())
                && source.value() == CleanupCaptureValue::Owner(lambda.sources()[0].owner())
        })
    }));
    let source_input = |binding, selector| {
        backedge
            .bindings()
            .iter()
            .find(|incoming| incoming.target() == binding)
            .unwrap()
            .origins()
            .iter()
            .find(|origin| origin.target() == selector)
            .unwrap()
            .environments()
            .iter()
            .flat_map(|environment| environment.sources())
            .find_map(|source| match (source.target(), source.value()) {
                (Some(target), CleanupCaptureValue::Owner(actual)) => Some((target, actual)),
                _ => None,
            })
            .unwrap()
    };
    let f_source = source_input(f.owner(), lambda.selector());
    let g_source = source_input(g.owner(), carried.selector());
    assert_eq!(g_source.1, f_source.0, "g reads the previous f source");
    assert_ne!(f_source.1, f_source.0, "f gets this round's fresh xs");
    fn selected(
        table: &CleanupConditions,
        id: CleanupConditionId,
        choices: &std::collections::BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
        }
    }
    fn apply_incoming(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        choices: &mut std::collections::BTreeMap<CleanupSelectorId, usize>,
        owners: &mut std::collections::BTreeMap<CleanupOwnerValueId, u32>,
        environments: &mut std::collections::BTreeMap<CleanupOwnerValueId, u32>,
        fresh_source: Option<(CleanupOwnerValueId, u32)>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        let before_environments = environments.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let mut choice_writes = Vec::new();
        let mut owner_writes = Vec::new();
        let mut environment_writes = Vec::new();
        for binding in incoming.bindings() {
            let available = selected(table, binding.available_when(), &before_choices);
            choice_writes.push((binding.availability_selector(), usize::from(available)));
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(
                values.len(),
                usize::from(available),
                "an available phi must copy exactly one environment value"
            );
            let environment = values.first().map(|value| {
                *before_environments
                    .get(&value.source())
                    .expect("incoming must read an initialized environment value")
            });
            if let Some(environment) = environment {
                environment_writes.push((binding.target(), environment));
            }
            let mut selected_environments = Vec::new();
            for write in binding.selector_writes() {
                choice_writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            for origin in binding.origins() {
                let present = selected(table, origin.condition(), &before_choices);
                if !present {
                    continue;
                }
                for environment in origin.environments() {
                    if !selected(table, environment.condition(), &before_choices) {
                        continue;
                    }
                    selected_environments.push(
                        *before_environments
                            .get(&environment.owner())
                            .expect("selected origin needs its actual environment"),
                    );
                    for source in environment.sources() {
                        if !selected(table, source.input().condition(), &before_choices) {
                            continue;
                        }
                        if let (Some(target), CleanupCaptureValue::Owner(value)) =
                            (source.target(), source.value())
                        {
                            let actual = before_owners.get(&value).copied().or_else(|| {
                                fresh_source
                                    .and_then(|(owner, round)| (value == owner).then_some(round))
                            });
                            let actual = actual.expect("incoming must read an initialized owner");
                            owner_writes.push((target, actual));
                        }
                    }
                }
            }
            if let Some(environment) = environment {
                assert_eq!(
                    selected_environments,
                    [environment],
                    "the selected capture origin belongs to the copied environment value"
                );
            } else {
                assert!(selected_environments.is_empty());
            }
        }
        choices.extend(choice_writes);
        owners.extend(owner_writes);
        environments.extend(environment_writes);
    }
    let table = owned.cleanup_conditions();
    let snapshots = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } => Some((
                *point,
                *condition,
                *value,
                table.owner_snapshot(*owner).unwrap(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 3, "the body saves prior, g and f once");
    assert!(
        snapshots
            .iter()
            .all(|(_, _, value, snapshot)| snapshot.value() == *value),
        "the saved snapshot must describe the evaluated action value"
    );
    let fresh_closure = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *closure == lambda.closure() =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .expect("the body creates the captured lambda environment");
    let fresh_source = match table.owner_value(fresh_closure).unwrap() {
        lang_frontend::ownership_checking::CleanupOwnerValue::Closure { inputs, .. } => {
            assert_eq!(inputs.len(), 1, "the fresh lambda captures this round's xs");
            let CleanupCaptureValue::Owner(source) = inputs[0].value() else {
                panic!("the fresh lambda must capture an owned source");
            };
            source
        }
        _ => unreachable!(),
    };
    assert_eq!(
        f_source.1, fresh_source,
        "backedge f must copy the lambda's actual captured xs owner"
    );
    let ClosureCaptureSource::Symbol(local_xs) = lambda.sources()[0].source() else {
        panic!("this fixture captures its body-local xs");
    };
    let source_owners = plan
        .closure_phis()
        .iter()
        .flat_map(|phi| phi.origins())
        .filter(|origin| origin.closure() == lambda.closure())
        .flat_map(|origin| origin.sources().iter().map(|source| source.owner()))
        .chain(std::iter::once(fresh_source))
        .collect::<std::collections::BTreeSet<_>>();
    let is_source_drop = |fact: &DropFact| {
        fact.owner()
            .is_some_and(|owner| source_owners.contains(&owner))
            || matches!(fact.target(), DropTarget::RetainedSource(_))
            || fact.target() == DropTarget::Named(local_xs)
            || matches!(fact.target(), DropTarget::Captured { source: ClosureCaptureSource::Symbol(symbol), .. } if symbol == local_xs)
            || sources.slice(fact.value_origin()).unwrap() == "listOf(1)"
    };
    assert_eq!(
        snapshots[2]
            .3
            .capture_inputs()
            .iter()
            .map(|input| input.owner())
            .collect::<Vec<_>>(),
        [fresh_closure],
        "f's saved value must come from the new lambda environment"
    );
    assert_eq!(
        snapshots[2]
            .3
            .value_inputs()
            .iter()
            .map(|input| input.owner())
            .collect::<Vec<_>>(),
        [fresh_closure],
        "the public snapshot keeps the evaluated RHS owner for handle transport"
    );
    assert_eq!(
        snapshots
            .iter()
            .map(|(_, _, value, _)| {
                sources
                    .slice(parsed.ast().expressions().get(*value).unwrap().span())
                    .unwrap()
            })
            .collect::<Vec<_>>(),
        ["f", "prior", "({ read(xs) })"],
        "replay models the fixture's evaluated values in order"
    );
    assert!(
        snapshots
            .iter()
            .all(|(point, _, value, _)| { *point == DropPoint::AfterExpression(*value) }),
        "each snapshot must execute immediately after its evaluated value"
    );
    let f_commit = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (point, action))| match action {
            IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *target == symbol("f") =>
            {
                Some((index, *point, *owner))
            }
            _ => None,
        })
        .expect("f replacement commits its saved environment");
    let f_save = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (point, action))| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                if *owner == f_commit.2 && *point == f_commit.1 =>
            {
                Some(index)
            }
            _ => None,
        })
        .expect("f replacement saves its RHS at the commit point");
    assert!(
        f_save < f_commit.0,
        "save must precede f replacement commit"
    );
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let mut choices = std::collections::BTreeMap::new();
    let mut owners = std::collections::BTreeMap::new();
    let mut environments = entry
        .bindings()
        .iter()
        .flat_map(|binding| {
            let instance = if binding.target() == f.owner() {
                100
            } else if binding.target() == g.owner() {
                101
            } else {
                panic!("unexpected entry binding")
            };
            binding
                .values()
                .iter()
                .map(move |value| (value.source(), instance))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    apply_incoming(
        table,
        entry,
        &mut choices,
        &mut owners,
        &mut environments,
        None,
    );
    let zero_round = (choices.clone(), owners.clone(), environments.clone());
    let loop_exit = DropPoint::LoopExit(plan.descriptor().statement());
    let mut modeled_early_points = snapshots
        .iter()
        .map(|(point, ..)| *point)
        .collect::<Vec<_>>();
    modeled_early_points.push(loop_exit);
    let assert_no_early_drop =
        |point, current: &std::collections::BTreeMap<CleanupSelectorId, usize>| {
            for (at, action) in owned.cleanup_steps() {
                let IterationCleanupAction::Drop(fact) = action else {
                    continue;
                };
                if *at == point && is_source_drop(fact) {
                    assert!(
                        !fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, current)),
                        "a live source cannot drop at {point:?}: {fact:?}"
                    );
                }
            }
        };
    for round in 1..=2 {
        for (index, (point, condition, _, snapshot)) in snapshots.iter().enumerate() {
            let prior = choices.clone();
            assert!(
                condition.is_none_or(|guard| selected(table, guard, &prior)),
                "round {round}: the fixture must execute every saved value"
            );
            let mut writes = Vec::new();
            for copy in snapshot.copies() {
                if selected(table, copy.when(), &prior) {
                    writes.push((copy.target(), prior[&copy.source()]));
                }
            }
            choices.extend(writes);
            let instance = if index == 2 {
                let input = &snapshot.capture_inputs()[0];
                assert!(selected(table, input.condition(), &prior));
                environments.insert(fresh_closure, round);
                environments[&input.owner()]
            } else if index == 0 {
                environments[&f.owner()]
            } else {
                environments[&snapshots[0].3.owner()]
            };
            environments.insert(snapshot.owner(), instance);
            assert_no_early_drop(*point, &choices);
        }
        apply_incoming(
            table,
            backedge,
            &mut choices,
            &mut owners,
            &mut environments,
            Some((fresh_source, round)),
        );
    }
    assert_eq!(environments[&g.owner()], 1, "g keeps round one's closure");
    assert_eq!(environments[&f.owner()], 2, "f keeps round two's closure");
    assert_eq!(owners[&g_source.0], 1, "g retains the earlier xs instance");
    assert_eq!(owners[&f_source.0], 2, "f owns this round's xs instance");
    assert_no_early_drop(loop_exit, &choices);
    apply_incoming(
        table,
        exhaustion,
        &mut choices,
        &mut owners,
        &mut environments,
        None,
    );
    let mut zero_choices = zero_round.0;
    let mut zero_owners = zero_round.1;
    let mut zero_environments = zero_round.2;
    assert_no_early_drop(loop_exit, &zero_choices);
    apply_incoming(
        table,
        exhaustion,
        &mut zero_choices,
        &mut zero_owners,
        &mut zero_environments,
        None,
    );
    for (name, expected) in [("f", 2), ("g", 1)] {
        let phi = plan
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol(name)
            })
            .unwrap();
        let origin = phi
            .origins()
            .iter()
            .find(|origin| origin.closure() == lambda.closure())
            .unwrap();
        let slot = origin.sources()[0].owner();
        assert_eq!(
            choices[&origin.selector()],
            1,
            "{name}: two-round exit origin"
        );
        assert_eq!(owners[&slot], expected, "{name}: exit source instance");
        assert_eq!(
            environments[&phi.owner()],
            expected,
            "{name}: exit closure instance"
        );
        assert_eq!(
            zero_choices[&origin.selector()],
            0,
            "{name}: zero-round exit"
        );
        assert!(!zero_owners.contains_key(&slot));
        assert_eq!(
            zero_environments[&phi.owner()],
            if name == "f" { 100 } else { 101 },
            "{name}: zero-round exit keeps the initial closure"
        );
    }
    let call_points = ["g()", "f()"].map(|text| {
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
            .unwrap();
        DropPoint::CallReturn(call)
    });
    for (_, action) in owned.cleanup_steps() {
        if let IterationCleanupAction::Drop(fact) = action
            && is_source_drop(fact)
        {
            assert!(
                modeled_early_points.contains(&fact.point()) || call_points.contains(&fact.point()),
                "unreplayed captured-source cleanup point: {fact:?}"
            );
        }
    }
    let mut live_loans = std::collections::BTreeMap::from([(1, 1_usize), (2, 1_usize)]);
    let mut source_drops = std::collections::BTreeMap::<u32, usize>::new();
    for (index, (name, expected)) in [("g", 1), ("f", 2)].into_iter().enumerate() {
        let phi = plan
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol(name)
            })
            .unwrap();
        let origin = phi
            .origins()
            .iter()
            .find(|origin| origin.closure() == lambda.closure())
            .unwrap();
        let slot = origin.sources()[0].owner();
        assert_eq!(owners[&slot], expected);
        let point = call_points[index];
        let retained = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == point && is_source_drop(fact))
            .collect::<Vec<_>>();
        assert!(
            !retained.is_empty(),
            "{name}: retained-source cleanup exists"
        );
        for fact in &retained {
            assert!(
                !selected(table, fact.condition().unwrap(), &zero_choices),
                "{name}: zero-round call cannot release a source from an unexecuted body"
            );
        }
        let drop = retained
            .iter()
            .find(|fact| fact.owner() == Some(slot))
            .unwrap();
        let actions = owned
            .cleanup_steps()
            .iter()
            .filter(|(at, _)| *at == point)
            .map(|(_, action)| action)
            .collect::<Vec<_>>();
        let ended = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                IterationCleanupAction::EndCaptureLoan {
                    owner,
                    closure,
                    value: CleanupCaptureValue::Owner(source),
                    condition,
                    ..
                } if *owner == phi.owner() && *closure == origin.closure() && *source == slot => {
                    assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
                    Some(index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ended.len(), 1, "{name}: end exactly one source loan");
        let (tested, selector) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                IterationCleanupAction::TestLastCaptureLoan {
                    owner,
                    selector,
                    condition,
                    ..
                } if *owner == slot => {
                    assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
                    Some((index, *selector))
                }
                _ => None,
            })
            .unwrap();
        let dropped = actions
            .iter()
            .position(
                |action| matches!(action, IterationCleanupAction::Drop(fact) if fact == *drop),
            )
            .unwrap();
        assert!(ended[0] < tested && tested < dropped);
        let selector_info = table.selector(selector).unwrap();
        assert_eq!(selector_info.selection(), CleanupSelection::LastCaptureLoan);
        assert_eq!(
            selector_info.source(),
            CleanupSelectorSource::CaptureLoan { owner: slot }
        );
        let remaining = live_loans.get_mut(&expected).unwrap();
        *remaining -= ended.len();
        choices.insert(selector, usize::from(*remaining == 0));
        let executed = actions
            .iter()
            .filter_map(|action| match action {
                IterationCleanupAction::Drop(fact)
                    if is_source_drop(fact)
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, &choices)) =>
                {
                    Some(fact)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            executed.len(),
            1,
            "{name}: exactly one retained source drop"
        );
        assert_eq!(executed[0].owner(), Some(slot));
        let actual = owners[&executed[0].owner().unwrap()];
        *source_drops.entry(actual).or_insert(0) += 1;
    }
    assert_eq!(
        source_drops,
        std::collections::BTreeMap::from([(1, 1), (2, 1)])
    );
}
