//! SPEC-0045 intrinsic `Rc<T>` shared-owner 所有权集成测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        CompilationUnitOwnership, ConstructionRootKind, LoanKind, RcOwnershipEffectKind,
        UnitLoanTarget, check_compilation_unit_ownership,
    },
    source::SourceMap,
    type_checking::{
        UnitConstructionTarget, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

// Keep two independently analyzed entry forms. A second call reparses the exact
// source and repeats each same entry, rather than treating cross-entry equality
// as the original repeated-analysis stability witness.
fn checked(text: &str) -> Vec<(usize, CompilationUnitOwnership)> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("ownership-rc.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "Rc ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let mut observations = Vec::new();
    for input in [
        SourceUnitInput::new("root", "ownership-rc.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).expect("names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        let [indexed] = names.names().index().source_units() else {
            panic!("one source")
        };
        assert_eq!(indexed.source_id(), source);
        let unit = indexed.id();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
        assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
        let complete = complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
            .expect("complete original types");
        assert!(complete.is_same_analysis(&raw));
        let operations = raw.rc_operations().len();
        let typed = raw.validate().expect("valid types");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .expect("ownership");
        assert!(owned.is_compatible_with(&typed));
        for effect in owned.rc_effects() {
            assert_eq!(effect.expression().source_unit(), unit);
            assert_eq!(effect.receiver().source_unit(), unit);
        }
        observations.push((operations, owned));
    }
    assert_eq!(observations.len(), 2);
    observations
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn share_preserves_owner_and_payload_reads_publish_stable_effects_and_loans() {
    let text = "class Resource {}
                fun inspect(resource: Resource): Unit {}
                fun valid(own input: Resource): Unit {
                    val first = Rc(input)
                    val second = first.share()
                    val third = first.share()
                    val inspected = inspect(first.value)
                    val numbers = Rc(1)
                    val copied = numbers.value
                    val moved = second
                }";
    let first = checked(text);
    let repeated = checked(text);
    for ((operations, owned), (_, repeated)) in first.iter().zip(&repeated) {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.rc_effects(), repeated.rc_effects());
        assert_eq!(owned.rc_effects().len(), 4);
        assert_eq!(
            owned
                .rc_effects()
                .iter()
                .filter(|effect| effect.kind() == RcOwnershipEffectKind::Retain)
                .count(),
            2
        );
        assert_eq!(
            owned
                .rc_effects()
                .iter()
                .filter(|effect| effect.kind() == RcOwnershipEffectKind::BorrowPayload)
                .count(),
            2
        );
        let rc_roots = owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.target() == UnitConstructionTarget::IntrinsicRc)
            .collect::<Vec<_>>();
        assert_eq!(rc_roots.len(), 2);
        assert!(rc_roots.iter().all(|plan| {
            plan.root_obligation()
                .is_some_and(|root| root.kind() == ConstructionRootKind::SharedOwner)
        }));
        assert_eq!(owned.loans().len(), 1);
        assert_eq!(owned.loans()[0].kind(), LoanKind::Shared);
        assert!(matches!(
            owned.loans()[0].target(),
            UnitLoanTarget::Place(_)
        ));
        assert_eq!(*operations, owned.rc_effects().len());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
    }
}

#[test]
fn move_only_payload_cannot_be_extracted_from_rc() {
    let text = "class Resource {}
                fun invalid(own input: Resource): Unit {
                    val owner = Rc(input)
                    val extracted = owner.value
                }";
    for (_, owned) in checked(text) {
        assert_eq!(codes(owned.diagnostics()), ["L0132"]);
        assert!(owned.rc_effects().is_empty());
        assert!(owned.construction_plans().is_empty());
        assert!(owned.drops().is_empty());
    }
}

#[test]
fn moved_rc_cannot_be_shared_again() {
    let text = "class Resource {}
                fun invalid(own input: Resource): Unit {
                    val owner = Rc(input)
                    val moved = owner
                    val shared = owner.share()
                }";
    for (_, owned) in checked(text) {
        assert_eq!(codes(owned.diagnostics()), ["L0131"]);
        assert!(owned.rc_effects().is_empty());
    }
}

#[test]
fn rc_payload_is_not_an_inout_place() {
    let text = "class Resource {}
                fun mutate(inout resource: Resource): Unit {}
                fun invalid(own input: Resource): Unit {
                    val owner = Rc(input)
                    val changed = mutate(&owner.value)
                }";
    for (_, owned) in checked(text) {
        assert_eq!(codes(owned.diagnostics()), ["L0134"]);
        assert!(owned.rc_effects().is_empty());
    }
}
