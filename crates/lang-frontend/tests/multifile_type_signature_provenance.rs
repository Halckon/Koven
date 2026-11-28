//! SPEC-0197 compilation-unit 签名产物的分析身份门禁。

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        NameEnvironment, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitTypeError, EnvironmentType, TypeCheckingError, TypeEnvironment,
        check_compilation_unit_types, check_types, collect_compilation_unit_signatures,
        standard_environments, validate_compilation_unit_body_inputs,
    },
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

#[test]
fn successful_binding_changes_the_snapshot_but_failed_and_repeated_operations_do_not() {
    use lang_frontend::type_checking::{BuiltinType, EnvironmentType};

    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", "package app\nclass Item");
    let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
    let mut names_environment = NameEnvironment::new();
    let external = names_environment.declare_value("external").unwrap();
    let mut types = TypeEnvironment::new(&names_environment);
    types.authorize_range_source(&sources, source).unwrap();
    let names = validated_names(&sources, &inputs, &names_environment);
    let signatures = collect_compilation_unit_signatures(&sources, &inputs, &names, &types)
        .expect("signature collection succeeds");

    let mut changed = types.clone();
    changed.authorize_range_source(&sources, source).unwrap();
    assert!(changed.bind_builtin(external, BuiltinType::Int).is_err());
    assert!(signatures.is_compatible_with(&sources, &inputs, &names, &changed));
    changed
        .bind_value(external, EnvironmentType::Builtin(BuiltinType::Int))
        .unwrap();
    assert!(!signatures.is_compatible_with(&sources, &inputs, &names, &changed));
    assert!(signatures.is_compatible_with(&sources, &inputs, &names, &types));
}

#[test]
fn divergent_name_environments_cannot_reinterpret_an_external_symbol_id() {
    let mut left = NameEnvironment::new();
    let int = left.declare_type("Int").unwrap();
    let string = left.declare_type("String").unwrap();
    let mut right = left.clone();
    let wanted = left.declare_value("wanted").unwrap();
    let unrelated = right.declare_value("unrelated").unwrap();
    assert_eq!(wanted, unrelated);

    let mut left_types = TypeEnvironment::new(&left);
    let mut right_types = TypeEnvironment::new(&right);
    for (types, value, ty) in [
        (&mut left_types, wanted, BuiltinType::String),
        (&mut right_types, unrelated, BuiltinType::Int),
    ] {
        types.bind_builtin(int, BuiltinType::Int).unwrap();
        types.bind_builtin(string, BuiltinType::String).unwrap();
        types
            .bind_value(value, EnvironmentType::Builtin(ty))
            .unwrap();
    }

    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", "fun entry(): Int = wanted");
    let inputs = [SourceUnitInput::memory_document(source, &file)];
    let unit_names = validated_names(&sources, &inputs, &left);
    let single_names = resolve_names(&sources, &file, &left).unwrap();
    let matching = check_types(&sources, &file, &single_names, &left_types).unwrap();
    assert_eq!(matching.diagnostics()[0].code().to_string(), "L0084");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &unit_names, &left_types)
            .unwrap()
            .validate()
            .unwrap();

    assert!(matches!(
        check_types(&sources, &file, &single_names, &right_types),
        Err(TypeCheckingError::MismatchedNameEnvironment)
    ));
    assert!(matches!(
        collect_compilation_unit_signatures(&sources, &inputs, &unit_names, &right_types),
        Err(CompilationUnitTypeError::MismatchedNameEnvironment)
    ));
    assert!(matches!(
        validate_compilation_unit_body_inputs(
            &sources,
            &inputs,
            &unit_names,
            &right_types,
            &signatures
        ),
        Err(CompilationUnitTypeError::MismatchedNameEnvironment)
    ));
}

#[test]
fn append_only_name_extension_preserves_existing_external_bindings() {
    let mut names = NameEnvironment::new();
    let string = names.declare_type("String").unwrap();
    let wanted = names.declare_value("wanted").unwrap();
    let mut extended = names.clone();
    extended.declare_value("appended").unwrap();

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "main.ko",
        "package app\nfun entry(): String = wanted",
    );
    let (other_source, other_file) = parsed(
        &mut sources,
        "other.ko",
        "package app\nfun other(): String = wanted",
    );
    let inputs = [
        SourceUnitInput::new("root", "app/main.ko", source, &file),
        SourceUnitInput::new("root", "app/other.ko", other_source, &other_file),
    ];

    for (name_environment, type_names) in [(&extended, &names), (&names, &extended)] {
        let mut types = TypeEnvironment::new(type_names);
        types.bind_builtin(string, BuiltinType::String).unwrap();
        types
            .bind_value(wanted, EnvironmentType::Builtin(BuiltinType::String))
            .unwrap();
        let unit_names = validated_names(&sources, &inputs, name_environment);
        let single_names = resolve_names(&sources, &file, name_environment).unwrap();
        assert!(
            check_types(&sources, &file, &single_names, &types)
                .unwrap()
                .diagnostics()
                .is_empty()
        );
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &unit_names, &types)
                .unwrap()
                .validate()
                .unwrap();
        validate_compilation_unit_body_inputs(&sources, &inputs, &unit_names, &types, &signatures)
            .unwrap();
        assert!(
            check_compilation_unit_types(&sources, &inputs, &unit_names, &types)
                .unwrap()
                .validate()
                .is_ok()
        );
    }
}
