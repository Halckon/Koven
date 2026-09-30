//! 条件 RHS 必须运输实际选中的递归实例，未选中链在分支交付时释放。
use super::*;
use crate::{
    ownership_checking::{CleanupSelectorSource, checker::drop_planner},
    parser::{Expression, Statement},
    source::SourceMap,
};

#[test]
fn conditional_recursive_roots_candidate_matrix() {
    candidate_matrix(Scenario::Recursive);
}

#[test]
fn conditional_shared_sources_candidate_matrix() {
    candidate_matrix(Scenario::SharedDistinct);
}

#[test]
fn conditional_same_shared_source_candidate_matrix() {
    candidate_matrix(Scenario::SharedSame);
}

#[test]
fn same_lambda_ordinary_phi_candidate_matrix() {
    candidate_matrix(Scenario::OrdinaryPhi);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Recursive,
    SharedSame,
    SharedDistinct,
    OrdinaryPhi,
}

fn candidate_matrix(sources: Scenario) {
    for tail in [
        "",
        "continue",
        "if (stop) { break }",
        "if (stop) { return }",
    ] {
        for branch in [0, 1] {
            for first_rounds in [0, 1, 2, 7] {
                for second_rounds in [0, 1, 2] {
                    replay_conditional_roots(
                        branch,
                        first_rounds,
                        second_rounds,
                        tail,
                        Fault::None,
                        sources,
                        1,
                    );
                    if !tail.is_empty() {
                        replay_conditional_roots(
                            branch,
                            first_rounds,
                            second_rounds,
                            tail,
                            Fault::None,
                            sources,
                            0,
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[should_panic(expected = "selected chain must survive call entry")]
fn conditional_replay_rejects_release_before_final_call_returns() {
    replay_conditional_roots(1, 2, 2, "", Fault::EarlyRoot, Scenario::Recursive, 1);
}

#[test]
#[should_panic(expected = "source released with live capture loans")]
fn conditional_replay_rejects_capture_of_the_other_source_instance() {
    replay_conditional_roots(
        0,
        2,
        1,
        "",
        Fault::WrongSharedSource,
        Scenario::SharedDistinct,
        1,
    );
}

#[test]
#[should_panic(expected = "unselected chain releases before snapshot")]
fn conditional_replay_rejects_swapped_same_lambda_instances() {
    replay_conditional_roots(
        0,
        2,
        2,
        "",
        Fault::SwapSameLambdaInstances,
        Scenario::OrdinaryPhi,
        1,
    );
}

#[test]
#[should_panic(expected = "replacement releases at RHS completion")]
fn conditional_replay_rejects_delayed_replacement_cleanup() {
    replay_conditional_roots(
        0,
        2,
        2,
        "",
        Fault::DelayReplacement,
        Scenario::OrdinaryPhi,
        1,
    );
}

enum Fault {
    None,
    EarlyRoot,
    WrongSharedSource,
    SwapSameLambdaInstances,
    DelayReplacement,
}

fn replay_conditional_roots(
    branch: usize,
    first_rounds: usize,
    second_rounds: usize,
    tail: &str,
    fault: Fault,
    scenario: Scenario,
    tail_loop: usize,
) {
    let shared = matches!(scenario, Scenario::SharedSame | Scenario::SharedDistinct);
    let mixed = scenario == Scenario::OrdinaryPhi;
    let source_count = match scenario {
        Scenario::Recursive | Scenario::OrdinaryPhi => 0,
        Scenario::SharedSame => 1,
        Scenario::SharedDistinct => 2,
    };
    let mut sources = SourceMap::new();
    let f_body = if shared {
        "move { val old = f()\nval borrowed = bx() }"
    } else {
        "move { f() }"
    };
    let g_body = if shared {
        "move { val old = g()\nval borrowed = by() }"
    } else {
        "move { g() }"
    };
    // stop 的实际元素先 true、最后 false；末轮 f/g 持有同一 next lambda 的不同实例。
    let jump_tail = if mixed && tail_loop == 0 {
        tail.replace("if (stop)", "if (!stop)")
    } else {
        tail.to_owned()
    };
    let source = sources
        .add_source(
            "conditional-instance-replay.ko",
            "fun read(xs: List<Int>) {}
fun run(first: List<Boolean>, second: List<Boolean>, pick: Boolean) {
SOURCES
var f: move () -> Unit = move {}
var g: move () -> Unit = move {}
for (stop in first) { FIRST_BODY
FIRST_TAIL }
var h: move () -> Unit = if (pick) f else g
for (stop in second) { h = move { h() }
SECOND_TAIL }
val used = h() }"
                .replace("FIRST_BODY", if mixed {
                    "val next: move () -> Unit = move {}\nf = F_BODY\nif (stop) { g = next } else { f = next }"
                } else {
                    "{ BORROW_X\nf = F_BODY }\n{ BORROW_Y\ng = G_BODY }"
                })
                .replace("FIRST_TAIL", if tail_loop == 0 { &jump_tail } else { "" })
                .replace("SECOND_TAIL", if tail_loop == 1 { tail } else { "" })
                .replace(
                    "SOURCES",
                    match scenario {
                        Scenario::Recursive | Scenario::OrdinaryPhi => "",
                        Scenario::SharedSame => "val xs = listOf(1)",
                        Scenario::SharedDistinct => "val xs = listOf(1)\nval ys = listOf(2)",
                    },
                )
                .replace(
                    "BORROW_X",
                    if shared {
                        "val bx: () -> Unit = { read(xs) }"
                    } else {
                        ""
                    },
                )
                .replace(
                    "BORROW_Y",
                    match scenario {
                        Scenario::Recursive | Scenario::OrdinaryPhi => "",
                        Scenario::SharedSame => "val by: () -> Unit = { read(xs) }",
                        Scenario::SharedDistinct => "val by: () -> Unit = { read(ys) }",
                    },
                )
                .replace("F_BODY", f_body)
                .replace("G_BODY", g_body),
        )
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
        crate::ownership_checking::checker::Checker::new(&sources, &parsed, &names, &typed)
            .unwrap();
    let liveness = drop_planner::capture_liveness(&checker).unwrap();
    checker.expression_live_after = liveness.expression_after;
    checker.statement_live_after = liveness.statement_after;
    let mut state = crate::ownership_checking::checker::State::default();
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
    assert_eq!(facts.iterations.len(), 2);
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
    let symbol = |text: &str| {
        names
            .symbols()
            .iter()
            .rev()
            .find(|symbol| sources.slice(symbol.span()) == Ok(text))
            .unwrap()
            .id()
    };
    if matches!(fault, Fault::EarlyRoot) {
        let call = expression("h()");
        let (point, action) = facts
            .cleanup_steps
            .iter_mut()
            .find(|(point, action)| {
                *point == DropPoint::CallReturn(call)
                    && matches!(action, Action::ReleaseClosureInstances { .. })
            })
            .unwrap();
        let Action::ReleaseClosureInstances { layout, root } = *action else {
            unreachable!()
        };
        assert!(root.instance_address().is_none());
        *point = DropPoint::CallEntry(call);
        let mut moved = DropFact::new(*point, root.target(), root.value_origin())
            .with_owner(root.owner().unwrap());
        if let Some(condition) = root.condition() {
            moved = moved.with_condition(condition);
        }
        *action = Action::ReleaseClosureInstances {
            layout,
            root: moved,
        };
        let (point, completion) = facts.cleanup_steps.iter_mut().find(|(_, action)|
            matches!(action, Action::ReleaseRetainedClosureSources { root: paired } if *paired == root)).unwrap();
        *point = DropPoint::CallEntry(call);
        *completion = Action::ReleaseRetainedClosureSources { root: moved };
    }
    if matches!(fault, Fault::WrongSharedSource) {
        let plan = facts
            .iterations
            .iter()
            .min_by_key(|plan| {
                parsed
                    .ast()
                    .statements()
                    .get(plan.descriptor().statement())
                    .unwrap()
                    .span()
                    .start()
            })
            .unwrap();
        let wrong = plan
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.symbol() == symbol("ys")
                    && phi.boundary() == crate::ownership_checking::IterationPhiBoundary::Header
            })
            .unwrap()
            .owner();
        let (_, action) = facts.cleanup_steps.iter_mut().find(|(_, action)| matches!(action,
            Action::SaveClosureCapture { input, .. } if input.mode() == ClosureCaptureMode::Shared
                && input.source() == crate::ownership_checking::ClosureCaptureSource::Symbol(symbol("xs")))).unwrap();
        let Action::SaveClosureCapture { input, .. } = action else {
            unreachable!()
        };
        input.value = CleanupCaptureValue::Owner(wrong);
    }
    if matches!(fault, Fault::DelayReplacement) {
        let assignment = expression("g = next");
        let Expression::Assignment { value, .. } = parsed
            .ast()
            .expressions()
            .get(assignment)
            .unwrap()
            .payload()
        else {
            unreachable!()
        };
        let old_point = DropPoint::AfterExpression(*value);
        let new_point = DropPoint::BranchExit {
            control: expression("if (stop) { g = next } else { f = next }"),
            branch: 0,
        };
        let mut moved = 0;
        for (point, action) in &mut facts.cleanup_steps {
            if *point != old_point {
                continue;
            }
            match action {
                Action::Drop(root) => {
                    assert!(root.capture_slot().is_none());
                    assert!(root.instance_address().is_none());
                    let mut delayed = DropFact::new(new_point, root.target(), root.value_origin())
                        .with_owner(root.owner().unwrap());
                    if let Some(condition) = root.condition() {
                        delayed = delayed.with_condition(condition);
                    }
                    *root = delayed;
                }
                Action::CommitOwnerSnapshot { .. } => {}
                _ => continue,
            }
            *point = new_point;
            moved += 1;
        }
        assert_eq!(moved, 2, "delay cleanup and commit together");
    }
    // 只结束本次表达式所在的语句及其内层 block；循环 body 由退出事实消费。
    let complete_until = |replay: &mut Replay, id, boundary: Option<StatementId>| {
        let span = parsed.ast().expressions().get(id).unwrap().span();
        let mut enclosing = parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| {
                node.span().start() <= span.start() && node.span().end() >= span.end()
            })
            .collect::<Vec<_>>();
        enclosing.sort_by_key(|(_, node)| node.span().end() - node.span().start());
        for (statement, _) in enclosing {
            if Some(statement) == boundary {
                break;
            }
            replay.point(&facts, DropPoint::AfterStatement(statement));
            if boundary.is_none() {
                break;
            }
        }
    };
    let mut replay = Replay {
        file_captures: checked.captures().to_vec(),
        ..Replay::default()
    };
    for &root in parsed.roots() {
        replay.point(&facts, DropPoint::FunctionEntry(root));
    }
    if shared {
        for (name, text) in [("xs", "listOf(1)"), ("ys", "listOf(2)")]
            .into_iter()
            .take(source_count)
        {
            let resource = expression(text);
            let plan = facts
                .iterations
                .iter()
                .min_by_key(|plan| {
                    parsed
                        .ast()
                        .statements()
                        .get(plan.descriptor().statement())
                        .unwrap()
                        .span()
                        .start()
                })
                .unwrap();
            let phi = plan
                .closure_phis()
                .iter()
                .find(|phi| {
                    phi.symbol() == symbol(name)
                        && phi.boundary() == crate::ownership_checking::IterationPhiBoundary::Header
                })
                .unwrap();
            let owner = plan
                .closure_phi_incomings()
                .iter()
                .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
                .unwrap()
                .bindings()
                .iter()
                .find(|binding| binding.target() == phi.owner())
                .unwrap()
                .values()[0]
                .source();
            let Expression::Call {
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
            replay.form_resource(&facts.cleanup_conditions, owner, resource);
            replay.point(&facts, DropPoint::CallReturn(resource));
            replay.point(&facts, DropPoint::AfterExpression(resource));
            replay.bind(symbol(name), owner);
            complete_until(&mut replay, resource, None);
        }
    }
    for (name, (seed, _)) in ["f", "g"].into_iter().zip(
        parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok("move {}")),
    ) {
        replay.point(&facts, DropPoint::AfterExpression(seed));
        replay.bind(symbol(name), replay.result.unwrap());
        complete_until(&mut replay, seed, None);
    }
    let mut plans = facts.iterations.iter().collect::<Vec<_>>();
    plans.sort_by_key(|plan| {
        parsed
            .ast()
            .statements()
            .get(plan.descriptor().statement())
            .unwrap()
            .span()
            .start()
    });
    let control = expression("if (pick) f else g");
    let Expression::If {
        condition,
        then_branch,
        else_branch,
        ..
    } = parsed.ast().expressions().get(control).unwrap().payload()
    else {
        unreachable!()
    };
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
    let selector = control_selector(control);
    let first_count = if shared {
        source_count + 2 + 4 * first_rounds
    } else {
        2 * (first_rounds + 1)
    };
    // 源码级参照模型：next 每轮形成一次，普通赋值替换并释放旧链。
    // 不读取消费者的 owner、slot 或实际 released 来生成期望结果。
    let mut mixed_chains = [vec![0], vec![1]];
    let mut mixed_releases = Vec::new();
    let mut mixed_prefixes = Vec::new();
    if mixed {
        for round in 0..first_rounds {
            let next = 2 + 2 * round;
            mixed_chains[0].push(next + 1);
            let replaced = usize::from(round + 1 != first_rounds);
            mixed_releases.append(&mut mixed_chains[replaced]);
            mixed_chains[replaced] = vec![next];
            mixed_prefixes.push(mixed_releases.clone());
        }
    }
    let expected_chain = |branch: usize| -> Vec<usize> {
        if mixed {
            mixed_chains[branch].clone()
        } else if shared {
            let mut chain = (0..first_rounds)
                .rev()
                .map(|round| source_count + 2 + 4 * round + 2 * branch)
                .collect::<Vec<_>>();
            chain.push(source_count + branch);
            chain.extend((0..first_rounds).map(|round| source_count + 3 + 4 * round + 2 * branch));
            chain
        } else {
            (0..=first_rounds).map(|round| 2 * round + branch).collect()
        }
    };
    let mut expected_unselected = if shared && first_rounds == 0 {
        (0..source_count).rev().collect()
    } else {
        Vec::new()
    };
    let mut returned = false;
    let mut returned_from_first = false;
    let mut replacement_prefixes = mixed_prefixes.iter();
    for (index, plan) in plans.into_iter().enumerate() {
        if index == 1 {
            assert_eq!(
                replay.released, expected_unselected,
                "both roots must survive the first loop"
            );
            if matches!(fault, Fault::SwapSameLambdaInstances) {
                let f = replay.bindings[&symbol("f")];
                let g = replay.bindings[&symbol("g")];
                let (left, right) = (replay.owners[&f], replay.owners[&g]);
                assert_ne!(left, right);
                assert_eq!(
                    replay.instances[left].closure,
                    replay.instances[right].closure
                );
                // 模拟只按 lambda 身份运输而把两个实际句柄调换。
                replay.owners.insert(f, right);
                replay.owners.insert(g, left);
            }
            replay.point(&facts, DropPoint::AfterExpression(*condition));
            replay.choices.insert(selector, branch);
            let selected_branch = if branch == 0 {
                *then_branch
            } else {
                else_branch.unwrap()
            };
            let Statement::Expression {
                expression: value, ..
            } = parsed
                .ast()
                .statements()
                .get(selected_branch)
                .unwrap()
                .payload()
            else {
                unreachable!()
            };
            replay.point(&facts, DropPoint::AfterExpression(*value));
            replay.point(&facts, DropPoint::AfterStatement(selected_branch));
            replay.point(&facts, DropPoint::BranchExit { control, branch });
            expected_unselected.extend(expected_chain(1 - branch));
            if scenario == Scenario::SharedDistinct && first_rounds > 0 {
                expected_unselected.push(1 - branch);
            }
            assert_eq!(
                replay.released, expected_unselected,
                "unselected chain releases before snapshot"
            );
            replay.point(&facts, DropPoint::AfterExpression(control));
            assert_eq!(replay.bindings[&symbol("h")], replay.result.unwrap());
            complete_until(&mut replay, control, None);
        }
        let statement = plan.descriptor().statement();
        let Statement::For { source, body, .. } =
            parsed.ast().statements().get(statement).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*source));
        replay.start_loop(statement);
        replay.edge(&facts, plan, IterationPhiIncomingKind::Entry);
        let rounds = if index == 0 {
            first_rounds
        } else {
            second_rounds
        };
        let mut broke = false;
        for round in 0..rounds {
            replay.start_element(statement);
            if mixed && index == 0 {
                let next = expression("move {}");
                replay.point(&facts, DropPoint::AfterExpression(next));
                if !replay.bindings.contains_key(&symbol("next")) {
                    replay.bind(symbol("next"), replay.result.unwrap());
                }
                complete_until(&mut replay, next, None);
            }
            for text in if index == 0 {
                if mixed {
                    vec![f_body]
                } else {
                    vec![f_body, g_body]
                }
            } else {
                vec!["move { h() }"]
            } {
                let formed = expression(text);
                if index == 0 && shared {
                    let (borrowed_text, name) = if text == f_body {
                        ("{ read(xs) }", "bx")
                    } else if scenario == Scenario::SharedSame {
                        ("{ read(xs) }", "by")
                    } else {
                        ("{ read(ys) }", "by")
                    };
                    let parent_start = parsed
                        .ast()
                        .expressions()
                        .get(formed)
                        .unwrap()
                        .span()
                        .start();
                    let borrowed = parsed
                        .ast()
                        .expressions()
                        .iter()
                        .filter(|(_, node)| {
                            sources.slice(node.span()) == Ok(borrowed_text)
                                && node.span().start() < parent_start
                        })
                        .max_by_key(|(_, node)| node.span().start())
                        .unwrap()
                        .0;
                    replay.point(&facts, DropPoint::AfterExpression(borrowed));
                    if !replay.bindings.contains_key(&symbol(name)) {
                        replay.bind(symbol(name), replay.result.unwrap());
                    }
                    complete_until(&mut replay, borrowed, None);
                }
                replay.point(&facts, DropPoint::AfterExpression(formed));
                let assignment = parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| match node.payload() {
                        Expression::Assignment { value, .. } if *value == formed => Some(id),
                        _ => None,
                    })
                    .unwrap();
                replay.point(&facts, DropPoint::AfterExpression(assignment));
                complete_until(&mut replay, assignment, Some(*body));
            }
            if mixed && index == 0 {
                let control = expression("if (stop) { g = next } else { f = next }");
                let Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } = parsed.ast().expressions().get(control).unwrap().payload()
                else {
                    unreachable!()
                };
                let branch = usize::from(round + 1 == rounds);
                replay.point(&facts, DropPoint::AfterExpression(*condition));
                replay.choices.insert(control_selector(control), branch);
                let assignment = expression(if branch == 0 { "g = next" } else { "f = next" });
                let Expression::Assignment { value, .. } = parsed
                    .ast()
                    .expressions()
                    .get(assignment)
                    .unwrap()
                    .payload()
                else {
                    unreachable!()
                };
                assert_eq!(
                    replay.released, expected_unselected,
                    "old chain survives until replacement RHS completes"
                );
                replay.point(&facts, DropPoint::AfterExpression(*value));
                expected_unselected = replacement_prefixes.next().unwrap().clone();
                assert_eq!(
                    replay.released, expected_unselected,
                    "replacement releases at RHS completion"
                );
                let replaced = symbol(if branch == 0 { "g" } else { "f" });
                assert_eq!(
                    replay.owners[&replay.bindings[&replaced]],
                    2 + 2 * round,
                    "replacement commits this round's next instance"
                );
                replay.point(&facts, DropPoint::AfterExpression(assignment));
                let selected = if branch == 0 {
                    *then_branch
                } else {
                    else_branch.unwrap()
                };
                complete_until(&mut replay, assignment, Some(selected));
                replay.point(&facts, DropPoint::AfterStatement(selected));
                replay.point(&facts, DropPoint::BranchExit { control, branch });
                replay.point(&facts, DropPoint::AfterExpression(control));
                complete_until(&mut replay, control, Some(*body));
                assert_eq!(
                    replay.released, expected_unselected,
                    "replacement releases the old instance chain"
                );
            }
            if index == tail_loop && tail.starts_with("if") {
                let jump_control = expression(&jump_tail);
                let Expression::If {
                    condition,
                    else_branch,
                    ..
                } = parsed
                    .ast()
                    .expressions()
                    .get(jump_control)
                    .unwrap()
                    .payload()
                else {
                    unreachable!()
                };
                assert!(else_branch.is_none());
                replay.point(&facts, DropPoint::AfterExpression(*condition));
                replay.choices.insert(
                    control_selector(jump_control),
                    usize::from(round + 1 != rounds),
                );
                if round + 1 != rounds {
                    replay.point(
                        &facts,
                        DropPoint::BranchExit {
                            control: jump_control,
                            branch: 1,
                        },
                    );
                    replay.point(&facts, DropPoint::AfterExpression(jump_control));
                    complete_until(&mut replay, jump_control, Some(*body));
                }
            }
            if index == tail_loop && tail.contains("return") && round + 1 == rounds {
                assert_eq!(
                    replay.released, expected_unselected,
                    "selected chain must survive until return"
                );
                if index == 1 {
                    let owner = replay.bindings[&symbol("h")];
                    assert_eq!(replay.owners[&owner], first_count + round);
                } else {
                    assert_eq!(
                        replay.owners[&replay.bindings[&symbol("g")]],
                        *expected_chain(1).last().unwrap()
                    );
                    returned_from_first = true;
                }
                replay.exit(
                    &facts,
                    plan,
                    IterationExitKind::Return(expression("return")),
                );
                returned = true;
                break;
            }
            let (exit, edge) = if index == tail_loop && tail == "continue" {
                let jump = expression("continue");
                (
                    IterationExitKind::Continue(jump),
                    IterationPhiIncomingKind::Continue(jump),
                )
            } else if index == tail_loop && tail.contains("break") && round + 1 == rounds {
                broke = true;
                let jump = expression("break");
                (
                    IterationExitKind::Break(jump),
                    IterationPhiIncomingKind::Break(jump),
                )
            } else {
                (
                    IterationExitKind::Fallthrough,
                    IterationPhiIncomingKind::Fallthrough,
                )
            };
            replay.exit(&facts, plan, exit);
            replay.edge(&facts, plan, edge);
        }
        if !broke && !returned {
            replay.exit(&facts, plan, IterationExitKind::Exhaustion);
            replay.edge(&facts, plan, IterationPhiIncomingKind::Exhaustion);
        }
        if returned {
            break;
        }
        replay.point(&facts, DropPoint::AfterStatement(statement));
    }
    if !returned {
        assert_eq!(
            replay.released, expected_unselected,
            "selected chain must survive the second loop"
        );
        let call = expression("h()");
        let Expression::Call { callee, .. } =
            parsed.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*callee));
        replay.point(&facts, DropPoint::CallEntry(call));
        assert_eq!(
            replay.released, expected_unselected,
            "selected chain must survive call entry"
        );
        let owner = replay.bindings[&symbol("h")];
        assert_eq!(
            replay.owners[&owner],
            if second_rounds == 0 {
                *expected_chain(branch).last().unwrap()
            } else {
                first_count + second_rounds - 1
            }
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
    if returned_from_first {
        assert_eq!(replay.instances.len(), first_count);
        let mut expected = mixed_releases;
        expected.extend(expected_chain(1));
        expected.extend(expected_chain(0));
        if shared {
            // Return 按声明逆序收尾：先 g/f，再 ys/xs；源一直保留到各自借用结束。
            expected.extend((0..source_count).rev());
        }
        assert_eq!(replay.released, expected);
        let loans = if shared {
            [1, 0]
                .into_iter()
                .flat_map(|branch| {
                    (0..first_rounds)
                        .rev()
                        .map(move |round| (source_count + 2 + 4 * round + 2 * branch, 0))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        assert_eq!(replay.loan_ends, loans);
        return;
    }
    let total = first_count + second_rounds;
    assert_eq!(replay.instances.len(), total);
    expected_unselected.extend(expected_chain(branch));
    expected_unselected.extend(first_count..total);
    if shared && first_rounds > 0 {
        expected_unselected.push(if scenario == Scenario::SharedSame {
            0
        } else {
            branch
        });
    }
    if shared {
        let borrowed = |branch| {
            (0..first_rounds)
                .rev()
                .map(move |round| (source_count + 2 + 4 * round + 2 * branch, 0))
        };
        assert_eq!(
            replay.loan_ends,
            borrowed(1 - branch)
                .chain(borrowed(branch))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(replay.released, expected_unselected);
}
