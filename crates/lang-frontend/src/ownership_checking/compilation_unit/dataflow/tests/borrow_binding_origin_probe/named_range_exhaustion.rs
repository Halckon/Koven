use super::*;

#[test]
fn named_range_loop_reuse_authority_requires_real_provider_and_owner_delivery() {
    use crate::{
        ownership_checking::{
            UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as Action,
            UnitIterationExitKind, UnitLoanTarget, UnitValueDeliveryKind,
        },
        parser::Statement,
    };
    let provider = include_str!("../../../../../../../lang-std/koven/algorithms/ranges.ko")
        .strip_prefix("package koven.algorithms\n")
        .unwrap();
    let src = format!(
        r#"{provider}
fun observe(part: View<Int>): Unit {{
    if (part.size == 1) {{ println("still live") }}
}}
fun consume(own xs: List<Int>): Unit {{
    if (xs.size == 2) {{ println("owner") }}
}}
fun main(): Unit {{
    val xs = listOf(7, 9)
    borrow val part = take(xs, 1)
    for (item in part) {{
        if (item == 7) {{ println("element") }}
    }}
    observe(part)
    consume(xs)
}}
"#
    );
    for rooted in [false, true] {
        let f = Fixture::range_receiver_source("range-reuse.ko", &src, rooted);
        let (owned, events) = capture(|| f.shared());
        assert!(
            owned.diagnostics().is_empty() && owned.deferred().is_empty(),
            "rooted={rooted}: {:?} {:?}",
            owned.diagnostics(),
            owned.deferred()
        );
        let expr = |s| UnitExpressionId::new(f.source(0), f.expression(0, s));
        let ast = f.parsed[0].ast();
        let [binding] = owned.borrow_results().bindings() else {
            panic!("one actual named descriptor");
        };
        let [end] = owned.borrow_results().ends() else {
            panic!(
                "one actual descriptor end: {:?}",
                owned.borrow_results().ends()
            );
        };
        assert_eq!(binding.binding(), end.binding());
        let source_loan = binding.source_loan().unwrap();
        assert_eq!(source_loan.call(), expr("take(xs, 1)"));
        let UnitLoanTarget::Place(root) = binding.origin() else {
            panic!("original backing root");
        };
        let loop_stmt = ast.statements().iter().find_map(|(id, statement)| {
            matches!(statement.payload(), Statement::For { .. })
                .then_some(UnitStatementId::new(f.source(0), id))
                .filter(|id| owned.iteration(*id).is_some_and(|plan|
                    matches!(plan.source(), Some(UnitLoanTarget::Place(place)) if place.root() == binding.binding())))
        }).expect("actual named Range provider");
        let plan = owned.iteration(loop_stmt).unwrap();
        let exhaust = plan
            .exits()
            .iter()
            .find(|edge| edge.kind() == UnitIterationExitKind::Exhaustion)
            .expect("real exhaustion");
        assert!(
            exhaust
                .actions()
                .iter()
                .any(|action| matches!(action, Action::FinishProvider(id) if *id == loop_stmt))
        );
        assert!(
            exhaust
                .actions()
                .iter()
                .any(|action| matches!(action, Action::EndSource(id) if *id == loop_stmt))
        );
        assert!(!exhaust.actions().iter().any(|action| match action {
            Action::EndBinding { symbol: ended, .. } => *ended == binding.binding(),
            Action::Drop(dropped) => dropped.target() == UnitDropTarget::Named(root.root()),
            _ => false,
        }));
        let UnitDropPoint::AfterStatement(last) = end.point() else {
            panic!("not lexical/loop exit");
        };
        let observe = expr("observe(part)");
        assert!(
            matches!(ast.statements().get(last.statement()).unwrap().payload(),
            Statement::Expression { expression } if *expression == observe.expression())
        );
        removal_index(&owned, &events, binding, end);
        let consume = expr("consume(xs)");
        let [witness] = owned.borrow_results().named_range_loop_reuses.as_slice() else {
            panic!("exactly one passive, source-qualified candidate");
        };
        assert_eq!(witness.loop_exit, UnitDropPoint::LoopExit(loop_stmt));
        assert_eq!(
            owned
                .named_range_loop_reuse_controls()
                .get(&binding.binding()),
            Some(&witness.body_control)
        );
        let mut missing_provider = owned.clone();
        missing_provider
            .iterations
            .retain(|plan| plan.descriptor().statement() != loop_stmt);
        assert!(
            missing_provider
                .named_range_loop_reuse_controls()
                .is_empty()
        );
        let mut duplicate = owned.clone();
        duplicate
            .borrow_results
            .named_range_loop_reuses
            .push(*witness);
        assert!(duplicate.named_range_loop_reuse_controls().is_empty());
        assert_eq!(witness.source, plan.descriptor().source());
        assert_eq!(
            witness.body_control,
            expr(r#"if (item == 7) { println("element") }"#)
        );
        assert_eq!(witness.binding, binding.binding());
        assert_eq!(witness.root, root.root());
        assert_eq!(witness.observer, observe);
        assert_eq!(witness.end, end.point());
        assert_eq!(witness.consume, consume);
        let moved = ast
            .expressions()
            .get(witness.consumed_operand.expression())
            .unwrap();
        assert_eq!(f.sources.slice(moved.span()), Ok("xs"));
        assert_ne!(witness.body_control, witness.source);
        assert!(owned.value_deliveries().iter().any(|delivery| {
            delivery.call() == witness.consume
                && delivery.argument() == witness.consumed_operand
                && delivery
                    .place()
                    .is_some_and(|place| place.is_root() && place.root() == root.root())
                && delivery.kind() == UnitValueDeliveryKind::Move
        }));
        assert!(
            owned
                .value_deliveries()
                .iter()
                .any(|delivery| delivery.call() == consume
                    && delivery.kind() == UnitValueDeliveryKind::Move)
        );
        for blocked in [
            src.replace("    observe(part)", "    observe(part)\n    observe(part)"),
            src.replace(
                "    observe(part)\n    consume(xs)",
                "    consume(xs)\n    observe(part)",
            ),
            src.replace(
                r#"if (item == 7) { println("element") }"#,
                r#"if (item == 7) { println("element") }; println("extra")"#,
            ),
        ] {
            let f = Fixture::range_receiver_source("wrong-range-reuse.ko", &blocked, rooted);
            let rejected = f.shared();
            assert!(rejected.borrow_results().named_range_loop_reuses.is_empty());
            assert!(rejected.named_range_loop_reuse_controls().is_empty());
            assert!(
                rejected
                    .diagnostics()
                    .iter()
                    .any(|diag| diag.code().to_string() == "L0135"),
                "{:?}",
                rejected.diagnostics()
            );
        }
    }
}
