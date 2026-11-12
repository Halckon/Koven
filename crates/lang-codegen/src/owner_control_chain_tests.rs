//! Grouped normal-body permissions do not grant whole-loop or SSA authority.
use super::assert_general_counter_baseline;

#[test]
fn three_live_controls_preserve_zero_and_normal_generations() {
    let mut calls = String::new();
    let mut expected = String::new();
    for rounds in [[0, 0, 0], [2, 0, 0], [0, 2, 0], [2, 0, 2], [2, 2, 2]] {
        calls.push_str(&format!(
            "run({}, {}, {});",
            rounds[0], rounds[1], rounds[2]
        ));
        let latest = rounds.iter().rposition(|n| *n > 0);
        let values = match latest {
            Some(0) => ["a0", "b0"],
            Some(1) => ["a1", "b1"],
            Some(2) => ["a2", "b2"],
            None => ["entry-a", "entry-b"],
            _ => unreachable!(),
        };
        expected.push_str(&format!("{}\n{}\n", values[0], values[1]));
    }
    let text = format!(
        r#"
        fun run(first: Int, second: Int, third: Int) {{
            var n0 = first; var g0 = n0 > 0; var a0 = "entry-a"; var b0 = "entry-b";
            while(g0) {{ val x = replace(&a0, "a0"); val y = replace(&b0, "b0");
                n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = second; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) {{ val x = replace(&a1, "a1"); val y = replace(&b1, "b1");
                n1 = n1 - 1; g1 = n1 > 0 }};
            var n2 = third; var g2 = n2 > 0; var b2 = b1; var a2 = a1;
            while(g2) {{ val y = replace(&b2, "b2"); val x = replace(&a2, "a2");
                n2 = n2 - 1; g2 = n2 > 0 }};
            println(a2); println(b2)
        }}
        fun entry() {{ {calls} }}
    "#
    );
    assert_general_counter_baseline(&text, expected.as_bytes(), false);
    // Closed cleanup selects both continuation bodies as one mandatory group;
    // entry/tail/exit and typed-SSA whole-chain authority remain unavailable.
    let closed = text.replace(
        "println(a2); println(b2)",
        "val final_a = a2; val final_b = b2; println(\"done\")",
    );
    assert_general_counter_baseline(&closed, b"done\ndone\ndone\ndone\ndone\n", false);
}

fn closed_chain_source() -> super::UnitSource {
    let mut text = String::from("fun run(n: Int) {");
    for i in 0..3 {
        text.push_str(&format!("var n{i}=n; var g{i}=n{i}>0;"));
        if i == 0 {
            text.push_str("var a0=\"a\"; var b0=\"b\";");
        } else {
            let p = i - 1;
            text.push_str(&format!("var a{i}=a{p}; var b{i}=b{p};"));
        }
        text.push_str(&format!("while(g{i}) {{ val x=replace(&a{i},\"x\"); val y=replace(&b{i},\"y\"); n{i}=n{i}-1; g{i}=n{i}>0 }};"));
    }
    text.push_str("}\nfun entry() {run(2)}");
    let source = super::UnitSource::analyze("chain-body-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .any(|o| o.control_chain_bodies_v1().is_some())
    );
    source
}
#[test]
fn closed_chain_body_group_consumes_all_members() {
    let source = closed_chain_source();
    let (result, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = result.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, n)| *kind == "control-chain-bodies-complete" && *n == 2)
            .count(),
        1
    );
}
#[test]
fn ci_only_chain_body_group_rejects_foreign_control_and_last_missing_drop() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = closed_chain_source();
    for fault in [
        OwnerFault::ChainBodyWrongControl,
        OwnerFault::ChainLastBodyMissingDrop,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-fault-hit")
                .count(),
            1
        );
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "control-chain-bodies-complete")
        );
    }
}
