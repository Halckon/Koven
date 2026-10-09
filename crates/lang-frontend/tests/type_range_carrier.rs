//! N1a compiler-bound 范围类型的身份和使用位置；不运行算法或 native。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn stage_codes(text: &str) -> (Vec<String>, Vec<String>) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("range_type.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let mut single: Vec<_> = names
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    single.extend(
        check_types(&sources, &parsed, &names, &types)
            .unwrap()
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string()),
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "range_type.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    let mut unit: Vec<_> = names
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    if let Ok(names) = names.validate() {
        unit.extend(
            check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string()),
        );
    }
    (single, unit)
}

#[test]
fn compiler_bound_view_is_available_as_a_non_owning_parameter() {
    for text in [
        "fun inspect(source: View<Int>): Unit {}",
        "fun inspect(borrow source: View<String>): Unit {}",
        "class Item(val number: Int) {}\nfun inspect(source: View<Item>): Unit {}",
    ] {
        let (single, unit) = stage_codes(text);
        assert!(single.is_empty(), "single {text}: {single:?}");
        assert!(unit.is_empty(), "unit {text}: {unit:?}");
    }
}

#[test]
fn range_type_cannot_be_optional_owned_or_stored_in_an_aggregate() {
    for text in [
        "fun reject(source: View<Int>?): Unit {}",
        "fun reject(own source: View<Int>): Unit {}",
        "fun reject(inout source: View<Int>): Unit {}",
        "class Holder(val source: View<Int>) {}",
        "fun reject(source: List<View<Int>>): Unit {}",
        "fun reject(source: Map<Int, View<String>>): Unit {}",
    ] {
        let (single, unit) = stage_codes(text);
        for (label, codes) in [("single", single), ("unit", unit)] {
            assert!(
                codes.iter().any(|c| c == "L0163"),
                "{label} must reject non-escaping storage, {text}: {codes:?}"
            );
        }
    }
}

#[test]
fn plain_result_cannot_deliver_a_range_as_an_owned_value() {
    let text = "fun reject(source: View<Int>): View<Int> = source";
    let (single, unit) = stage_codes(text);
    for codes in [single, unit] {
        assert!(codes.iter().any(|c| c == "L0162"), "{codes:?}");
    }
}

#[test]
fn source_nominal_named_view_does_not_gain_carrier_restrictions() {
    let text = "class View(val value: Int) {}\nclass Holder(val value: View) {}\nfun ordinary(own source: View): View = source";
    let (single, unit) = stage_codes(text);
    assert!(single.is_empty(), "{single:?}");
    assert!(unit.is_empty(), "{unit:?}");
}

#[test]
fn inferred_local_and_generic_argument_cannot_hide_a_range_carrier() {
    for text in [
        "fun reject(source: View<Int>): Unit { val saved = source }",
        "fun reject(source: View<Int>): Unit { var saved = source }",
        "fun reject(source: View<Int>): Unit { listOf(source) }",
        "fun <T> accept(source: T): Unit {}\nfun reject(source: View<Int>): Unit { accept(source) }",
        "fun <T> accept(source: T): Unit {}\nfun reject(source: View<Int>): Unit { accept<View<Int>>(source) }",
    ] {
        let (single, unit) = stage_codes(text);
        for (label, codes) in [("single", single), ("unit", unit)] {
            assert!(
                codes.iter().any(|c| c == "L0163"),
                "{label} {text}: {codes:?}"
            );
        }
    }
}

#[test]
fn new_carrier_source_contract_rejects_invalid_parameters_and_result_shapes() {
    for text in [
        "fun reject(own source: List<Int>): View<Int> from source {}",
        "fun reject(inout source: List<Int>): View<Int> from source {}",
        "fun reject(source: List<Int>): View<Int> from missing {}",
        "fun reject(source: List<Int>): View<Int> from this {}",
        "fun reject(source: String): View<Int> from source {}",
        "fun reject(source: List<String>): View<Int> from source {}",
        "fun reject(source: List<Int>): Int from source {}",
    ] {
        let (single, unit) = stage_codes(text);
        for (label, codes) in [("single", single), ("unit", unit)] {
            assert!(
                codes.iter().any(|c| c == "L0162"),
                "{label} {text}: {codes:?}"
            );
        }
    }
}

#[test]
fn carrier_delivery_contract_preserves_real_from_and_named_source_mapping_under_guard() {
    use lang_frontend::type_checking::{BorrowReturnOrigin, CallableResultSource};
    let mut sources = SourceMap::new();
    let source = sources.add_source("range-contract.ko", "fun fresh(aux: Int, source: List<Int>): View<Int> from source\nfun alias(source: View<Int>): borrow View<Int> from source = source\nfun caller(source: List<Int>): Unit { borrow val part = fresh(source = source, aux = 0) }").unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    let call = typed
        .calls()
        .iter()
        .find(|c| matches!(c.result_source(), CallableResultSource::Carrier(_)))
        .unwrap();
    let CallableResultSource::Carrier(contract) = call.result_source() else {
        unreachable!()
    };
    assert_eq!(contract.origin(), BorrowReturnOrigin::Parameter(1));
    assert_eq!(
        sources.slice(contract.from_span().unwrap()).unwrap(),
        "from"
    );
    assert_eq!(sources.slice(contract.source_span()).unwrap(), "source");
    assert_eq!(call.arguments()[0].parameter_index(), 1);
    assert!(call.borrow_return().is_none());
    assert!(
        typed
            .callables()
            .iter()
            .any(|c| matches!(c.result_source(), CallableResultSource::Borrow(_)))
    );
    assert!(
        typed
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164"
                && d.primary_span() == contract.from_span().unwrap())
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "range-contract.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let call = typed
        .calls()
        .iter()
        .find(|c| matches!(c.result_source(), CallableResultSource::Carrier(_)))
        .unwrap();
    assert_eq!(
        call.result_source(),
        CallableResultSource::Carrier(contract)
    );
    assert_eq!(call.arguments()[0].parameter_index(), 1);
    assert!(call.borrow_return().is_none());
    assert!(
        typed
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164"
                && d.primary_span() == contract.from_span().unwrap())
    );
    assert!(
        typed.validate().is_err(),
        "declaration facts are not ownership proof"
    );
}
