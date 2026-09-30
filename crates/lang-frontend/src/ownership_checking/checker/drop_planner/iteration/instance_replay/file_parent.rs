//! 两个不同循环的递归根由同一个 File 布局父环境持有。
use super::*;
use crate::{
    ownership_checking::{CleanupSelectorSource, checker::drop_planner},
    parser::{Expression, Statement},
    source::SourceMap,
};

#[test]
fn file_parent_candidate_exit_matrix() {
    candidate_matrix(false);
}

#[test]
fn nested_file_parent_candidate_exit_matrix() {
    candidate_matrix(true);
}

#[test]
fn conditional_file_parent_candidate_exit_matrix() {
    for nested in [false, true] {
        for branch in [0, 1] {
            candidate_child_matrix(nested, SharedChild::ConditionalBeforeLoops(branch));
        }
    }
}

fn candidate_matrix(nested: bool) {
    for shared in [
        SharedChild::None,
        SharedChild::BeforeLoops,
        SharedChild::TwoBeforeLoops,
        SharedChild::AfterLoops,
    ] {
        candidate_child_matrix(nested, shared);
    }
}

fn candidate_child_matrix(nested: bool, shared: SharedChild) {
    for tail in [
        "",
        "continue",
        "if (stop) { break }",
        "if (stop) { return }",
    ] {
        for first in [0, 1, 2, 7] {
            for second in [0, 1, 2, 7] {
                for return_parent in [false, true] {
                    replay_parent(
                        [first, second],
                        tail,
                        0,
                        shared,
                        return_parent,
                        Fault::None,
                        nested,
                    );
                    if !tail.is_empty() {
                        replay_parent(
                            [first, second],
                            tail,
                            1,
                            shared,
                            return_parent,
                            Fault::None,
                            nested,
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[should_panic(expected = "passed environment must match its checked lambda")]
fn file_parent_rejects_wrong_passed_environment() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::AfterLoops,
        false,
        Fault::WrongPassedLambda,
        false,
    );
}

#[test]
#[should_panic(expected = "lambda entry needs a passed environment")]
fn file_parent_rejects_missing_environment_pass() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::None,
        false,
        Fault::MissingPass,
        false,
    );
}

#[test]
#[should_panic(expected = "active call environment must not be released")]
fn file_parent_rejects_release_during_call_entry() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::AfterLoops,
        false,
        Fault::EarlyRelease,
        false,
    );
}

#[test]
#[should_panic(expected = "selected named root has no live binding")]
fn file_parent_rejects_duplicate_release_at_function_end() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::BeforeLoops,
        false,
        Fault::DuplicateScopeEnd,
        false,
    );
}

#[test]
#[should_panic(expected = "lambda entry needs a passed environment")]
fn nested_parent_rejects_missing_inner_environment_pass() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::None,
        false,
        Fault::MissingInnerPass,
        true,
    );
}

#[test]
#[should_panic(expected = "environment capture must read an initialized owned slot")]
fn nested_parent_rejects_capture_slot_without_formation() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::None,
        false,
        Fault::MissingOuterCapture,
        true,
    );
}

#[test]
#[should_panic(expected = "root release needs retained-source completion")]
fn nested_parent_rejects_missing_retained_source_cleanup() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::BeforeLoops,
        false,
        Fault::MissingNestedRetainedDrop,
        true,
    );
}

#[test]
#[should_panic(expected = "root release needs retained-source completion")]
fn conditional_parent_rejects_missing_selected_source_cleanup() {
    replay_parent(
        [2, 1],
        "",
        0,
        SharedChild::ConditionalBeforeLoops(1),
        false,
        Fault::MissingNestedRetainedDrop,
        true,
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SharedChild {
    None,
    BeforeLoops,
    TwoBeforeLoops,
    AfterLoops,
    ConditionalBeforeLoops(usize),
}

enum Fault {
    None,
    WrongPassedLambda,
    MissingPass,
    EarlyRelease,
    DuplicateScopeEnd,
    MissingInnerPass,
    MissingOuterCapture,
    MissingNestedRetainedDrop,
}

fn replay_parent(
    rounds: [usize; 2],
    tail: &str,
    tail_loop: usize,
    child: SharedChild,
    return_parent: bool,
    fault: Fault,
    nested: bool,
) {
    let shared = child != SharedChild::None;
    let branch = match child {
        SharedChild::ConditionalBeforeLoops(branch) => Some(branch),
        _ => None,
    };
    let source_count = if branch.is_some() {
        2
    } else {
        usize::from(shared)
    };
    let selected_source = branch.unwrap_or(0);
    let early_releases = branch.map_or_else(Vec::new, |branch| vec![1 - branch]);
    let before = matches!(
        child,
        SharedChild::BeforeLoops
            | SharedChild::TwoBeforeLoops
            | SharedChild::ConditionalBeforeLoops(_)
    );
    let before_count = if child == SharedChild::TwoBeforeLoops {
        2
    } else {
        usize::from(before)
    };
    let mut sources = SourceMap::new();
    let leaf_text = if child == SharedChild::TwoBeforeLoops {
        "move { val a = f()\nval b = g()\nval c = borrowed()\nval d = other() }"
    } else if shared {
        "move { val a = f()\nval b = g()\nval c = borrowed() }"
    } else {
        "move { val a = f()\nval b = g() }"
    };
    let parent_text = if nested {
        format!("move {{ val inner: move () -> Unit = {leaf_text}\nval used = inner() }}")
    } else {
        leaf_text.to_owned()
    };
    let source = sources.add_source("file-parent-replay.ko", format!(
        "fun read(xs: List<Int>) {{}}\nfun run(first: List<Boolean>, second: List<Boolean>, exit: Boolean, pick: Boolean) {{
{}
var f: move () -> Unit = move {{}}
for (stop in first) {{ f = move {{ f() }}\n{} }}
var g: move () -> Unit = move {{}}
for (stop in second) {{ g = move {{ g() }}\n{} }}
{}
val outer: move () -> Unit = {parent_text}
if (exit) {{ return }}
val used = outer() }}",
        match child {
            SharedChild::None => "",
            SharedChild::BeforeLoops => "val xs = listOf(1)\nval borrowed: () -> Unit = { read(xs) }",
            SharedChild::TwoBeforeLoops => "val xs = listOf(1)\nval borrowed: () -> Unit = { read(xs) }\nval other: () -> Unit = { val used = read(xs) }",
            SharedChild::AfterLoops => "val xs = listOf(1)",
            SharedChild::ConditionalBeforeLoops(_) => "val xs = listOf(1)\nval ys = listOf(2)\nval borrowed: () -> Unit = if (pick) ({ read(xs) }) else ({ read(ys) })",
        },
        if tail_loop == 0 { tail } else { "" },
        if tail_loop == 1 { tail } else { "" },
        if child == SharedChild::AfterLoops { "val borrowed: () -> Unit = { read(xs) }" } else { "" },
    )).unwrap();
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
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let checked =
        crate::ownership_checking::check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(
        checked.diagnostics().is_empty(),
        "{:?}",
        checked.diagnostics()
    );
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
    let call = expression("outer()");
    match fault {
        Fault::None => {}
        Fault::MissingInnerPass => {
            let before = facts.cleanup_steps.len();
            facts.cleanup_steps.retain(|(point, action)| {
                !(*point == DropPoint::CallEntry(expression("inner()"))
                    && matches!(action, Action::PassClosureEnvironment { .. }))
            });
            assert_eq!(before, facts.cleanup_steps.len() + 1);
        }
        Fault::MissingOuterCapture => {
            let point = DropPoint::AfterExpression(expression(&parent_text));
            let index = facts
                .cleanup_steps
                .iter()
                .position(|(at, action)| {
                    *at == point && matches!(action, Action::SaveClosureCapture { .. })
                })
                .unwrap();
            facts.cleanup_steps.remove(index);
        }
        Fault::MissingNestedRetainedDrop => {
            let before = facts.cleanup_steps.len();
            facts.cleanup_steps.retain(|(point, action)| {
                !(*point == DropPoint::CallReturn(expression("inner()"))
                    && matches!(action, Action::ReleaseRetainedClosureSources { .. }))
            });
            assert!(before > facts.cleanup_steps.len());
        }
        Fault::MissingPass => facts.cleanup_steps.retain(|(point, action)| {
            !(*point == DropPoint::CallEntry(call)
                && matches!(action, Action::PassClosureEnvironment { .. }))
        }),
        Fault::WrongPassedLambda => {
            let action = facts
                .cleanup_steps
                .iter_mut()
                .find_map(|(point, action)| {
                    (*point == DropPoint::CallEntry(call)
                        && matches!(action, Action::PassClosureEnvironment { .. }))
                    .then_some(action)
                })
                .unwrap();
            let Action::PassClosureEnvironment { closure, .. } = action else {
                unreachable!()
            };
            *closure = Some(expression("move { f() }"));
        }
        Fault::DuplicateScopeEnd => {
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
            let span = parsed.ast().expressions().get(call).unwrap().span();
            let body = parsed
                .ast()
                .statements()
                .iter()
                .filter(|(_, node)| {
                    node.span().start() <= span.start() && node.span().end() >= span.end()
                })
                .max_by_key(|(_, node)| node.span().end() - node.span().start())
                .unwrap()
                .0;
            let point = DropPoint::AfterStatement(body);
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
        Fault::EarlyRelease => {
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
        }
    }
    let complete = |replay: &mut Replay, value, boundary: Option<StatementId>| {
        let span = parsed.ast().expressions().get(value).unwrap().span();
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
    let form_borrowed = |replay: &mut Replay| {
        if let Some(branch) = branch {
            let control = expression("if (pick) ({ read(xs) }) else ({ read(ys) })");
            let Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } = parsed.ast().expressions().get(control).unwrap().payload()
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
            replay.choices.insert(selector, branch);
            let selected = if branch == 0 {
                *then_branch
            } else {
                else_branch.unwrap()
            };
            let formed = expression(if branch == 0 {
                "{ read(xs) }"
            } else {
                "{ read(ys) }"
            });
            replay.point(&facts, DropPoint::AfterExpression(formed));
            let Statement::Expression {
                expression: value, ..
            } = parsed.ast().statements().get(selected).unwrap().payload()
            else {
                unreachable!()
            };
            replay.point(&facts, DropPoint::AfterExpression(*value));
            replay.point(&facts, DropPoint::AfterStatement(selected));
            replay.point(&facts, DropPoint::BranchExit { control, branch });
            assert_eq!(
                replay.released, early_releases,
                "unselected source releases before the conditional snapshot"
            );
            assert!(replay.instances[selected_source].live);
            replay.point(&facts, DropPoint::AfterExpression(control));
            if !replay.bindings.contains_key(&symbol("borrowed")) {
                replay.bind(symbol("borrowed"), replay.result.unwrap());
            }
            complete(replay, control, None);
            return;
        }
        for (name, text) in [
            ("borrowed", "{ read(xs) }"),
            ("other", "{ val used = read(xs) }"),
        ]
        .into_iter()
        .take(if before_count == 2 { 2 } else { 1 })
        {
            let borrowed = expression(text);
            replay.point(&facts, DropPoint::AfterExpression(borrowed));
            if let Some(owner) = replay.bindings.get(&symbol(name)) {
                assert_eq!(Some(*owner), replay.result);
            } else {
                replay.bind(symbol(name), replay.result.unwrap());
            }
            complete(replay, borrowed, None);
        }
    };
    let mut replay = Replay {
        file_captures: checked.captures().to_vec(),
        ..Replay::default()
    };
    for &root in parsed.roots() {
        replay.point(&facts, DropPoint::FunctionEntry(root));
    }
    for (name, text) in [("xs", "listOf(1)"), ("ys", "listOf(2)")]
        .into_iter()
        .take(source_count)
    {
        let resource = expression(text);
        let phi = plans[0]
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.symbol() == symbol(name)
                    && phi.boundary() == crate::ownership_checking::IterationPhiBoundary::Header
            })
            .unwrap();
        let owner = plans[0]
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
        complete(&mut replay, resource, None);
    }
    if before {
        form_borrowed(&mut replay);
    }
    let seeds = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("move {}"))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let mut returned = None;
    for (index, plan) in plans.iter().enumerate() {
        replay.point(&facts, DropPoint::AfterExpression(seeds[index]));
        let name = symbol(if index == 0 { "f" } else { "g" });
        if let Some(owner) = replay.bindings.get(&name) {
            assert_eq!(Some(*owner), replay.result);
        } else {
            replay.bind(name, replay.result.unwrap());
        }
        complete(&mut replay, seeds[index], None);
        let statement = plan.descriptor().statement();
        let Statement::For { source, body, .. } =
            parsed.ast().statements().get(statement).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*source));
        replay.start_loop(statement);
        replay.edge(&facts, plan, IterationPhiIncomingKind::Entry);
        let formed = expression(if index == 0 {
            "move { f() }"
        } else {
            "move { g() }"
        });
        let assignment = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Expression::Assignment { value, .. } if *value == formed => Some(id),
                _ => None,
            })
            .unwrap();
        let mut broke = false;
        for round in 0..rounds[index] {
            replay.start_element(statement);
            replay.point(&facts, DropPoint::AfterExpression(formed));
            replay.point(&facts, DropPoint::AfterExpression(assignment));
            complete(&mut replay, assignment, Some(*body));
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
                    .insert(selector, usize::from(round + 1 != rounds[index]));
                if round + 1 != rounds[index] {
                    replay.point(&facts, DropPoint::BranchExit { control, branch: 1 });
                    replay.point(&facts, DropPoint::AfterExpression(control));
                    complete(&mut replay, control, Some(*body));
                }
            }
            assert_eq!(
                replay.released, early_releases,
                "both recursive roots must survive until delivery or return"
            );
            if index == tail_loop && tail.contains("return") && round + 1 == rounds[index] {
                let span = parsed.ast().statements().get(*body).unwrap().span();
                let jump = parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find(|(_, node)| {
                        sources.slice(node.span()) == Ok("return")
                            && node.span().start() >= span.start()
                            && node.span().end() <= span.end()
                    })
                    .unwrap()
                    .0;
                replay.exit(&facts, plan, IterationExitKind::Return(jump));
                returned = Some(index);
                break;
            }
            let (exit, edge) = if index == tail_loop && tail == "continue" {
                let jump = expression("continue");
                (
                    IterationExitKind::Continue(jump),
                    IterationPhiIncomingKind::Continue(jump),
                )
            } else if index == tail_loop && tail.contains("break") && round + 1 == rounds[index] {
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
        if returned.is_some() {
            break;
        }
        if !broke {
            replay.exit(&facts, plan, IterationExitKind::Exhaustion);
            replay.edge(&facts, plan, IterationPhiIncomingKind::Exhaustion);
        }
        replay.point(&facts, DropPoint::AfterStatement(statement));
    }
    let offset = source_count + before_count;
    let first_end = offset + 1 + rounds[0];
    let second_end = first_end + 1 + rounds[1];
    let mut expected = Vec::new();
    if returned != Some(0) {
        expected.extend(first_end..second_end);
    }
    expected.extend(offset..first_end);
    if returned.is_none() {
        assert_eq!(replay.released, early_releases);
        if shared {
            if !before {
                form_borrowed(&mut replay);
            }
            if before {
                expected.splice(0..0, (source_count..offset).rev());
            } else {
                expected.insert(0, second_end);
            }
        }
        let parent = expression(&parent_text);
        replay.point(&facts, DropPoint::AfterExpression(parent));
        if !replay.bindings.contains_key(&symbol("outer")) {
            replay.bind(symbol("outer"), replay.result.unwrap());
        }
        complete(&mut replay, parent, None);
        let control = expression("if (exit) { return }");
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
        replay.choices.insert(selector, usize::from(!return_parent));
        if return_parent {
            let point = DropPoint::ControlTransfer(expression("return"));
            assert!(facts.cleanup_steps.iter().any(|(at, action)| *at == point
                && matches!(
                    action,
                    Action::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::File,
                        ..
                    }
                )));
            assert_eq!(
                replay.released, early_releases,
                "File parent must survive until return"
            );
            replay.point(&facts, point);
        } else {
            replay.point(&facts, DropPoint::BranchExit { control, branch: 1 });
            replay.point(&facts, DropPoint::AfterExpression(control));
            complete(&mut replay, control, None);
            assert!(
                matches!(fault, Fault::EarlyRelease)
                    || facts.cleanup_steps.iter().any(|(point, action)| *point
                        == DropPoint::CallReturn(call)
                        && matches!(
                            action,
                            Action::ReleaseClosureInstances {
                                layout: ClosureReleaseLayout::File,
                                ..
                            }
                        ))
            );
            let mut expected_bodies = vec![(call, parent)];
            if nested {
                expected_bodies.push((expression("inner()"), expression(leaf_text)));
            }
            replay.call(&facts, &parsed, &names, &expected_bodies, call);
            if nested && before {
                assert!(
                    !replay.instances[selected_source].live,
                    "inner release must finish retained source"
                );
            }
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
        let parent_instance = second_end + usize::from(shared && !before);
        if nested && !return_parent {
            expected.push(parent_instance + 1);
            if before {
                expected.push(selected_source);
            }
        }
        expected.push(parent_instance);
    } else if before {
        expected.extend((source_count..offset).rev());
    }
    if shared && !(nested && before && !return_parent && returned.is_none()) {
        expected.push(selected_source);
    }
    expected.splice(0..0, early_releases);
    replay.done();
    assert_eq!(
        replay.instances.len(),
        expected.len(),
        "every formed instance must release exactly once"
    );
    assert_eq!(replay.released, expected);
    assert_eq!(
        replay.loan_ends,
        if before {
            (source_count..offset)
                .rev()
                .map(|instance| (instance, 0))
                .collect()
        } else if shared && returned.is_none() {
            vec![(second_end, 0)]
        } else {
            Vec::new()
        }
    );
}
