//! SPEC-0045 intrinsic `Rc<T>` shared-owner 所有权集成测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::resolve_names,
    ownership_checking::{
        ConstructionRootKind, LoanKind, LoanTarget, OwnershipCheckedFile, RcOwnershipEffectKind,
        check_ownership,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{ConstructionTarget, TypedFile, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

fn checked(text: &str) -> (SourceMap, ParsedFile, TypedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("ownership-rc.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "Rc ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    (sources, parsed, typed, owned)
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
    let (_, _, typed, owned) = checked(text);
    let (_, _, _, repeated) = checked(text);

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
        .filter(|plan| plan.target() == ConstructionTarget::IntrinsicRc)
        .collect::<Vec<_>>();
    assert_eq!(rc_roots.len(), 2);
    assert!(rc_roots.iter().all(|plan| {
        plan.root_obligation()
            .is_some_and(|root| root.kind() == ConstructionRootKind::SharedOwner)
    }));
    assert_eq!(owned.loans().len(), 1);
    assert_eq!(owned.loans()[0].kind(), LoanKind::Shared);
    assert!(matches!(owned.loans()[0].target(), LoanTarget::Place(_)));
    assert_eq!(typed.rc_operations().len(), owned.rc_effects().len());
}

#[test]
fn move_only_payload_cannot_be_extracted_from_rc() {
    let text = "class Resource {}
                fun invalid(own input: Resource): Unit {
                    val owner = Rc(input)
                    val extracted = owner.value
                }";
    let (_, _, _, owned) = checked(text);

    assert_eq!(codes(owned.diagnostics()), ["L0132"]);
    assert!(owned.rc_effects().is_empty());
    assert!(owned.construction_plans().is_empty());
    assert!(owned.drops().is_empty());
}

#[test]
fn moved_rc_cannot_be_shared_again() {
    let text = "class Resource {}
                fun invalid(own input: Resource): Unit {
                    val owner = Rc(input)
                    val moved = owner
                    val shared = owner.share()
                }";
    let (_, _, _, owned) = checked(text);

    assert_eq!(codes(owned.diagnostics()), ["L0131"]);
    assert!(owned.rc_effects().is_empty());
}

#[test]
fn rc_payload_is_not_an_inout_place() {
    let text = "class Resource {}
                fun mutate(inout resource: Resource): Unit {}
                fun invalid(own input: Resource): Unit {
                    val owner = Rc(input)
                    val changed = mutate(&owner.value)
                }";
    let (_, _, _, owned) = checked(text);

    assert_eq!(codes(owned.diagnostics()), ["L0134"]);
    assert!(owned.rc_effects().is_empty());
}
