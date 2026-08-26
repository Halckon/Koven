//! Local 与 compilation-unit 类型表共享规范初始类型代数。

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, check_types, collect_compilation_unit_signatures, standard_environments,
    },
};

#[test]
fn local_and_unit_tables_share_builtin_and_literal_seed_order() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("empty.ko", "package sample")
        .expect("unique source");
    let lexed = lex(&sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing succeeds internally");
    assert!(parsed.diagnostics().is_empty());
    let (name_environment, type_environment) = standard_environments();

    let local_names =
        resolve_names(&sources, &parsed, &name_environment).expect("local names resolve");
    let local =
        check_types(&sources, &parsed, &local_names, &type_environment).expect("local types check");

    let inputs = [SourceUnitInput::new(
        "root",
        "sample/empty.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    let unit_names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("unit names resolve")
        .validate()
        .expect("valid unit names");
    let unit =
        collect_compilation_unit_signatures(&sources, &inputs, &unit_names, &type_environment)
            .expect("unit signatures collect");

    for builtin in BuiltinType::ALL {
        assert_eq!(
            local.types().builtin(builtin).map(|id| id.index()),
            unit.types().builtin(builtin).map(|id| id.index())
        );
    }
    assert_eq!(local.types().len(), BuiltinType::ALL.len() + 3);
    assert_eq!(unit.types().len(), BuiltinType::ALL.len() + 3);
}
