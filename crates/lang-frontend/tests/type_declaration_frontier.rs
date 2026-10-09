//! 只完成声明语法时，single/unit 必须阻止 carrier 或扩展被当成普通 owned 函数。
use lang_frontend::{
    analysis::{SingleFileAnalysisError, SingleFileStage, analyze_single_file},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

const CASES: &[(&str, &str)] = &[
    ("fun Int.inspect(source: Int): Int = source", "."),
    (
        "borrow fun List<Int>.inspect(source: Int): Int = source",
        ".",
    ),
    ("own fun List<Int>.inspect(source: Int): Int = source", "."),
    ("fun view(source: Int): Int from source = source", "from"),
];

#[test]
fn single_frontier_declarations_have_an_explicit_typed_rejection_at_the_new_marker() {
    for &(text, marker) in CASES {
        let mut sources = SourceMap::new();
        let source = sources.add_source("frontier.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        let rejection = typed
            .diagnostics()
            .iter()
            .find(|d| d.code().to_string() == "L0164")
            .expect("new declaration must not become an ordinary owned callable");
        assert_eq!(sources.slice(rejection.primary_span()).unwrap(), marker);
        assert!(
            typed
                .callables()
                .iter()
                .all(|c| c.borrow_return().is_none())
        );
    }
}

#[test]
fn unit_frontier_rejection_prevents_validated_types_from_reaching_the_backend() {
    for &(text, marker) in CASES {
        let mut sources = SourceMap::new();
        let source = sources.add_source("frontier.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::new("root", "frontier.ko", source, &parsed)];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        let rejection = typed
            .diagnostics()
            .iter()
            .find(|d| d.code().to_string() == "L0164")
            .expect("unit frontier diagnostic");
        assert_eq!(sources.slice(rejection.primary_span()).unwrap(), marker);
        assert!(
            typed.validate().is_err(),
            "unsupported syntax must not acquire backend capability"
        );
    }
}

#[test]
fn single_analysis_stops_before_owned_output_or_backend_observation() {
    for &(text, _) in CASES {
        let mut sources = SourceMap::new();
        let source = sources.add_source("pipeline.ko", text).unwrap();
        let (environment, types) = standard_environments();
        let mut observed = false;
        let result = analyze_single_file(
            &sources,
            source,
            &environment,
            &types,
            |stage, diagnostics| {
                if diagnostics.is_empty() {
                    Ok(())
                } else {
                    Err(stage)
                }
            },
            |_| {
                observed = true;
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(SingleFileAnalysisError::Host(SingleFileStage::TypeChecking))
        ));
        assert!(!observed);
    }
}
