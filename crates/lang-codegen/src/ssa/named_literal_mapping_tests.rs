//! Ordinary named literal source: source order differs from parameter order.
use super::*;
const SOURCE: &str = "fun inspect(a: String, b: String): Int = 1\nfun run(): Unit { inspect(b = \"first\", a = \"second\") }";
#[test]
fn named_literal_caller_requires_real_refine_route() {
    check(false);
}
fn check(receipt: bool) {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "two-literal.ko", SOURCE);
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let regions = owned
        .ownership()
        .loan_action_observations()
        .iter()
        .flat_map(|o| o.regions())
        .collect::<Vec<_>>();
    let [region] = regions.as_slice() else {
        panic!("one actual region")
    };
    assert_eq!(region.protocol_version(), 1);
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    assert_eq!(region.action_ids().count(), 6);
    let run = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap();
    let ((lowered, routes), receipts) = super::super::refine::temporary_receipt::observe(|| {
        super::super::refine::observe_routing(|| {
            super::super::unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                run.id(),
            )
        })
    });
    let (program, entry) = lowered.unwrap();
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    let caller = program.modules[0].function(entry).unwrap();
    assert_eq!(
        routes.as_slice(),
        std::slice::from_ref(&caller.name),
        "only actual caller should migrate"
    );
    if receipt {
        assert_eq!(
            receipts.len(),
            1,
            "complete pair needs one actual call receipt"
        );
    }
}
