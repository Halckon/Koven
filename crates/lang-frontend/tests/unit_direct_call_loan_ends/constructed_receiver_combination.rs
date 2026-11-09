//! Constructed call operands coexist with receiver and String.clone observations.
use super::receiver_return::{NODE, assert_returns};
use super::temporary_field_pending::expression;
use super::*;
use lang_frontend::ownership_checking::UnitReceiverOwnershipTarget;

#[test]
fn constructed_arguments_preserve_receiver_and_clone_normal_completions() {
    for (operand, constructed) in [("Node()", true), ("((Node()))", true), ("factory()", false)] {
        assert_composition(operand, constructed, "", "", "0", None);
    }
}

#[test]
fn constructed_argument_prefixes_coexist_with_receiver_and_clone_ends() {
    for (open, close, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        for (operand, constructed) in [("Node()", true), ("((Node()))", true), ("factory()", false)]
        {
            let argument = format!("if (flag) {{ {jump} }} else {{ 0 }}");
            assert_composition(operand, constructed, open, close, &argument, Some(jump));
        }
    }
}

fn assert_composition(
    operand: &str,
    constructed: bool,
    open: &str,
    close: &str,
    argument: &str,
    jump: Option<&str>,
) {
    let clone_call = "\"prefix\".clone()";
    let inner = format!("node.touch({clone_call}, {argument})");
    let outer = format!("outer({operand}, {inner})");
    let text = format!(
        "{NODE}fun factory(): Node = Node()\nfun outer(item: Node, own next: Int): Unit {{}}\nfun run(node: Node, flag: Boolean): Unit {{ {open} {outer}; {close} }}"
    );
    let observed = assert_returns(&text, &[(&inner, "node")]);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let source = names.names().index().source_units()[0].id();
        let qualify = |id| UnitExpressionId::new(source, id);
        let outer_id = qualify(expression(&observed, &outer));
        let inner_id = qualify(expression(&observed, &inner));
        let clone_id = qualify(expression(&observed, clone_call));
        let outer_node = observed
            .parsed
            .ast()
            .expressions()
            .get(outer_id.expression())
            .unwrap();
        let Expression::Call { arguments, .. } = outer_node.payload() else {
            panic!("the original outer call");
        };
        let argument_id = qualify(arguments[0].value);
        let actual_argument = observed
            .parsed
            .ast()
            .expressions()
            .get(arguments[0].value)
            .unwrap();
        assert_eq!(observed.sources.slice(actual_argument.span()), Ok(operand));
        let loans = unit
            .loans()
            .iter()
            .filter(|loan| loan.call() == outer_id)
            .collect::<Vec<_>>();
        let [loan] = loans.as_slice() else {
            panic!("one actual outer Borrow operand")
        };
        assert_eq!(loan.argument(), argument_id);
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(argument_id));
        assert_eq!(loan.begin_span(), actual_argument.span());
        assert_eq!(loan.end_span(), outer_node.span());
        let jump_point =
            jump.map(|jump| UnitDropPoint::ControlTransfer(qualify(expression(&observed, jump))));
        let expected_outer = jump_point
            .into_iter()
            .chain([UnitDropPoint::CallReturn(outer_id)])
            .collect::<Vec<_>>();
        let direct = unit.direct_call_loan_ends();
        if constructed {
            assert_eq!(
                direct.iter().map(|end| end.point()).collect::<Vec<_>>(),
                expected_outer
            );
            for end in direct {
                assert_eq!(end.loan(), *loan);
                assert_eq!(end.preceding_drops(), 0);
                // Inner text Borrow retires before the pending outer operand at a jump.
                assert_eq!(
                    end.preceding_call_loan_ends(),
                    usize::from(Some(end.point()) == jump_point)
                );
            }
        } else {
            assert!(
                direct.is_empty(),
                "factory results retain their distinct admission boundary"
            );
        }
        let owner_drops = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(argument_id))
            .collect::<Vec<_>>();
        assert_eq!(
            owner_drops
                .iter()
                .map(|drop| drop.point())
                .collect::<Vec<_>>(),
            expected_outer
        );
        assert!(
            owner_drops
                .iter()
                .all(|drop| drop.value_origin() == actual_argument.span())
        );

        let [normal] = unit.receiver_call_return_ends() else {
            panic!("one normal receiver end")
        };
        assert_eq!(normal.receiver().call(), inner_id);
        assert_eq!(normal.point(), UnitDropPoint::CallReturn(inner_id));
        let UnitReceiverOwnershipTarget::Place(node) = normal.receiver().target() else {
            panic!("Borrow parameter receiver")
        };
        assert!(
            unit.drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(node.root()))
        );
        assert_eq!(
            unit.receiver_call_prefix_ends().len(),
            usize::from(jump.is_some())
        );
        if let Some(point) = jump_point {
            let [prefix] = unit.receiver_call_prefix_ends() else {
                panic!("one receiver prefix end")
            };
            assert_eq!(prefix.receiver(), normal.receiver());
            assert_eq!(prefix.point(), point);
            assert_eq!(prefix.preceding_drops(), 0);
            assert_eq!(prefix.preceding_call_loan_ends(), 1);
            assert_eq!(prefix.preceding_receiver_loan_ends(), 0);
            let drops = unit
                .drops()
                .iter()
                .filter(|drop| drop.point() == point)
                .map(|drop| drop.target())
                .collect::<Vec<_>>();
            assert_eq!(
                drops,
                [
                    UnitDropTarget::Temporary(clone_id),
                    UnitDropTarget::Temporary(argument_id)
                ]
            );
        }
        let [clone_end] = unit.string_clone_loan_ends() else {
            panic!("one completed clone expression")
        };
        let literal_id = qualify(expression(&observed, "\"prefix\""));
        assert_eq!(clone_end.loan().call(), clone_id);
        assert_eq!(clone_end.loan().argument(), literal_id);
        assert_eq!(clone_end.loan().kind(), LoanKind::Shared);
        assert_eq!(
            clone_end.loan().target(),
            &UnitLoanTarget::Temporary(literal_id)
        );
        assert_eq!(clone_end.point(), UnitDropPoint::CallReturn(clone_id));
        assert!(unit.loans().contains(clone_end.loan()));
        let literal_drops = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(literal_id))
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(literal_drops, [UnitDropPoint::CallReturn(clone_id)]);
        let expected_clone_result = jump_point
            .into_iter()
            .chain([UnitDropPoint::CallReturn(inner_id)])
            .collect::<Vec<_>>();
        let clone_result_drops = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(clone_id))
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(clone_result_drops, expected_clone_result);
    }
    for unit in observed.units {
        unit.validate().unwrap();
    }
}
