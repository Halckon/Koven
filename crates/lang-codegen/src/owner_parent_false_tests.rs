//! Independent parent false partition and native execution coverage.
use super::{UnitSource, assert_general_counter_baseline};

#[test]
fn parent_false_receipt_partitions_live_and_retired_roots() {
    for (suffix, live, retired) in [
        ("println(a); println(b)", 2, 0),
        ("println(b)", 1, 1),
        ("", 0, 2),
    ] {
        let text = format!(
            r#"fun run(outer: Int, inner: Int, own seed: String) {{
            var a=seed; var b="b"; var n0=outer; var g0=n0>0;
            while(g0) {{ var n1=inner; var g1=n1>0;
                while(g1) {{ val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b");
                    n1=n1-1; g1=n1>0 }};
                val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
            }}; {suffix}
        }}
        fun entry() {{ run(0,0,"seed"); run(1,1,"seed"); run(2,2,"seed"); run(2,0,"seed") }}"#
        );
        let source = UnitSource::analyze("parent-false.ko", &text).unwrap();
        source.assert_frontend_clean();
        let product = source
            .ownership()
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap();
        let p = product.parent_boundary_v1().expect("parent evidence");
        assert_eq!(
            (
                p.false_exit_partition().0.len(),
                p.false_exit_partition().1.len()
            ),
            (live, retired)
        );
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = lowered.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "nested-parent-false-root-vector-complete")
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "nested-parent-false-ssa-complete")
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            [retired]
        );
        let mut expected = String::new();
        for (a, b) in [
            ("seed", "b"),
            ("inner-a", "outer-b"),
            ("inner-a", "outer-b"),
            ("seed", "outer-b"),
        ] {
            if live == 2 {
                expected.push_str(&format!("{a}\n"));
            }
            if live > 0 {
                expected.push_str(&format!("{b}\n"));
            }
        }
        assert_general_counter_baseline(&text, expected.as_bytes(), false);
    }
}
