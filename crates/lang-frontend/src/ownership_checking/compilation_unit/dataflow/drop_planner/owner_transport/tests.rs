//! Compare the exact same admitted checker with observation enabled/disabled.
mod branch_boundaries;
mod choice_admission;
mod choice_boundaries;
mod choice_branch_local;
mod choice_capture_isolation;
mod combination;
mod control_tail;
mod loop_boundaries;

use super::super::super::*;
use super::super::plan_with_owner_observation;
use crate::ownership_checking::compilation_unit::formed_tests::{Fixture, Parts};

fn compare(fixture: &Fixture, parts: &Parts) {
    compare_and_inspect(fixture, parts, |_| {});
}

fn compare_and_inspect(
    fixture: &Fixture,
    parts: &Parts,
    inspect: impl FnOnce(&Checker<'_>),
) -> usize {
    let inputs = fixture.inputs();
    let fields = traversal::collect_field_mutability(&inputs, fixture.names.names()).unwrap();
    let constructions = collect_construction_descriptors(
        fixture.typed.types().constructions(),
        fixture.names.names().index().source_units().len(),
    )
    .unwrap();
    let empty = BTreeMap::new();
    let catalog = codes::catalog().unwrap();
    let codes = Codes {
        use_after_move: catalog.resolve(codes::USE_AFTER_MOVE).unwrap(),
        partial_move: catalog.resolve(codes::PARTIAL_MOVE).unwrap(),
        borrowed_move: catalog.resolve(codes::MOVE_FROM_BORROWED_BINDING).unwrap(),
        loan_conflict: catalog.resolve(codes::LOAN_CONFLICT).unwrap(),
        immutable_inout: catalog.resolve(codes::IMMUTABLE_INOUT_PLACE).unwrap(),
        container_element_move: catalog.resolve(codes::MOVE_FROM_CONTAINER_ELEMENT).unwrap(),
        borrowed_closure_escape: catalog.resolve(codes::BORROWED_CLOSURE_ESCAPE).unwrap(),
        illegal_owned_capture: catalog.resolve(codes::ILLEGAL_OWNED_CAPTURE).unwrap(),
        non_transferable_delivery: catalog.resolve(codes::NON_TRANSFERABLE_DELIVERY).unwrap(),
    };
    let mut diagnostics = Vec::new();
    let mut loans = Vec::new();
    let mut deliveries = Vec::new();
    let mut receivers = Vec::new();
    let mut conditional = Vec::new();
    let mut rc = Vec::new();
    let mut construction_plans = Vec::new();
    let mut assertions = Vec::new();
    let mut callable_sources = callable_provenance::Collection::default();
    let capture_index = closure::capture_index(&parts.capture.captures, fixture.typed.types());
    let mut checker = Checker::new(
        &fixture.sources,
        &fixture.parsed[0],
        fixture.source(0),
        fixture.names.names(),
        fixture.typed.types(),
        &parts.bindings,
        &fields,
        &parts.arguments,
        &parts.receivers,
        &capture_index,
        &parts.capture.closures,
        &parts.capture.transferabilities,
        constructions.get(&fixture.source(0)).unwrap_or(&empty),
        codes,
        &mut diagnostics,
        &mut loans,
        &mut deliveries,
        &mut receivers,
        &mut conditional,
        &mut rc,
        &mut construction_plans,
        &mut assertions,
        &mut callable_sources,
    )
    .unwrap();
    checker.range_producers = crate::ownership_checking::range_producer::collect_unit(
        &fixture.sources,
        &inputs,
        fixture.names.names(),
        fixture.typed.types(),
    )
    .unwrap();
    let drop_analysis = checker.run().unwrap();
    assert!(drop_analysis.deferred.is_empty());
    assert!(checker.primitive_deferred.is_empty());
    inspect(&checker);
    let mut observed = plan_with_owner_observation(&checker, true).unwrap();
    let disabled = plan_with_owner_observation(&checker, false).unwrap();
    assert!(!observed.owner_observations.is_empty());
    assert!(disabled.owner_observations.is_empty());
    observed.owner_observations.clear();
    assert_eq!(
        observed, disabled,
        "all legacy cleanup fields, their identities and order must be identical"
    );
    observed.string_clone_loan_ends.len()
}

#[test]
fn owner_observation_never_changes_existing_cleanup_products() {
    for text in [
        r#"fun run(): Unit { var a = "left"; var b = "right"; swap(&a, &b); val old = replace(&a, "new"); val kept = (b) }"#,
        r#"fun run(): Unit { var a = "old"; val incoming = "new"; val old = replace(&a, incoming) }"#,
        r#"fun run(flag: Boolean): Unit { var a = "old"; val old = replace(&a, "new"); if (flag) { val x = "branch" } }"#,
        r#"fun run(flag: Boolean): Unit { var a = "old"; val old = replace(&a, "new"); while (flag) { break } }"#,
        r#"fun make(): String = "opaque"
fun run(): Unit { var a = "old"; val old = replace(&a, "new"); val opaque = make() }"#,
        r#"fun run(): Unit { var a = "old"; val old = replace(&a, "new"); val callback = move { a + old }; callback() }"#,
        r#"fun run(): Unit { var a = "old"; val old = replace(&a, "new"); val callback = move { a + old }; val forwarded = (callback) }"#,
        r#"fun run(own a: String): Unit { val callback = move { a } }"#,
    ] {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}

#[test]
fn owner_observation_preserves_exclusive_return_prefix_events() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nreplace(&a, return)\n}",
    )]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    assert_eq!(shared.exclusive_call_prefix_ends().len(), 1);
    assert!(
        shared
            .owner_observations()
            .iter()
            .all(|observation| matches!(
                observation.status(),
                crate::ownership_checking::UnitOwnerObservationStatus::Unsupported { .. }
            ))
    );
    let parts = fixture.parts(&shared);
    compare(&fixture, &parts);
}

#[test]
fn shared_capture_observation_preserves_all_existing_cleanup_products() {
    use crate::ownership_checking::compilation_unit::owner_transport_tests::shared_capture::{
        BOUNDARIES, CROSS_IF_SOURCES, REPLACED_SHARED_SOURCE, RETIRE_IF_SOURCES, SOURCES,
    };
    for source in SOURCES
        .iter()
        .chain(BOUNDARIES)
        .chain(CROSS_IF_SOURCES)
        .chain(RETIRE_IF_SOURCES)
        .chain(std::iter::once(&REPLACED_SHARED_SOURCE))
    {
        let fixture = Fixture::new(&[("root.ko", source)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn branch_local_shared_capture_preserves_full_cleanup_analysis() {
    use crate::ownership_checking::compilation_unit::owner_transport_tests::shared_branch_local::{
        BOUNDARIES, SOURCES,
    };
    for source in SOURCES.iter().chain(BOUNDARIES) {
        let fixture = Fixture::new(&[("root.ko", source)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn unit_return_observation_keeps_full_legacy_analysis() {
    use crate::ownership_checking::compilation_unit::owner_transport_tests::shared_return::{
        BOUNDARIES, LOCAL_SOURCES, SOURCES,
    };
    for source in SOURCES.iter().chain(BOUNDARIES).chain(LOCAL_SOURCES) {
        let fixture = Fixture::new(&[("root.ko", source)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        compare(&fixture, &fixture.parts(&owned));
    }
    let source = crate::ownership_checking::compilation_unit::owner_transport_tests::branch_boundaries::source(r#"if (flag) { return } else { val local = "right" }"#);
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn owned_capture_return_preserves_full_cleanup_analysis() {
    use crate::ownership_checking::compilation_unit::owner_transport_tests::capture_branch_release::{
        BOTH_RETURN_RELEASE, RETURN_RELEASE,
    };
    for source in [RETURN_RELEASE, BOTH_RETURN_RELEASE] {
        let fixture = Fixture::new(&[("root.ko", source)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn terminal_if_observation_preserves_full_legacy_cleanup_analysis() {
    use crate::ownership_checking::compilation_unit::owner_transport_tests::terminal_return::{
        BOUNDARIES, SOURCES,
    };
    for source in SOURCES
        .iter()
        .map(|(source, _, _)| *source)
        .chain(BOUNDARIES.iter().copied())
    {
        let fixture = Fixture::new(&[("root.ko", source)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn owner_observation_preserves_nonempty_string_clone_ends() {
    for (text, ends) in [
        (r#"fun copy(text: String): String = text.clone()"#, 1),
        (r#"fun copy(): String = ("nested".clone()).clone()"#, 2),
        (
            r#"fun copy(text: String, flag: Boolean): Unit { while(flag) { val copy = text.clone(); println(copy); break } }"#,
            1,
        ),
        (
            r#"fun select(first: String, second: String): String = first.clone()
fun copy(): String { return select("inner".clone(), return "done").clone() }"#,
            2,
        ),
    ] {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        assert_eq!(shared.string_clone_loan_ends().len(), ends);
        let parts = fixture.parts(&shared);
        assert_eq!(compare_and_inspect(&fixture, &parts, |_| {}), ends);
    }
}

#[test]
fn owner_observation_preserves_simultaneously_nonempty_receiver_and_clone_end_tables() {
    let text = "class Node { fun touch(text: String, own next: Int): Int = next }\nfun run(node: Node): Unit { val seed = \"kept\"; node.touch(seed.clone(), 0); node.touch(\"prefix\".clone(), return) }";
    let fixture = Fixture::new(&[("root.ko", text)]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    assert_eq!(shared.receiver_call_return_ends().len(), 1);
    assert_eq!(shared.receiver_call_prefix_ends().len(), 1);
    assert_eq!(shared.string_clone_loan_ends().len(), 2);
    assert_eq!(
        compare_and_inspect(&fixture, &fixture.parts(&shared), |_| {}),
        2
    );
}
