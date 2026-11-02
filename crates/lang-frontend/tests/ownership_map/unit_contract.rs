//! Both entry modes retain original bytes and qualify independent AST identities.
use lang_frontend::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{
        SourceUnitId, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{Expression, ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

pub(super) fn check_each(
    text: &str,
    mut inspect: impl FnMut(&SourceMap, &ParsedFile, SourceUnitId, &CompilationUnitOwnership),
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "map.ko", source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        inspect(&sources, &file, entry.id(), &owned);
    }
}

pub(super) fn expression(
    sources: &SourceMap,
    file: &ParsedFile,
    text: &str,
    kind: impl Fn(&Expression) -> bool,
) -> ExpressionId {
    let found = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (kind(node.payload()) && sources.slice(node.span()).unwrap() == text).then_some(id)
        })
        .collect::<Vec<_>>();
    let [id] = found.as_slice() else {
        panic!("unique original expression: {text}")
    };
    *id
}
