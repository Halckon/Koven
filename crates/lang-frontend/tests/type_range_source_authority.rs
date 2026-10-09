//! N1a 的能力来自宿主 SourceId 授权，不能来自路径、package 或同名类型。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
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
