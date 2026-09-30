use super::*;
use crate::ownership_checking::checker::{self, drop_planner};

#[test]
fn owned_self_chain_candidate_exit_matrix() {
    for tail in [
        "",
        "continue",
        "if (stop) { break }",
        "if (stop) { return }",
    ] {
        for rounds in [0, 1, 2, 7] {
            replay_chain(tail, rounds, false);
        }
    }
}

#[test]
fn owned_alternating_chain_candidate_exit_matrix() {
    for tail in [
        "",
        "continue",
        "if (stop) { break }",
        "if (stop) { return }",
    ] {
        for rounds in [0, 1, 2, 7] {
            replay_chain(tail, rounds, true);
        }
    }
}

#[test]
fn shared_recursive_chain_candidate_exit_matrix() {
    for alternating in [false, true] {
        for tail in [
            "",
            "continue",
            "if (stop) { break }",
            "if (stop) { return }",
        ] {
            for rounds in [0, 1, 2, 7] {
                replay_chain_with_fault(
                    tail,
                    rounds,
                    alternating,
                    Fault::None,
                    SharedSourceKind::Owned,
                );
            }
        }
    }
}

#[test]
fn shared_recursive_parameter_candidate_exit_matrix() {
    for alternating in [false, true] {
        for tail in [
            "",
            "continue",
            "if (stop) { break }",
            "if (stop) { return }",
        ] {
            for rounds in [0, 1, 2, 7] {
                replay_chain_with_fault(
                    tail,
                    rounds,
                    alternating,
                    Fault::None,
                    SharedSourceKind::External,
                );
            }
        }
    }
}

#[test]
fn conditional_seed_candidate_exit_matrix() {
    for source in [
        SharedSourceKind::None,
        SharedSourceKind::Owned,
        SharedSourceKind::External,
    ] {
        for alternating in [false, true] {
            for branch in [0, 1] {
                for tail in [
                    "",
                    "continue",
                    "if (stop) { break }",
                    "if (stop) { return }",
                ] {
                    for rounds in [0, 1, 2, 7] {
                        replay_seed_chain(
                            tail,
                            rounds,
                            alternating,
                            Fault::None,
                            source,
                            Some(branch),
                        );
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SharedSourceKind {
    None,
    Owned,
    External,
}

fn replay_chain(tail: &str, rounds: usize, alternating: bool) {
    replay_chain_with_fault(
        tail,
        rounds,
        alternating,
        Fault::None,
        SharedSourceKind::None,
    );
}

#[test]
#[should_panic(expected = "selected named root has no live binding")]
fn owned_candidate_replay_rejects_release_after_final_call() {
    replay_chain_with_fault(
        "",
        2,
        false,
        Fault::DuplicateAfterCall,
        SharedSourceKind::None,
    );
}

#[test]
#[should_panic(expected = "release layout must retain every checked capture")]
fn shared_recursive_replay_rejects_missing_loan_layout() {
    replay_chain_with_fault(
        "",
        2,
        false,
        Fault::MissingSharedLayout,
        SharedSourceKind::Owned,
    );
}

#[test]
#[should_panic(expected = "source released with live capture loans")]
fn shared_recursive_replay_rejects_early_source_drop() {
    replay_chain_with_fault(
        "",
        2,
        false,
        Fault::EarlySourceDrop,
        SharedSourceKind::Owned,
    );
}

enum Fault {
    None,
    DuplicateAfterCall,
    MissingSharedLayout,
    EarlySourceDrop,
}

fn replay_chain_with_fault(
    tail: &str,
    rounds: usize,
    alternating: bool,
    fault: Fault,
    source_kind: SharedSourceKind,
) {
    replay_seed_chain(tail, rounds, alternating, fault, source_kind, None);
}

fn replay_seed_chain(
    tail: &str,
    rounds: usize,
    alternating: bool,
    fault: Fault,
    source_kind: SharedSourceKind,
    seed_branch: Option<usize>,
) {
    use crate::{ownership_checking::CleanupSelectorSource, source::SourceMap};
    let shared = source_kind != SharedSourceKind::None;
    let external = source_kind == SharedSourceKind::External;
    let mut sources = SourceMap::new();
    let (extra_seed, body, final_call) = if alternating {
        (
            "var g: move () -> Unit = move {}\n",
            "{ f = move { g() } }\ng = move { f() }",
            "g()",
        )
    } else {
        ("", "f = move { f() }", "f()")
    };
    let f_body = if shared {
        "move { val old = f()\nval borrowed = b() }"
    } else {
        "move { f() }"
    };
    let g_body = if shared {
        "move { val old = g()\nval borrowed = b() }"
    } else {
        "move { g() }"
    };
    let body = if shared {
        if alternating {
            format!(
                "{{ val b: () -> Unit = {{ read(xs) }}\nf = {g_body} }}\n{{ val b: () -> Unit = {{ read(xs) }}\ng = {f_body} }}"
            )
        } else {
            format!("val b: () -> Unit = {{ read(xs) }}\nf = {f_body}")
        }
    } else {
        body.to_owned()
    };
    let resource_decl = if shared && !external {
        "val xs = listOf(1)\n"
    } else {
        ""
    };
    let seed = if seed_branch.is_some() {
        "if (seed) (move {}) else (move {})"
    } else {
        "move {}"
    };
    let source = sources.add_source("owned-instance-replay.ko", format!(
        "fun read(xs: List<Int>) {{}}\nfun run(flags: List<Boolean>, seed: Boolean) {{ {resource_decl}var f: move () -> Unit = {seed}\n{extra_seed}for (stop in flags) {{ {body}\n{tail} }}\nval used = {final_call} }}"
    ).replace("fun run(flags:", if external { "fun run(xs: Int, flags:" } else { "fun run(flags:" })
     .replace("fun read(xs: List<Int>)", if external { "fun read(xs: Int)" } else { "fun read(xs: List<Int>)" })).unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = crate::type_checking::standard_environments();
    let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
    let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let checked =
        crate::ownership_checking::check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(checked.diagnostics().is_empty());
    let mut checker = checker::Checker::new(&sources, &parsed, &names, &typed).unwrap();
    let liveness = drop_planner::capture_liveness(&checker).unwrap();
    checker.expression_live_after = liveness.expression_after;
    checker.statement_live_after = liveness.statement_after;
    let mut state = checker::State::default();
    for &root in parsed.roots() {
        checker.check_item(root, &mut state).unwrap();
    }
    assert!(checker.diagnostics.is_empty());
    let liveness = drop_planner::liveness::Liveness::build(&checker).unwrap();
    let (origins, captures) = drop_planner::origins::analyze(&checker).unwrap();
    let mut planner = drop_planner::DropPlanner::new(&checker, liveness, origins, captures);
    for &root in parsed.roots() {
        planner.item(root).unwrap();
    }
    assert!(planner.recursive_capture_phi.is_some());
    let mut facts = planner.into_candidate_facts();
    let expression = |text: &str| {
        parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok(text))
            .max_by_key(|(_, node)| node.span().start())
            .unwrap()
            .0
    };
    let call = expression(final_call);
    if matches!(fault, Fault::DuplicateAfterCall) {
        let (layout, root) = facts
            .cleanup_steps
            .iter()
            .find_map(|(point, action)| match action {
                Action::ReleaseClosureInstances { layout, root }
                    if *point == DropPoint::CallReturn(call) =>
                {
                    Some((*layout, *root))
                }
                _ => None,
            })
            .unwrap();
        let point = DropPoint::AfterExpression(call);
        let mut duplicate = DropFact::new(point, root.target(), root.value_origin())
            .with_owner(root.owner().unwrap());
        if let Some(condition) = root.condition() {
            duplicate = duplicate.with_condition(condition);
        }
        facts.cleanup_steps.push((
            point,
            Action::ReleaseClosureInstances {
                layout,
                root: duplicate,
            },
        ));
    }
    if matches!(fault, Fault::MissingSharedLayout) {
        let mut removed = 0;
        for node in &mut facts.iterations[0].capture_graph.nodes {
            let before = node.release_captures.len();
            node.release_captures
                .retain(|capture| capture.mode() != ClosureCaptureMode::Shared);
            removed += before - node.release_captures.len();
        }
        assert_eq!(removed, 1);
    }
    if matches!(fault, Fault::EarlySourceDrop) {
        let xs = names
            .symbols()
            .iter()
            .rev()
            .find(|symbol| sources.slice(symbol.span()) == Ok("xs"))
            .unwrap()
            .id();
        let fact = facts
            .cleanup_steps
            .iter()
            .find_map(|(point, action)| match action {
                Action::Drop(fact)
                    if fact.target() == DropTarget::Named(xs)
                        && *point == DropPoint::CallReturn(call) =>
                {
                    Some(*fact)
                }
                _ => None,
            })
            .unwrap();
        let point = DropPoint::CallEntry(call);
        facts.cleanup_steps.push((
            point,
            Action::Drop(
                DropFact::new(point, fact.target(), fact.value_origin())
                    .with_owner(fact.owner().unwrap()),
            ),
        ));
    }
    let [plan] = facts.iterations.as_slice() else {
        panic!("one complete loop plan")
    };
    let statement = plan.descriptor().statement();
    let crate::parser::Statement::For {
        source: loop_source,
        body: loop_body,
        ..
    } = parsed.ast().statements().get(statement).unwrap().payload()
    else {
        unreachable!()
    };
    let enclosing_statement = |id| {
        let span = parsed.ast().expressions().get(id).unwrap().span();
        parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| {
                node.span().start() <= span.start() && node.span().end() >= span.end()
            })
            .min_by_key(|(_, node)| node.span().end() - node.span().start())
            .unwrap()
            .0
    };
    let mut replay = Replay {
        file_captures: checked.captures().to_vec(),
        ..Replay::default()
    };
    for &root in parsed.roots() {
        replay.point(&facts, DropPoint::FunctionEntry(root));
    }
    let symbol = |text: &str| {
        names
            .symbols()
            .iter()
            .rev()
            .find(|symbol| sources.slice(symbol.span()) == Ok(text))
            .unwrap()
            .id()
    };
    if external {
        replay.form_place(crate::ownership_checking::ClosureCaptureSource::Symbol(
            symbol("xs"),
        ));
    } else if shared {
        let resource = expression("listOf(1)");
        let source_phi = plan
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.symbol() == symbol("xs")
                    && phi.boundary() == crate::ownership_checking::IterationPhiBoundary::Header
            })
            .unwrap();
        let initial = plan
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| binding.target() == source_phi.owner())
            .unwrap()
            .values()[0]
            .source();
        let crate::parser::Expression::Call {
            callee, arguments, ..
        } = parsed.ast().expressions().get(resource).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*callee));
        for argument in arguments {
            replay.point(&facts, DropPoint::AfterExpression(argument.value));
        }
        replay.point(&facts, DropPoint::CallEntry(resource));
        replay.form_resource(&facts.cleanup_conditions, initial, resource);
        replay.point(&facts, DropPoint::CallReturn(resource));
        replay.point(&facts, DropPoint::AfterExpression(resource));
        replay.bind(source_phi.symbol(), initial);
        replay.point(
            &facts,
            DropPoint::AfterStatement(enclosing_statement(resource)),
        );
    }
    let control_selector = |control| {
        facts
            .cleanup_conditions
            .nodes()
            .iter()
            .find_map(|node| match node {
                CleanupCondition::Choice { selector, .. }
                    if facts
                        .cleanup_conditions
                        .selector(*selector)
                        .unwrap()
                        .source()
                        == CleanupSelectorSource::Control(control) =>
                {
                    Some(*selector)
                }
                _ => None,
            })
            .unwrap()
    };
    if let Some(branch) = seed_branch {
        let control = expression(seed);
        let crate::parser::Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = parsed.ast().expressions().get(control).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*condition));
        replay.choices.insert(control_selector(control), branch);
        let selected = if branch == 0 {
            *then_branch
        } else {
            else_branch.unwrap()
        };
        let crate::parser::Statement::Expression {
            expression: value, ..
        } = parsed.ast().statements().get(selected).unwrap().payload()
        else {
            unreachable!()
        };
        let selected_span = parsed.ast().expressions().get(*value).unwrap().span();
        let formed = parsed
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| {
                sources.slice(node.span()) == Ok("move {}")
                    && node.span().start() >= selected_span.start()
                    && node.span().end() <= selected_span.end()
            })
            .unwrap()
            .0;
        replay.point(&facts, DropPoint::AfterExpression(formed));
        if *value != formed {
            replay.point(&facts, DropPoint::AfterExpression(*value));
        }
        replay.point(&facts, DropPoint::AfterStatement(selected));
        replay.point(&facts, DropPoint::BranchExit { control, branch });
        replay.point(&facts, DropPoint::AfterExpression(control));
        if !replay.bindings.contains_key(&symbol("f")) {
            replay.bind(symbol("f"), replay.result.unwrap());
        }
        replay.point(
            &facts,
            DropPoint::AfterStatement(enclosing_statement(control)),
        );
    }
    for (index, (seed, _)) in parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("move {}"))
        .enumerate()
    {
        if seed_branch.is_some() && index < 2 {
            continue;
        }
        replay.point(&facts, DropPoint::AfterExpression(seed));
        let name = if index == 0 { "f" } else { "g" };
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()) == Ok(name))
            .unwrap()
            .id();
        replay.bind(symbol, replay.result.unwrap());
        replay.point(&facts, DropPoint::AfterStatement(enclosing_statement(seed)));
    }
    replay.point(&facts, DropPoint::AfterExpression(*loop_source));
    replay.start_loop(statement);
    replay.edge(&facts, plan, IterationPhiIncomingKind::Entry);
    let mut early_releases = if alternating {
        vec![usize::from(shared)]
    } else {
        Vec::new()
    };
    if shared && !external && rounds == 0 {
        early_releases.push(0);
    }
    // flags 的实际元素先 false、最后一轮 true；多轮跳转不改变恒定参数。
    let mut broke = false;
    let mut returned = false;
    for round in 0..rounds {
        replay.start_element(statement);
        for formed in if alternating {
            vec![expression(g_body), expression(f_body)]
        } else {
            vec![expression(f_body)]
        } {
            if shared {
                // 选择此 parent 直接所在作用域内、紧邻它之前声明的借用子环境。
                let start = parsed
                    .ast()
                    .expressions()
                    .get(formed)
                    .unwrap()
                    .span()
                    .start();
                let (borrowed, _) = parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter(|(_, node)| {
                        sources.slice(node.span()) == Ok("{ read(xs) }")
                            && node.span().start() < start
                    })
                    .max_by_key(|(_, node)| node.span().start())
                    .unwrap();
                replay.point(&facts, DropPoint::AfterExpression(borrowed));
                let binding = names
                    .symbols()
                    .iter()
                    .filter(|symbol| {
                        sources.slice(symbol.span()) == Ok("b") && symbol.span().start() < start
                    })
                    .max_by_key(|symbol| symbol.span().start())
                    .unwrap()
                    .id();
                if !replay.bindings.contains_key(&binding) {
                    replay.bind(binding, replay.result.unwrap());
                }
                replay.point(
                    &facts,
                    DropPoint::AfterStatement(enclosing_statement(borrowed)),
                );
            }
            replay.point(&facts, DropPoint::AfterExpression(formed));
            let assignment = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| match node.payload() {
                    crate::parser::Expression::Assignment { value, .. } if *value == formed => {
                        Some(id)
                    }
                    _ => None,
                })
                .unwrap();
            replay.point(&facts, DropPoint::AfterExpression(assignment));
            let span = parsed.ast().expressions().get(formed).unwrap().span();
            let mut completions = parsed
                .ast()
                .statements()
                .iter()
                .filter(|(_, node)| {
                    node.span().start() <= span.start() && node.span().end() >= span.end()
                })
                .collect::<Vec<_>>();
            completions.sort_by_key(|(_, node)| node.span().end() - node.span().start());
            for (statement, _) in completions {
                if statement == *loop_body {
                    break;
                }
                replay.point(&facts, DropPoint::AfterStatement(statement));
            }
        }
        if !tail.is_empty() && tail != "continue" {
            let control_expression = expression(tail);
            let crate::parser::Expression::If { condition, .. } = parsed
                .ast()
                .expressions()
                .get(control_expression)
                .unwrap()
                .payload()
            else {
                unreachable!()
            };
            replay.point(&facts, DropPoint::AfterExpression(*condition));
            replay.choices.insert(
                control_selector(control_expression),
                usize::from(round + 1 != rounds),
            );
            if round + 1 != rounds {
                replay.point(
                    &facts,
                    DropPoint::BranchExit {
                        control: control_expression,
                        branch: 1,
                    },
                );
                replay.point(&facts, DropPoint::AfterExpression(control_expression));
                replay.point(
                    &facts,
                    DropPoint::AfterStatement(enclosing_statement(control_expression)),
                );
            }
        }
        if tail.contains("return") && round + 1 == rounds {
            assert_eq!(
                replay.released, early_releases,
                "the carried chain must survive until return"
            );
            replay.exit(
                &facts,
                plan,
                IterationExitKind::Return(expression("return")),
            );
            returned = true;
            break;
        }
        let kind = if tail == "continue" {
            IterationPhiIncomingKind::Continue(expression("continue"))
        } else if tail.contains("break") && round + 1 == rounds {
            broke = true;
            IterationPhiIncomingKind::Break(expression("break"))
        } else {
            IterationPhiIncomingKind::Fallthrough
        };
        let exit_kind = match kind {
            IterationPhiIncomingKind::Fallthrough => IterationExitKind::Fallthrough,
            IterationPhiIncomingKind::Continue(jump) => IterationExitKind::Continue(jump),
            IterationPhiIncomingKind::Break(jump) => IterationExitKind::Break(jump),
            _ => unreachable!(),
        };
        replay.exit(&facts, plan, exit_kind);
        replay.edge(&facts, plan, kind);
    }
    if !broke && !returned {
        replay.exit(&facts, plan, IterationExitKind::Exhaustion);
        replay.edge(&facts, plan, IterationPhiIncomingKind::Exhaustion);
    }
    let width = if alternating { 2 } else { 1 };
    let formed_count = usize::from(shared) + width * (1 + rounds * if shared { 2 } else { 1 });
    assert_eq!(replay.instances.len(), formed_count);
    if !returned {
        replay.point(&facts, DropPoint::AfterStatement(statement));
        assert_eq!(
            replay.released, early_releases,
            "the carried chain must survive until the final call"
        );
        let crate::parser::Expression::Call { callee, .. } =
            parsed.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*callee));
        replay.point(&facts, DropPoint::CallEntry(call));
        assert_eq!(
            replay.released, early_releases,
            "the carried chain must survive call entry"
        );
        replay.point(&facts, DropPoint::CallReturn(call));
        replay.point(&facts, DropPoint::AfterExpression(call));
        let span = parsed.ast().expressions().get(call).unwrap().span();
        let mut completions = parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| {
                node.span().start() <= span.start() && node.span().end() >= span.end()
            })
            .collect::<Vec<_>>();
        completions.sort_by_key(|(_, node)| node.span().end() - node.span().start());
        for (statement, _) in completions {
            replay.point(&facts, DropPoint::AfterStatement(statement));
        }
    }
    replay.done();
    let expected = if shared {
        let mut chain = vec![if alternating { 2 } else { 1 }];
        for parent in ((1 + width)..formed_count).step_by(2) {
            chain.insert(0, parent);
            chain.push(parent + 1);
        }
        let mut releases = if alternating { vec![1] } else { Vec::new() };
        if rounds == 0 && !external {
            releases.push(0);
        }
        releases.extend(chain);
        if rounds > 0 && !external {
            releases.push(0);
        }
        let expected_loans = ((1 + width)..formed_count)
            .step_by(2)
            .rev()
            .map(|child| (child, 0))
            .collect::<Vec<_>>();
        assert_eq!(replay.loan_ends, expected_loans);
        releases
    } else {
        (0..formed_count).collect::<Vec<_>>()
    };
    assert_eq!(replay.released, expected);
}
