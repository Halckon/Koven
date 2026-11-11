//! Genuine frontend observation scopes. Off products are never edited before lowering.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

const BOTH: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun loan(): Unit { val owner = \"kept\"; consume(view(owner)) }\nfun owner_run(flag: Boolean) { var held = \"outer\"; while(flag) { val previous = replace(&held, \"next\"); break }; val retired = held }\nfun entry(): Unit { owner_run(false); loan(); owner_run(true) }";

#[test]
fn real_frontend_observer_off_preserves_products_ssa_and_owner_receipts() {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "observer-off.ko", BOTH);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "observer-off.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, on) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(!on.ownership().loan_action_observations().is_empty());
        assert!(
            off.ownership().loan_action_observations().is_empty(),
            "real frontend observer-off must suppress acquisition observation"
        );
        assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
        assert_eq!(
            on.ownership().owner_observations(),
            off.ownership().owner_observations()
        );
        assert!(
            off.ownership()
                .owner_observations()
                .iter()
                .any(|o| o.replace_break_observation_v1().is_some())
        );
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let mut rendered = Vec::new();
        let mut drops = Vec::new();
        for (owned, expected) in [(&on, 1), (&off, 0)] {
            let ((lowered, owner_drops), receipts) =
                super::super::refine::loan_receipt::observe(|| {
                    super::super::refine::observe_owner_drops(|| {
                        super::super::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, owned, entry,
                        )
                    })
                });
            let (program, _) = lowered.unwrap();
            verify_program(&program).unwrap();
            assert_eq!(receipts.len(), expected);
            assert_eq!(owner_drops.iter().filter(|d| d.3.is_some()).count(), 1);
            rendered.push((
                super::super::render_program(&program),
                crate::llvm::render_verified_program(&program).unwrap(),
            ));
            drops.push(owner_drops);
        }
        assert_eq!(rendered[0], rendered[1]);
        assert_eq!(drops[0], drops[1]);
        assert!(
            off.ownership().loan_action_observations().is_empty(),
            "comparison did not mutate the real off product"
        );
    }
}

fn observed() -> bool {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "scope.ko", SOURCE);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (_, _, owned) = unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    !owned.ownership().loan_action_observations().is_empty()
}

#[test]
fn observation_scope_restores_nested_state_and_does_not_cross_threads() {
    assert!(observed());
    with_loan_observation(false, || {
        assert!(!observed());
        with_loan_observation(true, || assert!(observed()));
        assert!(!observed());
        assert!(std::thread::spawn(observed).join().unwrap());
        assert!(!observed());
    });
    assert!(observed());
}

#[test]
fn ci_only_observation_scope_restores_after_panic() {
    let result =
        std::panic::catch_unwind(|| with_loan_observation(false, || panic!("CI-only unwind")));
    assert!(result.is_err());
    assert!(observed());
}
