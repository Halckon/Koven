//! Retired SSA identity is distinct from a later lifecycle of the same source root.
use super::*;

#[test]
fn nested_retired_symbol_can_start_a_new_lifecycle_after_resume() {
    let mut calls = String::new();
    let mut expected = String::new();
    for outer in [0, 1, 2] {
        for inner in [0, 1, 2] {
            calls.push_str(&format!("run({outer},{inner});"));
            for _ in 0..outer {
                expected.push_str("reborn\nouter-b\n");
            }
        }
    }
    let text = format!(
        r#"fun run(outer: Int, inner: Int) {{
        var n0=outer; var g0=n0>0;
        while(g0) {{
            var a="a"; var b="b"; var n1=inner; var g1=n1>0;
            while(g1) {{ val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b"); n1=n1-1; g1=n1>0 }};
            val old=replace(&b,"outer-b"); a="reborn"; println(a); println(b); n0=n0-1; g0=n0>0
        }}
    }}
    fun entry() {{ {calls} }}"#
    );
    let source = UnitSource::analyze("nested-local-exit.ko", &text).unwrap();
    source.assert_frontend_clean();
    let n = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .expect("nested normal");
    let (live, drops) = n.false_exit_partition().expect("actual cleanup partition");
    assert_eq!(live.len(), 1);
    assert_eq!(drops.len(), 1);
    assert_eq!(n.parent_resume_roots(), Some(live));
    let write = n
        .parent_resume_write()
        .expect("independent live-root resume");
    assert_eq!(write.root(), live[0]);
    assert_ne!(n.normal().header_roots()[0], write.root());
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap_or_else(|e| panic!("{e:?}; actions={actions:?}"));
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(k, _)| *k == "nested-false-cleanup-rechecked")
            .map(|(_, n)| *n)
            .collect::<Vec<_>>(),
        [1]
    );

    assert_eq!(
        actions
            .iter()
            .filter(|(k, _)| *k == "nested-false-ssa-complete")
            .map(|(_, n)| *n)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        actions
            .iter()
            .filter(|(k, _)| *k == "nested-false-root-vector-complete")
            .map(|(_, n)| *n)
            .collect::<Vec<_>>(),
        [2]
    );
    for marker in [
        "nested-parent-resume-consumed",
        "nested-parent-resume-ssa-complete",
    ] {
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == marker)
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            [1]
        );
    }
    assert!(
        actions
            .iter()
            .all(|(k, _)| *k != "nested-parent-normal-carry-complete")
    );
    assert_final_false_cleanup_transport(&source, &program);
    assert_general_counter_baseline(&text, expected.as_bytes(), false);
}
