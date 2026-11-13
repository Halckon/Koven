//! Ordinary scalar-prefix regression. Runtime samples are not an induction proof.
use super::UnitSource;
fn source(initial: &str, calls: &str) -> String {
    format!(
        r#"fun touch(own value:Int) {{}}
    fun run(own flag:Boolean,outer:Int,inner:Int,own seed:String) {{
        {initial} var b="b"; var n0=outer; var g0=n0>0;
        if(flag) {{ touch(1) }} else {{ touch(2) }};
        while(g0) {{ var n1=inner; var g1=n1>0;
            while(g1) {{ val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b"); n1=n1-1; g1=n1>0 }};
            val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
        }}; println(a); println(b)
    }}
    fun entry() {{ {calls} }}"#
    )
}
#[test]
fn scalar_prefix_keeps_actual_owner_vector_into_parent() {
    for own_flag in [false, true] {
        for initial in ["var a=seed;", "var a=\"a\";"] {
            let text = source(initial, "run(true,1,2,\"seed\"); run(false,0,0,\"seed\");");
            let text = if own_flag {
                text
            } else {
                text.replace("own flag:Boolean", "flag:Boolean")
            };
            let source = UnitSource::analyze("scalar-owner-entry.ko", &text).unwrap();
            source.assert_frontend_clean();
            let nested = source
                .ownership()
                .owner_observations()
                .iter()
                .find_map(|o| o.nested_normal_body_v1())
                .expect("nested fact");
            assert!(nested.scalar_entry_v1().is_some());
            let (result, actions) =
                crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
            let (program, _) = result.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
            crate::ssa::verify::verify_program(&program).unwrap();
            for marker in [
                "parent-scalar-entry-ssa-complete",
                "parent-initial-owner-base-ssa-complete",
                "parent-generation-composition-ssa-complete",
            ] {
                assert_eq!(
                    actions
                        .iter()
                        .filter(|(name, _)| *name == marker)
                        .map(|(_, n)| *n)
                        .collect::<Vec<_>>(),
                    [2],
                    "{marker}: {actions:?}"
                );
            }
        }
    }
}
#[test]
fn scalar_prefix_native_preserves_zero_and_current_generation() {
    for initial in ["var a=seed;", "var a=\"a\";"] {
        let mut calls = String::new();
        let mut expected = String::new();
        for flag in [false, true] {
            for outer in [0, 1, 2] {
                for inner in [0, 1, 2] {
                    calls.push_str(&format!("run({flag},{outer},{inner},\"seed\");"));
                    let a = if outer > 0 && inner > 0 {
                        "inner-a"
                    } else if initial.contains("seed") {
                        "seed"
                    } else {
                        "a"
                    };
                    let b = if outer > 0 { "outer-b" } else { "b" };
                    expected.push_str(&format!("{a}\n{b}\n"));
                }
            }
        }
        super::assert_general_counter_baseline(
            &source(initial, &calls),
            expected.as_bytes(),
            false,
        );
    }
}

#[test]
#[ignore = "CI-only fault injection; exact dual-host scalar entry runner"]
fn ci_only_scalar_entry_rejects_actual_source_transport_and_escape() {
    use crate::ssa::OwnerFault::*;
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let text = source("var a=seed;", "run(true,1,2,\"seed\");");
    let source = UnitSource::analyze("scalar-entry-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    source.lower_entry(source.entry()).expect("baseline");
    for fault in [
        ScalarEntryArmConsume,
        ScalarEntryJoinEscape,
        ScalarEntryArmDefinition,
        ScalarEntryArmSwap,
        ScalarEntryJoinReentry,
        ScalarEntryWrongCall,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        for marker in [
            "scalar-entry-fault-hit",
            "scalar-entry-independent-rejection",
        ] {
            assert_eq!(
                actions.iter().filter(|(name, _)| *name == marker).count(),
                1,
                "{actions:?}"
            );
        }
    }
}
