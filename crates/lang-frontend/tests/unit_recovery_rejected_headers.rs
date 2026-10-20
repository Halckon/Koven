//! 已有普通 source/header 错误不能隐藏 loop body 内独立的真实所有权错误。
//! 不查询、构造或改写任何私有 rejected-header/recovery 事实。

use lang_frontend::{
    analysis::{UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names, analyze_unit_recovery},
    diagnostic::{Diagnostic, Severity},
    ownership_checking::CompilationUnitOwnership,
    source::{SourceMap, Span},
    type_checking::{CompilationUnitTypes, TypeEnvironment, standard_environments},
};

fn memory(text: &str) -> (UnitNameSnapshot, TypeEnvironment) {
    let mut sources = SourceMap::new();
    let excluded = sources.add_source("excluded", "# absent").expect("real excluded registration");
    let source = sources.add_source("untitled:RejectedHeader", text).expect("ordinary source");
    assert_ne!(source, excluded);
    let (names, environment) = standard_environments();
    let snapshot = analyze_unit_names(
        sources, vec![UnitSourceDescriptor::memory_document(source)], names,
    )
    .expect("real name prefix");
    assert_eq!(snapshot.descriptors().len(), 1);
    assert_eq!(snapshot.descriptors()[0].source_id(), source);
    assert_eq!(snapshot.descriptors()[0].root_identity(), None);
    assert_eq!(snapshot.descriptors()[0].logical_path(), None);
    assert_eq!(snapshot.parsed_files().len(), 1);
    assert_eq!(snapshot.parsed_files()[0].source_id(), source);
    assert!(snapshot.parsed_files()[0].diagnostics().is_empty());
    assert_eq!(snapshot.sources().source_text(source).unwrap(), text);
    assert_eq!(snapshot.sources().source_text(excluded).unwrap(), "# absent");
    (snapshot, environment)
}

fn span_in(snapshot: &UnitNameSnapshot, context: &str, token: &str) -> Span {
    let source = snapshot.descriptors()[0].source_id();
    let text = snapshot.sources().source_text(source).unwrap();
    let contexts = text.match_indices(context).collect::<Vec<_>>();
    let tokens = context.match_indices(token).collect::<Vec<_>>();
    assert_eq!(contexts.len(), 1, "unique source context {context}");
    assert_eq!(tokens.len(), 1, "unique expected token {token}");
    let start = contexts[0].0 + tokens[0].0;
    snapshot.sources().span(source, start, start + token.len()).unwrap()
}

fn assert_diagnostics(diagnostics: &[Diagnostic], expected: &[(&str, Span)]) {
    assert_eq!(diagnostics.len(), expected.len(), "{diagnostics:?}");
    for (diagnostic, (code, span)) in diagnostics.iter().zip(expected) {
        assert_eq!(diagnostic.code().to_string(), *code);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.primary_span(), *span);
    }
}

fn assert_execution_facts_absent(typed: &CompilationUnitTypes, owned: &CompilationUnitOwnership) {
    assert!(typed.sequential_iterations().is_empty());
    assert!(typed.map_iterations().is_empty());
    assert!(owned.iterations().is_empty());
    assert!(owned.map_iterations().is_empty());
    assert!(owned.formed_owned_locals().is_empty());
    assert!(owned.loans().is_empty());
    assert!(owned.value_deliveries().is_empty());
    assert!(owned.receiver_facts().is_empty());
    assert!(owned.conditional_receiver_deliveries().is_empty());
    assert!(owned.construction_plans().is_empty());
    assert!(owned.non_null_assertions().is_empty());
    assert!(owned.ownership_primitives().is_empty());
    assert!(owned.field_replacements().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.conditional_receiver_drops().is_empty());
    assert!(typed.clone().validate().is_err());
    assert!(typed.clone().validate_constants().is_err());
    assert!(owned.clone().validate().is_err());
}

fn observe(
    text: &str,
    name_error: Option<(&str, &str)>,
    type_error: Option<(&str, &str)>,
) {
    let (snapshot, environment) = memory(text);
    let expected_names = name_error.map(|(code, token)| {
        (code, span_in(&snapshot, token, token))
    }).into_iter().collect::<Vec<_>>();
    assert_diagnostics(snapshot.names().diagnostics(), &expected_names);
    assert_eq!(snapshot.validated_names().is_some(), name_error.is_none());
    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
        .expect("source/header errors must not become missing-provider internal failures");
    let expected_types = type_error.map(|(code, token)| {
        (code, span_in(&snapshot, token, token))
    }).into_iter().collect::<Vec<_>>();
    assert_diagnostics(typed.diagnostics(), &expected_types);
    assert!(typed.signatures().diagnostics().is_empty());
    assert_eq!(typed.signatures().clone().validate().is_ok(), name_error.is_none());
    assert_diagnostics(
        owned.diagnostics(),
        &[("L0133", span_in(&snapshot, "take(text)", "text"))],
    );
    assert_eq!(typed.calls().len(), 1, "actual independent consuming body call");
    assert_eq!(owned.call_argument_contracts().len(), 1);
    assert!(owned.deferred().is_empty(), "keep the real error, not a deferred substitute");
    assert_execution_facts_absent(&typed, &owned);
}

#[test]
fn invalid_scalar_source_preserves_l0159_and_independent_body_consumption() {
    observe(
        "fun take(own item: String): Unit {}\n\
         fun run(text: String): Unit { for (item in 1) { take(text)\nbreak } }",
        None,
        Some(("L0159", "1")),
    );
}

#[test]
fn invalid_map_binding_preserves_l0160_and_independent_body_consumption() {
    observe(
        "fun take(own item: String): Unit {}\n\
         fun run(xs: Map<Int, String>, text: String): Unit {\n\
           for (entry in xs) { take(text)\nbreak }\n\
         }",
        None,
        Some(("L0160", "entry")),
    );
}

#[test]
fn map_argument_error_preserves_original_name_error_and_independent_body_consumption() {
    observe(
        "fun take(own item: String): Unit {}\n\
         fun run(xs: Map<Int, Missing>, text: String): Unit {\n\
           for ((key, value) in xs) { take(text)\nbreak }\n\
         }",
        Some(("L0080", "Missing")),
        None,
    );
}

#[test]
fn nominal_field_error_preserves_original_name_error_and_independent_body_consumption() {
    observe(
        "value class Pair(val a: Int, val b: Missing)\n\
         fun take(own item: String): Unit {}\n\
         fun run(xs: List<Pair>, text: String): Unit {\n\
           for ((a, b) in xs) { take(text)\nbreak }\n\
         }",
        Some(("L0080", "Missing")),
        None,
    );
}
