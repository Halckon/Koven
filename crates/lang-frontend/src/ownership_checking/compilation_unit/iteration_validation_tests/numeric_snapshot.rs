//! CI-only malformed-fact controls; ordinary completion, typed and ownership gates.
use super::mutate_point;
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, OwnershipBindingKind,
        UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        UnitIterationOwnershipPlan, UnitIterationSourceAccess, UnitLoanTarget,
        UnitOwnershipBindingDescriptor, check_compilation_unit_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, IterationProvider, ParameterMode, SequentialContainerKind,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};
use std::collections::BTreeSet;

fn ordinary(text: &str) -> CompilationUnitOwnership {
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
    let inputs = [SourceUnitInput::new(
        "root",
        "numeric_snapshot.ko",
        source,
        &parsed,
    )];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(owned.iterations_are_valid());
    owned.clone().validate().unwrap();
    owned
}

fn assert_numeric(owned: &CompilationUnitOwnership, endpoint: BuiltinType) {
    assert!(!owned.iterations().is_empty());
    for plan in owned.iterations() {
        assert_eq!(
            plan.descriptor().provider(),
            IterationProvider::NumericRange(endpoint)
        );
        assert_eq!(plan.descriptor().delivery(), ParameterMode::Value);
        assert_eq!(plan.source_access(), UnitIterationSourceAccess::Owned);
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

fn mutate_contract(
    owned: &mut CompilationUnitOwnership,
    change: impl Fn(&mut UnitIterationOwnershipPlan),
) {
    let statement = owned.iterations[0].descriptor().statement();
    let template = owned
        .iteration_templates
        .iter_mut()
        .find(|plan| plan.descriptor().statement() == statement)
        .unwrap();
    change(template);
    change(&mut owned.iterations[0]);
    let plan = &owned.iterations[0];
    // Equal corrupted contracts must reach numeric guards, not template mismatch.
    assert_eq!(plan.descriptor(), template.descriptor());
    assert_eq!(plan.source_kind(), template.source_kind());
    assert_eq!(plan.source_access(), template.source_access());
    assert_eq!(plan.bindings(), template.bindings());
}

fn assert_rejected(owned: CompilationUnitOwnership, case: &str) {
    assert!(!owned.iterations_are_valid(), "{case}");
    assert!(owned.validate().is_err(), "{case}");
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
fn numeric_snapshot_source_access_and_binding_mutations_are_rejected() {
    for (range, endpoint) in [
        ("IntRange", BuiltinType::Int),
        ("LongRange", BuiltinType::Long),
    ] {
        let valid = ordinary(&format!(
            "fun run(input: {range}, other: {range}): Unit {{ for (item in input) {{ val copy = item }}; for (other_item in other) {{}} }}"
        ));
        assert_eq!(valid.iterations().len(), 2);
        assert_numeric(&valid, endpoint);
        let source = valid.iterations()[0].descriptor().source();
        let replacement = valid.iterations()[1].descriptor().source();
        assert_ne!(source, replacement);

        let mut wrong_identity = valid.clone();
        mutate_contract(&mut wrong_identity, |plan| {
            plan.source = IterationSource::OwnedSnapshot(replacement)
        });
        assert_rejected(wrong_identity, "snapshot identity");

        let mut borrowed = valid.clone();
        mutate_contract(&mut borrowed, |plan| {
            plan.source = IterationSource::BorrowedSource(UnitLoanTarget::Temporary(source))
        });
        assert_rejected(borrowed, "borrowed numeric source");

        for access in [
            UnitIterationSourceAccess::Shared,
            UnitIterationSourceAccess::Exclusive,
            UnitIterationSourceAccess::Temporary,
        ] {
            let mut wrong_access = valid.clone();
            mutate_contract(&mut wrong_access, |plan| plan.source_access = access);
            assert_rejected(wrong_access, &format!("{range} source access {access:?}"));
        }

        let mut shared = valid.clone();
        mutate_contract(&mut shared, |plan| {
            let binding = plan.bindings[0];
            plan.bindings[0] = UnitOwnershipBindingDescriptor::new(
                binding.symbol(),
                OwnershipBindingKind::Shared,
                binding.declaration_span(),
            );
        });
        assert_rejected(shared, "shared numeric binding");
        valid.validate().unwrap();
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
            let valid = ordinary(&format!(
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
            for exit in plan.exits() {
                observed.insert(exit_name(exit.kind()));
                assert!(!exit.actions().contains(&Action::EndSource(statement)));
                let mut malformed = valid.clone();
                // Keep every publication of this point equal. The required control
                // sequence derived from the owned template must omit this source end.
                mutate_point(&mut malformed, exit.point(), |actions| {
                    actions.push(Action::EndSource(statement))
                });
                assert_rejected(
                    malformed,
                    &format!("{range} {} EndSource", exit_name(exit.kind())),
                );
            }
        }
        assert_eq!(
            observed,
            BTreeSet::from(["fallthrough", "continue", "break", "return", "exhaustion"])
        );

        let nested = ordinary(&format!(
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
        assert_eq!(borrowed.source_access(), UnitIterationSourceAccess::Shared);
        let exit = numeric
            .exits()
            .iter()
            .find(|exit| matches!(exit.kind(), Exit::Return(_)))
            .unwrap();
        assert!(
            exit.actions()
                .contains(&Action::EndSource(borrowed.descriptor().statement()))
        );
        assert!(
            !exit
                .actions()
                .contains(&Action::EndSource(numeric.descriptor().statement()))
        );
        assert_eq!(
            nested.iteration_cleanup_at(exit.point()),
            Some(exit.actions())
        );
        nested.validate().unwrap();
    }
}
