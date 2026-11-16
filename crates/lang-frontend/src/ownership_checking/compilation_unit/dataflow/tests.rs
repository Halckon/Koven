use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::{OwnershipCheckingError, collect_construction_descriptors};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

#[test]
fn duplicate_construction_descriptor_is_rejected_before_dataflow() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/source.ko",
        "package p\nclass Resource {}\nfun build(): Unit { val result = Resource() }",
    );
    let inputs = [SourceUnitInput::new("root", "p/source.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid types");
    let descriptor = typed.types().constructions()[0].clone();

    let error = collect_construction_descriptors(
        &[descriptor.clone(), descriptor],
        names.names().index().source_units().len(),
    )
    .expect_err("duplicate construction locator must fail before body traversal");

    assert!(matches!(
        error,
        OwnershipCheckingError::InvalidUnitConstruction { .. }
    ));
}
