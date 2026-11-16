//! Real bounded while sources; no native execution or altered receipt inputs.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, ownership_checking::UnitLoanActionCoverage,
    source::SourceMap, type_checking::standard_environments,
};
#[test]
fn original_partial_loop_closes_bounded_ssa_but_retains_general_obligation() {
    for reverse in [false, true] {
        for twice in [false, true] {
            check(reverse, twice, false);
        }
    }
}
#[test]
fn owned_boolean_loop_keeps_original_lowering_without_certification() {
    check(false, false, true);
}
fn check(reverse: bool, twice: bool, owned_flag: bool) {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "loop-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
    );
    let mode = if owned_flag { "own " } else { "" };
    let uses = if twice {
        "consume(item);other(item)"
    } else {
        "consume(item)"
    };
    let text = format!(
        "fun run({mode}flag:Boolean):Unit {{val owner=\"loop λ\\n\";borrow val item=view(owner);while(flag){{{uses}}}}}"
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "loop-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "loop-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "loop-caller.ko", caller_id, &caller),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    let lineages: Vec<_> = owned
        .ownership()
        .loan_action_observations()
        .iter()
        .flat_map(|c| c.loop_invariant_lineages())
        .collect();
    assert_eq!(lineages.len(), 1);
    assert_eq!(lineages[0].coverage(), UnitLoanActionCoverage::Partial);
    let (results, audits) = observe(|| {
        (0..2)
            .map(|_| {
                unit_lower::lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    &owned,
                    declaration("run"),
                )
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(audits.len(), 2);
    for (result, audit) in results.into_iter().zip(audits) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        assert!(function.blocks.len() >= 4);
        if owned_flag {
            assert!(audit.certified_loop_callers.is_empty());
            assert!(
                !audit
                    .borrow_edges
                    .contains(&(declaration("run"), declaration("view")))
            );
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.contains("loop missing/duplicate")),
                "{:?}",
                audit.unsupported
            );
        } else {
            assert_eq!(audit.certified_loop_callers, vec![declaration("run")]);
            assert!(
                audit
                    .borrow_edges
                    .contains(&(declaration("run"), declaration("view")))
            );
            assert!(
                audit
                    .direct_edges
                    .contains(&(declaration("run"), declaration("consume")))
            );
            if twice {
                assert!(
                    audit
                        .direct_edges
                        .contains(&(declaration("run"), declaration("other")))
                );
            }
            assert_eq!(audit.unsupported.len(), 1, "{:?}", audit.unsupported);
            assert!(audit.unsupported[0].starts_with("loop lineage remains Partial"));
        }
    }
}
