//! Same source bytes and original environment in MemoryDocument and Rooted mode.
//! Callers execute every original oracle in each mode. This is not a comparison
//! of complete cross-entry facts or a replacement for Single owner/loan witnesses.
//! Diagnostics and failed-analysis empty facts retain their original oracles.
use lang_frontend::{
    name_resolution::{
        NameEnvironment, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    source::SourceMap,
    type_checking::{TypeEnvironment, check_compilation_unit_types},
};

use crate::parser_test_assertions;

pub(super) fn checked(
    text: &str,
    file_name: &str,
    rooted: bool,
    (environment, types): (NameEnvironment, TypeEnvironment),
) -> (SourceMap, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source(file_name, text).unwrap();
    let parsed =
        parser_test_assertions::parse_file_twice(&sources, source, "borrow rejection Unit");
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    assert_eq!(sources.source_text(source), Ok(text));
    let input = if rooted {
        SourceUnitInput::new("root", file_name, source, &parsed)
    } else {
        SourceUnitInput::memory_document(source, &parsed)
    };
    let inputs = [input];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(
        index.diagnostics().is_empty(),
        "{text}: {:?}",
        index.diagnostics()
    );
    let [entry] = index.source_units() else {
        panic!("one original source")
    };
    assert_eq!(entry.source_id(), source);
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(
        names.diagnostics().is_empty(),
        "{text}: {:?}",
        names.diagnostics()
    );
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(
        typed.diagnostics().is_empty(),
        "{text}: {:?}",
        typed.diagnostics()
    );
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    (sources, owned)
}
