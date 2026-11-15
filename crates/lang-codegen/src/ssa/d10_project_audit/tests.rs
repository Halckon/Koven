//! Real ordinary source and original Unit lowering; no injected or cross-run facts.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
fn run(body: &str, twice: bool, reverse: bool) -> Vec<ProjectAudit> {
    run_source(&format!("fun run(): Unit {{ {body} }}"), twice, reverse)
}
fn run_source(caller_text: &str, twice: bool, reverse: bool) -> Vec<ProjectAudit> {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "provider.ko",
        "fun inspect(a: String, b: String): Int = 1\nfun observe(a: String, b: String): Unit {}\nfun number(): Int = 1",
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "caller.ko", caller_text);
    let mut inputs = [
        SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "caller.ko", caller_id, &caller),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap()
        .id();
    let (results, audits) = observe(|| {
        (0..if twice { 2 } else { 1 })
            .map(|_| {
                unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
            .collect::<Vec<_>>()
    });
    for result in results {
        result.unwrap();
    }
    audits
}
#[test]
fn original_refinement_receipt_closes_final_unit_program_and_runs_are_isolated() {
    // Two complete assemblies inside one observation must not share receipt collections.
    let audits = run("inspect(\"left\", \"right\")", true, false);
    assert_eq!(audits.len(), 2);
    for audit in audits {
        assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        let receipt = audit.literal_consumption;
        assert_eq!(receipt.expected.len(), 1);
        assert_eq!(receipt.linked, receipt.expected);
        assert!(receipt.gaps.is_empty(), "{:?}", receipt.gaps);
    }
    // A subsequent independent original analysis must also close without stale receipts.
    let independent = run("inspect(\"again\", \"fresh\")", false, true);
    assert_eq!(independent.len(), 1);
    assert_eq!(independent[0].literal_consumption.linked.len(), 1);
    assert!(independent[0].literal_consumption.gaps.is_empty());
}
#[test]
fn real_uncovered_route_and_empty_region_are_not_consumption_success() {
    let audits = run("inspect(\"a\", \"b\"); inspect(\"c\", \"d\")", false, false);
    let audit = &audits[0].literal_consumption;
    assert_eq!(
        audit.expected.len(),
        2,
        "frontend links alone do not establish the refinement route"
    );
    assert!(audit.linked.is_empty());
    assert_eq!(
        audit
            .gaps
            .iter()
            .filter(|g| g.contains("route gap"))
            .count(),
        2
    );
    let empty = run("number()", false, false);
    assert!(empty[0].literal_consumption.expected.is_empty());
    assert!(empty[0].literal_consumption.linked.is_empty());
    assert!(!empty[0].literal_consumption.gaps.is_empty());
}

#[test]
fn complete_entry_blocks_account_for_named_arguments_and_int_or_unit_results() {
    let audits = run_source(
        r#"fun first(): Unit { inspect(b = "right", a = "left") }
fun second(): Unit { observe(b = "second", a = "first") }
fun run(): Unit { first(); second() }"#,
        false,
        true,
    );
    assert_eq!(audits.len(), 1);
    let audit = &audits[0].literal_consumption;
    assert_eq!(audit.expected.len(), 2);
    assert_eq!(audit.linked, audit.expected);
    // The two zero-argument entry calls remain explicitly outside protocol 1.
    assert_eq!(audit.gaps.len(), 2);
    assert!(
        audit
            .gaps
            .iter()
            .all(|gap| gap.contains("unsupported original linked region"))
    );
    let unit = run("observe(b = \"right\", a = \"left\")", false, false);
    assert_eq!(unit[0].literal_consumption.expected.len(), 1);
    assert_eq!(
        unit[0].literal_consumption.linked,
        unit[0].literal_consumption.expected
    );
    assert!(
        unit[0].literal_consumption.gaps.is_empty(),
        "{:?}",
        unit[0].literal_consumption.gaps
    );
}
