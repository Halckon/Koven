//! 同一 lambda 的环境在不同父槽并存；核对独占资源及同一源的多个 shared loan。
use super::*;
use crate::{
    ownership_checking::{CleanupSelectorSource, checker::drop_planner},
    parser::{Expression, Statement},
    source::SourceMap,
};

#[test]
fn coexisting_owned_children_candidate_matrix() {
    candidate_matrix(false);
}

#[test]
fn coexisting_shared_children_candidate_matrix() {
    candidate_matrix(true);
}

#[test]
fn shared_source_survives_second_loop_return() {
    replay_children(1, 1, "if (stop) { return }", 1, Fault::None, true);
}

fn candidate_matrix(shared: bool) {
    for tail in [
        "",
        "continue",
        "if (stop) { break }",
        "if (stop) { return }",
    ] {
        for first_rounds in [0, 1, 2, 3, 7] {
            for second_rounds in [0, 1, 2] {
                replay_children(first_rounds, second_rounds, tail, 1, Fault::None, shared);
                if !tail.is_empty() {
                    replay_children(first_rounds, second_rounds, tail, 0, Fault::None, shared);
                }
            }
        }
    }
}

#[test]
#[should_panic(expected = "ordinary drop cannot hide owned captures")]
fn coexisting_replay_rejects_missing_replaced_resource_drop() {
    replay_children(3, 0, "", 1, Fault::MissingResourceDrop, false);
}

#[test]
#[should_panic(expected = "every live flattened capture needs its phi write")]
fn coexisting_replay_rejects_missing_phi_capture_transport() {
    replay_children(3, 0, "", 1, Fault::MissingPhiSource, false);
}

#[test]
#[should_panic(expected = "last borrower test must follow its loan end")]
fn shared_replay_rejects_missing_loan_end() {
    replay_children(2, 0, "", 1, Fault::MissingLoanEnd, true);
}

#[test]
#[should_panic(expected = "capture loan must end once")]
fn shared_replay_rejects_duplicate_loan_end() {
    replay_children(2, 0, "", 1, Fault::DuplicateLoanEnd, true);
}

#[test]
#[should_panic(expected = "cleanup path must retain the released child")]
fn shared_replay_rejects_invalid_loan_address() {
    replay_children(2, 0, "", 1, Fault::InvalidLoanAddress, true);
}

#[test]
#[should_panic(expected = "loan end must read its saved source")]
fn shared_replay_rejects_missing_matching_capture_slot() {
    replay_children(2, 0, "", 1, Fault::MissingSharedSlot, true);
}

#[test]
#[should_panic(expected = "source released with live capture loans")]
fn shared_replay_rejects_premature_source_drop() {
    replay_children(2, 0, "", 1, Fault::EarlySourceDrop, true);
}

#[test]
fn shared_replay_clears_inactive_last_loan_choice() {
    replay_children(1, 0, "", 1, Fault::StaleLastChoice, true);
}

enum Fault {
    None,
    MissingResourceDrop,
    MissingPhiSource,
    MissingLoanEnd,
    DuplicateLoanEnd,
    InvalidLoanAddress,
    MissingSharedSlot,
    EarlySourceDrop,
    StaleLastChoice,
}

fn replay_children(
    first_rounds: usize,
    second_rounds: usize,
    tail: &str,
    tail_loop: usize,
    fault: Fault,
    shared: bool,
) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "coexisting-instance-replay.ko",
            "fun read(xs: List<Int>) {}
fun run(flags: List<Boolean>, next: List<Boolean>) {
SHARED_SOURCE
var first: CAPTURE() -> Unit = CAPTURE{}
var second: CAPTURE() -> Unit = CAPTURE{}
for (stop in flags) {
    second = first
    OWNED_SOURCE
    { first = CAPTURE{ read(xs) } }
    FIRST_TAIL
}
var outer: move () -> Unit = move { val a = first()\nval b = second() }
for (stop in next) { SECOND_TAIL }
val used = outer()
}"
            .replace("CAPTURE", if shared { "" } else { "move " })
            .replace(
                "SHARED_SOURCE",
                if shared { "val xs = listOf(1)" } else { "" },
            )
            .replace(
                "OWNED_SOURCE",
                if shared { "" } else { "val xs = listOf(1)" },
            )
            .replace("FIRST_TAIL", if tail_loop == 0 { tail } else { "" })
            .replace("SECOND_TAIL", if tail_loop == 1 { tail } else { "" }),
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
    assert!(planner.coexisting_capture_phi.is_some());
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
    let symbol = |text: &str| {
        names
            .symbols()
            .iter()
            .rev()
            .find(|symbol| sources.slice(symbol.span()) == Ok(text))
            .unwrap()
            .id()
    };
    if matches!(fault, Fault::MissingResourceDrop) {
        let assignment = expression("second = first");
        let Expression::Assignment { value, .. } = parsed
            .ast()
            .expressions()
            .get(assignment)
            .unwrap()
            .payload()
        else {
            unreachable!()
        };
        let count = facts.cleanup_steps.len();
        facts.cleanup_steps.retain(|(point, action)| !(*point == DropPoint::AfterExpression(*value)
            && matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::Captured { .. }))));
        assert!(facts.cleanup_steps.len() < count);
    }
    if matches!(fault, Fault::MissingPhiSource) {
        let plan = &mut facts.iterations[0];
        let target = plan
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.symbol() == symbol("first")
                    && phi.boundary() == crate::ownership_checking::IterationPhiBoundary::Header
            })
            .unwrap()
            .owner();
        let binding = plan
            .closure_phi_incomings
            .iter_mut()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough)
            .unwrap()
            .bindings
            .iter_mut()
            .find(|binding| binding.target() == target)
            .unwrap();
        let mut removed = 0;
        for environment in binding
            .origins
            .iter_mut()
            .flat_map(|origin| &mut origin.environments)
        {
            removed += environment.sources.len();
            environment.sources.clear();
        }
        assert_eq!(removed, 1);
    }
    let final_call = expression("outer()");
    if matches!(fault, Fault::MissingLoanEnd) {
        let count = facts.cleanup_steps.len();
        facts.cleanup_steps.retain(|(point, action)| {
            !(*point == DropPoint::CallReturn(final_call)
                && matches!(action, Action::EndCaptureLoan { .. }))
        });
        assert!(facts.cleanup_steps.len() < count);
    }
    if matches!(fault, Fault::DuplicateLoanEnd) {
        facts.cleanup_steps = facts
            .cleanup_steps
            .iter()
            .flat_map(|&(point, action)| {
                let duplicate = (point == DropPoint::CallReturn(final_call)
                    && matches!(action, Action::EndCaptureLoan { .. }))
                .then_some((point, action));
                std::iter::once((point, action)).chain(duplicate)
            })
            .collect();
    }
    if matches!(fault, Fault::InvalidLoanAddress) {
        for (point, action) in &mut facts.cleanup_steps {
            if *point == DropPoint::CallReturn(final_call)
                && let Action::EndCaptureLoan {
                    instance_address, ..
                } = action
            {
                let root = facts
                    .cleanup_conditions
                    .instance_address(*instance_address)
                    .unwrap()
                    .root();
                *instance_address = facts
                    .cleanup_conditions
                    .register_instance_address(root, &[usize::MAX]);
            }
        }
    }
    if matches!(fault, Fault::EarlySourceDrop) {
        let drop = facts
            .cleanup_steps
            .iter()
            .find_map(|(point, action)| match action {
                Action::Drop(fact)
                    if *point == DropPoint::CallReturn(final_call)
                        && matches!(fact.target(), DropTarget::RetainedSource(_)) =>
                {
                    Some(*fact)
                }
                _ => None,
            })
            .unwrap();
        facts.cleanup_steps.push((
            DropPoint::CallEntry(final_call),
            Action::Drop(drop.with_condition(CleanupConditionId::ALWAYS)),
        ));
    }
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
    let resource = expression("listOf(1)");
    let capture_owner = facts
        .cleanup_steps
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture { input, .. }
                if input.source()
                    == crate::ownership_checking::ClosureCaptureSource::Symbol(symbol("xs")) =>
            {
                match input.value() {
                    CleanupCaptureValue::Owner(owner) => Some(owner),
                    _ => None,
                }
            }
            _ => None,
        })
        .unwrap();
    let resource_owner = if shared {
        facts.iterations[0]
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| binding.target() == capture_owner)
            .unwrap()
            .values()[0]
            .source()
    } else {
        capture_owner
    };
    let evaluate_resource = |replay: &mut Replay| {
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
        replay.form_resource(&facts.cleanup_conditions, resource_owner, resource);
        replay.point(&facts, DropPoint::CallReturn(resource));
        replay.point(&facts, DropPoint::AfterExpression(resource));
        replay.bind(symbol("xs"), resource_owner);
        complete_until(replay, resource, None);
    };
    let mut replay = Replay {
        file_captures: checked.captures().to_vec(),
        ..Replay::default()
    };
    let run = *parsed.roots().last().unwrap();
    replay.point(&facts, DropPoint::FunctionEntry(run));
    if shared {
        evaluate_resource(&mut replay);
    }
    for (name, (seed, _)) in
        ["first", "second"]
            .into_iter()
            .zip(parsed.ast().expressions().iter().filter(|(_, node)| {
                sources.slice(node.span()) == Ok(if shared { "{}" } else { "move {}" })
            }))
    {
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
    assert_eq!(plans.len(), 2);
    let outer = expression("move { val a = first()\nval b = second() }");
    let mut returned = false;
    let mut returned_from_first = false;
    let mut expected_replaced = Vec::new();
    let parent = if shared {
        3 + first_rounds
    } else {
        2 + 2 * first_rounds
    };
    if shared && first_rounds == 0 {
        expected_replaced.push(0);
    }
    // 独立于事实的源码顺序：先释放旧 second；最终父环境逆序释放 second、first。
    for round in 0..first_rounds {
        if shared {
            expected_replaced.push(match round {
                0 => 2,
                1 => 1,
                _ => round + 1,
            });
            continue;
        }
        match round {
            0 => expected_replaced.push(1),
            1 => expected_replaced.push(0),
            _ => expected_replaced.extend([2 * round - 2, 2 * round - 1]),
        }
    }
    for (index, plan) in plans.into_iter().enumerate() {
        let statement = plan.descriptor().statement();
        let Statement::For { source, body, .. } =
            parsed.ast().statements().get(statement).unwrap().payload()
        else {
            unreachable!()
        };
        if index == 1 {
            assert_eq!(replay.released, expected_replaced);
            replay.point(&facts, DropPoint::AfterExpression(outer));
            assert_eq!(replay.bindings[&symbol("outer")], replay.result.unwrap());
            complete_until(&mut replay, outer, None);
        }
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
            if index == 0 {
                let assignment = expression("second = first");
                let Expression::Assignment { value, .. } = parsed
                    .ast()
                    .expressions()
                    .get(assignment)
                    .unwrap()
                    .payload()
                else {
                    unreachable!()
                };
                replay.point(&facts, DropPoint::AfterExpression(*value));
                replay.point(&facts, DropPoint::AfterExpression(assignment));
                complete_until(&mut replay, assignment, Some(*body));
                if !shared {
                    evaluate_resource(&mut replay);
                }
                let formed = expression(if shared {
                    "{ read(xs) }"
                } else {
                    "move { read(xs) }"
                });
                replay.point(&facts, DropPoint::AfterExpression(formed));
                let assignment = expression(if shared {
                    "first = { read(xs) }"
                } else {
                    "first = move { read(xs) }"
                });
                replay.point(&facts, DropPoint::AfterExpression(assignment));
                complete_until(&mut replay, assignment, Some(*body));
            }
            if index == tail_loop && tail.starts_with("if") {
                let control = expression(tail);
                let Expression::If { condition, .. } =
                    parsed.ast().expressions().get(control).unwrap().payload()
                else {
                    unreachable!()
                };
                replay.point(&facts, DropPoint::AfterExpression(*condition));
                let selector = facts
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
                    .unwrap();
                replay
                    .choices
                    .insert(selector, usize::from(round + 1 != rounds));
                if round + 1 != rounds {
                    replay.point(&facts, DropPoint::BranchExit { control, branch: 1 });
                    replay.point(&facts, DropPoint::AfterExpression(control));
                    complete_until(&mut replay, control, Some(*body));
                }
            }
            if index == tail_loop && tail.contains("return") && round + 1 == rounds {
                assert_eq!(
                    replay.released, expected_replaced,
                    "children must survive until return"
                );
                if index == 1 {
                    assert_eq!(replay.owners[&replay.bindings[&symbol("outer")]], parent);
                } else {
                    assert_eq!(
                        replay.owners[&replay.bindings[&symbol("first")]],
                        parent - 1
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
        let call = expression("outer()");
        let Expression::Call { callee, .. } =
            parsed.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*callee));
        replay.point(&facts, DropPoint::CallEntry(call));
        assert_eq!(
            replay.released, expected_replaced,
            "children must survive call entry"
        );
        assert_eq!(replay.owners[&replay.bindings[&symbol("outer")]], parent);
        if matches!(fault, Fault::MissingSharedSlot) {
            for instance in &mut replay.instances {
                instance.shared.clear();
            }
        }
        if matches!(fault, Fault::StaleLastChoice) {
            // 一轮时 second 是无捕获 seed；图级 presence 仍包含另一子实例的 shared lambda。
            let inactive = facts
                .cleanup_steps
                .iter()
                .find_map(|(point, action)| {
                    if *point == DropPoint::CallReturn(call)
                        && let Action::TestLastCaptureLoan {
                            instance_address,
                            capture_slot,
                            ..
                        } = action
                        && replay
                            .cleanup_source(
                                &facts.cleanup_conditions,
                                *instance_address,
                                *capture_slot,
                            )
                            .is_none()
                    {
                        Some(*action)
                    } else {
                        None
                    }
                })
                .unwrap();
            let Action::TestLastCaptureLoan { selector, .. } = inactive else {
                unreachable!()
            };
            for condition in [None, Some(CleanupConditionId::NEVER)] {
                let mut action = inactive;
                let Action::TestLastCaptureLoan {
                    condition: guard, ..
                } = &mut action
                else {
                    unreachable!()
                };
                *guard = condition;
                replay.choices.insert(selector, 1);
                replay.action(&facts, action);
                assert_eq!(
                    replay.choices[&selector], 0,
                    "inactive last-loan query must overwrite stale true"
                );
            }
        }
        replay.point(&facts, DropPoint::CallReturn(call));
        replay.point(&facts, DropPoint::AfterExpression(call));
        let span = parsed.ast().expressions().get(call).unwrap().span();
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
            replay.point(&facts, DropPoint::AfterStatement(statement));
        }
    }
    replay.done();
    if shared {
        match first_rounds {
            0 => expected_replaced.extend([2, 1]),
            1 => expected_replaced.extend([1, 3]),
            _ => expected_replaced.extend([parent - 2, parent - 1]),
        }
    } else {
        match first_rounds {
            0 => expected_replaced.extend([1, 0]),
            1 => expected_replaced.extend([0, 2, 3]),
            _ => expected_replaced.extend([parent - 4, parent - 3, parent - 2, parent - 1]),
        }
    }
    if !returned_from_first {
        expected_replaced.push(parent);
    }
    if shared && first_rounds > 0 {
        expected_replaced.push(0);
    }
    if shared {
        let expected_ends = expected_replaced
            .iter()
            .copied()
            .filter(|instance| (3..parent).contains(instance))
            .map(|instance| (instance, 0))
            .collect::<Vec<_>>();
        assert_eq!(replay.loan_ends, expected_ends);
        assert_eq!(replay.loan_ends.len(), first_rounds);
    }
    assert_eq!(
        replay.instances.len(),
        parent + usize::from(!returned_from_first)
    );
    assert_eq!(replay.released, expected_replaced);
}
