//! Real all-dead, partial-live and all-live false exits of the second control.
use super::UnitSource;

#[test]
fn bridge_exit_partition_consumes_only_actual_dead_roots() {
    for live in 0..=2 {
        let suffix = match live {
            0 => "",
            1 => "println(a1);",
            _ => "println(a1); println(b1);",
        };
        let text = format!(
            r#"fun run(n: Int) {{
            var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
            while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) {{ val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }};
            {suffix}
        }}
        fun entry() {{ run(0); run(2); println("done") }}"#
        );
        let source = UnitSource::analyze("bridge-exit-partition.ko", &text).unwrap();
        source.assert_frontend_clean();
        let bridges: Vec<_> = source
            .ownership()
            .owner_observations()
            .iter()
            .filter_map(|o| o.control_bridge_evidence_v1())
            .collect();
        assert_eq!(bridges.len(), 1);
        let (live_roots, drops) = bridges[0].next_false_exit_partition().unwrap();
        assert_eq!(live_roots.len(), live);
        assert_eq!(drops.len(), 2 - live);
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-exit-drop-consumed")
                .count(),
            2 - live
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-false-exit-partition-consumed")
                .count(),
            1
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-false-exit-all-dead")
                .count(),
            usize::from(live == 0)
        );
        let expected = match live {
            0 => "done\n",
            1 => "a\nx1\ndone\n",
            _ => "a\nb\nx1\ny1\ndone\n",
        };
        super::super::super::assert_native_output(&source, expected.as_bytes());
    }
}

#[test]
fn bridge_exit_does_not_claim_reinitialized_root_generation() {
    let text = r#"fun run(n: Int) {
        var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
        while(g0) { val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 };
        var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
        while(g1) { val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 };
        a1 = "later-a"; b1 = "later-b"; println(a1); println(b1)
    }
    fun entry() { run(0); run(2) }"#;
    let source = UnitSource::analyze("bridge-exit-reinit.ko", text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "bridge-exit-drop-consumed")
            .count(),
        2
    );
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "bridge-false-exit-all-dead")
            .count(),
        1
    );
    super::super::super::assert_native_output(&source, b"later-a\nlater-b\nlater-a\nlater-b\n");
}
