//! Private mutations of real analyzed products; no public capability mutation.
use super::super::{IterationExitKind, LoanTarget, check_ownership, validate_iteration_facts};
use super::*;
use crate::{
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

fn fixture() -> (ParsedFile, NameResolution, TypedFile, OwnershipCheckedFile) {
    fixture_source(
        "fun source(): Array<Int> = arrayOf(1, 2)\nfun scan(): Int { for (item in source()) { if (item == 1) { return item } }; return 0 }",
    )
}

fn fixture_source(text: &str) -> (ParsedFile, NameResolution, TypedFile, OwnershipCheckedFile) {
    fixture_source_authority(text, false)
}

fn fixture_source_authority(
    text: &str,
    trusted: bool,
) -> (ParsedFile, NameResolution, TypedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::default();
    let source = sources.add_source("validator.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, mut types) = standard_environments();
    if trusted {
        types.authorize_range_source(&sources, source).unwrap();
    }
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 1);
    (parsed, names, typed, owned)
}

#[test]
fn lambda_return_inside_a_loop_does_not_exit_the_loop() {
    let (parsed, names, typed, owned) = fixture_source(
        "fun source(): Array<Int> = arrayOf(1, 2)\nfun scan(): Int { for (item in source()) { val callback: () -> Int = { return 7 }; if (item == 1) { return callback() } }; return 0 }",
    );
    assert_eq!(
        owned.iterations()[0]
            .exits()
            .iter()
            .filter(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
            .count(),
        1
    );
    validate_iteration_facts(&parsed, &names, &typed, &owned).unwrap();
}

#[test]
fn a_loop_inside_a_lambda_still_requires_its_return_descriptor() {
    let (parsed, names, typed, mut owned) = fixture_source(
        "fun source(): Array<Int> = arrayOf(1, 2)\nfun scan(): Int { val callback: () -> Int = { for (item in source()) { return item }; return 0 }; return callback() }",
    );
    validate_iteration_facts(&parsed, &names, &typed, &owned).unwrap();
    owned.iterations[0]
        .exits
        .retain(|exit| !matches!(exit.kind, IterationExitKind::Return(_)));
    assert_eq!(
        validate_iteration_facts(&parsed, &names, &typed, &owned)
            .unwrap_err()
            .reason(),
        "missing Return descriptor"
    );
}

#[test]
fn real_products_validate_without_changing_facts() {
    let (parsed, names, typed, owned) = fixture();
    let before = owned.iterations().to_vec();
    validate_iteration_facts(&parsed, &names, &typed, &owned).unwrap();
    assert_eq!(owned.iterations(), before);
}

#[test]
fn temporary_root_substitution_cannot_bypass_source_call() {
    let (parsed, names, typed, mut owned) = fixture();
    let replacement = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| matches!(node.payload(), crate::parser::Expression::Literal(_)))
        .unwrap()
        .0;
    owned.iterations[0].source = LoanTarget::Temporary(replacement);
    let error = validate_iteration_facts(&parsed, &names, &typed, &owned).unwrap_err();
    assert_eq!(error.reason(), "temporary root bypasses source evaluation");
    assert!(error.span().is_some());
}

#[test]
fn missing_return_descriptor_and_reordered_cleanup_are_rejected() {
    let (parsed, names, typed, original) = fixture();
    let mut missing = original.clone();
    missing.iterations[0]
        .exits
        .retain(|exit| !matches!(exit.kind, IterationExitKind::Return(_)));
    assert_eq!(
        validate_iteration_facts(&parsed, &names, &typed, &missing)
            .unwrap_err()
            .reason(),
        "missing Return descriptor"
    );
    let mut reordered = original;
    let exit = reordered.iterations[0]
        .exits
        .iter_mut()
        .find(|exit| exit.kind == IterationExitKind::Exhaustion)
        .unwrap();
    exit.actions.reverse();
    assert_eq!(
        validate_iteration_facts(&parsed, &names, &typed, &reordered)
            .unwrap_err()
            .reason(),
        "element/provider/source cleanup order"
    );
}

#[test]
fn temporary_range_schema_requires_matching_continuation_and_shared_source_loan() {
    let (parsed, names, typed, valid) = fixture_source_authority(
        "fun run():Unit{for(item in rangeView(listOf(\"a\"),0,1)){println(item)}}",
        true,
    );
    validate_iteration_facts(&parsed, &names, &typed, &valid).unwrap();
    let mut missing = valid.clone();
    missing.borrow_results.range_uses.clear();
    assert!(validate_iteration_facts(&parsed, &names, &typed, &missing).is_err());
    let mut missing_loan = valid.clone();
    let source = valid.borrow_results.range_uses[0].source_loan();
    missing_loan
        .loans
        .retain(|loan| loan.call() != source.call() || loan.argument() != source.argument());
    assert!(validate_iteration_facts(&parsed, &names, &typed, &missing_loan).is_err());
    let mut wrong_origin = valid;
    wrong_origin.borrow_results.range_uses[0].origin = LoanTarget::Temporary(source.call());
    assert!(validate_iteration_facts(&parsed, &names, &typed, &wrong_origin).is_err());
}
