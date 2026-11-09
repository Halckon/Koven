//! Local result termination consumes the source token transported at initializer return.
use super::*;

const VIEW: &str = "fun view(source: String): borrow String from source = source\n";

#[test]
fn actual_source_token_survives_initializer_until_local_last_use() {
    let observed = observe(&format!(
        "{VIEW}fun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); println(item) }}"
    ));
    assert_units_admitted(&observed);
    super::normal_oracle::assert_normal_handoff(
        &observed,
        (110, 114),
        ((117, 128), (122, 127), (9, 23), (83, 88)),
        &[],
    );
    for unit in &observed.units {
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one actual source-to-binding transport")
        };
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one local binding")
        };
        assert_eq!(transport.binding(), binding.binding());
        assert_eq!(
            transport.source_loan(),
            unit.borrow_call_handoffs()[0].source_loan()
        );
        assert_eq!(
            transport.handoff_point(),
            unit.borrow_call_handoffs()[0].point()
        );
        assert_eq!(point_text(&observed, transport.point()), "println(item)");
        assert_eq!(transport.source_loan().target(), binding.origin());
        assert_eq!(
            transport.source_loan().call(),
            binding.source_loan().unwrap().call()
        );
        assert!(unit.loans().contains(transport.source_loan()));
        assert_ne!(transport.point(), transport.handoff_point());
        assert_eq!(transport.point(), unit.borrow_results().ends()[0].point());
        let UnitLoanTarget::Place(place) = transport.source_loan().target() else {
            panic!("owned root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), transport.point());
    }
}

#[test]
fn repeated_local_binding_names_select_only_their_own_call_and_token() {
    use std::{collections::BTreeSet, fmt::Write};

    let mut text = VIEW.to_owned();
    for index in 0..32 {
        writeln!(
            text,
            "fun run{index}(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); println(item) }}"
        )
        .unwrap();
    }
    let observed = observe(&text);
    assert_units_admitted(&observed);
    super::borrow_handoff::assert_handoff_identity(&observed);
    for unit in &observed.units {
        let bindings = unit.borrow_results().bindings();
        let handoffs = unit.borrow_call_handoffs();
        let transports = unit.borrow_binding_transports();
        assert_eq!(bindings.len(), 32);
        assert_eq!(handoffs.len(), bindings.len());
        assert_eq!(transports.len(), bindings.len());
        let mut calls = BTreeSet::new();
        for ((binding, handoff), transport) in bindings.iter().zip(handoffs).zip(transports) {
            assert_eq!(handoff.binding(), binding.binding());
            assert_eq!(transport.binding(), binding.binding());
            assert_eq!(handoff.source_loan(), transport.source_loan());
            assert_eq!(transport.handoff_point(), handoff.point());
            assert_eq!(handoff.source_loan().call(), binding.initializer());
            assert_eq!(
                handoff.source_loan().argument(),
                binding.source_loan().unwrap().argument()
            );
            assert_eq!(handoff.source_loan().target(), binding.origin());
            let end = unit
                .borrow_results()
                .ends()
                .iter()
                .find(|end| end.binding() == binding.binding())
                .expect("the original binding has its own end");
            assert_eq!(transport.point(), end.point());
            assert_eq!(point_text(&observed, transport.point()), "println(item)");
            assert!(calls.insert(binding.initializer()));
        }
    }
}

fn point_text(observed: &Observed, point: UnitDropPoint) -> &str {
    let span = match point {
        UnitDropPoint::AfterStatement(id) => observed
            .parsed
            .ast()
            .statements()
            .get(id.statement())
            .unwrap()
            .span(),
        UnitDropPoint::ControlTransfer(id) => observed
            .parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
            .span(),
        _ => panic!("actual local statement or control-transfer endpoint"),
    };
    assert_eq!(span.source_id(), observed.parsed.source_id());
    observed.sources.slice(span).unwrap()
}

#[test]
fn named_same_root_auxiliary_is_not_the_transported_source() {
    let observed = observe(
        "fun select(other: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(source = owner, other = owner); println(item) }",
    );
    assert_units_admitted(&observed);
    super::normal_oracle::assert_normal_handoff(
        &observed,
        (127, 131),
        ((134, 171), (150, 155), (26, 40), (100, 105)),
        &[((134, 171), (165, 170), (11, 24), (100, 105))],
    );
    for unit in &observed.units {
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one source token, not the auxiliary token")
        };
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one handoff")
        };
        assert_eq!(transport.source_loan(), handoff.source_loan());
        assert_ne!(transport.source_loan(), &handoff.auxiliary_loan_ends()[0]);
        assert_eq!(point_text(&observed, transport.point()), "println(item)");
    }
}

#[test]
fn descendant_binding_ends_before_the_original_source_is_released() {
    for child in ["parent", "view(parent)"] {
        let observed = observe(&format!(
            "{VIEW}fun run(): Unit {{ val owner = \"kept\"; borrow val parent = view(owner); borrow val child = {child}; println(child) }}"
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            let [transport] = unit.borrow_binding_transports() else {
                panic!("only owned-root parent source is transported")
            };
            let [parent, child] = unit.borrow_results().bindings() else {
                panic!("two bindings")
            };
            assert_eq!(child.parent(), Some(parent.binding()));
            assert_eq!(transport.binding(), parent.binding());
            assert_eq!(point_text(&observed, transport.point()), "println(child)");
            let ends: Vec<_> = unit
                .borrow_results()
                .ends()
                .iter()
                .map(|end| (end.binding(), end.point()))
                .collect();
            assert_eq!(
                ends,
                [
                    (child.binding(), transport.point()),
                    (parent.binding(), transport.point())
                ]
            );
            let UnitLoanTarget::Place(place) = transport.source_loan().target() else {
                panic!("owned root")
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), transport.point());
        }
    }
}

#[test]
fn distinct_bindings_keep_distinct_tokens_even_for_the_same_root() {
    let observed = observe(&format!(
        "{VIEW}fun run(): Unit {{ val owner = \"kept\"; borrow val first = view(owner); borrow val second = view(owner); println(first); println(second) }}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [first, second] = unit.borrow_binding_transports() else {
            panic!("two independently consumed tokens")
        };
        assert_ne!(first.source_loan().call(), second.source_loan().call());
        assert_ne!(
            first.source_loan().argument(),
            second.source_loan().argument()
        );
        assert_eq!(first.source_loan().target(), second.source_loan().target());
        assert_ne!(first.binding(), second.binding());
        assert_eq!(point_text(&observed, first.point()), "println(first)");
        assert_eq!(point_text(&observed, second.point()), "println(second)");
        let UnitLoanTarget::Place(place) = first.source_loan().target() else {
            panic!("owned root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), second.point());
    }
}

#[test]
fn established_binding_releases_its_source_on_return_prefix_abandonment() {
    let observed = observe(&format!(
        "{VIEW}fun observe(item: String, own n: Int): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); observe(item, return) }}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one established binding")
        };
        assert!(matches!(
            transport.point(),
            UnitDropPoint::ControlTransfer(_)
        ));
        assert_eq!(point_text(&observed, transport.point()), "return");
        assert_eq!(unit.borrow_results().ends()[0].point(), transport.point());
        assert!(
            unit.borrow_call_prefix_ends().is_empty(),
            "initializer already returned"
        );
        let UnitLoanTarget::Place(place) = transport.source_loan().target() else {
            panic!("owned root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), transport.point());
    }
}

#[test]
fn incomplete_initializer_or_uncovered_group_does_not_gain_a_transport() {
    for initializer in ["select(owner, return)", "(select(owner, 0))"] {
        let observed = observe(&format!(
            "fun select(source: String, own n: Int): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = {initializer}; println(item) }}"
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(unit.borrow_binding_transports().is_empty());
        }
    }
}

#[test]
fn error_and_deferred_revoke_an_already_completed_transport_prefix() {
    let prefix = format!(
        "{VIEW}fun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); println(item)"
    );
    let good = observe(&format!("{prefix} }}"));
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.borrow_binding_transports().len(), 1);
    }
    for tail in [
        "; val moved = owner; println(owner)",
        "; var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 })",
    ] {
        let observed = observe(&format!("{prefix}{tail} }}"));
        for unit in &observed.units {
            assert!(!unit.diagnostics().is_empty() || !unit.deferred().is_empty());
            assert!(unit.borrow_binding_transports().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn unused_binding_consumes_its_transported_token_after_initialization() {
    let observed = observe(&format!(
        "{VIEW}fun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner) }}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one initialized but unused local")
        };
        assert_eq!(
            point_text(&observed, transport.point()),
            "borrow val item = view(owner)"
        );
        assert_ne!(transport.point(), transport.handoff_point());
        let UnitLoanTarget::Place(place) = transport.source_loan().target() else {
            panic!("owned root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), transport.point());
    }
}

#[test]
fn inherited_binding_token_survives_cfg_clones_and_continuing_merges() {
    for control in [
        "if (flag) { println(item); return }",
        "while (flag) { println(item); if (flag) { return } }",
    ] {
        let body = format!(
            "{{ val owner = \"kept\"; borrow val item = view(owner); {control}; println(item) }}"
        );
        let observed = observe(&format!("{VIEW}fun run(flag: Boolean): Unit {body}"));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            let [returned, continued] = unit.borrow_binding_transports() else {
                panic!("the same token ends on both mutually exclusive reachable exits")
            };
            assert_eq!(returned.source_loan(), continued.source_loan());
            assert_eq!(returned.binding(), continued.binding());
            assert!(matches!(
                returned.point(),
                UnitDropPoint::ControlTransfer(_)
            ));
            assert_eq!(point_text(&observed, returned.point()), "return");
            // CFG disables the straight-line last-use optimization. Preserve that
            // existing scope endpoint instead of inventing an earlier source end.
            assert!(matches!(
                continued.point(),
                UnitDropPoint::AfterStatement(_)
            ));
            assert_eq!(point_text(&observed, continued.point()), body);
            let UnitLoanTarget::Place(place) = returned.source_loan().target() else {
                panic!("owned root")
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .map(|drop| drop.point())
                .collect();
            assert_eq!(drops, [returned.point(), continued.point()]);
        }
    }
}
