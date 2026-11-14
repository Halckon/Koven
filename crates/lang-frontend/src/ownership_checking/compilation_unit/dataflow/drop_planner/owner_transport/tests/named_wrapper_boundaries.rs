//! Actual root boundaries only. No wrapper bound, admission, or filtered callee capability.
#![cfg(feature = "d10-catalog-experiment")]
use super::super::origin_probe::{Event, WhileRole};
use super::*;

#[cfg(feature = "d10-catalog-experiment")]
#[test]
fn ordinary_named_wrapper_keeps_complete_root_trace_boundaries() {
    let text = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) {
val parentOld = replace(&a, "parent");
var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 };
outer -= 1
}; println(a); println(b)
}
fun entry() { val p = 1; val c = 2; run(p, c) }"#;
    let fixture = Fixture::new(&[("named-wrapper.ko", text)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let make = || {
            super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            )
        };
        let baseline = make().run().unwrap();
        let mut planner = make();
        planner.enable_loop_generation_probe();
        assert!(
            planner.origin_probe.is_none(),
            "old automatic probe remains closed"
        );
        planner.origin_probe = Some(Vec::new());
        planner.root_trace_ranges = Some(Vec::new());
        let observed = planner.run().unwrap();
        assert_eq!(observed.facts, baseline.facts);
        assert_eq!(
            observed.direct_call_loan_ends(),
            baseline.direct_call_loan_ends()
        );
        assert_eq!(observed.owner_observations, baseline.owner_observations);
        assert!(observed.dual_nested_pending().is_none());
        assert!(observed.after_child_pending().is_none());
        let events = observed.origin_probe.as_ref().unwrap();
        let ranges = observed.root_trace_ranges.as_ref().unwrap();
        assert_eq!(ranges.len(), 2);
        for ((id, _), root) in ranges.iter().zip(checker.parsed.roots()) {
            assert_eq!(id.source_unit(), checker.source_unit);
            assert_eq!(id.item(), *root);
        }
        assert_eq!(ranges[0].1, 0..34);
        assert_eq!(ranges[1].1, 34..events.len());
        assert_eq!(checker.parsed.roots().len(), 2);
        assert!(
            !checker
                .parsed
                .ast()
                .expressions()
                .iter()
                .any(|(_, node)| matches!(
                    node.payload(),
                    crate::parser::Expression::Lambda { .. }
                ))
        );
        assert_eq!(ranges[0].1.start, 0);
        assert_eq!(ranges[0].1.end, ranges[1].1.start);
        assert_eq!(ranges[1].1.end, events.len());
        let wrapper_call = checker.unit_expression(fixture.expression(0, "run(p, c)"));
        assert_eq!(wrapper_call.source_unit(), checker.source_unit);
        let [
            Event::UncoveredCall(actual_call),
            Event::BorrowRegistered(first),
            Event::BorrowRegistered(second),
            Event::DirectBorrowEnded {
                loan: second_end,
                point: second_point,
            },
            Event::DirectBorrowEnded {
                loan: first_end,
                point: first_point,
            },
        ] = &events[ranges[1].1.clone()]
        else {
            panic!(
                "complete actual wrapper trace: {:?}",
                &events[ranges[1].1.clone()]
            );
        };
        assert_eq!(*actual_call, wrapper_call);
        assert_eq!(first.call(), wrapper_call);
        assert_eq!(second.call(), wrapper_call);
        assert_eq!(
            first.argument(),
            checker.unit_expression(fixture.expression(0, "p"))
        );
        assert_eq!(
            second.argument(),
            checker.unit_expression(fixture.expression(0, "c"))
        );
        assert_eq!(first, first_end);
        assert_eq!(second, second_end);
        let point = crate::ownership_checking::UnitDropPoint::CallReturn(wrapper_call);
        assert_eq!(*first_point, point);
        assert_eq!(*second_point, point);
        let [parent, child, final_a, final_b, actual_first, actual_second] = owned.loans() else {
            panic!("exact original whole-owned six loans");
        };
        assert_eq!(actual_first, first);
        assert_eq!(actual_second, second);
        for (index, actual) in [(7, parent), (16, child), (27, final_a), (31, final_b)] {
            let Event::BorrowRegistered(loan) = &events[index] else {
                panic!("callee registration")
            };
            assert_eq!(loan, actual);
        }
        let [actual_second_end, actual_first_end] = owned.direct_call_loan_ends() else {
            panic!("exact original whole-owned wrapper ends");
        };
        for (actual, loan, at) in [
            (actual_second_end, second_end, second_point),
            (actual_first_end, first_end, first_point),
        ] {
            assert_eq!(actual.loan(), loan);
            assert_eq!(actual.point(), *at);
        }
        assert_eq!(actual_second_end.preceding_call_loan_ends(), 0);
        assert_eq!(actual_first_end.preceding_call_loan_ends(), 1);
        let callee_drops: Vec<_> = [12, 21, 29, 33]
            .into_iter()
            .map(|index| {
                let Event::Dropped(fact) = &events[index] else {
                    panic!("callee drop")
                };
                *fact
            })
            .collect();
        assert_eq!(callee_drops, owned.drops());
        assert_eq!(
            observed
                .facts
                .iter()
                .map(|fact| fact.into_unit(checker.source_unit))
                .collect::<Vec<_>>(),
            owned.drops()
        );
        assert_eq!(
            observed.direct_call_loan_ends(),
            owned.direct_call_loan_ends()
        );

        let expected = [
            "literal",
            "bound",
            "literal",
            "bound",
            "enter",
            "condition",
            "body",
            "borrow",
            "literal",
            "replace",
            "removed",
            "bound",
            "drop",
            "enter",
            "condition",
            "body",
            "borrow",
            "literal",
            "replace",
            "removed",
            "bound",
            "drop",
            "body_exit",
            "exit",
            "body_exit",
            "exit",
            "call",
            "borrow",
            "removed",
            "drop",
            "call",
            "borrow",
            "removed",
            "drop",
            "call",
            "borrow",
            "borrow",
            "direct_end",
            "direct_end",
        ];
        assert_eq!(events.len(), expected.len());
        for (event, expected) in events.iter().zip(expected) {
            let role = match event {
                Event::Literal(_) => "literal",
                Event::Bound(_, _) => "bound",
                Event::Applied { .. } => "replace",
                Event::Dropped(_) => "drop",
                Event::BorrowRegistered(_) => "borrow",
                Event::PendingBorrowRemoved { .. } => "removed",
                Event::DirectBorrowEnded { .. } => "direct_end",
                Event::UncoveredCall(_) => "call",
                Event::WhileTraversal {
                    role: WhileRole::Enter,
                    ..
                } => "enter",
                Event::WhileTraversal {
                    role: WhileRole::ConditionEvaluated,
                    ..
                } => "condition",
                Event::WhileTraversal {
                    role: WhileRole::BodyEnter,
                    ..
                } => "body",
                Event::WhileTraversal {
                    role: WhileRole::BodyExit(Some(true)),
                    ..
                } => "body_exit",
                Event::WhileTraversal {
                    role: WhileRole::Exit(Some(true)),
                    ..
                } => "exit",
                _ => panic!("unconsumed actual event {event:?}"),
            };
            assert_eq!(role, expected);
        }
    });
}
