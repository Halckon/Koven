//! CI-only malformed-fact controls cloned from ordinary, complete source products.
use crate::{
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{
        IterationCleanupAction as Action, IterationExitKind as Exit, IterationSource, LoanTarget,
        OwnershipBindingDescriptor, OwnershipBindingKind, OwnershipCheckedFile, check_ownership,
        validate_iteration_facts,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        BuiltinType, IterationProvider, ParameterMode, SequentialContainerKind, TypedFile,
        check_types, complete_file_types, standard_environments,
    },
};
use std::collections::BTreeSet;

fn ordinary(text: &str) -> (ParsedFile, NameResolution, TypedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("numeric_snapshot.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    complete_file_types(&sources, &parsed, &names, &types, &typed).unwrap();
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    validate_iteration_facts(&parsed, &names, &typed, &owned).unwrap();
    (parsed, names, typed, owned)
}

fn assert_numeric(owned: &OwnershipCheckedFile, endpoint: BuiltinType) {
    assert!(!owned.iterations().is_empty());
    for plan in owned.iterations() {
        assert_eq!(
            plan.descriptor().provider(),
            IterationProvider::NumericRange(endpoint)
        );
        assert_eq!(plan.descriptor().delivery(), ParameterMode::Value);
        assert_eq!(
            plan.source_kind(),
            &IterationSource::OwnedSnapshot(plan.descriptor().source())
        );
        assert!(plan.source().is_none());
        assert!(
            plan.bindings()
                .iter()
                .all(|binding| binding.kind() == OwnershipBindingKind::Owned)
        );
    }
}

fn exit_name(exit: Exit) -> &'static str {
    match exit {
        Exit::Fallthrough => "fallthrough",
        Exit::Continue(_) => "continue",
        Exit::Break(_) => "break",
        Exit::Return(_) => "return",
        Exit::Exhaustion => "exhaustion",
    }
}

#[test]
fn numeric_snapshot_source_and_binding_mutations_are_rejected() {
    for (range, endpoint) in [
        ("IntRange", BuiltinType::Int),
        ("LongRange", BuiltinType::Long),
    ] {
        let (parsed, names, typed, valid) = ordinary(&format!(
            "fun run(input: {range}, other: {range}): Unit {{ for (item in input) {{ val copy = item }}; for (other_item in other) {{}} }}"
        ));
        assert_eq!(valid.iterations().len(), 2);
        assert_numeric(&valid, endpoint);
        let source = valid.iterations()[0].descriptor().source();
        let replacement = valid.iterations()[1].descriptor().source();
        assert_ne!(source, replacement);

        let mut wrong_identity = valid.clone();
        wrong_identity.iterations[0].source = IterationSource::OwnedSnapshot(replacement);
        assert_eq!(
            validate_iteration_facts(&parsed, &names, &typed, &wrong_identity)
                .unwrap_err()
                .reason(),
            "invalid owned snapshot identity"
        );

        let mut borrowed = valid.clone();
        borrowed.iterations[0].source =
            IterationSource::BorrowedSource(LoanTarget::Temporary(source));
        assert_eq!(
            validate_iteration_facts(&parsed, &names, &typed, &borrowed)
                .unwrap_err()
                .reason(),
            "provider source capability mismatch"
        );

        let mut shared = valid.clone();
        let symbol = shared.iterations[0].bindings[0].symbol();
        shared.iterations[0].bindings[0] =
            OwnershipBindingDescriptor::new(symbol, OwnershipBindingKind::Shared);
        assert_eq!(
            validate_iteration_facts(&parsed, &names, &typed, &shared)
                .unwrap_err()
                .reason(),
            "element bindings must preserve typed shared delivery"
        );
        validate_iteration_facts(&parsed, &names, &typed, &valid).unwrap();
    }
}

#[test]
fn numeric_snapshot_end_source_is_rejected_on_every_exit() {
    for (range, endpoint) in [
        ("IntRange", BuiltinType::Int),
        ("LongRange", BuiltinType::Long),
    ] {
        let mut observed = BTreeSet::new();
        for (expected, body) in [
            ("fallthrough", "val copy = item"),
            ("continue", "continue"),
            ("break", "break"),
            ("return", "return"),
        ] {
            let (parsed, names, typed, valid) = ordinary(&format!(
                "fun run(input: {range}): Unit {{ for (item in input) {{ {body} }} }}"
            ));
            assert_eq!(valid.iterations().len(), 1);
            assert_numeric(&valid, endpoint);
            let plan = &valid.iterations()[0];
            let statement = plan.descriptor().statement();
            assert!(
                plan.exits()
                    .iter()
                    .any(|exit| exit_name(exit.kind()) == expected)
            );
            for (index, exit) in plan.exits().iter().enumerate() {
                observed.insert(exit_name(exit.kind()));
                assert!(!exit.actions().contains(&Action::EndSource(statement)));
                let mut malformed = valid.clone();
                malformed.iterations[0].exits[index]
                    .actions
                    .push(Action::EndSource(statement));
                assert_eq!(
                    validate_iteration_facts(&parsed, &names, &typed, &malformed)
                        .unwrap_err()
                        .reason(),
                    "snapshot must not end a source loan",
                    "{range} {}",
                    exit_name(exit.kind())
                );
            }
        }
        assert_eq!(
            observed,
            BTreeSet::from(["fallthrough", "continue", "break", "return", "exhaustion"])
        );

        // A return exits both providers: only the outer Array owns a source loan.
        let (parsed, names, typed, nested) = ordinary(&format!(
            "fun run(values: Array<Int>, input: {range}): Unit {{ for (outer in values) {{ for (item in input) {{ return }} }} }}"
        ));
        assert_eq!(nested.iterations().len(), 2);
        let numeric = nested
            .iterations()
            .iter()
            .find(|plan| plan.descriptor().provider() == IterationProvider::NumericRange(endpoint))
            .unwrap();
        let borrowed = nested
            .iterations()
            .iter()
            .find(|plan| plan.source().is_some())
            .unwrap();
        assert_eq!(
            borrowed.descriptor().provider(),
            IterationProvider::Sequential(SequentialContainerKind::Array)
        );
        assert!(matches!(
            borrowed.source_kind(),
            IterationSource::BorrowedSource(_)
        ));
        assert_eq!(borrowed.descriptor().delivery(), ParameterMode::Borrow);
        let actions = numeric
            .exits()
            .iter()
            .find(|exit| matches!(exit.kind(), Exit::Return(_)))
            .unwrap()
            .actions();
        assert!(actions.contains(&Action::EndSource(borrowed.descriptor().statement())));
        assert!(!actions.contains(&Action::EndSource(numeric.descriptor().statement())));
        validate_iteration_facts(&parsed, &names, &typed, &nested).unwrap();
    }
}
