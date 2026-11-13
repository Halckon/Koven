//! Ordinary static receipt evidence; finite execution is not an induction proof.
use super::UnitSource;
#[test]
fn child_cycle_receipt_connects_actual_current_and_next_root_vectors() {
    for replacements in [
        r#"val x=replace(&a,"next-a"); val y=replace(&b,"next-b");"#,
        r#"val y=replace(&b,"next-b"); val x=replace(&a,"next-a");"#,
    ] {
        let text = format!(
            r#"fun run(outer: Int, inner: Int, own seed: String) {{
            var a=seed; var b="b"; var n0=outer; var g0=n0>0;
            while(g0) {{ var n1=inner; var g1=n1>0;
                while(g1) {{ {replacements} n1=n1-1; g1=n1>0 }};
                val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
            }}; println(a); println(b)
        }}
        fun entry() {{ run(0,0,"seed"); run(1,2,"seed") }}"#
        );
        let source = UnitSource::analyze("child-cycle.ko", &text).unwrap();
        source.assert_frontend_clean();
        let product = source
            .ownership()
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap();
        assert!(product.child_generation_rule_v1().is_some());
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = lowered.unwrap_or_else(|error| panic!("{error:?}; actions={actions:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        for marker in [
            "nested-body-retirement-ssa-complete",
            "child-cycle-ssa-complete",
        ] {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(name, _)| *name == marker)
                    .map(|(_, count)| *count)
                    .collect::<Vec<_>>(),
                [2]
            );
        }
    }
}
