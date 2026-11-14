//! Every direct old capability is first obtained from real source, then corrupted.
use super::*;
use crate::ownership_checking::UnitOwnerObservation;
fn available(o: &UnitOwnerObservation) -> [bool; 15] {
    [
        o.loop_observation_v1().is_some(),
        o.invariant_loop_observation_v1().is_some(),
        o.invariant_break_observation_v1().is_some(),
        o.invariant_normal_join_observation_v1().is_some(),
        o.invariant_mixed_join_observation_v1().is_some(),
        o.invariant_split_continue_observation_v1().is_some(),
        o.invariant_split_exit_observation_v1().is_some(),
        o.open_loop_observation_v1().is_some(),
        o.closed_loop_batch_v1().is_some(),
        o.counter_tail_evidence_v1().is_some(),
        o.counter_batch_evidence_v1().is_some(),
        o.multi_root_boundary_v1().is_some(),
        o.control_bridge_evidence_v1().is_some(),
        o.value_observation_v1().is_some(),
        o.multi_root_normal_transport_v1().is_some(),
    ]
}
// counter_batch.rs real two-member source, without corruption.
fn counter(i: usize) -> String {
    format!(
        r#"var n{i} = n; var g{i} = n{i} > 0; var a{i} = "a"; var b{i} = "b";
    while(g{i}) {{ val x = replace(&a{i}, "x"); val y = replace(&b{i}, "y"); n{i} = n{i} - 1; g{i} = n{i} > 0 }};"#
    )
}
fn cases() -> Vec<(usize, String)> {
    // Exact admitted shapes from invariant_{while,break,normal_join,mixed_join,
    // split_continue,split_exits} source regressions. No extra unused parameter.
    let invariant = |tail: &str| {
        let params = if tail.contains("other") {
            "flag: Boolean, other: Boolean"
        } else {
            "flag: Boolean"
        };
        let body = if tail.is_empty() {
            "val local = \"inner\"".to_owned()
        } else {
            format!("val local = \"inner\"; {tail}")
        };
        format!(
            "fun run({params}) {{ val held = \"outer\"; while(flag) {{ {body} }}; val retired = held }}"
        )
    };
    let mut cases = vec![
        (0, "fun run(flag: Boolean) { while(flag) { val local=\"inner\" } }".into()),
        (1, invariant("")), (2, invariant("break")),
        (3, invariant("if(other) {} else {}")),
        (4, invariant("if(other) {} else { continue }")),
        (5, invariant("if(other) { continue } else { continue }")),
        (6, invariant("if(other) { continue } else { break }")),
        (7, r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    val forwarded = held
    return forwarded
}"#.into()),
        (8, r#"fun run(flag: Boolean) { while(flag) { val first = "first" }
while(flag) { val second = "second"; break } }"#.into()),
        // root_tokens/tests.rs single_counter_candidate, step=1, empty suffix.
        (9, r#"fun run(n: Int) { var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b"; while(g0) { val x0 = replace(&a0, "x"); val y0 = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0; }; }"#.into()),
        (10, format!("fun run(n: Int) {{ {} {} }}", counter(0),counter(1))),
        (11, "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val outA = a; val outB = b }".into()),
        // counter_segment.rs bridge_preserves... fresh=false, alias=false.
        (12, r#"fun run(n: Int) {
            var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
            while(g0) { val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 };
            var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) { val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }
        }"#.into()),
        (13, r#"fun run(flag: Boolean): Unit { val chosen = if (flag) { ((("left"))) } else ((("right"))) }"#.into()),
    ];
    cases.push((14, cases[11].1.clone()));
    cases
}

#[test]
fn private_chain_old_getter_fixtures_have_real_products() {
    let mut failures = Vec::new();
    for (which, text) in cases() {
        let fixture = Fixture::new(&[("old-product.ko", &text)]);
        let owned = fixture.shared();
        if !owned.diagnostics().is_empty() {
            failures.push(format!("case {which}: {:?}", owned.diagnostics()));
        }
        if !owned
            .owner_observations()
            .iter()
            .any(|o| available(o)[which])
        {
            failures.push(format!("missing original product {which}: {text}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ci_only_private_chain_excludes_all_old_getters_on_mixed_observations() {
    let chain_source = source("", "", "n");
    let chain_fixture = Fixture::new(&[("chain.ko", &chain_source)]);
    let chain_owned = chain_fixture.shared();
    let chain = chain_owned
        .owner_observations()
        .iter()
        .find_map(|o| o.private_control_chain.clone())
        .unwrap();
    for (which, text) in cases() {
        let fixture = Fixture::new(&[("old-product.ko", &text)]);
        let owned = fixture.shared();
        assert!(
            owned.diagnostics().is_empty(),
            "case {which}: {:?}",
            owned.diagnostics()
        );
        let original = owned
            .owner_observations()
            .iter()
            .find(|o| available(o)[which])
            .unwrap_or_else(|| panic!("missing original product {which}: {text}"));
        let mut mixed = original.clone();
        mixed.private_control_chain = Some(chain.clone());
        assert!(!mixed.is_consistent(), "case {which}");
        assert!(mixed.control_chain_bodies_v1().is_none(), "case {which}");
        assert_eq!(available(&mixed), [false; 15], "case {which}");
        assert!(mixed.replace_break_observation_v1().is_none());
    }
}
