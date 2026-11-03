//! Original MutableList sources and environments, independently checked in both Unit entries.
//! Diagnostic codes are fixed by each original test, not obtained from another producer.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    type_checking::{
        CompilationUnitTypes, FinalTypeAdmissionError, check_compilation_unit_types,
        complete_compilation_unit_types,
    },
};

pub(super) fn check_each(
    text: &str,
    expected: &[&str],
    mut inspect: impl FnMut(&CompilationUnitTypes),
) {
    let (sources, file) = super::parsed(text);
    let source = file.source_id();
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = super::environments();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "containers.ko", source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("original source index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("original names, not an internal failure");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("original name qualification");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .expect("original type analysis, not an internal failure");
        assert_eq!(super::codes(typed.diagnostics()), expected, "{text}");
        // Qualify the original source before treating a Unit expression as a local AST ID.
        let calls = typed
            .container_appends()
            .iter()
            .map(|fact| fact.expression())
            .chain(
                typed
                    .container_clears()
                    .iter()
                    .map(|fact| fact.expression()),
            )
            .chain(
                typed
                    .container_remove_ats()
                    .iter()
                    .map(|fact| fact.expression()),
            )
            .chain(
                typed
                    .container_remove_lasts()
                    .iter()
                    .map(|fact| fact.expression()),
            )
            .chain(
                typed
                    .container_remove_firsts()
                    .iter()
                    .map(|fact| fact.expression()),
            )
            .chain(
                typed
                    .container_insert_ats()
                    .iter()
                    .map(|fact| fact.expression()),
            );
        for call in calls {
            assert_eq!(call.source_unit(), entry.id());
            assert!(matches!(
                file.ast()
                    .expressions()
                    .get(call.expression())
                    .expect("original call AST")
                    .payload(),
                lang_frontend::parser::Expression::Call { .. }
            ));
        }
        let admission = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed);
        if expected.is_empty() {
            let complete = admission.expect("successful source must pass the actual final gate");
            assert!(std::ptr::eq(complete.types(), &typed));
            assert!(complete.is_same_analysis(&typed));
            typed.clone().validate().expect("basic type qualification");
        } else {
            let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = admission else {
                panic!("expected source diagnostics, never admission success or Internal")
            };
            assert_eq!(super::codes(&diagnostics), expected, "{text}");
            assert_eq!(
                diagnostics,
                typed.diagnostics(),
                "final gate preserves full raw diagnostics"
            );
            assert!(typed.clone().validate().is_err());
        }
        inspect(&typed);
    }
}
