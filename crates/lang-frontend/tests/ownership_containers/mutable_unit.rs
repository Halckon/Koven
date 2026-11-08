//! Same-source MutableList and size-RHS contracts, with final admission before actual P3.
//! Deferred and deeper place/lifetime contracts are observed separately in remaining_unit.
use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, complete_compilation_unit_types},
};

pub(super) fn check_each(
    text: &str,
    expected: &[&str],
    mut inspect: impl FnMut(&SourceMap, &ParsedFile, SourceUnitId, &CompilationUnitOwnership),
) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("ownership-containers.ko", text)
        .expect("source");
    let file = super::parse_file_twice(&sources, source, "container ownership source");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = super::environments();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "ownership-containers.ko", source, &file),
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
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
        let complete = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .expect("P3 source must pass the actual final type gate");
        assert!(std::ptr::eq(complete.types(), &typed));
        assert!(complete.is_same_analysis(&typed));
        let typed = typed.validate().expect("basic type qualification");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .expect("actual ownership analysis, never a substitute callback");
        assert!(owned.is_compatible_with(&typed));
        assert_eq!(super::codes(owned.diagnostics()), expected, "{text}");
        if expected.is_empty() {
            assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
            owned
                .clone()
                .validate()
                .expect("successful ownership qualification");
        } else {
            assert!(owned.clone().validate().is_err());
        }
        inspect(&sources, &file, entry.id(), &owned);
    }
}
