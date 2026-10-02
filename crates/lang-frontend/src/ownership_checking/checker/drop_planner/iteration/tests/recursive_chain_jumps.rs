use super::*;

#[test]
fn alternating_recursive_capture_jump_edges_keep_the_formed_chain() {
    for (jump, tail, boundary, rounds) in [
        (
            "continue",
            "continue",
            IterationPhiBoundary::Header,
            2_usize,
        ),
        (
            "break",
            "if (flag) { break }",
            IterationPhiBoundary::Exit,
            1_usize,
        ),
    ] {
        let mut sources = SourceMap::new();
        let source = sources
                .add_source(
                    "alternating-recursive-jump.ko",
                    format!("fun run(flags: List<Boolean>) {{ var f: move () -> Unit = move {{}}\nvar g: move () -> Unit = move {{}}\nfor (flag in flags) {{ {{ f = move {{ g() }} }}\ng = move {{ f() }}\n{tail} }}\nval used = g() }}"),
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
        let graph = plan.capture_graph();
        let node = |literal| {
            graph
                .nodes()
                .iter()
                .position(|node| {
                    sources.slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(node.closure())
                            .unwrap()
                            .span(),
                    ) == Ok(literal)
                })
                .unwrap()
        };
        let f_node = node("move { g() }");
        let g_node = node("move { f() }");
        let ClosureCaptureSource::Symbol(f_symbol) =
            graph.nodes()[g_node].sources()[0].capture().source()
        else {
            panic!("g must capture f")
        };
        let ClosureCaptureSource::Symbol(g_symbol) =
            graph.nodes()[f_node].sources()[0].capture().source()
        else {
            panic!("f must capture g")
        };
        let phis = plan.closure_phis();
        let phi = |symbol, boundary| {
            phis.iter()
                .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                .unwrap()
        };
        let f_header = phi(f_symbol, IterationPhiBoundary::Header);
        let g_header = phi(g_symbol, IterationPhiBoundary::Header);
        let g_exit = phi(g_symbol, IterationPhiBoundary::Exit);
        let incoming = |kind| {
            plan.closure_phi_incomings()
                .iter()
                .find(|edge| edge.kind() == kind)
                .unwrap()
        };
        let entry = incoming(IterationPhiIncomingKind::Entry);
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
        assert_eq!(jump_edge.boundary(), boundary, "{jump}");
        let jump_target = phi(g_symbol, boundary);
        fn binding(
            edge: &crate::ownership_checking::IterationPhiIncoming,
            owner: CleanupOwnerValueId,
        ) -> &IterationPhiIncomingBinding {
            edge.bindings()
                .iter()
                .find(|binding| binding.target() == owner)
                .unwrap()
        }
        let entry_g = binding(entry, g_header.owner());
        let jump_g = binding(jump_edge, jump_target.owner());
        assert_eq!(jump_g.values().len(), 1, "{jump}");
        for edge in [entry, jump_edge] {
            for binding in edge.bindings() {
                assert!(binding.capture_slots_to_clear().is_empty(), "{jump}");
                if phis.iter().any(|phi| {
                    phi.owner() == binding.target()
                        && phi
                            .root_nodes()
                            .iter()
                            .any(|&node| node == f_node || node == g_node)
                }) {
                    assert!(binding.origins().is_empty(), "{jump}");
                }
            }
        }
        assert!(binding(entry, f_header.owner()).values().is_empty());
        assert!(binding(entry, f_header.owner()).root_sources().is_empty());
        assert!(
            binding(jump_edge, phi(f_symbol, boundary).owner())
                .values()
                .is_empty()
        );
        assert!(
            binding(jump_edge, phi(f_symbol, boundary).owner())
                .root_sources()
                .is_empty()
        );
        let steps = &candidate.cleanup_steps;
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
        let formed = |closure| {
            let index = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::CreateClosureOwner { closure: formed, .. }
                        if *formed == closure)
            });
            let IterationCleanupAction::CreateClosureOwner { owner, .. } = steps[index].1 else {
                unreachable!()
            };
            owner
        };
        let f_owner = formed(graph.nodes()[f_node].closure());
        let g_owner = formed(graph.nodes()[g_node].closure());
        let capture = |owner| {
            let index = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveClosureCapture { owner: saved, .. }
                        if *saved == owner)
            });
            let IterationCleanupAction::SaveClosureCapture { target, input, .. } = steps[index].1
            else {
                unreachable!()
            };
            (target, input)
        };
        let (f_slot, f_input) = capture(f_owner);
        let (g_slot, g_input) = capture(g_owner);
        let snapshot = |closure| {
            let index = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveOwnerSnapshot { value, .. }
                        if *value == closure)
            });
            let IterationCleanupAction::SaveOwnerSnapshot { owner, .. } = steps[index].1 else {
                unreachable!()
            };
            owner
        };
        let f_snapshot = snapshot(graph.nodes()[f_node].closure());
        let g_snapshot = snapshot(graph.nodes()[g_node].closure());
        let formation = |closure, owner, slot, input, saved, symbol| {
            let create_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::CreateClosureOwner { owner: formed, .. }
                        if *formed == owner)
            });
            assert_eq!(
                steps[create_at].1,
                IterationCleanupAction::CreateClosureOwner { owner, closure }
            );
            let capture_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveClosureCapture { owner: saved_owner, .. }
                        if *saved_owner == owner)
            });
            assert_eq!(
                steps[capture_at].1,
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target: slot,
                    input,
                }
            );
            let snapshot_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner: snapshot, .. }
                        if *snapshot == saved)
            });
            let commit_at = unique_action_index(steps, |action| {
                matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner: committed, .. }
                        if *committed == saved)
            });
            assert_eq!(
                steps[commit_at].1,
                IterationCleanupAction::CommitOwnerSnapshot {
                    owner: saved,
                    target: symbol,
                }
            );
            assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
            for index in [create_at, capture_at, snapshot_at, commit_at] {
                assert_eq!(steps[index].0, DropPoint::AfterExpression(closure));
            }
            let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } = steps[snapshot_at].1
            else {
                unreachable!()
            };
            assert!(condition.is_none_or(|guard| guard == CleanupConditionId::ALWAYS));
            let snapshot = table.owner_snapshot(saved).unwrap();
            assert_eq!(snapshot.capture_inputs().len(), 1);
            assert_eq!(snapshot.capture_inputs()[0].owner(), owner);
            (create_at, commit_at)
        };
        let (_, f_commit_at) = formation(
            graph.nodes()[f_node].closure(),
            f_owner,
            f_slot,
            f_input,
            f_snapshot,
            f_symbol,
        );
        let (g_create_at, _) = formation(
            graph.nodes()[g_node].closure(),
            g_owner,
            g_slot,
            g_input,
            g_snapshot,
            g_symbol,
        );
        assert!(f_commit_at < g_create_at);
        assert_eq!(
            f_input.value(),
            CleanupCaptureValue::Owner(g_header.owner())
        );
        assert_eq!(g_input.value(), CleanupCaptureValue::Owner(f_snapshot));
        assert_eq!(jump_g.values()[0].source(), g_snapshot, "{jump}");
        let f_position = table.capture_slot_value(f_slot).unwrap().position();
        let g_position = table.capture_slot_value(g_slot).unwrap().position();
        let initial_owner = entry_g.values()[0].source();
        let initial_closure = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *owner == initial_owner =>
                {
                    Some(*closure)
                }
                _ => None,
            })
            .unwrap();
        let initial_node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == initial_closure)
            .unwrap();
        let active_root = |binding: &IterationPhiIncomingBinding,
                           owners: &BTreeMap<_, _>,
                           nodes: &BTreeMap<_, _>,
                           choices: &BTreeMap<_, _>| {
            let roots = binding
                .root_sources()
                .iter()
                .filter(|source| {
                    selected(table, source.condition(), choices)
                        && nodes[&owners[&source.source()]] == source.node()
                })
                .collect::<Vec<_>>();
            let [root] = roots.as_slice() else {
                panic!("{jump} must transport exactly one formed root")
            };
            **root
        };
        let active_root_actions = |owner, choices: &BTreeMap<_, _>| {
            steps
                .iter()
                .filter_map(|(point, action)| {
                    let (root, release) = match action {
                        IterationCleanupAction::Drop(root) => (root, false),
                        IterationCleanupAction::ReleaseClosureInstances { root, .. } => {
                            (root, true)
                        }
                        _ => return None,
                    };
                    ((root.owner() == Some(owner)
                        || root.instance_address().is_some_and(|address| {
                            table.instance_address(address).unwrap().root() == owner
                        }))
                        && root
                            .condition()
                            .is_none_or(|condition| selected(table, condition, choices)))
                    .then_some((*point, release))
                })
                .collect::<Vec<_>>()
        };
        let mut instance_nodes = BTreeMap::from([(1_usize, initial_node)]);
        let mut owners = BTreeMap::from([(initial_owner, 1_usize)]);
        let mut captured = BTreeMap::new();
        let mut choices = BTreeMap::new();
        let entry_root = active_root(entry_g, &owners, &instance_nodes, &choices);
        assert_eq!(entry_root.source(), initial_owner);
        assert_eq!(entry_root.node(), initial_node);
        let entry_values = replay_captured_edge(
            table,
            graph,
            phis,
            entry,
            ReplayInstances {
                values: &owners,
                nodes: &instance_nodes,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(entry_values.len(), 1);
        assert_eq!(entry_values[&g_header.owner()], (initial_owner, 1));
        assert_eq!(choices[&f_header.availability_selector()], 0, "{jump}");
        for root in g_header.root_origins() {
            assert_eq!(
                choices[&root.selector()],
                usize::from(root.node() == initial_node),
                "{jump}"
            );
        }
        assert_eq!(owners.remove(&entry_root.source()), Some(1));
        assert!(owners.insert(g_header.owner(), 1).is_none());
        let mut next_instance = 1;
        for round in 0..rounds {
            assert!(selected(table, f_input.condition(), &choices));
            next_instance += 1;
            let f_instance = next_instance;
            assert!(instance_nodes.insert(f_instance, f_node).is_none());
            assert!(owners.insert(f_owner, f_instance).is_none());
            let prior_g = owners.remove(&g_header.owner()).unwrap();
            assert!(captured.insert((f_instance, f_position), prior_g).is_none());
            assert_eq!(owners.remove(&f_owner), Some(f_instance));
            assert!(owners.insert(f_snapshot, f_instance).is_none());
            replay_snapshot_choices(table, f_snapshot, &mut choices);
            assert!(selected(table, g_input.condition(), &choices));
            next_instance += 1;
            let g_instance = next_instance;
            assert!(instance_nodes.insert(g_instance, g_node).is_none());
            assert!(owners.insert(g_owner, g_instance).is_none());
            let prior_f = owners.remove(&f_snapshot).unwrap();
            assert!(captured.insert((g_instance, g_position), prior_f).is_none());
            assert_eq!(owners.remove(&g_owner), Some(g_instance));
            assert!(owners.insert(g_snapshot, g_instance).is_none());
            replay_snapshot_choices(table, g_snapshot, &mut choices);
            for owner in [
                initial_owner,
                f_header.owner(),
                g_header.owner(),
                f_owner,
                f_snapshot,
                g_owner,
                g_snapshot,
            ] {
                assert!(active_root_actions(owner, &choices).is_empty(), "{jump}");
            }
            if jump == "break" {
                let Some(CleanupCondition::Choice { selector, branches }) =
                    table.get(jump_edge.condition())
                else {
                    panic!("break edge must depend on the current flag")
                };
                assert_eq!(
                    branches,
                    &[CleanupConditionId::ALWAYS, CleanupConditionId::NEVER]
                );
                let control = table.selector(*selector).unwrap().control().unwrap();
                assert_eq!(
                    sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                    Ok("if (flag) { break }")
                );
                let mut false_choices = choices.clone();
                false_choices.insert(*selector, 1);
                assert!(!selected(table, jump_edge.condition(), &false_choices));
                choices.insert(*selector, 0);
            } else {
                assert_eq!(jump_edge.condition(), CleanupConditionId::ALWAYS);
            }
            let jump_root = active_root(jump_g, &owners, &instance_nodes, &choices);
            assert_eq!(jump_root.source(), g_snapshot, "{jump}");
            assert_eq!(jump_root.node(), g_node, "{jump}");
            assert_eq!(owners[&jump_root.source()], g_instance, "{jump}");
            let jump_values = replay_captured_edge(
                table,
                graph,
                phis,
                jump_edge,
                ReplayInstances {
                    values: &owners,
                    nodes: &instance_nodes,
                    captured: &captured,
                },
                &mut choices,
            );
            assert_eq!(jump_values.len(), 1);
            assert_eq!(jump_values[&jump_g.target()], (g_snapshot, g_instance));
            assert_eq!(
                choices[&phi(f_symbol, boundary).availability_selector()],
                0,
                "{jump}"
            );
            for root in jump_target.root_origins() {
                assert_eq!(
                    choices[&root.selector()],
                    usize::from(root.node() == g_node || root.node() == initial_node),
                    "{jump}"
                );
            }
            assert_eq!(owners.remove(&jump_root.source()), Some(g_instance));
            assert!(owners.insert(jump_g.target(), g_instance).is_none());
            if jump == "continue" && round + 1 < rounds {
                assert_eq!(jump_g.target(), g_header.owner());
            }
        }
        if jump == "continue" {
            let exhausted = binding(
                incoming(IterationPhiIncomingKind::Exhaustion),
                g_exit.owner(),
            );
            assert!(exhausted.capture_slots_to_clear().is_empty());
            assert!(exhausted.origins().is_empty());
            assert_eq!(exhausted.values()[0].source(), g_header.owner());
            let exhausted_root = active_root(exhausted, &owners, &instance_nodes, &choices);
            assert_eq!(exhausted_root.source(), g_header.owner());
            assert_eq!(exhausted_root.node(), g_node);
            let exit_values = replay_captured_edge(
                table,
                graph,
                phis,
                incoming(IterationPhiIncomingKind::Exhaustion),
                ReplayInstances {
                    values: &owners,
                    nodes: &instance_nodes,
                    captured: &captured,
                },
                &mut choices,
            );
            assert_eq!(exit_values.len(), 1);
            assert_eq!(exit_values[&g_exit.owner()].0, g_header.owner());
            let instance = owners.remove(&exhausted_root.source()).unwrap();
            assert_eq!(instance, exit_values[&g_exit.owner()].1);
            assert!(owners.insert(g_exit.owner(), instance).is_none());
        }
        let releases = steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(released_statement),
                    root,
                } if *released_statement == statement
                    && root.owner() == Some(g_exit.owner())
                    && root
                        .condition()
                        .is_none_or(|condition| selected(table, condition, &choices)) =>
                {
                    Some((*point, *root))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(release_point, release)] = releases.as_slice() else {
            panic!("{jump} exit must have exactly one active instance release")
        };
        let final_call = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| (sources.slice(node.span()) == Ok("g()")).then_some(id))
            .last()
            .unwrap();
        assert_eq!(*release_point, DropPoint::CallReturn(final_call));
        assert_eq!(
            active_root_actions(g_exit.owner(), &choices),
            vec![(*release_point, true)]
        );
        assert_eq!(release.target(), DropTarget::Named(g_symbol));
        let chain_owners = phis
            .iter()
            .map(|phi| phi.owner())
            .chain([initial_owner, f_owner, f_snapshot, g_owner, g_snapshot])
            .collect::<BTreeSet<_>>();
        let active_chain_actions = steps
            .iter()
            .filter_map(|(point, action)| {
                let (fact, instance_release) = match action {
                    IterationCleanupAction::Drop(fact) => (fact, false),
                    IterationCleanupAction::ReleaseClosureInstances { root, .. } => (root, true),
                    _ => return None,
                };
                let from_chain = fact
                    .owner()
                    .is_some_and(|owner| chain_owners.contains(&owner))
                    || fact.instance_address().is_some_and(|address| {
                        chain_owners.contains(&table.instance_address(address).unwrap().root())
                    })
                    || (*point == *release_point && fact.target() == DropTarget::Named(g_symbol));
                (from_chain
                    && fact
                        .condition()
                        .is_none_or(|condition| selected(table, condition, &choices)))
                .then_some((*point, instance_release, *fact))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            active_chain_actions,
            vec![(*release_point, true, *release)],
            "{jump} must release the formed chain only from its exit root"
        );
        let root = owners.remove(&release.owner().unwrap()).unwrap();
        let released = replay_owned_closure_release(graph, &instance_nodes, &mut captured, root);
        assert_eq!(released, (1..=next_instance).collect::<Vec<_>>(), "{jump}");
        assert!(owners.is_empty() && captured.is_empty(), "{jump}");
    }
}
