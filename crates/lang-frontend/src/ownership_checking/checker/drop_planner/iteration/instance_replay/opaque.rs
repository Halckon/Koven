//! owned 参数的实际闭包可以不在被调用方循环的有限候选图中。
use super::*;
use crate::{
    ownership_checking::{CleanupOwnerValue, CleanupSelectorSource, checker::drop_planner},
    parser::{Expression, Statement},
    source::SourceMap,
};

#[test]
fn known_or_opaque_child_keeps_actual_instance_across_phi() {
    for branch in [0, 1] {
        for rounds in [0, 1, 2, 7] {
            replay(branch, rounds, Fault::None);
        }
    }
}

#[test]
#[should_panic(expected = "unlisted closure requires an opaque capture edge")]
fn opaque_child_requires_published_open_boundary() {
    replay(1, 2, Fault::MissingOpaqueBoundary);
}

#[test]
#[should_panic(expected = "unlisted closure requires an opaque capture edge")]
fn closed_capture_edge_rejects_a_node_only_present_elsewhere_in_graph() {
    replay(0, 2, Fault::UnlistedKnownChild);
}

enum Fault {
    None,
    MissingOpaqueBoundary,
    UnlistedKnownChild,
}

fn replay(branch: usize, rounds: usize, fault: Fault) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "opaque-child.ko",
            "fun seed(): move () -> Unit = move { val marker = 1 }
fun run(own incoming: move () -> Unit, flag: Boolean, flags: List<Int>) {
val base: move () -> Unit = if (flag) (move {}) else (incoming)
var f: move () -> Unit = move { base() }
for (_ in flags) {}
val used = f()
}",
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
    assert!(owned.deferred().iter().any(|fact| fact.reason()
        == crate::ownership_checking::OwnershipDeferredReason::AmbiguousClosureInstanceTransport));
    assert!(owned.cleanup_steps().is_empty() && owned.iterations().is_empty());
    assert!(owned.drops().is_empty() && owned.loan_ends().is_empty());
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
    let mut facts = planner.into_candidate_facts();
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
    let parent = expression("move { base() }");
    let graph = &mut facts.iterations[0].capture_graph;
    let parent_node = graph
        .nodes
        .iter()
        .position(|node| node.closure() == parent)
        .unwrap();
    let known_node = graph
        .nodes
        .iter()
        .position(|node| node.closure() == expression("move {}"))
        .unwrap();
    let source = &mut graph.nodes[parent_node].sources[0];
    assert_eq!(source.captured(), &[known_node]);
    assert!(
        source.may_be_opaque(),
        "the known candidate is not an exhaustive list"
    );
    match fault {
        Fault::None => {}
        Fault::MissingOpaqueBoundary => source.may_be_opaque = false,
        Fault::UnlistedKnownChild => {
            // 图中存在某 lambda，不表示当前捕获边允许指向它。
            source.captured.clear();
            source.may_be_opaque = false;
        }
    }
    let complete = |replay: &mut Replay, value| {
        let span = parsed.ast().expressions().get(value).unwrap().span();
        let statement = parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| {
                node.span().start() <= span.start() && node.span().end() >= span.end()
            })
            .min_by_key(|(_, node)| node.span().end() - node.span().start())
            .unwrap()
            .0;
        replay.point(&facts, DropPoint::AfterStatement(statement));
    };
    let incoming = symbol("incoming");
    let parameter = facts.drops.iter().filter_map(|fact| fact.owner()).find(|&owner|
        matches!(facts.cleanup_conditions.owner_value(owner), Some(CleanupOwnerValue::Parameter { symbol, .. }) if *symbol == incoming)).unwrap();
    let mut replay = Replay {
        file_captures: owned.captures().to_vec(),
        ..Replay::default()
    };
    // 调用者先实际形成 seed，再把该值交给 run 的 owned 参数，不为 opaque 伪造布局节点。
    let seed = expression("move { val marker = 1 }");
    replay.point(&facts, DropPoint::AfterExpression(seed));
    let seed_owner = replay.result.take().unwrap();
    let instance = replay.take_owner(seed_owner);
    assert_eq!(instance, 0);
    assert!(replay.owners.insert(parameter, instance).is_none());
    replay.bind(incoming, parameter);
    for &root in parsed.roots() {
        replay.point(&facts, DropPoint::FunctionEntry(root));
    }
    let control = expression("if (flag) (move {}) else (incoming)");
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
    replay.choices.insert(selector, branch);
    let selected = if branch == 0 {
        expression("move {}")
    } else {
        expression("incoming")
    };
    replay.point(&facts, DropPoint::AfterExpression(selected));
    replay.point(&facts, DropPoint::BranchExit { control, branch });
    replay.point(&facts, DropPoint::AfterExpression(control));
    complete(&mut replay, control);
    replay.point(&facts, DropPoint::AfterExpression(parent));
    complete(&mut replay, parent);
    if !replay.bindings.contains_key(&symbol("f")) {
        replay.bind(symbol("f"), replay.result.unwrap());
    }
    assert_eq!(replay.released, if branch == 0 { vec![0] } else { vec![] });
    let plan = &facts.iterations[0];
    assert!(
        plan.capture_graph()
            .nodes()
            .iter()
            .all(|node| node.closure() != seed)
    );
    // 仅实际已知子实例置位；opaque 实例不能冒充已知候选。
    let check_edge = |replay: &Replay, kind| {
        let edge = plan
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == kind)
            .unwrap();
        let binding = edge
            .bindings()
            .iter()
            .find(|binding| {
                plan.closure_phis().iter().any(|layout| {
                    layout.owner() == binding.target() && layout.symbol() == symbol("f")
                })
            })
            .unwrap();
        assert_eq!(binding.selector_writes().len(), 2);
        for write in binding.selector_writes() {
            let present =
                write.node() == parent_node || (write.node() == known_node && branch == 0);
            assert_eq!(
                replay.choices.get(&write.target()),
                Some(&usize::from(present))
            );
        }
        let parent_instance = if branch == 0 { 2 } else { 1 };
        assert_eq!(replay.owners.get(&binding.target()), Some(&parent_instance));
        assert_eq!(
            replay.instances[parent_instance].captures.get(&0),
            Some(&if branch == 0 { 1 } else { 0 })
        );
    };
    let statement = plan.descriptor().statement();
    let Statement::For { source, .. } = parsed.ast().statements().get(statement).unwrap().payload()
    else {
        unreachable!()
    };
    replay.point(&facts, DropPoint::AfterExpression(*source));
    replay.start_loop(statement);
    replay.edge(&facts, plan, IterationPhiIncomingKind::Entry);
    check_edge(&replay, IterationPhiIncomingKind::Entry);
    for _ in 0..rounds {
        replay.start_element(statement);
        replay.exit(&facts, plan, IterationExitKind::Fallthrough);
        replay.edge(&facts, plan, IterationPhiIncomingKind::Fallthrough);
        check_edge(&replay, IterationPhiIncomingKind::Fallthrough);
    }
    replay.exit(&facts, plan, IterationExitKind::Exhaustion);
    replay.edge(&facts, plan, IterationPhiIncomingKind::Exhaustion);
    check_edge(&replay, IterationPhiIncomingKind::Exhaustion);
    replay.point(&facts, DropPoint::AfterStatement(statement));
    replay.call(&facts, &parsed, &names, &[], expression("f()"));
    complete(&mut replay, expression("f()"));
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
        .filter(|(_, node)| node.span().start() <= span.start() && node.span().end() >= span.end())
        .max_by_key(|(_, node)| node.span().end() - node.span().start())
        .unwrap()
        .0;
    replay.point(&facts, DropPoint::AfterStatement(body));
    replay.done();
    assert_eq!(
        replay.released,
        if branch == 0 {
            vec![0, 1, 2]
        } else {
            vec![0, 1]
        }
    );
    assert!(replay.loan_ends.is_empty());
}
