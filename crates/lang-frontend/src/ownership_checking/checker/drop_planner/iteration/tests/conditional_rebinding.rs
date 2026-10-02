use super::*;

#[test]
fn conditional_seed_recursive_rebinding_jump_matrix() {
    for tail in ["", "continue", "if (stop) { break }"] {
        assert_conditional_seed_rebinding(tail);
    }
}

fn assert_conditional_seed_rebinding(tail: &str) {
    let breaking = tail == "if (stop) { break }";
    let mut sources = SourceMap::new();
    let source = sources
            .add_source(
                "conditional-recursive-seed.ko",
                format!("fun run(flag: Boolean, stop: Boolean, flags: List<Int>) {{\nvar f: move () -> Unit = if (flag) (move {{}}) else (move {{}})\nfor (_ in flags) {{ f = move {{ f() }}\n{tail} }}\nval used = f() }}"),
            )
            .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (names, types) = crate::type_checking::standard_environments();
    let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
    let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let mut checker =
        super::super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
    let capture_liveness = super::super::super::capture_liveness(&checker).unwrap();
    checker.expression_live_after = capture_liveness.expression_after;
    checker.statement_live_after = capture_liveness.statement_after;
    let mut state = super::super::super::super::State::default();
    for &root in parsed.roots() {
        checker.check_item(root, &mut state).unwrap();
    }
    assert!(checker.diagnostics.is_empty());
    let liveness = super::super::super::liveness::Liveness::build(&checker).unwrap();
    let (origins, captures) = super::super::super::origins::analyze(&checker).unwrap();
    let mut planner = super::super::super::DropPlanner::new(&checker, liveness, origins, captures);
    for &root in parsed.roots() {
        planner.item(root).unwrap();
    }
    assert!(planner.recursive_capture_phi.is_some(), "{tail}");
    let statement = checker
        .iterations
        .values()
        .next()
        .unwrap()
        .descriptor()
        .statement();
    let crate::parser::Statement::For { source, body, .. } =
        parsed.ast().statements().get(statement).unwrap().payload()
    else {
        panic!("expected for loop");
    };
    let candidate = planner.into_candidate_facts();
    let [plan] = candidate.iterations.as_slice() else {
        panic!("one loop plan must be assembled for {tail}")
    };
    assert_eq!(plan.descriptor().statement(), statement);
    let table = &candidate.cleanup_conditions;
    let steps = &candidate.cleanup_steps;
    let graph = plan.capture_graph();
    let phis = plan.closure_phis();
    let header = phis
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
        .unwrap();
    let exit = phis
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        .unwrap();
    let incoming = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == kind)
            .unwrap()
    };
    let entry = incoming(IterationPhiIncomingKind::Entry);
    let jump = plan
        .closure_phi_incomings()
        .iter()
        .find(|edge| {
            matches!(
                (tail, edge.kind()),
                ("", IterationPhiIncomingKind::Fallthrough)
                    | ("continue", IterationPhiIncomingKind::Continue(_))
                    | ("if (stop) { break }", IterationPhiIncomingKind::Break(_))
            )
        })
        .unwrap();
    let exhaustion = incoming(IterationPhiIncomingKind::Exhaustion);
    assert_eq!(entry.point(), DropPoint::AfterExpression(*source));
    assert_eq!(entry.boundary(), IterationPhiBoundary::Header);
    let jump_boundary = if breaking {
        IterationPhiBoundary::Exit
    } else {
        IterationPhiBoundary::Header
    };
    assert_eq!(jump.boundary(), jump_boundary);
    match jump.point() {
        DropPoint::AfterStatement(completed) if tail.is_empty() => {
            assert_eq!(completed, *body);
        }
        DropPoint::ControlTransfer(control) if !tail.is_empty() => {
            assert_eq!(
                sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                Ok(if breaking { "break" } else { tail })
            );
        }
        other => panic!("{tail} has the wrong phi execution point: {other:?}"),
    }
    assert_eq!(exhaustion.point(), DropPoint::LoopExit(statement));
    assert_eq!(exhaustion.boundary(), IterationPhiBoundary::Exit);
    fn binding(
        edge: &crate::ownership_checking::IterationPhiIncoming,
        owner: CleanupOwnerValueId,
    ) -> &IterationPhiIncomingBinding {
        edge.bindings()
            .iter()
            .find(|binding| binding.target() == owner)
            .unwrap()
    }
    fn active_root(
        table: &CleanupConditions,
        binding: &IterationPhiIncomingBinding,
        owners: &BTreeMap<CleanupOwnerValueId, usize>,
        nodes: &BTreeMap<usize, usize>,
        choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) -> Option<(usize, CleanupOwnerValueId)> {
        let available = selected(table, binding.available_when(), choices);
        let values = binding
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), choices))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), usize::from(available));
        let roots = binding
            .root_sources()
            .iter()
            .filter(|root| {
                values
                    .first()
                    .is_some_and(|value| value.source() == root.source())
                    && selected(table, root.condition(), choices)
                    && nodes[&owners[&root.source()]] == root.node()
            })
            .collect::<Vec<_>>();
        assert_eq!(roots.len(), values.len());
        roots.first().map(|root| {
            assert_eq!(root.source(), values[0].source());
            (root.node(), root.source())
        })
    }
    let entry_binding = binding(entry, header.owner());
    let jump_target = if breaking { exit } else { header };
    let jump_binding = binding(jump, jump_target.owner());
    let exit_binding = binding(exhaustion, exit.owner());
    let [entry_value] = entry_binding.values() else {
        panic!("the seed must have one snapshot source")
    };
    let seed_snapshot = table.owner_snapshot(entry_value.source()).unwrap();
    assert_eq!(seed_snapshot.capture_inputs().len(), 2);
    let control = seed_snapshot.copies()[0].source();
    assert!(matches!(
        table.selector(control).unwrap().source(),
        CleanupSelectorSource::Control(_)
    ));
    fn unique_action_index(
        steps: &[(DropPoint, IterationCleanupAction)],
        matches: impl Fn(&IterationCleanupAction) -> bool,
    ) -> usize {
        let found = steps
            .iter()
            .enumerate()
            .filter_map(|(index, (_, action))| matches(action).then_some(index))
            .collect::<Vec<_>>();
        let [index] = found.as_slice() else {
            panic!("one matching formation action must exist")
        };
        *index
    }
    let capture_at = unique_action_index(steps, |action| {
        matches!(action, IterationCleanupAction::SaveClosureCapture { input, .. }
                if input.value() == CleanupCaptureValue::Owner(header.owner()))
    });
    let recursive_create = steps[capture_at].1;
    let IterationCleanupAction::SaveClosureCapture {
        owner: recursive_owner,
        target: recursive_slot,
        input: recursive_input,
    } = recursive_create
    else {
        unreachable!()
    };
    assert_eq!(
        unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == recursive_owner)
        }),
        capture_at
    );
    let create_at = unique_action_index(steps, |action| {
        matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. }
                if *owner == recursive_owner)
    });
    let IterationCleanupAction::CreateClosureOwner {
        closure: recursive_closure,
        ..
    } = steps[create_at].1
    else {
        unreachable!()
    };
    assert_eq!(
        unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::CreateClosureOwner { closure, .. }
                    if *closure == recursive_closure)
        }),
        create_at
    );
    let snapshot_at = unique_action_index(steps, |action| {
        matches!(action, IterationCleanupAction::SaveOwnerSnapshot { value, .. }
                if *value == recursive_closure)
    });
    let IterationCleanupAction::SaveOwnerSnapshot {
        owner: recursive_snapshot,
        ..
    } = steps[snapshot_at].1
    else {
        unreachable!()
    };
    assert_eq!(
        unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                    if *owner == recursive_snapshot)
        }),
        snapshot_at
    );
    let commit_at = unique_action_index(steps, |action| {
        matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, .. }
                if *owner == recursive_snapshot)
    });
    let formation_point = DropPoint::AfterExpression(recursive_closure);
    assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
    for index in [create_at, capture_at, snapshot_at, commit_at] {
        assert_eq!(steps[index].0, formation_point);
    }
    let formation = [create_at, capture_at, snapshot_at, commit_at]
        .iter()
        .map(|&index| steps[index].1)
        .collect::<Vec<_>>();
    let recursive_node = graph
        .nodes()
        .iter()
        .position(|node| node.closure() == recursive_closure)
        .unwrap();
    assert_eq!(jump_binding.values()[0].source(), recursive_snapshot);
    let capture_position = table.capture_slot_value(recursive_slot).unwrap().position();
    assert!(matches!(formation.as_slice(), [
            IterationCleanupAction::CreateClosureOwner { owner, closure },
            IterationCleanupAction::SaveClosureCapture { owner: captured, target, input },
            IterationCleanupAction::SaveOwnerSnapshot { owner: snapshot, value, .. },
            IterationCleanupAction::CommitOwnerSnapshot { owner: committed, target: symbol },
        ] if *owner == recursive_owner
            && *closure == recursive_closure
            && *captured == recursive_owner
            && *target == recursive_slot
            && *input == recursive_input
            && *snapshot == recursive_snapshot
            && *value == recursive_closure
            && *committed == recursive_snapshot
            && *symbol == header.symbol()));
    assert!(!steps.iter().any(|(point, action)| {
        *point == formation_point
            && matches!(action,
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. }
                    if fact.target() == DropTarget::Named(header.symbol())
                        || fact.owner() == Some(header.owner()))
    }));
    let final_call = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("f()"))
        .max_by_key(|(_, node)| node.span().start())
        .map(|(id, _)| id)
        .unwrap();
    let releases = steps
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(loop_id),
                root,
            } if *loop_id == statement
                && root.owner() == Some(exit.owner())
                && root.target() == DropTarget::Named(header.symbol()) =>
            {
                Some((*point, *root))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(point, release)] = releases.as_slice() else {
        panic!("final f root must have one instance release")
    };
    assert_eq!(*point, DropPoint::CallReturn(final_call));
    for branch in 0..2 {
        for rounds in [0, if breaking { 1 } else { 2 }] {
            let mut choices = BTreeMap::from([(control, branch)]);
            let mut executed_points = Vec::new();
            let seed_input = seed_snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(table, input.condition(), &choices))
                .collect::<Vec<_>>();
            let [seed_input] = seed_input.as_slice() else {
                panic!("one branch must form the seed environment")
            };
            let watched = [
                seed_input.owner(),
                entry_value.source(),
                header.owner(),
                recursive_owner,
                recursive_snapshot,
                exit.owner(),
            ];
            let root_actions_at = |at: DropPoint,
                                   choices: &BTreeMap<
                crate::ownership_checking::CleanupSelectorId,
                usize,
            >| {
                steps
                    .iter()
                    .filter(|(point, _)| *point == at)
                    .filter_map(|(_, action)| {
                        let (fact, instance_release) = match action {
                            IterationCleanupAction::Drop(fact) => (fact, false),
                            IterationCleanupAction::ReleaseClosureInstances { root, .. } => {
                                (root, true)
                            }
                            _ => return None,
                        };
                        let address_root = fact
                            .instance_address()
                            .map(|address| table.instance_address(address).unwrap().root());
                        ((fact.owner().is_some_and(|owner| watched.contains(&owner))
                            || address_root.is_some_and(|owner| watched.contains(&owner))
                            || fact.target() == DropTarget::Named(header.symbol()))
                            && fact
                                .condition()
                                .is_none_or(|guard| selected(table, guard, choices)))
                        .then_some((instance_release, *fact))
                    })
                    .collect::<Vec<_>>()
            };
            let seed_closure = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == seed_input.owner() =>
                    {
                        Some(*closure)
                    }
                    _ => None,
                })
                .unwrap();
            let seed_node = graph
                .nodes()
                .iter()
                .position(|node| node.closure() == seed_closure)
                .unwrap();
            assert_ne!(seed_node, recursive_node);
            let seed_create = steps
                .iter()
                .find(|(_, action)| {
                    matches!(action,
                        IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == seed_input.owner() && *closure == seed_closure)
                })
                .unwrap();
            assert_eq!(seed_create.0, DropPoint::AfterExpression(seed_closure));
            assert!(root_actions_at(seed_create.0, &choices).is_empty());
            executed_points.push((seed_create.0, choices.clone()));
            let seed_save = steps
                .iter()
                .find(|(_, action)| {
                    matches!(action,
                        IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *owner == entry_value.source() && *value == seed_snapshot.value())
                })
                .unwrap();
            assert_eq!(
                seed_save.0,
                DropPoint::AfterExpression(seed_snapshot.value())
            );
            let seed_create_at = steps.iter().position(|step| step == seed_create).unwrap();
            let seed_save_at = steps.iter().position(|step| step == seed_save).unwrap();
            assert!(seed_create_at < seed_save_at);
            let mut nodes = BTreeMap::new();
            let mut owners = BTreeMap::new();
            let mut captured = BTreeMap::new();
            let IterationCleanupAction::CreateClosureOwner { owner, closure } = seed_create.1
            else {
                unreachable!()
            };
            assert_eq!(closure, seed_closure);
            assert!(nodes.insert(1_usize, seed_node).is_none());
            assert!(owners.insert(owner, 1_usize).is_none());
            let IterationCleanupAction::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } = seed_save.1
            else {
                unreachable!()
            };
            assert_eq!(value, seed_snapshot.value());
            assert!(condition.is_none_or(|guard| { selected(table, guard, &choices) }));
            let inputs = seed_snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(table, input.condition(), &choices))
                .collect::<Vec<_>>();
            assert_eq!(inputs.len(), 1);
            let formed = owners.remove(&inputs[0].owner()).unwrap();
            assert!(owners.insert(owner, formed).is_none());
            replay_snapshot_choices(table, owner, &mut choices);
            assert!(root_actions_at(seed_save.0, &choices).is_empty());
            executed_points.push((seed_save.0, choices.clone()));
            let entry_root = active_root(table, entry_binding, &owners, &nodes, &choices).unwrap();
            assert_eq!(entry_root, (seed_node, entry_value.source()));
            assert!(root_actions_at(entry.point(), &choices).is_empty());
            let entry_values = replay_captured_edge(
                table,
                graph,
                phis,
                entry,
                ReplayInstances {
                    values: &owners,
                    nodes: &nodes,
                    captured: &captured,
                },
                &mut choices,
            );
            assert_eq!(entry_values.len(), 1);
            assert_eq!(entry_values[&header.owner()], (entry_value.source(), 1));
            executed_points.push((entry.point(), choices.clone()));
            assert_eq!(owners.remove(&entry_value.source()), Some(1));
            assert!(owners.insert(header.owner(), 1).is_none());
            let mut chain = vec![1_usize];
            for round in 0..rounds {
                let instance = round + 2;
                let before_formation = choices.clone();
                for action in &formation {
                    match action {
                        IterationCleanupAction::CreateClosureOwner { owner, closure } => {
                            assert_eq!((*owner, *closure), (recursive_owner, recursive_closure));
                            assert!(nodes.insert(instance, recursive_node).is_none());
                            assert!(owners.insert(*owner, instance).is_none());
                        }
                        IterationCleanupAction::SaveClosureCapture {
                            owner,
                            target,
                            input,
                        } => {
                            assert_eq!(*owner, recursive_owner);
                            assert!(selected(table, input.condition(), &choices));
                            assert_eq!(input.effect(), ClosureCaptureEffect::Move);
                            let CleanupCaptureValue::Owner(source) = input.value() else {
                                panic!("recursive capture must read the old header")
                            };
                            let old = owners.remove(&source).unwrap();
                            let position = table.capture_slot_value(*target).unwrap().position();
                            assert_eq!(position, capture_position);
                            assert!(captured.insert((owners[owner], position), old).is_none());
                        }
                        IterationCleanupAction::SaveOwnerSnapshot {
                            condition,
                            owner,
                            value,
                        } => {
                            assert_eq!((*owner, *value), (recursive_snapshot, recursive_closure));
                            assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
                            let snapshot = table.owner_snapshot(*owner).unwrap();
                            let inputs = snapshot
                                .capture_inputs()
                                .iter()
                                .filter(|input| selected(table, input.condition(), &choices))
                                .collect::<Vec<_>>();
                            assert_eq!(inputs.len(), 1);
                            let formed = owners.remove(&inputs[0].owner()).unwrap();
                            assert!(owners.insert(*owner, formed).is_none());
                            replay_snapshot_choices(table, *owner, &mut choices);
                        }
                        IterationCleanupAction::CommitOwnerSnapshot { owner, target } => {
                            assert_eq!((*owner, *target), (recursive_snapshot, header.symbol()));
                            assert_eq!(owners[owner], instance);
                        }
                        _ => unreachable!(),
                    }
                }
                assert!(root_actions_at(formation_point, &before_formation).is_empty());
                assert!(root_actions_at(formation_point, &choices).is_empty());
                executed_points.push((formation_point, before_formation));
                executed_points.push((formation_point, choices.clone()));
                if breaking {
                    let Some(CleanupCondition::Choice { selector, branches }) =
                        table.get(jump.condition())
                    else {
                        panic!("break edge must depend on stop")
                    };
                    assert_eq!(
                        branches,
                        &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
                    );
                    let control = table.selector(*selector).unwrap().control().unwrap();
                    assert_eq!(
                        sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                        Ok(tail)
                    );
                    let mut skipped = choices.clone();
                    skipped.insert(*selector, 1);
                    assert!(!selected(table, jump.condition(), &skipped));
                    choices.insert(*selector, 0);
                }
                let jump_root =
                    active_root(table, jump_binding, &owners, &nodes, &choices).unwrap();
                assert_eq!(jump_root, (recursive_node, recursive_snapshot));
                assert!(root_actions_at(jump.point(), &choices).is_empty());
                let jump_values = replay_captured_edge(
                    table,
                    graph,
                    phis,
                    jump,
                    ReplayInstances {
                        values: &owners,
                        nodes: &nodes,
                        captured: &captured,
                    },
                    &mut choices,
                );
                assert_eq!(jump_values.len(), 1);
                assert_eq!(
                    jump_values[&jump_target.owner()],
                    (recursive_snapshot, instance)
                );
                executed_points.push((jump.point(), choices.clone()));
                assert_eq!(owners.remove(&recursive_snapshot), Some(instance));
                assert!(owners.insert(jump_target.owner(), instance).is_none());
                chain.push(instance);
            }
            if breaking && rounds > 0 {
                assert!(!owners.contains_key(&header.owner()));
                assert_eq!(owners[&exit.owner()], chain[rounds]);
            } else {
                let exit_root =
                    active_root(table, exit_binding, &owners, &nodes, &choices).unwrap();
                assert_eq!(exit_root.0, *nodes.get(&chain[rounds]).unwrap());
                assert_eq!(exit_root.1, header.owner());
                assert!(root_actions_at(exhaustion.point(), &choices).is_empty());
                let exit_values = replay_captured_edge(
                    table,
                    graph,
                    phis,
                    exhaustion,
                    ReplayInstances {
                        values: &owners,
                        nodes: &nodes,
                        captured: &captured,
                    },
                    &mut choices,
                );
                assert_eq!(exit_values.len(), 1);
                assert_eq!(exit_values[&exit.owner()], (header.owner(), chain[rounds]));
                executed_points.push((exhaustion.point(), choices.clone()));
                let root = owners.remove(&header.owner()).unwrap();
                assert!(owners.insert(exit.owner(), root).is_none());
            }
            executed_points.push((DropPoint::AfterStatement(statement), choices.clone()));
            assert!(
                release
                    .condition()
                    .is_none_or(|guard| { selected(table, guard, &choices) })
            );
            assert_eq!(root_actions_at(*point, &choices), [(true, *release)]);
            executed_points.push((*point, choices.clone()));
            executed_points.push((DropPoint::AfterExpression(final_call), choices.clone()));
            let selected_root_actions = executed_points
                .iter()
                .flat_map(|(at, at_choices)| {
                    root_actions_at(*at, at_choices)
                        .into_iter()
                        .map(|(instance_release, fact)| (*at, instance_release, fact))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                selected_root_actions,
                [(*point, true, *release)],
                "{tail} branch {branch}, rounds {rounds} must release only at f()"
            );
            let root = owners.remove(&release.owner().unwrap()).unwrap();
            assert_eq!(root, chain[rounds]);
            let released = replay_owned_closure_release(graph, &nodes, &mut captured, root);
            assert_eq!(released, chain);
            assert!(captured.is_empty());
            assert!(owners.is_empty());
        }
    }
}
