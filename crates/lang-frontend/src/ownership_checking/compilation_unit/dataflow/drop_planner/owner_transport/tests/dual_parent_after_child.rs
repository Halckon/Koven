//! Observation only: parent replacement follows the complete child traversal.
//! Static visits do not prove the child zero-iteration CFG edge or dynamic generations.
use super::super::origin_probe::{Event, WhileRole};
use super::*;
use crate::ownership_checking::UnitDropTarget;

#[cfg(feature = "d10-catalog-experiment")]
#[test]
fn ordinary_parent_after_child_records_actual_order_without_old_sidecar() {
    let text = "fun run(count: Int, childCount: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1 }; println(a); println(b) }";
    let fixture = Fixture::new(&[("parent-after-child.ko", text)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        assert!(observed.dual_nested_pending().is_none());
        assert!(observed.nested_pending().is_none());
        let events = observed.origin_probe.as_ref().unwrap();
        let root = fixture.local(0, "a");
        let child_call = checker.unit_expression(fixture.expression(0, "replace(&a, \"next\")"));
        let parent_call = checker.unit_expression(fixture.expression(0, "replace(&a, \"parent\")"));
        let child_incoming = checker.unit_expression(fixture.expression(0, "\"next\""));
        let parent_incoming = checker.unit_expression(fixture.expression(0, "\"parent\""));
        let actual_drops = observed
            .facts
            .iter()
            .map(|fact| fact.into_unit(checker.source_unit))
            .collect::<Vec<_>>();
        assert_eq!(actual_drops, owned.drops());
        let [child_old, parent_old, final_a, final_b] = actual_drops.as_slice() else {
            panic!("four actual cleanups");
        };
        for (fact, symbol) in [
            (child_old, fixture.local(0, "old")),
            (parent_old, fixture.local(0, "parentOld")),
            (final_a, root),
            (final_b, fixture.local(0, "b")),
        ] {
            assert_eq!(fact.target(), UnitDropTarget::Named(symbol));
        }
        let expression = |text| checker.unit_expression(fixture.expression(0, text));
        let ast = fixture.parsed[0].ast();
        let control = |text| {
            let condition = fixture.expression(0, text);
            let matching = ast.statements().iter().filter_map(|(id,node)|
                matches!(node.payload(),crate::parser::Statement::While {condition:actual,..} if *actual==condition).then_some(id)).collect::<Vec<_>>();
            let [id] = matching.as_slice() else {
                panic!("unique real while");
            };
            crate::type_checking::UnitStatementId::new(checker.source_unit, *id)
        };
        let parent = control("outer > 0");
        let child = control("inner > 0");
        assert_ne!(parent, child);
        let final_call = expression("println(a)");
        let suffix_call = expression("println(b)");
        let [child_loan, parent_loan, final_loan, suffix_loan] = owned.loans() else {
            panic!("four actual registered loans");
        };
        for (loan, call, root, kind) in [
            (
                child_loan,
                child_call,
                root,
                crate::ownership_checking::LoanKind::Exclusive,
            ),
            (
                parent_loan,
                parent_call,
                root,
                crate::ownership_checking::LoanKind::Exclusive,
            ),
            (
                final_loan,
                final_call,
                root,
                crate::ownership_checking::LoanKind::Shared,
            ),
            (
                suffix_loan,
                suffix_call,
                fixture.local(0, "b"),
                crate::ownership_checking::LoanKind::Shared,
            ),
        ] {
            assert_eq!(loan.call(), call);
            assert_eq!(loan.kind(), kind);
            assert!(
                matches!(loan.target(),crate::ownership_checking::UnitLoanTarget::Place(place) if place.is_root() && place.root()==root)
            );
        }
        use crate::ownership_checking::UnitDropPoint;
        for (drop, call) in [(child_old, child_call), (parent_old, parent_call)] {
            let UnitDropPoint::AfterStatement(statement) = drop.point() else {
                panic!("detached local cleanup");
            };
            let crate::parser::Statement::LocalVariable { declaration } = ast
                .statements()
                .get(statement.statement())
                .unwrap()
                .payload()
            else {
                panic!("real local");
            };
            let crate::parser::Item::Variable {
                initializer,
                name: crate::parser::NameMarker::Present(span),
                ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                panic!("real binding");
            };
            assert_eq!(*initializer, call.expression());
            assert_eq!(drop.value_origin(), *span);
        }
        assert_eq!(final_a.point(), UnitDropPoint::CallReturn(final_call));
        assert_eq!(final_b.point(), UnitDropPoint::CallReturn(suffix_call));
        // Complete observed sequence, fixed after the first real ordinary run. The actual
        // event slice is compared directly, never filtered or adapted to the old matcher.
        assert_eq!(
            events.as_slice(),
            &[
                Event::Literal(expression("\"first\"")),
                Event::Bound(root, expression("\"first\"")),
                Event::Literal(expression("\"kept\"")),
                Event::Bound(fixture.local(0, "b"), expression("\"kept\"")),
                Event::WhileTraversal {
                    control: parent,
                    role: WhileRole::Enter
                },
                Event::WhileTraversal {
                    control: parent,
                    role: WhileRole::ConditionEvaluated
                },
                Event::WhileTraversal {
                    control: parent,
                    role: WhileRole::BodyEnter
                },
                Event::WhileTraversal {
                    control: child,
                    role: WhileRole::Enter
                },
                Event::WhileTraversal {
                    control: child,
                    role: WhileRole::ConditionEvaluated
                },
                Event::WhileTraversal {
                    control: child,
                    role: WhileRole::BodyEnter
                },
                Event::BorrowRegistered(child_loan.clone()),
                Event::Literal(child_incoming),
                Event::Applied {
                    call: child_call,
                    root,
                    incoming: child_incoming
                },
                Event::PendingBorrowRemoved {
                    loan: child_loan.clone(),
                    call: child_call
                },
                Event::Bound(fixture.local(0, "old"), child_call),
                Event::Dropped(*child_old),
                Event::WhileTraversal {
                    control: child,
                    role: WhileRole::BodyExit(Some(true))
                },
                Event::WhileTraversal {
                    control: child,
                    role: WhileRole::Exit(Some(true))
                },
                Event::BorrowRegistered(parent_loan.clone()),
                Event::Literal(parent_incoming),
                Event::Applied {
                    call: parent_call,
                    root,
                    incoming: parent_incoming
                },
                Event::PendingBorrowRemoved {
                    loan: parent_loan.clone(),
                    call: parent_call
                },
                Event::Bound(fixture.local(0, "parentOld"), parent_call),
                Event::Dropped(*parent_old),
                Event::WhileTraversal {
                    control: parent,
                    role: WhileRole::BodyExit(Some(true))
                },
                Event::WhileTraversal {
                    control: parent,
                    role: WhileRole::Exit(Some(true))
                },
                Event::UncoveredCall(final_call),
                Event::BorrowRegistered(final_loan.clone()),
                Event::PendingBorrowRemoved {
                    loan: final_loan.clone(),
                    call: final_call
                },
                Event::Dropped(*final_a),
                Event::UncoveredCall(suffix_call),
                Event::BorrowRegistered(suffix_loan.clone()),
                Event::PendingBorrowRemoved {
                    loan: suffix_loan.clone(),
                    call: suffix_call
                },
                Event::Dropped(*final_b),
            ]
        );
        // These real parameter references are distinct; this is not a zero-trip execution.
        let counter_parameter = |text| {
            checker
                .reference_symbol(
                    fixture.parsed[0]
                        .ast()
                        .expressions()
                        .get(fixture.expression(0, text))
                        .unwrap()
                        .span(),
                )
                .unwrap()
        };
        assert_ne!(counter_parameter("count"), counter_parameter("childCount"));
    });
}
