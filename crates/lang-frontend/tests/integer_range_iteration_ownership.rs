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
        BuiltinType, IterationProvider, ParameterMode, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

fn checked(text: &str) -> (OwnershipCheckedFile, Vec<CompilationUnitOwnership>) {
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
    // Preserve the old loan/ownership witness and run both real Unit input kinds.
    let mut units = Vec::new();
    for (mode, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "iteration.ko", source, &file),
        ),
        ("memory", SourceUnitInput::memory_document(source, &file)),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let typed = typed.validate().unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        println!(
            "UNIT {mode} diagnostics={:?} plans={:?}",
            unit.diagnostics(),
            unit.iterations()
        );
        if unit.diagnostics().is_empty() {
            unit.clone().validate().unwrap();
        }
        units.push(unit);
    }
    (single, units)
}

fn assert_numeric(
    single: &OwnershipCheckedFile,
    units: &[CompilationUnitOwnership],
    endpoint: BuiltinType,
    bindings: usize,
) {
    assert_eq!(single.iterations().len(), 1);
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(single.deferred().is_empty(), "{:?}", single.deferred());
    for plan in single.iterations().iter() {
        assert_eq!(plan.descriptor().delivery(), ParameterMode::Value);
        assert_eq!(
            plan.descriptor().provider(),
            IterationProvider::NumericRange(endpoint)
        );
        assert_eq!(plan.bindings().len(), bindings);
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
    assert_eq!(units.len(), 2, "rooted and MemoryDocument Unit");
    for unit in units {
        assert_eq!(unit.iterations().len(), 1);
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        for plan in unit.iterations().iter() {
            assert_eq!(plan.descriptor().delivery(), ParameterMode::Value);
            assert_eq!(
                plan.descriptor().provider(),
                IterationProvider::NumericRange(endpoint)
            );
            assert_eq!(plan.bindings().len(), bindings);
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
}

#[test]
fn numeric_snapshots_allow_owned_items_source_reassignment_and_local_cleanup() {
    let mut observed = Vec::new();
    for (range, integer, endpoint) in [
        ("IntRange", "Int", BuiltinType::Int),
        ("LongRange", "Long", BuiltinType::Long),
    ] {
        for body in [
            "val copy = item; return copy".to_owned(),
            "current = replacement; val copy = item".to_owned(),
            "val held = \"held\".clone(); println(held); continue".to_owned(),
            "val held = \"held\".clone(); println(held); break".to_owned(),
        ] {
            let text = format!(
                "fun entry(input: {range}, replacement: {range}, fallback: {integer}): {integer} {{ var current = input; for (item in current) {{ {body} }}; return fallback }}"
            );
            observed.push((checked(&text), endpoint, 1));
        }
        observed.push((
            checked(&format!(
                "fun entry(input: {range}): Unit {{ for (_ in input) {{}} }}"
            )),
            endpoint,
            0,
        ));
    }
    for ((single, unit), endpoint, bindings) in observed {
        assert_numeric(&single, &unit, endpoint, bindings);
    }
}

#[test]
fn numeric_inner_loop_preserves_outer_container_borrow() {
    let text = "fun consume(own values: Array<Int>): Unit {}\nfun entry(values: Array<Int>, range: IntRange): Unit { for (outer in values) { for (inner in range) { consume(values) } } }";
    let (single, unit) = checked(text);
    assert_eq!(single.diagnostics().len(), 1, "{:?}", single.diagnostics());
    assert_eq!(single.diagnostics()[0].code().to_string(), "L0135");
    assert_eq!(unit.len(), 2);
    for unit in &unit {
        assert_eq!(
            unit.diagnostics(),
            single.diagnostics(),
            "full source diagnostics"
        );
    }
}
