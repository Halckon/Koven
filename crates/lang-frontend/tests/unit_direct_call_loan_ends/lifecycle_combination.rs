//! Coexistence contracts for the integrated receiver and String.clone tables.
use super::receiver_return::{NODE, assert_returns};
use super::*;

#[test]
fn completed_clones_survive_receiver_prefixes_without_visiting_unreachable_suffixes() {
    for (open, close, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let normal = "node.touch(seed.clone(), 0)";
        let prefix = format!("node.touch(\"prefix\".clone(), {jump})");
        let text = format!(
            "{NODE}fun run(node: Node, flag: Boolean): Unit {{ val seed = \"seed\"; {normal}; {open} {prefix}; node.touch(\"dead\".clone(), 0) {close} }}"
        );
        let observed = assert_returns(&text, &[(normal, "node")]);
        for (entry, unit) in observed.units.iter().enumerate() {
            let source = observed.unit_names[entry].names().index().source_units()[0].id();
            let expression_text = |id: UnitExpressionId| {
                assert_eq!(id.source_unit(), source);
                observed
                    .sources
                    .slice(
                        observed
                            .parsed
                            .ast()
                            .expressions()
                            .get(id.expression())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
            };
            let [end] = unit.receiver_call_prefix_ends() else {
                panic!("only the entered outer receiver has a prefix end");
            };
            assert_eq!(expression_text(end.receiver().call()), prefix);
            let UnitDropPoint::ControlTransfer(transfer) = end.point() else {
                panic!("actual control transfer");
            };
            assert_eq!(expression_text(transfer), jump);
            assert!(unit.receiver_facts().contains(end.receiver()));
            assert_eq!(unit.string_clone_loan_ends().len(), 2);
            for (clone_end, (call, receiver)) in unit.string_clone_loan_ends().iter().zip([
                ("seed.clone()", "seed"),
                ("\"prefix\".clone()", "\"prefix\""),
            ]) {
                assert_eq!(expression_text(clone_end.loan().call()), call);
                assert_eq!(expression_text(clone_end.loan().argument()), receiver);
                assert_eq!(clone_end.loan().kind(), LoanKind::Shared);
                assert_eq!(
                    clone_end.point(),
                    UnitDropPoint::CallReturn(clone_end.loan().call())
                );
                assert!(unit.loans().contains(clone_end.loan()));
            }
            assert_eq!(
                observed.unit_types[entry].types().string_operations().len(),
                3,
                "the unreachable clone has a descriptor but no completion event"
            );
        }
    }
}
