//! Original construction sources, with independent MemoryDocument and Rooted entry checks.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::check_compilation_unit_types,
};

pub(super) fn check_each(
    text: &str,
    inspect: impl FnMut(&SourceMap, &ParsedFile, &CompilationUnitOwnership),
) {
    check(text, false, inspect);
}

pub(super) fn check_each_stable(
    text: &str,
    inspect: impl FnMut(&SourceMap, &ParsedFile, &CompilationUnitOwnership),
) {
    check(text, true, inspect);
}

fn check(
    text: &str,
    repeat: bool,
    mut inspect: impl FnMut(&SourceMap, &ParsedFile, &CompilationUnitOwnership),
) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("construction-ownership.ko", text)
        .unwrap();
    let parsed = super::parse_file_twice(&sources, source, "construction ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = super::environments();
    for input in [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "construction-ownership.ko", source, &parsed),
    ] {
        let inputs = [input];
        // Reanalysis deliberately shares the original source/AST identity; it does not
        // compare unrelated arenas' bare local IDs or different entry modes' IDs.
        let analyze = || {
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
            let [entry] = index.source_units() else {
                panic!("one original construction source")
            };
            assert_eq!(entry.source_id(), source);
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
            assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
            let names = names.validate().unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let typed = typed.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            // Qualify identities before the caller accesses the original local AST.
            for plan in owned.construction_plans() {
                assert_eq!(plan.construction().source_unit(), entry.id());
                for delivery in plan.deliveries() {
                    assert_eq!(delivery.construction(), plan.construction());
                    assert_eq!(delivery.argument().source_unit(), entry.id());
                    if let Some(parameter) = delivery.parameter_symbol() {
                        assert_eq!(parameter.source_unit(), entry.id());
                    }
                }
                if let Some(root) = plan.root_obligation() {
                    assert_eq!(root.construction(), plan.construction());
                }
                if let Some(terminator) = plan.terminating_operand() {
                    assert_eq!(terminator.source_unit(), entry.id());
                }
            }
            for drop in owned.drops() {
                if let lang_frontend::ownership_checking::UnitDropTarget::Temporary(expression) =
                    drop.target()
                {
                    assert_eq!(expression.source_unit(), entry.id());
                }
            }
            owned
        };
        let owned = analyze();
        if repeat {
            let repeated = analyze();
            assert_eq!(owned.construction_plans(), repeated.construction_plans());
        }
        inspect(&sources, &parsed, &owned);
    }
}
