//! Compare the exact same admitted checker with observation enabled/disabled.
mod branch_boundaries;
mod choice_admission;
mod choice_alias_chain;
mod choice_boundaries;
mod choice_branch_local;
mod choice_capture_isolation;
mod choice_conditions;
mod choice_dual_forwarding;
mod choice_forwarding;
mod choice_installed_first;
mod choice_installed_forwarding;
mod choice_predicates;
mod choice_replace;
mod choice_replace_forwarding;
mod choice_replace_groups;
mod choice_result_groups;
mod closed_while;
mod closed_while_conditional;
mod closed_while_fallthrough;
mod combination;
mod control_tail;
mod invariant_break;
mod invariant_break_suffix;
mod invariant_continue;
mod invariant_mixed_join;
mod invariant_normal_join;
mod invariant_split_continue;
mod invariant_split_exits;
mod invariant_suffix;
mod invariant_while;
mod loop_boundaries;
mod open_loop_return;
mod parameter_open_loop;

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
    compare_and_inspect_control_mode(fixture, parts, false, inspect)
}

fn compare_and_inspect_control_mode(
    fixture: &Fixture,
    parts: &Parts,
    constant_control: bool,
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
    checker.constant_control = constant_control;
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

mod closed_while_transfers;

mod loop_negated_conditions;

mod invariant_continue_suffix;

mod loop_input_alias;

#[test]
fn owner_observation_preserves_exclusive_loop_exit_prefix_events() {
    for jump in ["break", "continue"] {
        let text = format!(
            "class Item(val n: Int)\nfun run(): Unit {{ loop {{ var a = Item(1); replace(&a, {jump}) }} }}"
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        assert_eq!(shared.exclusive_call_prefix_ends().len(), 1);
        compare(&fixture, &fixture.parts(&shared));
    }
}

#[test]
fn owner_observation_preserves_all_three_borrow_publication_tables() {
    let source = "fun view(source: String, aux: String): borrow String from source = source\nfun prefix(source: String, tail: Unit): borrow String from source = source\nfun sink(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; val aux = \"aux\"; sink(aux); borrow val item = view(owner, aux); println(item) }\nfun abandon(): Unit { val root = \"root\"; borrow val item = prefix(root, return) }";
    let fixture = Fixture::new(&[("root.ko", source)]);
    let shared = fixture.shared();
    shared.clone().validate().unwrap();
    assert_eq!(shared.direct_call_loan_ends().len(), 1);
    assert_eq!(shared.borrow_call_handoffs().len(), 1);
    assert_eq!(
        shared.borrow_call_handoffs()[0].auxiliary_loan_ends().len(),
        1
    );
    assert_eq!(shared.borrow_call_prefix_ends().len(), 1);
    compare(&fixture, &fixture.parts(&shared));
}

#[test]
fn owner_observation_preserves_interpolated_string_loan_ends_and_input_cleanup() {
    let source = r#"fun read(item: String): Unit {}
fun run(flag: Boolean): Unit { read("${"inner"}-${if (flag) { return } else { 0 }}") }"#;
    let fixture = Fixture::new(&[("root.ko", source)]);
    let shared = fixture.shared();
    shared.clone().validate().unwrap();
    assert_eq!(shared.direct_call_loan_ends().len(), 1);
    assert_eq!(shared.drops().len(), 3);
    compare(&fixture, &fixture.parts(&shared));
}

#[test]
fn owner_observation_preserves_grouped_initializer_handoff_and_unwind() {
    let source = "fun view(source: String): borrow String from source = source\nfun select(source: String, tail: Unit): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = ((view(owner))); println(item) }\nfun abandon(): Unit { val owner = \"kept\"; borrow val item = ((select(owner, return))) }";
    let fixture = Fixture::new(&[("root.ko", source)]);
    let shared = fixture.shared();
    shared.clone().validate().unwrap();
    assert_eq!(shared.borrow_call_handoffs().len(), 1);
    assert_eq!(shared.borrow_binding_transports().len(), 1);
    assert_eq!(shared.borrow_call_prefix_ends().len(), 1);
    compare(&fixture, &fixture.parts(&shared));
}

#[test]
fn temporary_auxiliary_handoff_preserves_complete_owner_analysis() {
    for tail in ["\"ready\"", "if (flag) { return } else { \"ready\" }"] {
        let source = format!(
            "fun select(source: String, aux: String): borrow String from source = source\nfun run(flag: Boolean): Unit {{ val owner = \"kept\"; borrow val item = select(owner, {tail}); println(item) }}"
        );
        let fixture = Fixture::new(&[("root.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let [handoff] = owned.borrow_call_handoffs() else {
            panic!("one normal handoff")
        };
        assert_eq!(handoff.auxiliary_loan_ends().len(), 1);
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn unrelated_iteration_forwarding_preserves_complete_owner_analysis() {
    for (source, count) in [
        (
            "fun view(source: String): borrow String from source = source\nfun wrap(input: String): borrow String from input { for (_ in listOf(1)) { for (_ in listOf(2)) { return view(input) } }; return view((input)) }",
            2,
        ),
        (
            "fun view(source: List<Int>): borrow List<Int> from source = source\nfun wrap(input: List<Int>): borrow List<Int> from input { borrow val alias = input; for (_ in alias) { return view(input) }; return view((input)) }",
            1,
        ),
    ] {
        let fixture = Fixture::new(&[("root.ko", source)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        assert_eq!(owned.borrow_forward_transports().len(), count);
        compare(&fixture, &fixture.parts(&owned));
    }
}

mod loop_input_alias_conditional;

#[test]
fn exclusive_borrow_auxiliary_preserves_all_cleanup_with_owner_observation() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun view(source: String, inout aux: String, own next: Int): borrow String from source = source
fun run(flag: Boolean): Unit { val owner = "kept"; var other = "other"; borrow val item = view(owner, &other, if (flag) { return } else { 0 }); println(item) }"#,
    )]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    let [handoff] = shared.borrow_call_handoffs() else {
        panic!("real handoff");
    };
    let [loan] = handoff.auxiliary_loan_ends() else {
        panic!("real exclusive token");
    };
    assert_eq!(loan.kind(), crate::ownership_checking::LoanKind::Exclusive);
    assert_eq!(shared.borrow_call_prefix_ends().len(), 1);
    compare(&fixture, &fixture.parts(&shared));
}

mod loop_replace_break_baseline;

mod loop_replace_break;
#[test]
fn loan_action_observer_preserves_all_existing_planner_products() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }",
    )]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    let parts = fixture.parts(&shared);
    compare_and_inspect(&fixture, &parts, |checker| {
        let mut observed = super::super::plan_with_observations(checker, true, true).unwrap();
        let disabled = super::super::plan_with_observations(checker, true, false).unwrap();
        assert!(!observed.loan_action_observations.is_empty());
        assert!(disabled.loan_action_observations.is_empty());
        observed.loan_action_observations.clear();
        assert_eq!(observed, disabled);
    });
}

#[test]
fn loan_receiver_observer_preserves_nonempty_legacy_return_projection() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch(); node.touch() }",
    )]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    compare_and_inspect(&fixture, &fixture.parts(&shared), |checker| {
        let mut observed = super::super::plan_with_observations(checker, true, true).unwrap();
        let disabled = super::super::plan_with_observations(checker, true, false).unwrap();
        assert_eq!(observed.receiver_call_return_ends.len(), 2);
        assert_eq!(
            observed.receiver_call_return_ends,
            disabled.receiver_call_return_ends
        );
        assert!(
            observed.loan_action_observations[0]
                .regions()
                .iter()
                .any(|region| region.coverage()
                    == crate::ownership_checking::UnitLoanActionCoverage::Complete)
        );
        assert!(disabled.loan_action_observations.is_empty());
        observed.loan_action_observations.clear();
        assert_eq!(observed, disabled);
    });
}

#[test]
fn loan_named_observer_preserves_all_existing_planner_products() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source); inspect(source) }",
    )]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    compare_and_inspect(&fixture, &fixture.parts(&shared), |checker| {
        let mut observed = super::super::plan_with_observations(checker, true, true).unwrap();
        let disabled = super::super::plan_with_observations(checker, true, false).unwrap();
        assert_eq!(observed.direct_call_loan_ends.len(), 2);
        assert_eq!(
            observed.direct_call_loan_ends,
            disabled.direct_call_loan_ends
        );
        assert!(
            observed.loan_action_observations[0]
                .regions()
                .iter()
                .any(|region| region.protocol_version() == 3
                    && region.coverage()
                        == crate::ownership_checking::UnitLoanActionCoverage::Complete)
        );
        assert!(disabled.loan_action_observations.is_empty());
        observed.loan_action_observations.clear();
        assert_eq!(observed, disabled);
    });
}

#[test]
fn result_action_observer_preserves_nonempty_handoff_and_end_products() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun view(source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = view(parent); println(child) }",
    )]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    compare_and_inspect(&fixture, &fixture.parts(&shared), |checker| {
        let mut observed = super::super::plan_with_observations(checker, true, true).unwrap();
        let disabled = super::super::plan_with_observations(checker, true, false).unwrap();
        assert_eq!(observed.borrow_call_handoffs.len(), 1);
        assert_eq!(observed.borrow_ends.len(), 2);
        assert!(
            observed.loan_action_observations[0]
                .actions()
                .iter()
                .any(|action| matches!(
                    action,
                    crate::ownership_checking::UnitLoanAction::BorrowHandoffCommitted(_)
                ))
        );
        assert!(disabled.loan_action_observations.is_empty());
        observed.loan_action_observations.clear();
        assert_eq!(observed, disabled);
    });
}

mod owner_loan_combination;
#[test]
fn consumer_dependency_observer_preserves_all_planner_snapshots() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }",
    )]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    compare_and_inspect(&fixture, &fixture.parts(&shared), |checker| {
        let mut observed = super::super::plan_with_observations(checker, true, true).unwrap();
        let disabled = super::super::plan_with_observations(checker, true, false).unwrap();
        assert_eq!(observed.borrow_call_transports.len(), 1);
        assert!(
            observed.loan_action_observations[0]
                .regions()
                .iter()
                .any(|region| region.protocol_version() == 4
                    && region.coverage()
                        == crate::ownership_checking::UnitLoanActionCoverage::Complete)
        );
        assert!(observed.loan_action_observations[0].actions().iter().any(|action| matches!(action, crate::ownership_checking::UnitLoanAction::BorrowConsumerRetired(retired) if retired.transfer().is_some())));
        assert!(disabled.loan_action_observations.is_empty());
        observed.loan_action_observations.clear();
        assert_eq!(observed, disabled);
    });
}

mod replace_break_call;

mod replace_break_return;

mod replace_break_grouped_condition;

mod replace_break_negated_condition;

mod replace_break_boolean_comparison;

mod replace_break_short_circuit;

mod replace_break_leaf_not;

mod replace_break_pair_call;

mod replace_break_alias_return;

mod replace_break_replace_return;

mod normal_backedge;

mod qualified_owner_edges;

mod general_control_frontier;

mod sequential_closed_transactions;

mod multi_root_stages;

mod root_write_receipts;
