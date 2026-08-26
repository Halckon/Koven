//! SPEC-0197 compilation-unit 签名产物的分析身份门禁。

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        NameEnvironment, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{TypeEnvironment, collect_compilation_unit_signatures, standard_environments},
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(parsed.diagnostics().is_empty());
    (source, parsed)
}

fn validated_names<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> lang_frontend::name_resolution::ValidatedCompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names")
}

#[test]
fn signature_provenance_rejects_structurally_equal_foreign_analyses() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", "package app\nfun make(): Unit {}");
    let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
    let (names_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &names_environment);
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
            .expect("signature collection succeeds");

    assert!(signatures.is_compatible_with(&sources, &inputs, &names, &type_environment));
    assert!(signatures.is_same_analysis(&signatures.clone()));

    let repeated_names = validated_names(&sources, &inputs, &names_environment);
    let repeated_signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &repeated_names, &type_environment)
            .expect("repeated signature collection succeeds");
    assert_eq!(signatures, repeated_signatures);
    assert!(!signatures.is_same_analysis(&repeated_signatures));
    assert!(!signatures.is_compatible_with(&sources, &inputs, &repeated_names, &type_environment,));

    let (foreign_names_environment, foreign_type_environment) = standard_environments();
    let foreign_names = validated_names(&sources, &inputs, &foreign_names_environment);
    let foreign_signatures = collect_compilation_unit_signatures(
        &sources,
        &inputs,
        &foreign_names,
        &foreign_type_environment,
    )
    .expect("foreign signature collection succeeds");
    assert_eq!(signatures, foreign_signatures);
    assert!(!signatures.is_compatible_with(
        &sources,
        &inputs,
        &foreign_names,
        &foreign_type_environment,
    ));

    let unrelated_type_environment = TypeEnvironment::new(&NameEnvironment::new());
    assert!(
        !signatures.is_compatible_with(&sources, &inputs, &names, &unrelated_type_environment,)
    );
}

#[test]
fn signature_provenance_rejects_structurally_equal_foreign_inputs() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", "package app\nclass Item");
    let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
    let (names_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &names_environment);
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
            .expect("signature collection succeeds");

    let mut foreign_sources = SourceMap::new();
    let (foreign_source, foreign_file) =
        parsed(&mut foreign_sources, "main.ko", "package app\nclass Item");
    let foreign_inputs = [SourceUnitInput::new(
        "root",
        "app/main.ko",
        foreign_source,
        &foreign_file,
    )];

    assert!(!signatures.is_compatible_with(
        &foreign_sources,
        &foreign_inputs,
        &names,
        &type_environment,
    ));
}
