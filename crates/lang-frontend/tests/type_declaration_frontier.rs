//! 只完成声明语法时，不授予 borrow-return；Unit 严格链不得推进到 owned 能力。
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        FinalTypeAdmissionError, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, standard_environments,
    },
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
fn unit_frontier_rejects_at_the_marker_without_publishing_borrow_return_or_ownership() {
    for &(text, marker) in CASES {
        let mut sources = SourceMap::new();
        let source = sources.add_source("frontier.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
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
            .expect("new declaration must not become an ordinary owned callable");
        let start = text.find(marker).unwrap();
        assert_eq!(
            rejection.primary_span(),
            sources.span(source, start, start + marker.len()).unwrap()
        );
        assert_eq!(sources.slice(rejection.primary_span()).unwrap(), marker);
        assert!(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|declaration| declaration.callable())
                .all(|callable| callable.borrow_return().is_none())
        );
        let before = typed.clone();
        let mut ownership_started = false;
        let result = (|| {
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)?;
            ownership_started = true;
            let _ = analyze_basic_unit_ownership(
                &sources,
                &inputs,
                &names,
                &types,
                typed.clone(),
            )
            .expect("ownership internal failures are not source rejection");
            Ok::<(), FinalTypeAdmissionError>(())
        })();
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = result else {
            panic!("frontier must fail with real final type diagnostics")
        };
        assert_eq!(diagnostics, before.diagnostics());
        assert!(!ownership_started, "final admission precedes actual ownership");
        assert_eq!(typed, before, "strict failure cannot rewrite raw facts");
        let rejected = typed
            .validate()
            .expect_err("unsupported syntax must not acquire backend capability");
        assert_eq!(*rejected, before);
    }
}
