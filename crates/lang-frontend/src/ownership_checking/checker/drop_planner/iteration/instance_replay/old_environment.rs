//! 同一 lambda 后轮形成新环境后，调用前轮环境并在 body 内转移捕获。
use super::*;
use crate::{
    ownership_checking::{CleanupSelectorSource, ClosureCaptureSource},
    parser::{Expression, Statement},
    source::SourceMap,
};

#[test]
fn old_environment_keeps_its_selected_capture_after_next_formation() {
    for rounds in [0, 1, 2, 7] {
        for first_branch in [0, 1] {
            for owned_sources in [false, true] {
                for copy_capture in [false, true] {
                    replay_old_environment(
                        rounds,
                        first_branch,
                        owned_sources,
                        copy_capture,
                        Fault::None,
                    );
                }
            }
        }
    }
}

#[test]
#[should_panic(expected = "lambda entry needs a passed environment")]
fn old_environment_rejects_missing_dynamic_pass() {
    replay_old_environment(2, 0, false, false, Fault::MissingPass);
}

#[test]
#[should_panic(expected = "ordinary drop cannot hide owned captures")]
fn old_environment_rejects_plain_drop_for_conditional_shared_child() {
    replay_old_environment(2, 0, false, false, Fault::PlainDrop);
}

#[test]
#[should_panic(expected = "Copyable source capture must have been initialized")]
fn old_environment_rejects_uninitialized_copyable_source() {
    replay_old_environment(1, 0, false, true, Fault::MissingCopy);
}

#[test]
#[should_panic(expected = "selected named root has no live binding")]
fn old_environment_rejects_duplicate_function_end_cleanup() {
    replay_old_environment(2, 0, false, false, Fault::DuplicateFunctionEnd);
}

#[derive(Clone, Copy)]
enum Fault {
    None,
    MissingPass,
    PlainDrop,
    MissingCopy,
    DuplicateFunctionEnd,
}

fn replay_old_environment(
    rounds: usize,
    first_branch: usize,
    owned_sources: bool,
    copy_capture: bool,
    fault: Fault,
) {
    let mut sources = SourceMap::new();
    let parameters = if owned_sources {
        "flags: List<Boolean>"
    } else {
        "flags: List<Boolean>, xs: List<Int>, ys: List<Int>"
    };
    let declarations = if owned_sources {
        "val xs = listOf(1)\nval ys = listOf(2)"
    } else {
        ""
    };
    let inner_text = if copy_capture {
        "move { val copied = flag\nval used = chosen() }"
    } else {
        "move { val used = chosen() }"
    };
    let outer_text =
        format!("move {{\nval inner: move () -> Unit = {inner_text}\nval used = inner()\n}}");
    let source = sources
        .add_source(
            "old-environment.ko",
            format!(
                "fun read(xs: List<Int>) {{}}
fun run({parameters}) {{
{declarations}
var saved: move () -> Unit = move {{}}
for (flag in flags) {{
val chosen: () -> Unit = if (flag) ({{ read(xs) }}) else ({{ read(ys) }})
val current: move () -> Unit = {outer_text}
val old = saved
{{ val used = old() }}
saved = current
}}
val final = saved
val used = final()
}}"
            ),
        )
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = crate::type_checking::standard_environments();
    let names = crate::name_resolution::resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned =
        crate::ownership_checking::check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let mut facts = if owned_sources {
        assert!(owned.deferred().iter().any(|fact| fact.reason() == crate::ownership_checking::OwnershipDeferredReason::AmbiguousClosureInstanceTransport));
        assert!(owned.cleanup_steps().is_empty() && owned.iterations().is_empty());
        // owning 来源交叉组合仍受实例运输门禁保护，回放组装后的候选产物。
        use crate::ownership_checking::checker::drop_planner;
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
        planner.into_candidate_facts()
    } else {
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        // 非 owning source 切片已经公开，直接消费公开产物。
        DropPlan {
            borrow_ends: Vec::new(),
            cleanup_steps: owned.cleanup_steps().to_vec(),
            cleanup_conditions: owned.cleanup_conditions().clone(),
            drops: owned.drops().to_vec(),
            loan_ends: owned.loan_ends().to_vec(),
            iterations: owned.iterations().to_vec(),
            deferred: Vec::new(),
        }
    };
    let expression = |text: &str| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
            .unwrap()
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
    let final_span = parsed
        .ast()
        .expressions()
        .get(expression("final()"))
        .unwrap()
        .span();
    let function_body = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Statement::Block { .. })
                && node.span().start() <= final_span.start()
                && node.span().end() >= final_span.end()
        })
        .max_by_key(|(_, node)| node.span().end() - node.span().start())
        .unwrap()
        .0;
    match fault {
        Fault::None => {}
        Fault::DuplicateFunctionEnd => {
            let root = facts
                .cleanup_steps
                .iter()
                .find_map(|(point, action)| match action {
                    Action::Drop(root)
                        if *point == DropPoint::CallReturn(expression("final()"))
                            && root.target() == DropTarget::Named(symbol("final")) =>
                    {
                        Some(*root)
                    }
                    _ => None,
                })
                .unwrap();
            let point = DropPoint::AfterStatement(function_body);
            let mut duplicate = DropFact::new(point, root.target(), root.value_origin())
                .with_owner(root.owner().unwrap());
            if let Some(condition) = root.condition() {
                duplicate = duplicate.with_condition(condition);
            }
            facts.cleanup_steps.push((point, Action::Drop(duplicate)));
        }
        Fault::MissingCopy => {
            let before = facts.cleanup_steps.len();
            facts.cleanup_steps.retain(|(point, action)| !(*point == DropPoint::AfterExpression(expression(&outer_text))
                && matches!(action, Action::SaveClosureCapture { input, .. } if input.effect() == ClosureCaptureEffect::Copy)));
            assert_eq!(before, facts.cleanup_steps.len() + 1);
        }
        Fault::MissingPass => {
            let before = facts.cleanup_steps.len();
            facts.cleanup_steps.retain(|(point, action)| {
                !(*point == DropPoint::CallEntry(expression("old()"))
                    && matches!(action, Action::PassClosureEnvironment { .. }))
            });
            assert_eq!(before, facts.cleanup_steps.len() + 1);
        }
        Fault::PlainDrop => {
            let action = facts
                .cleanup_steps
                .iter_mut()
                .find_map(|(point, action)| {
                    (*point == DropPoint::CallReturn(expression("inner()"))
                        && matches!(action, Action::ReleaseClosureInstances { .. }))
                    .then_some(action)
                })
                .unwrap();
            let Action::ReleaseClosureInstances { root, .. } = *action else {
                unreachable!()
            };
            *action = Action::Drop(root);
            facts.cleanup_steps.retain(|(_, action)| !matches!(action, Action::ReleaseRetainedClosureSources { root: paired } if *paired == root));
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
        file_captures: owned.captures().to_vec(),
        ..Replay::default()
    };
    for &root in parsed.roots() {
        replay.point(&facts, DropPoint::FunctionEntry(root));
    }
    for (name, text) in [("xs", "listOf(1)"), ("ys", "listOf(2)")] {
        if !owned_sources {
            replay.form_place(ClosureCaptureSource::Symbol(symbol(name)));
            continue;
        }
        let value = expression(text);
        let source_phi = facts.iterations[0]
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.symbol() == symbol(name)
                    && phi.boundary() == crate::ownership_checking::IterationPhiBoundary::Header
            })
            .unwrap();
        let owner = facts.iterations[0]
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
        let Expression::Call {
            callee, arguments, ..
        } = parsed.ast().expressions().get(value).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*callee));
        for argument in arguments {
            replay.point(&facts, DropPoint::AfterExpression(argument.value));
        }
        replay.point(&facts, DropPoint::CallEntry(value));
        replay.form_resource(&facts.cleanup_conditions, owner, value);
        replay.point(&facts, DropPoint::CallReturn(value));
        replay.point(&facts, DropPoint::AfterExpression(value));
        replay.bind(symbol(name), owner);
        complete(&mut replay, value, None);
    }
    let seed = expression("move {}");
    replay.point(&facts, DropPoint::AfterExpression(seed));
    replay.bind(symbol("saved"), replay.result.unwrap());
    complete(&mut replay, seed, None);
    let plan = &facts.iterations[0];
    let statement = plan.descriptor().statement();
    let Statement::For { source, body, .. } =
        parsed.ast().statements().get(statement).unwrap().payload()
    else {
        unreachable!()
    };
    let control = expression("if (flag) ({ read(xs) }) else ({ read(ys) })");
    let Expression::If { condition, .. } =
        parsed.ast().expressions().get(control).unwrap().payload()
    else {
        unreachable!()
    };
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
    let outer = expression(&outer_text);
    let inner = expression(inner_text);
    replay.point(&facts, DropPoint::AfterExpression(*source));
    replay.start_loop(statement);
    replay.edge(&facts, plan, IterationPhiIncomingKind::Entry);
    let mut expected_releases = Vec::new();
    let mut expected_loans = Vec::new();
    let mut next_instance = 3;
    let mut prior_parent = 2;
    let mut prior_borrower = None;
    for round in 0..rounds {
        let branch = (first_branch + round) % 2;
        replay.start_element(statement);
        replay.point(&facts, DropPoint::AfterExpression(*condition));
        replay.choices.insert(selector, branch);
        let chosen = expression(if branch == 0 {
            "{ read(xs) }"
        } else {
            "{ read(ys) }"
        });
        replay.point(&facts, DropPoint::AfterExpression(chosen));
        replay.point(&facts, DropPoint::BranchExit { control, branch });
        replay.point(&facts, DropPoint::AfterExpression(control));
        complete(&mut replay, control, Some(*body));
        replay.point(&facts, DropPoint::AfterExpression(outer));
        complete(&mut replay, outer, Some(*body));
        let borrower = next_instance;
        let parent = next_instance + 1;
        next_instance += 2;
        let old_initializer = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()) == Ok("saved")
                    && node.span().start()
                        > parsed.ast().expressions().get(outer).unwrap().span().end())
                .then_some(id)
            })
            .unwrap();
        replay.point(&facts, DropPoint::AfterExpression(old_initializer));
        complete(&mut replay, old_initializer, Some(*body));
        assert_eq!(
            replay.owners[&replay.bindings[&symbol("old")]],
            prior_parent,
            "old must still hold the previous instance after the new formation"
        );
        assert_eq!(replay.owners[&replay.bindings[&symbol("current")]], parent);
        assert_eq!(
            replay.instances[borrower].shared[&0], branch,
            "the new borrower's source follows the actual loop element"
        );
        if let Some(borrower) = prior_borrower {
            expected_releases.extend([borrower, next_instance]);
            expected_loans.push((borrower, 0));
            next_instance += 1;
        }
        expected_releases.push(prior_parent);
        let calls = [
            (expression("old()"), if round == 0 { seed } else { outer }),
            (expression("inner()"), inner),
        ];
        replay.call(&facts, &parsed, &names, &calls, expression("old()"));
        assert_eq!(
            replay.released, expected_releases,
            "old call must release the previous environment"
        );
        assert_eq!(replay.loan_ends, expected_loans);
        assert!(
            replay.instances[parent].live && replay.instances[borrower].live,
            "the later environment survives the old call"
        );
        prior_parent = parent;
        prior_borrower = Some(borrower);
        complete(&mut replay, expression("old()"), Some(*body));
        let assign = expression("saved = current");
        let Expression::Assignment { value, .. } =
            parsed.ast().expressions().get(assign).unwrap().payload()
        else {
            unreachable!()
        };
        replay.point(&facts, DropPoint::AfterExpression(*value));
        replay.point(&facts, DropPoint::AfterExpression(assign));
        complete(&mut replay, assign, Some(*body));
        replay.exit(&facts, plan, IterationExitKind::Fallthrough);
        replay.edge(&facts, plan, IterationPhiIncomingKind::Fallthrough);
    }
    replay.exit(&facts, plan, IterationExitKind::Exhaustion);
    replay.edge(&facts, plan, IterationPhiIncomingKind::Exhaustion);
    replay.point(&facts, DropPoint::AfterStatement(statement));
    if owned_sources {
        if rounds == 0 {
            expected_releases.extend([1, 0]);
        } else {
            expected_releases.push(1 - (first_branch + rounds - 1) % 2);
        }
        assert_eq!(
            replay.released, expected_releases,
            "loop exit releases only unborrowed sources"
        );
    }
    let final_initializer = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("saved"))
        .max_by_key(|(_, node)| node.span().start())
        .unwrap()
        .0;
    replay.point(&facts, DropPoint::AfterExpression(final_initializer));
    complete(&mut replay, final_initializer, None);
    let calls = [
        (
            expression("final()"),
            if rounds == 0 { seed } else { outer },
        ),
        (expression("inner()"), inner),
    ];
    replay.call(&facts, &parsed, &names, &calls, expression("final()"));
    complete(&mut replay, expression("final()"), Some(function_body));
    replay.point(&facts, DropPoint::AfterStatement(function_body));
    replay.done();
    if let Some(borrower) = prior_borrower {
        expected_releases.extend([borrower, next_instance]);
        expected_loans.push((borrower, 0));
        if owned_sources {
            expected_releases.push((first_branch + rounds - 1) % 2);
        }
    }
    expected_releases.push(prior_parent);
    assert_eq!(replay.released, expected_releases);
    assert_eq!(replay.loan_ends, expected_loans);
}
