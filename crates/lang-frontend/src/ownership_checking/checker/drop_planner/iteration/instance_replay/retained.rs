//! 具名作用域结束后的 source 随实际递归捕获链延寿。
use super::*;
use crate::{
    ownership_checking::{CleanupSelectorSource, checker::drop_planner},
    parser::{Expression, Statement},
    source::SourceMap,
};

#[test]
fn scoped_source_recursive_chain_candidate_matrix() {
    candidate_matrix(SourceKind::Resource);
}

#[test]
fn retained_owned_closure_candidate_matrix() {
    candidate_matrix(SourceKind::OwnedClosure);
}

#[test]
fn retained_shared_closure_cascade_candidate_matrix() {
    candidate_matrix(SourceKind::SharedClosure);
}

#[test]
#[should_panic(expected = "root release needs retained-source completion")]
fn recursive_retained_source_rejects_missing_completion() {
    replay_scoped_source(
        "",
        [2, 2],
        SourceKind::Resource,
        true,
        Fault::MissingCompletion,
    );
}

#[test]
#[should_panic(expected = "retained completion needs an unconsumed root batch")]
fn recursive_retained_source_rejects_duplicate_completion() {
    replay_scoped_source(
        "",
        [2, 2],
        SourceKind::SharedClosure,
        true,
        Fault::DuplicateCompletion,
    );
}

#[test]
#[should_panic(expected = "retained completion needs an unconsumed root batch")]
fn recursive_retained_source_rejects_completion_before_its_root() {
    replay_scoped_source(
        "",
        [2, 2],
        SourceKind::OwnedClosure,
        true,
        Fault::EarlyCompletion,
    );
}

enum Fault {
    None,
    MissingCompletion,
    DuplicateCompletion,
    EarlyCompletion,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Resource,
    OwnedClosure,
    SharedClosure,
}

fn candidate_matrix(kind: SourceKind) {
    for tail in [
        "",
        "continue",
        "if (stop) { break }",
        "if (stop) { return }",
    ] {
        for first in [0, 1, 2, 7] {
            for second in [0, 1, 2, 7] {
                for siblings in [false, true] {
                    replay_scoped_source(tail, [first, second], kind, siblings, Fault::None);
                }
            }
        }
    }
}

fn replay_scoped_source(
    tail: &str,
    rounds: [usize; 2],
    kind: SourceKind,
    siblings: bool,
    fault: Fault,
) {
    let [first_rounds, rounds] = rounds;
    let mut sources = SourceMap::new();
    let holder = if siblings {
        "move { val old = f()\nval used = borrowed()\nval also = other() }"
    } else {
        "move { val old = f()\nval used = borrowed() }"
    };
    let declarations = match kind {
        SourceKind::Resource => "val xs = listOf(1)",
        SourceKind::OwnedClosure => {
            "val payload = listOf(1)\nval xs: move () -> Unit = move { read(payload) }"
        }
        SourceKind::SharedClosure => {
            "val payload = listOf(1)\nval xs: () -> Unit = { read(payload) }"
        }
    };
    let borrowed_text = if kind == SourceKind::Resource {
        "{ read(xs) }"
    } else {
        "{ xs() }"
    };
    let other_text = if kind == SourceKind::Resource {
        "{ val used = read(xs) }"
    } else {
        "{ val used = xs() }"
    };
    let other_decl = if siblings {
        format!("val other: () -> Unit = {other_text}")
    } else {
        String::new()
    };
    let source_instance = if kind == SourceKind::Resource { 1 } else { 2 };
    let borrowed_instance = source_instance + 1;
    let warmup_instance = borrowed_instance + 1 + usize::from(siblings);
    let holder_instance = warmup_instance + first_rounds;
    let source = sources
        .add_source(
            "scoped-recursive-source.ko",
            format!(
        "fun read(xs: List<Int>) {{}}\nfun run(first: List<Boolean>, flags: List<Boolean>) {{
var f: move () -> Unit = move {{}}
{{
    {declarations}
    val borrowed: () -> Unit = {borrowed_text}
    {other_decl}
    for (_ in first) {{ f = move {{ f() }} }}
    f = {holder}
}}
for (stop in flags) {{ f = move {{ f() }}\n{tail} }}
val used = f() }}"
            ),
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
    let [warmup, plan] = plans.as_slice() else {
        panic!("two complete loops")
    };
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
    let final_point = DropPoint::CallReturn(expression("f()"));
    let completion = facts
        .cleanup_steps
        .iter()
        .position(|(point, action)| {
            *point == final_point && matches!(action, Action::ReleaseRetainedClosureSources { .. })
        })
        .unwrap();
    match fault {
        Fault::None => {}
        Fault::MissingCompletion => {
            facts.cleanup_steps.remove(completion);
        }
        Fault::DuplicateCompletion => {
            facts
                .cleanup_steps
                .insert(completion + 1, facts.cleanup_steps[completion]);
        }
        Fault::EarlyCompletion => {
            let paired = facts.cleanup_steps.remove(completion);
            let Action::ReleaseRetainedClosureSources { root } = paired.1 else {
                unreachable!()
            };
            let producer = facts.cleanup_steps.iter().position(|(_, action)|
                matches!(action, Action::ReleaseClosureInstances { root: found, .. } if *found == root)).unwrap();
            facts.cleanup_steps.insert(producer, paired);
        }
    }
    let complete = |replay: &mut Replay, id, boundary: Option<StatementId>| {
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
    let seed = expression("move {}");
    replay.point(&facts, DropPoint::AfterExpression(seed));
    replay.bind(symbol("f"), replay.result.unwrap());
    complete(&mut replay, seed, None);
    let resource = expression("listOf(1)");
    let borrowed = expression(borrowed_text);
    let resource_symbol = symbol(if kind == SourceKind::Resource {
        "xs"
    } else {
        "payload"
    });
    let owner = facts
        .cleanup_steps
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture { input, .. }
                if input.source()
                    == crate::ownership_checking::ClosureCaptureSource::Symbol(resource_symbol) =>
            {
                match input.value() {
                    CleanupCaptureValue::Owner(owner) => Some(owner),
                    _ => None,
                }
            }
            _ => None,
        })
        .unwrap();
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
    replay.bind(resource_symbol, owner);
    complete(&mut replay, resource, None);
    if kind != SourceKind::Resource {
        let source_closure = expression(if kind == SourceKind::OwnedClosure {
            "move { read(payload) }"
        } else {
            "{ read(payload) }"
        });
        replay.point(&facts, DropPoint::AfterExpression(source_closure));
        if !replay.bindings.contains_key(&symbol("xs")) {
            replay.bind(symbol("xs"), replay.result.unwrap());
        }
        complete(&mut replay, source_closure, None);
    }
    replay.point(&facts, DropPoint::AfterExpression(borrowed));
    if !replay.bindings.contains_key(&symbol("borrowed")) {
        replay.bind(symbol("borrowed"), replay.result.unwrap());
    }
    complete(&mut replay, borrowed, None);
    if siblings {
        let other = expression(other_text);
        replay.point(&facts, DropPoint::AfterExpression(other));
        if !replay.bindings.contains_key(&symbol("other")) {
            replay.bind(symbol("other"), replay.result.unwrap());
        }
        complete(&mut replay, other, None);
    }
    // 借用者仍独立持有源；named source phi 缺席后，唯一清理义务转入
    // retained。多轮先把 f 形成递归链，再整体捕获进持有借用者的环境。
    let warmup_statement = warmup.descriptor().statement();
    let Statement::For {
        source: warmup_source,
        body: warmup_body,
        ..
    } = parsed
        .ast()
        .statements()
        .get(warmup_statement)
        .unwrap()
        .payload()
    else {
        unreachable!()
    };
    replay.point(&facts, DropPoint::AfterExpression(*warmup_source));
    replay.start_loop(warmup_statement);
    replay.edge(&facts, warmup, IterationPhiIncomingKind::Entry);
    let assignment = |value| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Expression::Assignment { value: rhs, .. } if *rhs == value => Some(id),
                _ => None,
            })
            .unwrap()
    };
    let warmup_closure = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("move { f() }"))
        .min_by_key(|(_, node)| node.span().start())
        .unwrap()
        .0;
    for _ in 0..first_rounds {
        replay.start_element(warmup_statement);
        replay.point(&facts, DropPoint::AfterExpression(warmup_closure));
        replay.point(
            &facts,
            DropPoint::AfterExpression(assignment(warmup_closure)),
        );
        complete(&mut replay, assignment(warmup_closure), Some(*warmup_body));
        replay.exit(&facts, warmup, IterationExitKind::Fallthrough);
        replay.edge(&facts, warmup, IterationPhiIncomingKind::Fallthrough);
        assert!(
            replay.released.is_empty(),
            "both the old chain and borrowed source stay live"
        );
    }
    replay.exit(&facts, warmup, IterationExitKind::Exhaustion);
    replay.edge(&facts, warmup, IterationPhiIncomingKind::Exhaustion);
    replay.point(&facts, DropPoint::AfterStatement(warmup_statement));
    assert!(
        replay.retained.contains(&source_instance),
        "source must enter the recursive chain already retained"
    );
    let holder = expression(holder);
    replay.point(&facts, DropPoint::AfterExpression(holder));
    replay.point(&facts, DropPoint::AfterExpression(assignment(holder)));
    let span = parsed.ast().expressions().get(holder).unwrap().span();
    let scope = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Statement::Block { .. })
                && node.span().start() <= span.start()
                && node.span().end() >= span.end()
        })
        .min_by_key(|(_, node)| node.span().end() - node.span().start())
        .unwrap()
        .0;
    complete(&mut replay, assignment(holder), Some(scope));
    replay.point(&facts, DropPoint::AfterStatement(scope));
    assert!(
        replay.released.is_empty(),
        "source survives its declaring scope"
    );
    let statement = plan.descriptor().statement();
    let Statement::For { source, body, .. } =
        parsed.ast().statements().get(statement).unwrap().payload()
    else {
        unreachable!()
    };
    replay.point(&facts, DropPoint::AfterExpression(*source));
    replay.start_loop(statement);
    replay.edge(&facts, plan, IterationPhiIncomingKind::Entry);
    assert!(replay.retained.contains(&source_instance));
    if kind == SourceKind::SharedClosure {
        assert!(
            replay.retained.contains(&1),
            "shared source itself retains its source"
        );
    }
    let formed = expression("move { f() }");
    let mut returned = false;
    let mut broke = false;
    for round in 0..rounds {
        replay.start_element(statement);
        replay.point(&facts, DropPoint::AfterExpression(formed));
        replay.point(&facts, DropPoint::AfterExpression(assignment(formed)));
        complete(&mut replay, assignment(formed), Some(*body));
        if tail.starts_with("if") {
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
                complete(&mut replay, control, Some(*body));
            }
        }
        assert!(
            replay.released.is_empty(),
            "chain and source survive until their last use"
        );
        if tail.contains("return") && round + 1 == rounds {
            replay.exit(
                &facts,
                plan,
                IterationExitKind::Return(expression("return")),
            );
            returned = true;
            break;
        }
        let (exit, edge) = if tail == "continue" {
            let jump = expression("continue");
            (
                IterationExitKind::Continue(jump),
                IterationPhiIncomingKind::Continue(jump),
            )
        } else if tail.contains("break") && round + 1 == rounds {
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
    if !returned {
        replay.point(&facts, DropPoint::AfterStatement(statement));
        assert!(replay.released.is_empty());
        replay.call(&facts, &parsed, &names, &[], expression("f()"));
        let span = parsed
            .ast()
            .expressions()
            .get(expression("f()"))
            .unwrap()
            .span();
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
        complete(&mut replay, expression("f()"), Some(body));
        replay.point(&facts, DropPoint::AfterStatement(body));
    }
    replay.done();
    let mut expected = (borrowed_instance..warmup_instance)
        .rev()
        .collect::<Vec<_>>();
    expected.push(0);
    expected.extend(warmup_instance..=holder_instance);
    expected.extend(holder_instance + 1..holder_instance + 1 + rounds);
    match kind {
        SourceKind::Resource => expected.push(1),
        SourceKind::OwnedClosure => expected.extend([1, 2]),
        SourceKind::SharedClosure => expected.extend([2, 1]),
    }
    assert_eq!(replay.released, expected);
    let mut loans = (borrowed_instance..warmup_instance)
        .rev()
        .map(|instance| (instance, 0))
        .collect::<Vec<_>>();
    if kind == SourceKind::SharedClosure {
        loans.push((2, 0));
    }
    assert_eq!(replay.loan_ends, loans);
}
