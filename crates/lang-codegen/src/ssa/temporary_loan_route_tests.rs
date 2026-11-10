//! Ordinary source-only P1 route capability; no native execution or faults.
use super::*;

#[test]
fn temporary_borrow_caller_uses_refine_without_migrating_callee() {
    check(false);
}

#[test]
fn temporary_borrow_requires_real_production_receipt() {
    check(true);
}

fn check(require_receipt: bool) {
    let text = "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "temporary.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "temporary.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        assert!(
            owned
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|o| o.regions())
                .any(|r| r.protocol_version() == 1
                    && r.coverage() == UnitLoanActionCoverage::Complete)
        );
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let ((lowered, routing), receipts) =
            super::super::refine::temporary_receipt::observe(|| {
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
        if require_receipt {
            assert_eq!(
                receipts.len(),
                1,
                "nonempty Complete1 requires production receipt"
            );
        }
        assert_eq!(
            routing.as_slice(),
            std::slice::from_ref(&caller.name),
            "only the temporary Borrow caller should migrate"
        );
    }
}
