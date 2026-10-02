use super::*;

fn assert_cross_loop_parent_release(
    source_text: &str,
    outer_text: &str,
    capture_count: usize,
    binding_name: &str,
    from_environment: bool,
    needs_file_release: bool,
    expect_source_guard_copy: bool,
) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("cross-loop-release-root.ko", source_text)
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (names, types) = crate::type_checking::standard_environments();
    let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
    let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let checked =
        crate::ownership_checking::check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(checked.diagnostics().is_empty());
    let mut checker =
        super::super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
    let capture_liveness = super::super::super::capture_liveness(&checker).unwrap();
    checker.expression_live_after = capture_liveness.expression_after;
    checker.statement_live_after = capture_liveness.statement_after;
    let mut state = super::super::super::super::State::default();
    for &root in parsed.roots() {
        checker.check_item(root, &mut state).unwrap();
    }
    assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
    let liveness = super::super::super::liveness::Liveness::build(&checker).unwrap();
    let (origins, captures) = super::super::super::origins::analyze(&checker).unwrap();
    let mut planner = super::super::super::DropPlanner::new(&checker, liveness, origins, captures);
    for &root in parsed.roots() {
        planner.item(root).unwrap();
    }
    assert!(planner.recursive_capture_phi.is_some());
    let candidate = planner.into_candidate_facts();
    let plans = candidate
        .iterations
        .iter()
        .map(|plan| (plan.descriptor().statement().index(), plan))
        .collect::<BTreeMap<_, _>>();
    let steps = &candidate.cleanup_steps;
    let table = &candidate.cleanup_conditions;
    let (outer, closure) = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if sources.slice(parsed.ast().expressions().get(*closure).unwrap().span())
                    == Ok(outer_text) =>
            {
                Some((*owner, *closure))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(checked.captures_of(closure).count(), capture_count);
    assert_eq!(
        table
            .closure_capture_edges(outer)
            .unwrap()
            .iter()
            .any(|edge| {
                matches!(
                    edge.input().value(),
                    CleanupCaptureValue::Environment { .. }
                )
            }),
        from_environment
    );
    let outer_symbol = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok(binding_name))
        .unwrap()
        .id();
    let outer_actions = steps
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::ReleaseClosureInstances { layout, root }
                if root.target() == DropTarget::Named(outer_symbol) =>
            {
                Some((*layout, *root))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let ordinary_drops = steps
        .iter()
        .filter(|(_, action)| {
            matches!(action, IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::Named(outer_symbol))
        })
        .count();
    if needs_file_release {
        let [(layout, root)] = outer_actions.as_slice() else {
            panic!("parent needs exactly one release action: {outer_actions:?}")
        };
        assert_eq!(*layout, ClosureReleaseLayout::File);
        assert_eq!(root.owner(), Some(outer));
        let DropPoint::CallReturn(call) = root.point() else {
            panic!("the File root must release after its call")
        };
        let expected_call = format!("{binding_name}()");
        assert_eq!(
            sources.slice(parsed.ast().expressions().get(call).unwrap().span()),
            Ok(expected_call.as_str())
        );
        assert_eq!(ordinary_drops, 0);
        let all_outer_roots = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::Drop(fact)
                | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. }
                    if fact.owner() == Some(outer) =>
                {
                    Some(*action)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            all_outer_roots,
            [IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::File,
                root: *root,
            }]
        );
    } else {
        assert!(outer_actions.is_empty());
        assert_eq!(ordinary_drops, 1);
    }
    let borrowed_child = if capture_count == 3 {
        let borrowed = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, expression)| {
                (sources.slice(expression.span()) == Ok("{ read(xs) }")).then_some(id)
            })
            .unwrap();
        let borrowed_captures = checked.captures_of(borrowed).collect::<Vec<_>>();
        let [loan] = borrowed_captures.as_slice() else {
            panic!("the borrowed child must have one shared source")
        };
        let ClosureCaptureSource::Symbol(source_symbol) = loan.source() else {
            panic!("the shared source must be the owned local list")
        };
        assert_eq!(
            (loan.mode(), loan.effect()),
            (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow)
        );
        let release_index = steps
            .iter()
            .position(|(_, action)| {
                matches!(action,
                    IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::File,
                        root,
                    } if root.owner() == Some(outer))
            })
            .unwrap();
        let source_expression = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, expression)| {
                (sources.slice(expression.span()) == Ok("listOf(1)")).then_some(id)
            })
            .unwrap();
        let source_drops = steps
            .iter()
            .enumerate()
            .filter_map(|(index, (_, action))| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::Named(source_symbol)
                        || fact.target() == DropTarget::Temporary(source_expression)
                        || fact.target() == DropTarget::RetainedSource(loan.source())
                        || matches!(fact.target(), DropTarget::Captured { source, .. }
                                if source == loan.source()) =>
                {
                    Some((index, *fact))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(source_drop_index, source_drop)] = source_drops.as_slice() else {
            panic!("the shared source must have one drop: {source_drops:?}")
        };
        assert_eq!(source_drop.target(), DropTarget::Named(source_symbol));
        assert!(
            *source_drop_index > release_index,
            "the shared source must drop after the File root: {source_drops:?}"
        );
        assert_eq!(
            source_drop.point(),
            steps[release_index].0,
            "the named source must drop immediately after the root ends its final loan"
        );
        assert!(matches!(source_drop.point(), DropPoint::CallReturn(_)));
        let borrowed_symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
            .unwrap()
            .id();
        let (position, capture) = checked
            .captures_of(closure)
            .enumerate()
            .find(|(_, capture)| capture.source() == ClosureCaptureSource::Symbol(borrowed_symbol))
            .unwrap();
        assert_eq!(position, 2);
        assert_eq!(
            (capture.mode(), capture.effect()),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
        );
        let parent_input = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == outer && input.source() == capture.source() => {
                    assert_eq!(
                        table.capture_slot_value(*target).unwrap().position(),
                        position
                    );
                    Some(*input)
                }
                _ => None,
            })
            .unwrap();
        let child_owner = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *closure == borrowed =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        let child_snapshot = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == borrowed =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            parent_input.value(),
            CleanupCaptureValue::Owner(child_snapshot)
        );
        let child_inputs = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == child_owner => Some((*target, *input)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(target, child_input)] = child_inputs.as_slice() else {
            panic!("the borrowed child must form exactly one shared capture")
        };
        let child_slot = table.capture_slot_value(*target).unwrap();
        assert_eq!(child_slot.environment(), child_owner);
        assert_eq!(child_slot.position(), 0);
        assert_eq!(child_slot.source(), loan.source());
        assert_eq!(child_input.source(), loan.source());
        assert_eq!(
            (child_input.mode(), child_input.effect()),
            (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow)
        );
        assert!(!steps.iter().any(|(_, action)| matches!(action,
                IterationCleanupAction::EndCaptureLoan { closure, .. }
                    if *closure == borrowed)));
        Some((
            borrowed_symbol,
            borrowed,
            child_owner,
            child_snapshot,
            *child_input,
            child_slot.position(),
            *source_drop,
        ))
    } else {
        None
    };
    if capture_count != 2 && capture_count != 3 {
        assert!(!expect_source_guard_copy);
        return;
    }

    // Execute one round in each recursive loop before forming the outside parent.
    let statements = checker
        .iterations
        .values()
        .map(|plan| plan.descriptor().statement())
        .collect::<Vec<_>>();
    assert_eq!(statements.len(), 2);
    let parent_saves = steps
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if *owner == outer => Some((*target, *input)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(parent_saves.len(), capture_count);
    for (position, ((target, input), capture)) in parent_saves
        .iter()
        .zip(checked.captures_of(closure))
        .enumerate()
    {
        let slot = table.capture_slot_value(*target).unwrap();
        assert_eq!(slot.environment(), outer);
        assert_eq!(slot.position(), position);
        assert_eq!(slot.source(), capture.source());
        assert_eq!(input.source(), capture.source());
        assert_eq!(
            (input.mode(), input.effect()),
            (capture.mode(), capture.effect())
        );
    }
    let mut next_instance = 0;
    let mut instances = BTreeMap::new();
    let mut owners = BTreeMap::new();
    let mut source_instances = BTreeMap::new();
    let mut captures = BTreeMap::new();
    let mut shared_loans = BTreeMap::new();
    let mut choices = BTreeMap::new();
    let mut chains = Vec::new();
    let mut carried_roots = Vec::new();
    let mut exit_roots = Vec::new();
    for statement in statements {
        let phis = &plans[&statement.index()].closure_phis();
        let graph = &plans[&statement.index()].capture_graph();
        let (parent_slot, symbol) = parent_saves
            .iter()
            .find_map(|(target, input)| {
                let ClosureCaptureSource::Symbol(symbol) = input.source() else {
                    return None;
                };
                let header = phis.iter().find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol
                })?;
                steps
                    .iter()
                    .any(|(_, action)| {
                        matches!(action,
                            IterationCleanupAction::SaveClosureCapture { input, .. }
                                if input.value() == CleanupCaptureValue::Owner(header.owner()))
                    })
                    .then_some((target, symbol))
            })
            .unwrap();
        let phi = |boundary| {
            phis.iter()
                .find(|phi| phi.boundary() == boundary && phi.symbol() == symbol)
                .unwrap()
        };
        let (header, exit) = (
            phi(IterationPhiBoundary::Header),
            phi(IterationPhiBoundary::Exit),
        );
        let incomings = &plans[&statement.index()].closure_phi_incomings();
        let edge = |kind| incomings.iter().find(|edge| edge.kind() == kind).unwrap();
        let edge_values =
            |owners: &BTreeMap<CleanupOwnerValueId, usize>,
             sources: &BTreeMap<CleanupOwnerValueId, usize>| {
                let mut values = owners.clone();
                for (&owner, &instance) in sources {
                    assert!(values.insert(owner, instance).is_none());
                }
                values
            };
        let forward_sources =
            |kind,
             transported: &BTreeMap<CleanupOwnerValueId, (CleanupOwnerValueId, usize)>,
             sources: &mut BTreeMap<CleanupOwnerValueId, usize>| {
                for binding in edge(kind).bindings() {
                    if !binding.root_sources().is_empty() {
                        continue;
                    }
                    let &(source, instance) = &transported[&binding.target()];
                    if source == binding.target() {
                        assert_eq!(sources[&source], instance);
                    } else {
                        assert_eq!(sources.remove(&source), Some(instance));
                        assert!(sources.insert(binding.target(), instance).is_none());
                    }
                }
            };
        let binding = |kind, target| {
            edge(kind)
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
        };
        let entry = binding(IterationPhiIncomingKind::Entry, header.owner());
        let [initial] = entry.root_sources() else {
            panic!("entry needs one formed root")
        };
        assert_eq!(entry.values()[0].source(), initial.source());
        let initial_snapshot = table.owner_snapshot(initial.source());
        let initial_source = if let Some(snapshot) = initial_snapshot {
            let [source] = snapshot.capture_inputs() else {
                panic!("entry snapshot needs one formed owner")
            };
            assert!(selected(table, source.condition(), &choices));
            source.owner()
        } else {
            initial.source()
        };
        let initial_create = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *owner == initial_source =>
                {
                    Some(*closure)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing initial closure for {initial:?} in {statement:?}"));
        if initial_snapshot.is_some() {
            assert!(steps.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *owner == initial.source() && *value == initial_create)));
            assert!(steps.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                        if *owner == initial.source() && *target == symbol)));
        }
        next_instance += 1;
        let seed = next_instance;
        assert!(instances.insert(seed, initial_create).is_none());
        assert!(owners.insert(initial_source, seed).is_none());
        let mut instance_nodes = instances
            .iter()
            .filter_map(|(&instance, &closure)| {
                graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == closure)
                    .map(|node| (instance, node))
            })
            .collect::<BTreeMap<_, _>>();
        if initial_snapshot.is_some() {
            let instance = owners.remove(&initial_source).unwrap();
            assert!(owners.insert(initial.source(), instance).is_none());
            replay_snapshot_choices(table, initial.source(), &mut choices);
        }
        assert!(selected(table, initial.condition(), &choices));
        for source_binding in edge(IterationPhiIncomingKind::Entry)
            .bindings()
            .iter()
            .filter(|binding| binding.root_sources().is_empty())
        {
            let [source] = source_binding.values() else {
                panic!("ordinary source needs one entry value")
            };
            if let std::collections::btree_map::Entry::Vacant(entry) =
                source_instances.entry(source.source())
            {
                let Some(CleanupOwnerValue::Expression { expression, .. }) =
                    table.owner_value(source.source())
                else {
                    panic!("ordinary source must originate from its evaluated expression")
                };
                assert_eq!(
                    sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()),
                    Ok("listOf(1)")
                );
                next_instance += 1;
                entry.insert(next_instance);
            }
        }
        for (carried_symbol, carried_owner) in &carried_roots {
            let carry_header = phis
                .iter()
                .find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Header
                        && phi.symbol() == *carried_symbol
                })
                .unwrap();
            let carry_entry = binding(IterationPhiIncomingKind::Entry, carry_header.owner());
            let [source] = carry_entry.values() else {
                panic!("carried root needs one entry source")
            };
            assert_eq!(source.source(), *carried_owner);
            assert!(selected(table, source.condition(), &choices));
        }
        let entry_values = replay_captured_edge(
            table,
            graph,
            phis,
            edge(IterationPhiIncomingKind::Entry),
            ReplayInstances {
                values: &edge_values(&owners, &source_instances),
                nodes: &instance_nodes,
                captured: &captures,
            },
            &mut choices,
        );
        assert_eq!(
            entry_values.len(),
            edge(IterationPhiIncomingKind::Entry).bindings().len()
        );
        assert_eq!(entry_values[&header.owner()], (initial.source(), seed));
        forward_sources(
            IterationPhiIncomingKind::Entry,
            &entry_values,
            &mut source_instances,
        );
        for (carried_symbol, carried_owner) in &mut carried_roots {
            let carry_header = phis
                .iter()
                .find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Header
                        && phi.symbol() == *carried_symbol
                })
                .unwrap();
            let instance = owners.remove(carried_owner).unwrap();
            assert_eq!(
                entry_values[&carry_header.owner()],
                (*carried_owner, instance)
            );
            assert!(owners.insert(carry_header.owner(), instance).is_none());
            *carried_owner = carry_header.owner();
        }
        assert_eq!(owners.remove(&initial.source()), Some(seed));
        assert!(owners.insert(header.owner(), seed).is_none());

        let (formed, recursive_closure, capture_slot, input) = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if input.value() == CleanupCaptureValue::Owner(header.owner()) => {
                    let closure = steps.iter().find_map(|(_, action)| match action {
                        IterationCleanupAction::CreateClosureOwner {
                            owner: created,
                            closure,
                        } if created == owner => Some(*closure),
                        _ => None,
                    })?;
                    Some((*owner, closure, *target, *input))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(input.mode(), ClosureCaptureMode::Owned);
        assert_eq!(input.effect(), ClosureCaptureEffect::Move);
        assert!(selected(table, input.condition(), &choices));
        next_instance += 1;
        let new_instance = next_instance;
        assert!(instances.insert(new_instance, recursive_closure).is_none());
        assert!(owners.insert(formed, new_instance).is_none());
        assert!(
            instance_nodes
                .insert(
                    new_instance,
                    graph
                        .nodes()
                        .iter()
                        .position(|node| node.closure() == recursive_closure)
                        .unwrap(),
                )
                .is_none()
        );
        let position = table.capture_slot_value(capture_slot).unwrap().position();
        let prior = owners.remove(&header.owner()).unwrap();
        assert_eq!(prior, seed);
        assert!(captures.insert((new_instance, position), prior).is_none());
        let snapshot = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == recursive_closure =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        assert!(steps.iter().any(|(_, action)| matches!(
            action,
            IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == snapshot && *target == symbol
        )));
        let saved = table.owner_snapshot(snapshot).unwrap();
        let [source] = saved.capture_inputs() else {
            panic!("replacement must save the formed owner")
        };
        assert_eq!(source.owner(), formed);
        assert!(selected(table, source.condition(), &choices));
        let instance = owners.remove(&formed).unwrap();
        assert!(owners.insert(snapshot, instance).is_none());
        replay_snapshot_choices(table, snapshot, &mut choices);
        let backedge = binding(IterationPhiIncomingKind::Fallthrough, header.owner());
        let selected_values = backedge
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), &choices))
            .collect::<Vec<_>>();
        let [back_value] = selected_values.as_slice() else {
            panic!("one backedge owner value must be selected")
        };
        let selected_roots = backedge
            .root_sources()
            .iter()
            .filter(|root| {
                root.source() == back_value.source()
                    && root.node() == instance_nodes[&owners[&back_value.source()]]
                    && selected(table, root.condition(), &choices)
            })
            .collect::<Vec<_>>();
        let [back_root] = selected_roots.as_slice() else {
            panic!("one backedge root must be selected")
        };
        assert_eq!(back_root.source(), snapshot);
        for (carried_symbol, carried_owner) in &carried_roots {
            let carry_backedge = binding(IterationPhiIncomingKind::Fallthrough, *carried_owner);
            let [source] = carry_backedge.values() else {
                panic!("carried root needs one backedge source")
            };
            assert_eq!(source.source(), *carried_owner);
            assert!(selected(table, source.condition(), &choices));
            assert!(phis.iter().any(|phi| {
                phi.boundary() == IterationPhiBoundary::Header
                    && phi.symbol() == *carried_symbol
                    && phi.owner() == *carried_owner
            }));
        }
        let back_values = replay_captured_edge(
            table,
            graph,
            phis,
            edge(IterationPhiIncomingKind::Fallthrough),
            ReplayInstances {
                values: &edge_values(&owners, &source_instances),
                nodes: &instance_nodes,
                captured: &captures,
            },
            &mut choices,
        );
        assert_eq!(
            back_values.len(),
            edge(IterationPhiIncomingKind::Fallthrough).bindings().len()
        );
        forward_sources(
            IterationPhiIncomingKind::Fallthrough,
            &back_values,
            &mut source_instances,
        );
        let instance = owners.remove(&back_root.source()).unwrap();
        assert_eq!(back_values[&header.owner()], (back_root.source(), instance));
        assert!(owners.insert(header.owner(), instance).is_none());
        let exhausted = binding(IterationPhiIncomingKind::Exhaustion, exit.owner());
        let selected_values = exhausted
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), &choices))
            .collect::<Vec<_>>();
        let [exit_value] = selected_values.as_slice() else {
            panic!("one exhaustion owner value must be selected")
        };
        let selected_roots = exhausted
            .root_sources()
            .iter()
            .filter(|root| {
                root.source() == exit_value.source()
                    && root.node() == instance_nodes[&owners[&exit_value.source()]]
                    && selected(table, root.condition(), &choices)
            })
            .collect::<Vec<_>>();
        let [exit_root] = selected_roots.as_slice() else {
            panic!("one exhaustion root must be selected")
        };
        assert_eq!(exit_root.source(), header.owner());
        for (carried_symbol, carried_owner) in &mut carried_roots {
            let carry_exit = phis
                .iter()
                .find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == *carried_symbol
                })
                .unwrap();
            let carry_exhausted = binding(IterationPhiIncomingKind::Exhaustion, carry_exit.owner());
            let [source] = carry_exhausted.values() else {
                panic!("carried root needs one exhaustion source")
            };
            assert_eq!(source.source(), *carried_owner);
            assert!(selected(table, source.condition(), &choices));
        }
        let exit_values = replay_captured_edge(
            table,
            graph,
            phis,
            edge(IterationPhiIncomingKind::Exhaustion),
            ReplayInstances {
                values: &edge_values(&owners, &source_instances),
                nodes: &instance_nodes,
                captured: &captures,
            },
            &mut choices,
        );
        assert_eq!(
            exit_values.len(),
            edge(IterationPhiIncomingKind::Exhaustion).bindings().len()
        );
        forward_sources(
            IterationPhiIncomingKind::Exhaustion,
            &exit_values,
            &mut source_instances,
        );
        for (carried_symbol, carried_owner) in &mut carried_roots {
            let carry_exit = phis
                .iter()
                .find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == *carried_symbol
                })
                .unwrap();
            let instance = owners.remove(carried_owner).unwrap();
            assert_eq!(exit_values[&carry_exit.owner()], (*carried_owner, instance));
            assert!(owners.insert(carry_exit.owner(), instance).is_none());
            *carried_owner = carry_exit.owner();
            exit_roots.push(carry_exit.owner());
        }
        let instance = owners.remove(&exit_root.source()).unwrap();
        assert_eq!(exit_values[&exit.owner()], (exit_root.source(), instance));
        assert!(owners.insert(exit.owner(), instance).is_none());
        exit_roots.push(exit.owner());
        let parent_position = table.capture_slot_value(*parent_slot).unwrap().position();
        assert!(
            chains
                .iter()
                .all(|(position, _, _)| *position != parent_position)
        );
        chains.push((parent_position, seed, new_instance));
        carried_roots.push((symbol, exit.owner()));
    }
    chains.sort_by_key(|(position, _, _)| *position);
    assert_eq!(chains.len(), 2);
    let borrowed_instance = borrowed_child.map(
        |(symbol, closure, owner, snapshot, input, position, source_drop)| {
            let CleanupCaptureValue::Owner(source_owner) = input.value() else {
                panic!("shared capture must read its source owner")
            };
            if let std::collections::btree_map::Entry::Vacant(entry) =
                source_instances.entry(source_owner)
            {
                let Some(CleanupOwnerValue::Expression { expression, .. }) =
                    table.owner_value(source_owner)
                else {
                    panic!("new shared source must originate from its evaluated expression")
                };
                assert_eq!(
                    sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()),
                    Ok("listOf(1)")
                );
                next_instance += 1;
                entry.insert(next_instance);
            }
            if expect_source_guard_copy {
                assert_eq!(source_drop.owner(), Some(source_owner));
            }
            let source_instance = source_instances[&source_owner];
            next_instance += 1;
            let instance = next_instance;
            assert!(instances.insert(instance, closure).is_none());
            assert!(owners.insert(owner, instance).is_none());
            assert!(selected(table, input.condition(), &choices));
            assert!(
                shared_loans
                    .insert((instance, position), source_instance)
                    .is_none()
            );
            let [source] = table.owner_snapshot(snapshot).unwrap().capture_inputs() else {
                panic!("borrowed child snapshot must read its formed owner")
            };
            assert_eq!(source.owner(), owner);
            assert!(selected(table, source.condition(), &choices));
            assert_eq!(owners.remove(&owner), Some(instance));
            assert!(owners.insert(snapshot, instance).is_none());
            replay_snapshot_choices(table, snapshot, &mut choices);
            assert!(steps.iter().any(|(_, action)| matches!(
                action,
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == snapshot && *target == symbol
            )));
            carried_roots.push((symbol, snapshot));
            instance
        },
    );
    for (_, input) in &parent_saves {
        let ClosureCaptureSource::Symbol(symbol) = input.source() else {
            unreachable!("checked above")
        };
        let source = carried_roots
            .iter()
            .find(|(carried_symbol, _)| *carried_symbol == symbol)
            .unwrap()
            .1;
        assert_eq!(input.value(), CleanupCaptureValue::Owner(source));
    }
    next_instance += 1;
    let parent_instance = next_instance;
    assert!(instances.insert(parent_instance, closure).is_none());
    assert!(owners.insert(outer, parent_instance).is_none());
    for (target, input) in &parent_saves {
        assert!(selected(table, input.condition(), &choices));
        let CleanupCaptureValue::Owner(source) = input.value() else {
            panic!("parent must read each loop exit instance")
        };
        let child = owners.remove(&source).unwrap();
        let position = table.capture_slot_value(*target).unwrap().position();
        assert!(
            captures
                .insert((parent_instance, position), child)
                .is_none()
        );
    }
    let (parent_snapshot_index, parent_snapshot, parent_snapshot_guard) = steps
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            IterationCleanupAction::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } if *value == closure => Some((index, *owner, *condition)),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        steps[parent_snapshot_index].0,
        DropPoint::AfterExpression(closure)
    );
    assert!(parent_snapshot_guard.is_none_or(|guard| selected(table, guard, &choices)));
    let parent_creates = steps
        .iter()
        .enumerate()
        .filter_map(|(index, (point, action))| {
            matches!(action,
                IterationCleanupAction::CreateClosureOwner { owner, closure: value }
                    if *owner == outer && *value == closure)
            .then_some((index, *point))
        })
        .collect::<Vec<_>>();
    let [(parent_create_index, parent_create_point)] = parent_creates.as_slice() else {
        panic!("parent must have one CreateClosureOwner: {parent_creates:?}")
    };
    assert_eq!(*parent_create_point, DropPoint::AfterExpression(closure));
    assert!(*parent_create_index < parent_snapshot_index);
    let parent_capture_steps = steps
        .iter()
        .enumerate()
        .filter_map(|(index, (point, action))| {
            matches!(action,
                IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == outer)
            .then_some((index, *point))
        })
        .collect::<Vec<_>>();
    assert_eq!(parent_capture_steps.len(), parent_saves.len());
    assert!(parent_capture_steps.iter().all(|(index, point)| {
        *point == DropPoint::AfterExpression(closure)
            && *parent_create_index < *index
            && *index < parent_snapshot_index
    }));
    let [source] = table
        .owner_snapshot(parent_snapshot)
        .unwrap()
        .capture_inputs()
    else {
        panic!("parent snapshot must read the formed environment")
    };
    assert_eq!(source.owner(), outer);
    assert!(selected(table, source.condition(), &choices));
    let instance = owners.remove(&source.owner()).unwrap();
    assert!(owners.insert(parent_snapshot, instance).is_none());
    let source_guard_copy = borrowed_child
        .and_then(|(_, _, _, _, _, _, source_drop)| source_drop.condition())
        .and_then(|guard| match table.get(guard) {
            Some(CleanupCondition::Choice { selector, .. }) => Some(*selector),
            _ => None,
        })
        .and_then(|selector| {
            table
                .owner_snapshot(parent_snapshot)
                .unwrap()
                .copies()
                .iter()
                .copied()
                .find(|copy| copy.target() == selector)
        });
    assert_eq!(source_guard_copy.is_some(), expect_source_guard_copy);
    let copied_source_value = source_guard_copy.map(|copy| {
        assert!(selected(table, copy.when(), &choices));
        assert!(copy.source_value().is_none());
        assert!(!choices.contains_key(&copy.target()));
        (copy.target(), choices[&copy.source()])
    });
    replay_snapshot_choices(table, parent_snapshot, &mut choices);
    if let Some((target, expected)) = copied_source_value {
        assert_eq!(choices[&target], expected);
    }
    let parent_commit_index = steps
        .iter()
        .position(|(_, action)| {
            matches!(
                action,
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == parent_snapshot && *target == outer_symbol
            )
        })
        .unwrap();
    assert!(parent_commit_index > parent_snapshot_index);
    assert_eq!(
        steps[parent_commit_index].0,
        DropPoint::AfterExpression(closure)
    );
    let binding_instance = owners.remove(&parent_snapshot).unwrap();
    assert_eq!(binding_instance, parent_instance);
    assert_eq!(exit_roots.len(), 3);
    for owner in exit_roots {
        assert!(steps.iter().all(|(_, action)| {
            let root = match action {
                IterationCleanupAction::Drop(root)
                | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                _ => return true,
            };
            root.owner() != Some(owner)
                || root
                    .condition()
                    .is_some_and(|condition| !selected(table, condition, &choices))
        }));
    }
    for (symbol, owner) in &carried_roots {
        assert!(steps.iter().all(|(_, action)| {
            let root = match action {
                IterationCleanupAction::Drop(root)
                | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                _ => return true,
            };
            (root.owner() != Some(*owner) && root.target() != DropTarget::Named(*symbol))
                || root
                    .condition()
                    .is_some_and(|condition| !selected(table, condition, &choices))
        }));
    }
    let [(layout, root)] = outer_actions.as_slice() else {
        unreachable!("checked above")
    };
    assert_eq!(*layout, ClosureReleaseLayout::File);
    assert!(
        root.condition()
            .is_none_or(|guard| selected(table, guard, &choices))
    );
    enum ReleaseStep {
        Enter(usize),
        EndLoan(usize, usize),
        Finish(usize),
    }
    let mut pending = vec![ReleaseStep::Enter(binding_instance)];
    let mut visited = BTreeSet::new();
    let mut released = Vec::new();
    let mut ended_loans = Vec::new();
    let mut remaining_source_loans = usize::from(borrowed_instance.is_some());
    while let Some(step) = pending.pop() {
        match step {
            ReleaseStep::Enter(instance) => {
                assert!(visited.insert(instance), "an instance was released twice");
                pending.push(ReleaseStep::Finish(instance));
                for (position, capture) in checked.captures_of(instances[&instance]).enumerate() {
                    match (capture.mode(), capture.effect()) {
                        (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                            let child = captures.remove(&(instance, position)).unwrap();
                            pending.push(ReleaseStep::Enter(child));
                        }
                        (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow) => {
                            assert_eq!(Some(instance), borrowed_instance);
                            pending.push(ReleaseStep::EndLoan(instance, position));
                        }
                        other => panic!("unexpected capture in File replay: {other:?}"),
                    }
                }
            }
            ReleaseStep::EndLoan(instance, position) => {
                let Some((_, _, _, _, input, _, _)) = borrowed_child else {
                    unreachable!("only the borrowed child has a shared loan")
                };
                let CleanupCaptureValue::Owner(source_owner) = input.value() else {
                    panic!("shared loan must refer to a source owner")
                };
                assert_eq!(
                    shared_loans.remove(&(instance, position)),
                    Some(source_instances[&source_owner])
                );
                assert!(!released.contains(&instance));
                assert!(remaining_source_loans > 0);
                remaining_source_loans -= 1;
                ended_loans.push(instance);
            }
            ReleaseStep::Finish(instance) => released.push(instance),
        }
    }
    let mut expected = Vec::new();
    if let Some(instance) = borrowed_instance {
        expected.push(instance);
    }
    expected.extend([
        chains[1].1,
        chains[1].2,
        chains[0].1,
        chains[0].2,
        parent_instance,
    ]);
    assert_eq!(released, expected);
    assert_eq!(
        ended_loans,
        borrowed_instance.into_iter().collect::<Vec<_>>()
    );
    if let Some((_, _, _, _, _, _, source_drop)) = borrowed_child {
        assert_eq!(remaining_source_loans, 0);
        assert_eq!(released.last(), Some(&parent_instance));
        assert!(
            source_drop
                .condition()
                .is_none_or(|guard| selected(table, guard, &choices))
        );
        assert!(
            source_instances
                .remove(&source_drop.owner().unwrap())
                .is_some()
        );
    }
    assert!(
        owners.is_empty()
            && source_instances.is_empty()
            && captures.is_empty()
            && shared_loans.is_empty()
    );
}

#[test]
fn parent_of_two_recursive_loops_needs_file_wide_release_layout() {
    assert_cross_loop_parent_release(
        "fun run(first: List<Int>, second: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar g: move () -> Unit = move {}\nfor (_ in second) { g = move { g() } }\nval outer: move () -> Unit = move { val x = f()\nval y = g() }\nval used = outer() }",
        "move { val x = f()\nval y = g() }",
        2,
        "outer",
        false,
        true,
        false,
    );
}

#[test]
fn file_parent_with_recursive_chains_and_shared_child_keeps_one_release_root() {
    assert_cross_loop_parent_release(
        "fun read(xs: List<Int>) {}\nfun run(first: List<Int>, second: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar h: move () -> Unit = move {}\nfor (_ in second) { h = move { h() } }\nval xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nval outer: move () -> Unit = move { val a = f()\nval b = h()\nval c = g() }\nval used = outer() }",
        "move { val a = f()\nval b = h()\nval c = g() }",
        3,
        "outer",
        false,
        true,
        false,
    );
}

#[test]
fn file_parent_shared_source_crosses_two_recursive_loops() {
    assert_cross_loop_parent_release(
        "fun read(xs: List<Int>) {}\nfun run(first: List<Int>, second: List<Int>) {\nval xs = listOf(1)\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar h: move () -> Unit = move {}\nfor (_ in second) { h = move { h() } }\nval g: () -> Unit = { read(xs) }\nval outer: move () -> Unit = move { val a = f()\nval b = h()\nval c = g() }\nval used = outer() }",
        "move { val a = f()\nval b = h()\nval c = g() }",
        3,
        "outer",
        false,
        true,
        true,
    );
}

#[test]
fn parent_of_recursive_snapshot_needs_file_wide_release_layout() {
    assert_cross_loop_parent_release(
        "fun run(first: List<Int>, second: List<Int>, pick: Boolean) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar g: move () -> Unit = move {}\nfor (_ in second) { g = move { g() } }\nvar h: move () -> Unit = if (pick) f else g\nval outer: move () -> Unit = move { h() }\nval used = outer() }",
        "move { h() }",
        1,
        "outer",
        false,
        true,
        false,
    );
}

#[test]
fn nested_environment_capture_needs_file_wide_release_layout() {
    assert_cross_loop_parent_release(
        "fun run(flags: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval outer: move () -> Unit = move { val inner: move () -> Unit = move { val x = f() }\nval used = inner() }\nval used = outer() }",
        "move { val x = f() }",
        1,
        "inner",
        true,
        true,
        false,
    );
}

#[test]
fn nested_environment_capture_of_ordinary_sibling_keeps_ordinary_drop() {
    assert_cross_loop_parent_release(
        "fun run(flags: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval g: move () -> Unit = move {}\nval outer: move () -> Unit = move { val inner: move () -> Unit = move { val x = g() }\nval used = inner()\nval recursive = f() }\nval used = outer() }",
        "move { val x = g() }",
        1,
        "inner",
        true,
        false,
        false,
    );
}
