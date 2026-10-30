//! N1a 的能力来自宿主 SourceId 授权，不能来自路径、package 或同名类型。
use lang_frontend::{
    diagnostic::{Diagnostic, Severity, codes},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::{SourceId, SourceMap},
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(text: &str, authorized: bool) -> (Vec<String>, Vec<String>) {
    let mut sources = SourceMap::new();
    let text = if text.starts_with("package ") {
        text.to_owned()
    } else {
        format!("package koven\n{text}")
    };
    let source = sources.add_source("koven/algorithms.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    if authorized {
        types.authorize_range_source(&sources, source).unwrap();
    }
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &parsed, &names, &types).unwrap();
    let inputs = [SourceUnitInput::new(
        "koven-std",
        "koven/algorithms.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    (
        single
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect(),
        unit.diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect(),
    )
}

#[test]
fn authorized_source_can_declare_the_minimum_carrier_producer() {
    for text in [
        "fun produce(source: List<Int>): View<Int> from source",
        "fun <T> produce(source: List<T>): View<T> from source",
        "fun produce(source: View<String>): View<String> from source",
    ] {
        let (single, unit) = checked(text, true);
        assert!(single.is_empty(), "single {text}: {single:?}");
        assert!(unit.is_empty(), "unit {text}: {unit:?}");
    }
}

#[test]
fn std_path_package_and_type_spelling_do_not_authorize_a_source() {
    for text in [
        "fun produce(source: List<Int>): View<Int> from source",
        "package koven\nfun produce(source: List<Int>): View<Int> from source",
        "package koven\nclass View(val value: Int) {}\nfun produce(source: List<Int>): View from source",
    ] {
        let (single, unit) = checked(text, false);
        for codes in [single, unit] {
            assert!(codes.iter().any(|c| c == "L0164"), "{text}: {codes:?}");
        }
    }
}

#[test]
fn authority_rejects_foreign_map_and_does_not_spread_to_a_second_source() {
    let mut sources = SourceMap::new();
    let trusted = sources.add_source("trusted.ko", "").unwrap();
    let other = sources.add_source("koven/algorithms.ko", "").unwrap();
    let mut foreign = SourceMap::new();
    let same_index = foreign.add_source("trusted.ko", "").unwrap();
    let (_, mut environment) = standard_environments();
    assert!(
        environment
            .authorize_range_source(&sources, same_index)
            .is_err()
    );
    assert!(!environment.is_authorized_range_source(same_index));
    environment
        .authorize_range_source(&sources, trusted)
        .unwrap();
    assert!(environment.is_authorized_range_source(trusted));
    assert!(!environment.is_authorized_range_source(other));
    assert!(!environment.is_authorized_range_source(same_index));
}

#[test]
fn authorization_does_not_enable_extensions_or_invalid_carrier_contracts() {
    let (single, unit) = checked(
        "borrow fun List<Int>.take(count: Int): View<Int> from this",
        true,
    );
    for codes in [single, unit] {
        assert!(codes.iter().any(|c| c == "L0164"), "{codes:?}");
    }
    let (single, unit) = checked(
        "fun produce(own source: List<Int>): View<Int> from source",
        true,
    );
    for codes in [single, unit] {
        assert!(codes.iter().any(|c| c == "L0162"), "{codes:?}");
    }
}

// Stage A: 保留上面的所有 legacy witness；签名接受不授予 complete/ownership/native 资格。
fn expected_diagnostic(
    sources: &SourceMap,
    source: SourceId,
    code: &str,
    message: &str,
    token: &str,
) -> Diagnostic {
    let text = sources.source_text(source).unwrap();
    let start = text.rfind(token).expect("fixed diagnostic token");
    Diagnostic::new(
        sources,
        Severity::Error,
        codes::catalog().unwrap().resolve(code).unwrap(),
        message,
        sources.span(source, start, start + token.len()).unwrap(),
    )
    .unwrap()
}

#[test]
fn memory_document_signatures_preserve_full_authority_diagnostics() {
    let unauthorized = ("L0164", "carrier producer source is not authorized", "from");
    let shape = (
        "L0162",
        "carrier result and its List/View source must have the same element type",
        "source",
    );
    let cases = [
        (
            "fun produce(source: List<Int>): View<Int> from source",
            true,
            vec![],
        ),
        (
            "fun <T> produce(source: List<T>): View<T> from source",
            true,
            vec![],
        ),
        (
            "fun produce(source: View<String>): View<String> from source",
            true,
            vec![],
        ),
        (
            "fun produce(source: List<Int>): View<Int> from source",
            false,
            vec![unauthorized],
        ),
        (
            "class View(val value: Int) {}\nfun produce(source: List<Int>): View from source",
            false,
            vec![unauthorized, shape],
        ),
        (
            "borrow fun List<Int>.take(count: Int): View<Int> from this",
            true,
            vec![(
                "L0164",
                "extension receiver lacks a trusted canonical standard Borrow binding",
                ".",
            )],
        ),
        (
            "fun produce(own source: List<Int>): View<Int> from source",
            true,
            vec![("L0162", "carrier result source must be Borrow", "source")],
        ),
    ];
    for (text, authorized, expected) in cases {
        for locator in ["koven/algorithms.ko", "untitled:Authority"] {
            let mut sources = SourceMap::new();
            let decoy = sources.add_source("decoy.ko", "").unwrap();
            let source = sources
                .add_source(locator, format!("package koven\n{text}"))
                .unwrap();
            assert_ne!(source, decoy);
            let lexed = lex(&sources, source).unwrap();
            assert!(lexed.diagnostics().is_empty());
            let parsed = parse_file(&sources, &lexed).unwrap();
            assert!(parsed.diagnostics().is_empty());
            let (environment, mut types) = standard_environments();
            if authorized {
                types.authorize_range_source(&sources, source).unwrap();
            }
            assert_eq!(types.is_authorized_range_source(source), authorized);
            assert!(!types.is_authorized_range_source(decoy));
            assert!(!types.is_authorized_range_extension_source(source));
            let expected = expected
                .iter()
                .map(|&(code, message, token)| {
                    expected_diagnostic(&sources, source, code, message, token)
                })
                .collect::<Vec<_>>();
            // 固定完整 Diagnostic（severity/code/message/真实 Span/有序 details），不是 code 子集。
            let names = resolve_names(&sources, &parsed, &environment).unwrap();
            assert!(names.diagnostics().is_empty());
            let single = check_types(&sources, &parsed, &names, &types).unwrap();
            assert_eq!(single.diagnostics(), expected, "legacy {text}");
            for input in [
                SourceUnitInput::memory_document(source, &parsed),
                SourceUnitInput::new("koven-std", "koven/algorithms.ko", source, &parsed),
            ] {
                let inputs = [input];
                assert_eq!(inputs[0].source_id(), source);
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed =
                    check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
                assert_eq!(typed.diagnostics(), expected, "{:?}: {text}", input.key());
            }
        }
    }
}

#[test]
fn rooted_multi_source_authority_follows_source_id_not_input_order_or_package() {
    for trusted_first in [true, false] {
        for authorize_first in [true, false] {
            let mut sources = SourceMap::new();
            let first = sources
                .add_source(
                    "koven/algorithms.ko",
                    "package koven\nfun first(source: List<Int>): View<Int> from source",
                )
                .unwrap();
            let second = sources
                .add_source(
                    "koven/other.ko",
                    "package koven\nfun second(source: List<Int>): View<Int> from source",
                )
                .unwrap();
            let first_parsed = parse_file(&sources, &lex(&sources, first).unwrap()).unwrap();
            let second_parsed = parse_file(&sources, &lex(&sources, second).unwrap()).unwrap();
            assert!(first_parsed.diagnostics().is_empty());
            assert!(second_parsed.diagnostics().is_empty());
            let (environment, mut types) = standard_environments();
            let (trusted, untrusted) = if authorize_first {
                (first, second)
            } else {
                (second, first)
            };
            types.authorize_range_source(&sources, trusted).unwrap();
            assert!(types.is_authorized_range_source(trusted));
            assert!(!types.is_authorized_range_source(untrusted));
            let mut inputs = [
                SourceUnitInput::new("koven-std", "koven/algorithms.ko", first, &first_parsed),
                SourceUnitInput::new("koven-std", "koven/other.ko", second, &second_parsed),
            ];
            if (inputs[0].source_id() == trusted) != trusted_first {
                inputs.reverse();
            }
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert_eq!(
                typed.diagnostics(),
                [expected_diagnostic(
                    &sources,
                    untrusted,
                    "L0164",
                    "carrier producer source is not authorized",
                    "from",
                )]
            );
        }
    }
}
