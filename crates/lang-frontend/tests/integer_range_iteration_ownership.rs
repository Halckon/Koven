//! Ordinary source through complete admission and ownership; no program execution.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationCleanupAction, IterationSource, OwnershipBindingKind,
        OwnershipCheckedFile, UnitIterationCleanupAction, check_compilation_unit_ownership,
        check_ownership, validate_iteration_facts,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        ParameterMode, check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

fn checked(text: &str) -> (OwnershipCheckedFile, CompilationUnitOwnership) {
    println!("SOURCE {text}");
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    complete_file_types(&sources, &file, &names, &types, &typed).unwrap();
    let single = check_ownership(&sources, &file, &names, &typed).unwrap();
    println!(
        "SINGLE diagnostics={:?} deferred={:?} plans={:?}",
        single.diagnostics(),
        single.deferred(),
        single.iterations()
    );
    if single.diagnostics().is_empty() && single.deferred().is_empty() {
        validate_iteration_facts(&file, &names, &typed, &single).unwrap();
    }
    let inputs = [SourceUnitInput::new("root", "iteration.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
    let typed = typed.validate().unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    println!(
        "UNIT diagnostics={:?} plans={:?}",
        unit.diagnostics(),
        unit.iterations()
    );
    if unit.diagnostics().is_empty() {
        unit.clone().validate().unwrap();
    }
    (single, unit)
}

fn assert_numeric(single: &OwnershipCheckedFile, unit: &CompilationUnitOwnership) {
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(single.deferred().is_empty(), "{:?}", single.deferred());
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    for plan in single
        .iterations()
        .iter()
        .filter(|p| p.descriptor().delivery() == ParameterMode::Value)
    {
        assert_eq!(
            plan.source_kind(),
            &IterationSource::OwnedSnapshot(plan.descriptor().source())
        );
        assert!(plan.source().is_none());
        assert!(
            plan.bindings()
                .iter()
                .all(|b| b.kind() == OwnershipBindingKind::Owned)
        );
        assert!(plan.exits().iter().all(|e| !e.actions().contains(
            &IterationCleanupAction::EndSource(plan.descriptor().statement())
        )));
        assert!(!plan.exits().is_empty());
    }
    for plan in unit
        .iterations()
        .iter()
        .filter(|p| p.descriptor().delivery() == ParameterMode::Value)
    {
        assert_eq!(
            plan.source_kind(),
            &IterationSource::OwnedSnapshot(plan.descriptor().source())
        );
        assert!(plan.source().is_none());
        assert!(
            plan.bindings()
                .iter()
                .all(|b| b.kind() == OwnershipBindingKind::Owned)
        );
        assert!(plan.exits().iter().all(|e| !e.actions().contains(
            &UnitIterationCleanupAction::EndSource(plan.descriptor().statement())
        )));
        assert!(!plan.exits().is_empty());
    }
}

#[test]
fn numeric_snapshots_allow_owned_items_source_reassignment_and_local_cleanup() {
    let mut observed = Vec::new();
    for (range, integer) in [("IntRange", "Int"), ("LongRange", "Long")] {
        for body in [
            "val copy = item; return copy".to_owned(),
            "current = replacement; val copy = item".to_owned(),
            "val held = \"held\".clone(); println(held); continue".to_owned(),
            "val held = \"held\".clone(); println(held); break".to_owned(),
        ] {
            let text = format!(
                "fun entry(input: {range}, replacement: {range}, fallback: {integer}): {integer} {{ var current = input; for (item in current) {{ {body} }}; return fallback }}"
            );
            observed.push(checked(&text));
        }
        observed.push(checked(&format!(
            "fun entry(input: {range}): Unit {{ for (_ in input) {{}} }}"
        )));
    }
    for (single, unit) in observed {
        assert_numeric(&single, &unit);
    }
}

#[test]
fn numeric_inner_loop_preserves_outer_container_borrow() {
    let text = "fun consume(own values: Array<Int>): Unit {}\nfun entry(values: Array<Int>, range: IntRange): Unit { for (outer in values) { for (inner in range) { consume(values) } } }";
    let (single, unit) = checked(text);
    assert_eq!(single.diagnostics().len(), 1, "{:?}", single.diagnostics());
    assert_eq!(unit.diagnostics().len(), 1, "{:?}", unit.diagnostics());
    assert_eq!(single.diagnostics()[0].code().to_string(), "L0135");
    assert_eq!(unit.diagnostics()[0].code().to_string(), "L0135");
}
