//! A call prefix still owns evaluated operands until the call commits.
use super::{UnitDropPoint, UnitDropTarget, constants_tests::analyze};

#[test]
fn an_aborting_return_operand_has_no_return_cleanup() {
    let owned = analyze(
        "package a\nconst val TEXT = \"hi\"\nfun stop(): Nothing = stop()\nfun call(text: String, number: Int): Unit {}\nfun run(flag: Boolean): Int { val result = call(TEXT, if (flag) { return stop() } else { 0 })\nreturn 0 }",
    );
    assert!(owned.diagnostics().is_empty());
    assert!(
        !owned
            .drops()
            .iter()
            .any(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
    );
}

#[test]
fn consumed_named_value_is_cleaned_as_an_uncommitted_argument() {
    let owned = analyze(
        "package a\nconst val TEXT = \"hi\"\nfun call(own text: String, number: Int): Unit {}\nfun run(flag: Boolean): Unit { val local = TEXT\nval result = call(local, if (flag) { return } else { 0 }) }",
    );
    assert!(owned.diagnostics().is_empty());
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert!(matches!(drops[0].target(), UnitDropTarget::Temporary(_)));
    assert!(
        !owned
            .drops()
            .iter()
            .any(|drop| matches!(drop.point(), UnitDropPoint::CallReturn(_)))
    );
}

#[test]
fn abandoned_nested_prefix_cleans_new_local_then_reverse_temporaries_then_old_local() {
    let (sources, owned) = super::constants_tests::analyze_with_sources(
        "package a\nconst val TEXT = \"hi\"\nfun call(a: String, b: String, c: String, n: Unit): Unit {}\nfun view(s: String, n: Int): Unit {}\nfun run(flag: Boolean): Unit { val old = \"old\"\nval result = call(old, TEXT, TEXT, if (flag) { val new = \"new\"\nval nested = view(new, if (flag) { return } else { 0 }) } else {}) }",
    );
    assert!(owned.diagnostics().is_empty());
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 4, "{:?}", owned.drops());
    assert_eq!(sources.slice(drops[0].value_origin()).unwrap(), "new");
    assert_eq!(sources.slice(drops[3].value_origin()).unwrap(), "old");
    for drop in &drops[1..3] {
        assert!(matches!(drop.target(), UnitDropTarget::Temporary(_)));
    }
    assert!(
        drops[1].value_origin().start() > drops[2].value_origin().start(),
        "reverse evaluation order"
    );
}

#[test]
fn abandoned_call_prefix_drops_borrow_and_value_operands_on_the_exit_edge() {
    for value in ["TEXT", "\"hi\""] {
        for mode in ["", "own "] {
            for (body, transfers, returns) in [
                (
                    "val result = call(VALUE, if (flag) { return } else { 0 })",
                    1,
                    1,
                ),
                (
                    "loop { val result = call(VALUE, if (flag) { break } else { 0 })\nbreak }",
                    1,
                    1,
                ),
                (
                    "loop { val result = call(VALUE, if (flag) { continue } else { 0 })\nbreak }",
                    1,
                    1,
                ),
                (
                    "val result = call(VALUE, if (flag) { stop() } else { 0 })",
                    0,
                    1,
                ),
                ("val result = call(VALUE, 0)", 0, 1),
                ("val result = call(VALUE, stop())", 0, 0),
            ] {
                let body = body.replace("VALUE", value);
                let owned = analyze(&format!(
                    "package a\nconst val TEXT = \"hi\"\nfun stop(): Nothing = stop()\nfun call({mode}text: String, number: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ {body} }}"
                ));
                assert!(
                    owned.diagnostics().is_empty(),
                    "{body}: {:?}",
                    owned.diagnostics()
                );
                assert!(owned.deferred().is_empty());
                let drops = owned
                    .drops()
                    .iter()
                    .filter(|drop| matches!(drop.target(), UnitDropTarget::Temporary(_)))
                    .collect::<Vec<_>>();
                let control = drops
                    .iter()
                    .filter(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
                    .count();
                let normal = drops
                    .iter()
                    .filter(|drop| matches!(drop.point(), UnitDropPoint::CallReturn(_)))
                    .count();
                assert_eq!(control, transfers, "{mode}{body}: {:?}", owned.drops());
                assert_eq!(
                    normal,
                    if mode.is_empty() { returns } else { 0 },
                    "{mode}{body}: {:?}",
                    owned.drops()
                );
                assert_eq!(drops.len(), control + normal, "no duplicate cleanup");
            }
        }
    }
}

#[test]
fn inner_loop_break_preserves_an_outer_call_prefix_owner() {
    let owned = analyze(
        "package a\nconst val TEXT = \"hi\"\nfun call(text: String, number: Int): Unit {}\nfun run(flag: Boolean): Unit { val result = call(TEXT, if (flag) { loop { break }\n0 } else { 1 }) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| matches!(drop.target(), UnitDropTarget::Temporary(_)))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "only the eventual call return owns cleanup");
    assert!(matches!(drops[0].point(), UnitDropPoint::CallReturn(_)));
}
