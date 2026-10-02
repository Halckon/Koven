use super::*;

#[test]
fn recursive_capture_jump_edges_carry_the_new_root_instance() {
    for (jump, boundary) in [
        ("continue", IterationPhiBoundary::Header),
        ("break", IterationPhiBoundary::Exit),
    ] {
        let mut sources = SourceMap::new();
        let body = if jump == "break" {
            "if (flag) { break }"
        } else {
            "continue"
        };
        let source = sources
                .add_source(
                    "recursive-jump.ko",
                    format!("fun run(flags: List<Boolean>) {{ var f: move () -> Unit = move {{}}\nfor (flag in flags) {{ f = move {{ f() }}\n{body} }}\nval used = f() }}"),
                )
                .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty(), "{jump}");
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{jump}");
        let mut checker =
            super::super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let capture_liveness = super::super::super::capture_liveness(&checker).unwrap();
        checker.expression_live_after = capture_liveness.expression_after;
        checker.statement_live_after = capture_liveness.statement_after;
        let mut state = super::super::super::super::State::default();
        for &root in parsed.roots() {
            checker.check_item(root, &mut state).unwrap();
        }
        assert!(checker.diagnostics.is_empty(), "{jump}");
        let liveness = super::super::super::liveness::Liveness::build(&checker).unwrap();
        let (origins, captures) = super::super::super::origins::analyze(&checker).unwrap();
        let mut planner =
            super::super::super::DropPlanner::new(&checker, liveness, origins, captures);
        for &root in parsed.roots() {
            planner.item(root).unwrap();
        }
        assert!(planner.recursive_capture_phi.is_some(), "{jump}");
        let statement = checker
            .iterations
            .values()
            .next()
            .unwrap()
            .descriptor()
            .statement();
        let candidate = planner.into_candidate_facts();
        let [plan] = candidate.iterations.as_slice() else {
            panic!("one loop plan must be assembled for {jump}")
        };
        assert_eq!(plan.descriptor().statement(), statement);
        let table = &candidate.cleanup_conditions;
        let steps = &candidate.cleanup_steps;
        let graph = plan.capture_graph();
        let header = plan
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
        let target = plan
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary)
            .unwrap();
        let edge = plan
            .closure_phi_incomings()
            .iter()
            .find(|incoming| {
                matches!(
                    (jump, incoming.kind()),
                    ("continue", IterationPhiIncomingKind::Continue(_))
                        | ("break", IterationPhiIncomingKind::Break(_))
                )
            })
            .unwrap();
        assert_eq!(edge.boundary(), boundary, "{jump}");
        assert_ne!(edge.condition(), CleanupConditionId::NEVER, "{jump}");
        if jump == "continue" {
            assert_eq!(edge.condition(), CleanupConditionId::ALWAYS);
        } else {
            let Some(CleanupCondition::Choice { selector, branches }) = table.get(edge.condition())
            else {
                panic!("break must be guarded by the current flag")
            };
            assert_eq!(
                branches,
                &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
            );
            let choice = table.selector(*selector).unwrap();
            let control = choice.control().unwrap();
            assert_eq!(
                sources
                    .slice(parsed.ast().expressions().get(control).unwrap().span())
                    .unwrap(),
                "if (flag) { break }"
            );
        }
        let input = edge
            .bindings()
            .iter()
            .find(|input| input.target() == target.owner())
            .unwrap();
        assert!(input.capture_slots_to_clear().is_empty(), "{jump}");
        assert_eq!(
            input.availability_selector(),
            target.availability_selector(),
            "{jump}"
        );
        assert_eq!(input.available_when(), edge.condition(), "{jump}");
        assert_eq!(input.values().len(), 1, "{jump}");
        assert_eq!(input.values()[0].condition(), edge.condition(), "{jump}");
        let snapshot = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *target == header.symbol() =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(input.values()[0].source(), snapshot, "{jump}");

        // 回放实际形成/捕获/快照动作：continue 的第二轮必须捕获第一轮实例，
        // break 则只把本轮新根交给 exit；两条边都不能把后代改指当前 phi。
        let entry = plan
            .closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let entry = entry
            .bindings()
            .iter()
            .find(|binding| binding.target() == header.owner())
            .unwrap();
        let active_root = |binding: &IterationPhiIncomingBinding,
                           choices: &BTreeMap<_, _>,
                           owners: &BTreeMap<_, usize>,
                           nodes: &BTreeMap<usize, usize>| {
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), choices))
                .collect::<Vec<_>>();
            let [value] = values.as_slice() else {
                panic!("{jump} must carry exactly one owner value")
            };
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| {
                    root.source() == value.source()
                        && root.node() == nodes[&owners[&value.source()]]
                        && selected(table, root.condition(), choices)
                })
                .collect::<Vec<_>>();
            let [root] = roots.as_slice() else {
                panic!("{jump} must carry exactly one formed root")
            };
            **root
        };
        let initial_owner = entry.values()[0].source();
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
        let mut formed = BTreeMap::new();
        for (_, action) in steps {
            if let IterationCleanupAction::CreateClosureOwner { owner, closure } = action {
                assert!(formed.insert(*owner, *closure).is_none());
            }
        }
        let captures = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if input.value() == CleanupCaptureValue::Owner(header.owner()) => {
                    Some((*owner, *target, *input))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [capture] = captures.as_slice() else {
            panic!("one formed environment must capture the previous header")
        };
        let capture = *capture;
        assert_eq!(capture.2.condition(), header.availability_condition());
        assert_eq!(capture.2.mode(), ClosureCaptureMode::Owned);
        assert_eq!(capture.2.effect(), ClosureCaptureEffect::Move);
        let slot = table.capture_slot_value(capture.1).unwrap();
        assert_eq!(slot.environment(), capture.0);
        assert_eq!(slot.source(), capture.2.source());
        let position = slot.position();
        let snapshot_input = table.owner_snapshot(snapshot).unwrap();
        assert_eq!(snapshot_input.capture_inputs().len(), 1);
        assert_eq!(snapshot_input.capture_inputs()[0].owner(), capture.0);
        let create_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. }
                    if *owner == capture.0)
        });
        let capture_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == capture.0)
        });
        let snapshot_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                    if *owner == snapshot)
        });
        let commit_at = unique_action_index(steps, |action| {
            matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, .. }
                    if *owner == snapshot)
        });
        assert_eq!(
            steps[commit_at].1,
            IterationCleanupAction::CommitOwnerSnapshot {
                owner: snapshot,
                target: header.symbol(),
            }
        );
        assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
        assert_eq!(steps[create_at].0, steps[capture_at].0);
        let IterationCleanupAction::SaveOwnerSnapshot {
            condition,
            owner,
            value,
        } = steps[snapshot_at].1
        else {
            unreachable!()
        };
        assert_eq!(owner, snapshot);
        assert_eq!(value, snapshot_input.value());
        assert!(condition.is_none_or(|guard| guard == CleanupConditionId::ALWAYS));
        assert_eq!(value, formed[&capture.0]);
        assert_eq!(
            sources
                .slice(parsed.ast().expressions().get(value).unwrap().span())
                .unwrap(),
            "move { f() }"
        );
        for index in [create_at, capture_at, snapshot_at, commit_at] {
            assert_eq!(steps[index].0, DropPoint::AfterExpression(value));
        }
        let exit = plan
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
            .unwrap();
        let root_drop = candidate
            .drops
            .iter()
            .find(|fact| fact.target() == DropTarget::Named(header.symbol()))
            .unwrap();
        assert_eq!(root_drop.owner(), Some(exit.owner()));
        let releases = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(release_statement),
                    root,
                } if *release_statement == statement && *root == *root_drop => {
                    Some((*point, *root))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(release_point, release_root)] = releases.as_slice() else {
            panic!("{jump} must have exactly one exit instance release")
        };
        let final_call = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| (sources.slice(node.span()) == Ok("f()")).then_some(id))
            .last()
            .unwrap();
        assert_eq!(*release_point, DropPoint::CallReturn(final_call));
        let chain_owners = plan
            .closure_phis()
            .iter()
            .map(|phi| phi.owner())
            .chain([initial_owner, capture.0, snapshot])
            .collect::<BTreeSet<_>>();
        let active_chain_actions = |choices: &BTreeMap<_, _>| {
            steps
                .iter()
                .filter_map(|(point, action)| {
                    let (fact, instance_release) = match action {
                        IterationCleanupAction::Drop(fact) => (fact, false),
                        IterationCleanupAction::ReleaseClosureInstances { root, .. } => {
                            (root, true)
                        }
                        _ => return None,
                    };
                    let from_chain = fact
                        .owner()
                        .is_some_and(|owner| chain_owners.contains(&owner))
                        || fact.instance_address().is_some_and(|address| {
                            chain_owners.contains(&table.instance_address(address).unwrap().root())
                        })
                        || (*point == *release_point
                            && fact.target() == DropTarget::Named(header.symbol()));
                    (from_chain
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, choices)))
                    .then_some((*point, instance_release, *fact))
                })
                .collect::<Vec<_>>()
        };
        let expected_release = (*release_point, true, *release_root);
        let check_before_exit = |choices: &BTreeMap<_, _>| {
            let actions = active_chain_actions(choices);
            assert!(
                actions.is_empty() || actions == vec![expected_release],
                "{jump} must not release a captured instance before the exit: {actions:?}"
            );
        };
        let mut owners = BTreeMap::from([(initial_owner, 1_usize)]);
        let mut instance_closures = BTreeMap::from([(1_usize, formed[&initial_owner])]);
        let mut instance_nodes = BTreeMap::from([(
            1_usize,
            graph
                .nodes()
                .iter()
                .position(|node| node.closure() == formed[&initial_owner])
                .unwrap(),
        )]);
        let mut captured = BTreeMap::new();
        let mut choices = table
            .nodes()
            .iter()
            .filter_map(|node| match node {
                CleanupCondition::Choice { selector, .. } => Some((*selector, 0)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        assert_ne!(formed[&initial_owner], formed[&capture.0]);
        let entry_root = active_root(entry, &choices, &owners, &instance_nodes);
        assert_eq!(entry_root.source(), initial_owner);
        assert_eq!(
            graph.nodes()[entry_root.node()].closure(),
            formed[&initial_owner]
        );
        let entry_edge = plan
            .closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let entry_values = replay_captured_edge(
            table,
            graph,
            plan.closure_phis(),
            entry_edge,
            ReplayInstances {
                values: &owners,
                nodes: &instance_nodes,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(entry_values[&entry.target()], (entry_root.source(), 1));
        let seed = owners.remove(&entry_root.source()).unwrap();
        owners.insert(entry.target(), seed);
        for root in header.root_origins() {
            assert_eq!(
                choices[&root.selector()],
                usize::from(root.node() == entry_root.node()),
                "{jump}"
            );
        }
        let rounds = if jump == "continue" { 2 } else { 1 };
        for instance in 2..=rounds + 1 {
            assert!(selected(table, capture.2.condition(), &choices));
            assert!(
                instance_closures
                    .insert(instance, formed[&capture.0])
                    .is_none()
            );
            assert!(
                instance_nodes
                    .insert(
                        instance,
                        graph
                            .nodes()
                            .iter()
                            .position(|node| node.closure() == formed[&capture.0])
                            .unwrap(),
                    )
                    .is_none()
            );
            let old = owners.remove(&header.owner()).unwrap();
            assert!(captured.insert((instance, position), old).is_none());
            owners.insert(capture.0, instance);
            let formed_value = owners
                .remove(&snapshot_input.capture_inputs()[0].owner())
                .unwrap();
            owners.insert(snapshot, formed_value);
            check_before_exit(&choices);
            replay_snapshot_choices(table, snapshot, &mut choices);
            check_before_exit(&choices);
            let incoming = if instance == rounds + 1 {
                input
            } else {
                plan.closure_phi_incomings()
                    .iter()
                    .find(|incoming| {
                        matches!(incoming.kind(), IterationPhiIncomingKind::Continue(_))
                    })
                    .unwrap()
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == header.owner())
                    .unwrap()
            };
            let root = active_root(incoming, &choices, &owners, &instance_nodes);
            assert_eq!(root.source(), snapshot);
            assert_eq!(graph.nodes()[root.node()].closure(), formed[&capture.0]);
            let jump_edge = plan
                .closure_phi_incomings()
                .iter()
                .find(|edge| {
                    matches!(
                        (jump, edge.kind()),
                        ("continue", IterationPhiIncomingKind::Continue(_))
                            | ("break", IterationPhiIncomingKind::Break(_))
                    )
                })
                .unwrap();
            let jump_values = replay_captured_edge(
                table,
                graph,
                plan.closure_phis(),
                jump_edge,
                ReplayInstances {
                    values: &owners,
                    nodes: &instance_nodes,
                    captured: &captured,
                },
                &mut choices,
            );
            assert_eq!(jump_values[&incoming.target()], (root.source(), instance));
            let carried = owners.remove(&root.source()).unwrap();
            let target_phi = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == incoming.target())
                .unwrap();
            let reached_nodes = BTreeSet::from([instance_nodes[&1], instance_nodes[&instance]]);
            for target_root in target_phi.root_origins() {
                assert_eq!(
                    choices[&target_root.selector()],
                    usize::from(reached_nodes.contains(&target_root.node())),
                    "{jump}"
                );
            }
            assert!(owners.insert(incoming.target(), carried).is_none());
        }
        if jump == "continue" {
            let exhaustion = plan
                .closure_phi_incomings()
                .iter()
                .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
                .unwrap();
            let forwarded = exhaustion
                .bindings()
                .iter()
                .find(|binding| binding.target() == exit.owner())
                .unwrap();
            assert_eq!(forwarded.values()[0].source(), header.owner());
            let forwarded_root = active_root(forwarded, &choices, &owners, &instance_nodes);
            assert_eq!(forwarded_root.source(), header.owner());
            assert_eq!(
                graph.nodes()[forwarded_root.node()].closure(),
                formed[&capture.0]
            );
            let exit_values = replay_captured_edge(
                table,
                graph,
                plan.closure_phis(),
                exhaustion,
                ReplayInstances {
                    values: &owners,
                    nodes: &instance_nodes,
                    captured: &captured,
                },
                &mut choices,
            );
            assert_eq!(
                exit_values[&forwarded.target()],
                (forwarded_root.source(), rounds + 1)
            );
            let root = owners.remove(&forwarded_root.source()).unwrap();
            let reached_nodes = BTreeSet::from([instance_nodes[&1], instance_nodes[&root]]);
            for target_root in exit.root_origins() {
                assert_eq!(
                    choices[&target_root.selector()],
                    usize::from(reached_nodes.contains(&target_root.node())),
                    "{jump}"
                );
            }
            owners.insert(forwarded.target(), root);
        }
        assert_eq!(active_chain_actions(&choices), vec![expected_release]);
        assert_eq!(instance_nodes.len(), instance_closures.len());
        let root_instance = owners.remove(&release_root.owner().unwrap()).unwrap();
        let released =
            replay_owned_closure_release(graph, &instance_nodes, &mut captured, root_instance);
        assert_eq!(released, (1..=rounds + 1).collect::<Vec<_>>(), "{jump}");
        assert!(owners.is_empty() && captured.is_empty(), "{jump}");
    }
}
