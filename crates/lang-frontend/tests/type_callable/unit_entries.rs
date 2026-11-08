//! 同一原始 AST 与显式环境的双 Unit 入口，不增加 final/ownership 前置。
use lang_frontend::{
    name_resolution::{
        NameEnvironment, SourceUnitInput, SourceUnitKey, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{CompilationUnitTypes, TypeEnvironment, check_compilation_unit_types},
};
pub(super) fn checked_entries(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) -> [CompilationUnitTypes; 2] {
    checked_entries_with_names(sources, parsed, environment, types).map(|(_, typed)| typed)
}

pub(super) fn checked_entries_with_names(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) -> [(ValidatedCompilationUnitNames, CompilationUnitTypes); 2] {
    [false, true].map(|rooted| {
        let input = if rooted {
            SourceUnitInput::new("root", "callable.ko", parsed.source_id(), parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(sources, &inputs).expect("unit index");
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), parsed.source_id());
        match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "callable.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(sources, &inputs, &index, environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid unit names");
        let typed =
            check_compilation_unit_types(sources, &inputs, &names, types).expect("unit types");
        assert!(
            typed
                .calls()
                .iter()
                .all(|call| call.expression().source_unit() == source.id())
        );
        (names, typed)
    })
}
