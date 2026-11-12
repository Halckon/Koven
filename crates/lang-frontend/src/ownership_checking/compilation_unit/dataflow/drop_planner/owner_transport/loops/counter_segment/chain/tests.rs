use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
thread_local! { static SKIP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub(in super::super) fn skip_scalar() -> bool {
    if SKIP.get() && HITS.get() == 0 {
        HITS.set(1);
        true
    } else {
        false
    }
}
fn source(suffix: &str, extra: &str, initial: &str) -> String {
    format!(
        r#"fun run(n: Int) {{
        var n0=n; var g0=n0>0; var a0="a"; var b0="b";
        while(g0) {{val x=replace(&a0,"x"); val y=replace(&b0,"y"); n0=n0-1; g0=n0>0}};
        var n1=n; var g1=n1>0; var a1=a0; var b1=b0;
        while(g1) {{val x=replace(&a1,"x1"); val y=replace(&b1,"y1"); n1=n1-1; g1=n1>0}};
        var n2={initial}; var g2=n2>0; val saved_b=b1; val saved_a=a1; var b2=saved_b; var a2=saved_a;
        while(g2) {{val y=replace(&b2,"y2"); val x=replace(&a2,"x2"); {extra} n2=n2-1; g2=n2>0}};
        {suffix}
    }}"#
    )
}
#[test]
fn private_three_control_chain_is_retained_only_as_a_complete_group() {
    for (suffix, extra, initial, expected) in [
        ("", "", "n", true),
        ("val end_a=a2; val end_b=b2;", "", "2", true),
        ("println(a2); println(b2);", "", "n", false),
        ("", "val unrelated=\"extra\";", "n", false),
        ("", "", "-1", false),
    ] {
        let text = source(suffix, extra, initial);
        let fixture = Fixture::new(&[("private-chain.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let observations = owned.owner_observations();
        assert_eq!(
            observations
                .iter()
                .filter(|o| o.private_control_chain.is_some())
                .count(),
            usize::from(expected)
        );
        let deferred =
            crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
                observations.to_vec(),
                true,
                false,
            );
        assert!(deferred.iter().all(|o| o.private_control_chain.is_none()));
        for o in observations {
            assert!(o.is_consistent());
            // Unary -1 is outside counter-prefix discovery: there is no
            // three-member candidate, so preserve the pre-existing two-control
            // bridge rather than incorrectly revoking that independent proof.
            assert_eq!(o.control_bridge_evidence_v1().is_some(), initial == "-1");
            assert!(o.counter_batch_evidence_v1().is_none());
            if let Some(group) = &o.private_control_chain {
                assert!(group.is_consistent());
                assert!(group.matches_observation(&o.identity, o.callable));
            }
        }
    }
}
#[test]
fn ci_only_private_chain_requires_actual_third_scalar_completion() {
    let text = source("", "", "n");
    let fixture = Fixture::new(&[("missing-third-entry.ko", &text)]);
    assert!(
        fixture
            .shared()
            .owner_observations()
            .iter()
            .any(|o| o.private_control_chain.is_some())
    );
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SKIP.set(false);
            HITS.set(0);
        }
    }
    let _reset = Reset;
    HITS.set(0);
    SKIP.set(true);
    let owned = fixture.shared();
    assert_eq!(HITS.get(), 1);
    assert!(
        owned
            .owner_observations()
            .iter()
            .all(|o| o.private_control_chain.is_none() && o.control_bridge_evidence_v1().is_none())
    );
}

#[test]
fn ci_only_private_chain_rejects_foreign_observation_identity() {
    let text = source("", "", "n");
    let fixture = Fixture::new(&[("foreign-chain.ko", &text)]);
    let owned = fixture.shared();
    let mut observation = owned
        .owner_observations()
        .iter()
        .find(|o| o.private_control_chain.is_some())
        .unwrap()
        .clone();
    assert!(observation.is_consistent());
    observation.identity = std::sync::Arc::new(());
    assert!(!observation.is_consistent());
}

mod getter_exclusion;
