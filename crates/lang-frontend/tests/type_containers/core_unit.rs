//! Core container consumers use both real Unit entries without adding a final gate.
use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, SourceUnitKey, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{CompilationUnitTypes, check_compilation_unit_types},
};

pub(super) fn checked_entries(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    [(SourceUnitId, CompilationUnitTypes); 2],
) {
    let (sources, parsed) = super::parsed(text);
    let (environment, types) = super::environments();
    let entries = [false, true].map(|rooted| {
        let input = if rooted {
            SourceUnitInput::new("root", "containers.ko", parsed.source_id(), &parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), parsed.source_id());
        match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "containers.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid unit names");
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
        (source.id(), typed)
    });
    (sources, parsed, entries)
}
