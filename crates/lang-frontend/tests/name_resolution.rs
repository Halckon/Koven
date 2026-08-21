//! SPEC-0018 单文件作用域、symbol 与名称诊断契约测试。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{
        NameEnvironment, Namespace, ReferenceTarget, ScopeKind, SymbolKind, resolve_names,
    },
    parser::ParsedFile,
    source::SourceMap,
};
use std::{fs, path::Path};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

fn parsed(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("names.ko", text).expect("unique source");
    let parsed = parse_file_twice(&sources, source, "name resolution source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (sources, parsed)
}

fn environment() -> NameEnvironment {
    let mut environment = NameEnvironment::new();
    for name in ["Any", "Boolean", "Int", "Items", "String", "Unit"] {
        environment.declare_type(name).expect("unique type");
    }
    environment
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn span_text<'a>(sources: &'a SourceMap, diagnostic: &Diagnostic) -> &'a str {
    sources
        .slice(diagnostic.primary_span())
        .expect("valid primary")
}

#[test]
fn empty_file_and_explicit_environment_are_stable_and_non_mutating() {
    let (sources, parsed) = parsed("");
    let mut environment = environment();
    let external = environment.declare_function("print").expect("function");
    let before = environment.clone();
    let first = resolve_names(&sources, &parsed, &environment).expect("resolve");
    let second = resolve_names(&sources, &parsed, &environment).expect("resolve again");

    assert_eq!(environment, before);
    assert_eq!(first, second);
    assert_eq!(environment.symbols().last().expect("print").id(), external);
    assert_eq!(first.scopes().len(), 1);
    assert_eq!(first.scopes()[0].kind(), ScopeKind::File);
    assert!(first.symbols().is_empty());
    assert!(first.references().is_empty());
    assert!(first.diagnostics().is_empty());
}

#[test]
fn top_level_types_values_forward_references_and_overloads_resolve() {
    let text = "val first: Later = later\n\
                fun call(x: Int): Int = target(x)\n\
                fun target(x: Int): Int = x\n\
                fun target(x: Int, y: Int): Int = x\n\
                class Later(val field: Int)\n\
                val later: Later = Later(field = 1)";
    let (sources, parsed) = parsed(text);
    let resolution = resolve_names(&sources, &parsed, &environment()).expect("resolve");

    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let targets: Vec<_> = resolution
        .symbols()
        .iter()
        .filter(|s| s.name() == "target")
        .collect();
    assert_eq!(targets.len(), 2);
    let target_reference = resolution
        .references()
        .iter()
        .find(|reference| sources.slice(reference.span()).expect("span") == "target")
        .expect("target reference");
    assert_eq!(target_reference.namespace(), Namespace::Value);
    assert_eq!(
        target_reference.target(),
        &ReferenceTarget::OverloadSet(targets.iter().map(|symbol| symbol.id()).collect())
    );
    assert!(resolution.references().iter().any(|reference| {
        sources.slice(reference.span()).expect("span") == "Later"
            && reference.namespace() == Namespace::Type
    }));
}

#[test]
fn members_are_predeclared_and_companion_does_not_inherit_instance_values() {
    let text = "class Service(val field: Int) {\n\
                    fun read(): Int = helper(field)\n\
                    fun helper(arg: Int): Int = arg\n\
                    companion object {\n\
                        const val VERSION: Int = 1\n\
                        fun version(): Int = VERSION\n\
                        fun invalid(): Int = field\n\
                    }\n\
                }";
    let (sources, parsed) = parsed(text);
    let resolution = resolve_names(&sources, &parsed, &environment()).expect("resolve");

    assert_eq!(codes(resolution.diagnostics()), ["L0080"]);
    assert_eq!(span_text(&sources, &resolution.diagnostics()[0]), "field");
    assert!(
        resolution
            .scopes()
            .iter()
            .any(|scope| scope.kind() == ScopeKind::Companion)
    );
}

#[test]
fn enum_cases_share_identity_across_namespaces_and_preserve_payload_candidates() {
    let text = "enum class Shape {\n\
                    Circle(radius: Int), Point;\n\
                    fun area(): Int = when (this) {\n\
                        is Circle -> radius\n\
                        is Point -> 0\n\
                    }\n\
                }\n\
                fun inspect(shape: Shape): Boolean = shape is Shape.Circle";
    let (sources, parsed) = parsed(text);
    let resolution = resolve_names(&sources, &parsed, &environment()).expect("resolve");

    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    assert_eq!(resolution.enum_cases().len(), 2);
    let circle = &resolution.enum_cases()[0];
    assert_ne!(circle.value_symbol(), circle.type_symbol());
    assert_eq!(circle.payloads().len(), 1);
    assert_eq!(
        resolution.symbols()[circle.value_symbol().index()].kind(),
        SymbolKind::EnumVariant
    );
    assert_eq!(
        resolution.symbols()[circle.type_symbol().index()].kind(),
        SymbolKind::EnumCaseType
    );
    assert!(resolution.references().iter().any(|reference| {
        sources.slice(reference.span()).expect("span") == "radius"
            && matches!(
                reference.target(),
                ReferenceTarget::EnumCasePayloadCandidates(candidates)
                    if candidates == circle.payloads()
            )
    }));
    assert!(resolution.references().iter().any(|reference| {
        sources.slice(reference.span()).expect("span") == "Circle"
            && reference.namespace() == Namespace::Type
            && reference.target() == &ReferenceTarget::Symbol(circle.type_symbol())
    }));
}

#[test]
fn qualified_enum_case_resolution_rejects_unknown_tail_segments() {
    let text = "enum class Shape { Circle(radius: Int), Point }
                fun invalid(shape: Shape): Boolean {
                    val missing = Shape.Unknown
                    return shape is Shape.Missing
                }";
    let (sources, parsed) = parsed(text);
    let resolution = resolve_names(&sources, &parsed, &environment()).expect("resolve");

    assert_eq!(codes(resolution.diagnostics()), ["L0080", "L0080"]);
    assert_eq!(
        resolution
            .diagnostics()
            .iter()
            .map(|diagnostic| span_text(&sources, diagnostic))
            .collect::<Vec<_>>(),
        ["Unknown", "Missing"]
    );
}

#[test]
fn locals_bind_after_initializer_and_nested_scopes_shadow() {
    let text = "fun demo(base: Int): Int {\n\
                    val first = later\n\
                    val later = base\n\
                    val callback = { base -> base + later }\n\
                    val base = first\n\
                    return base\n\
                }";
    let (sources, parsed) = parsed(text);
    let resolution = resolve_names(&sources, &parsed, &environment()).expect("resolve");

    assert_eq!(codes(resolution.diagnostics()), ["L0081"]);
    assert_eq!(span_text(&sources, &resolution.diagnostics()[0]), "later");
    let label = resolution.diagnostics()[0]
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label),
            _ => None,
        })
        .expect("later declaration label");
    assert_eq!(sources.slice(label.span()).expect("label"), "later");
    let outer_symbols: Vec<_> = resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "base")
        .collect();
    assert_eq!(outer_symbols.len(), 3);
    assert_ne!(outer_symbols[0].scope(), outer_symbols[1].scope());
}

#[test]
fn lambda_for_and_destructuring_bindings_resolve_in_their_owners() {
    let text = "fun demo(items: Items): Int {\n\
                    val outer = 1\n\
                    val callback = { item -> item + outer }\n\
                    for ((item, _) in items) { val copy = item }\n\
                    val (left, right) = pair\n\
                    return left\n\
                }";
    let (sources, parsed) = parsed(text);
    let mut environment = environment();
    environment.declare_value("pair").expect("pair");
    let resolution = resolve_names(&sources, &parsed, &environment).expect("resolve");

    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    for kind in [
        SymbolKind::LambdaParameter,
        SymbolKind::ForBinding,
        SymbolKind::DestructuringBinding,
    ] {
        assert!(
            resolution
                .symbols()
                .iter()
                .any(|symbol| symbol.kind() == kind),
            "{kind:?}"
        );
    }
    assert!(
        !resolution
            .symbols()
            .iter()
            .any(|symbol| symbol.name() == "_")
    );
}

#[test]
fn duplicate_names_report_second_span_and_first_label_but_functions_overload() {
    let text = "class Same\n\
                class Same\n\
                val datum = 1\n\
                fun datum(): Unit {}\n\
                fun repeated(x: Int): Int = x\n\
                fun repeated(x: Int, x: Int): Int = x";
    let (sources, parsed) = parsed(text);
    let resolution = resolve_names(&sources, &parsed, &environment()).expect("resolve");

    assert_eq!(codes(resolution.diagnostics()), ["L0079", "L0079", "L0079"]);
    assert_eq!(
        resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == "repeated")
            .count(),
        2
    );
    for diagnostic in resolution.diagnostics() {
        assert!(
            diagnostic
                .details()
                .iter()
                .any(|detail| matches!(detail, DiagnosticDetail::Label(_)))
        );
        assert!(!span_text(&sources, diagnostic).is_empty());
    }
}

#[test]
fn unresolved_value_and_type_are_distinct_references_while_member_names_are_deferred() {
    let text = "fun demo(input: Missing): Unit {\n\
                    val first = unknown.member\n\
                    val second = input.deferred\n\
                    val qualified: Known.Deferred = input\n\
                }";
    let (sources, parsed) = parsed(text);
    let mut environment = environment();
    environment.declare_type("Known").expect("Known");
    let resolution = resolve_names(&sources, &parsed, &environment).expect("resolve");

    assert_eq!(codes(resolution.diagnostics()), ["L0080", "L0080"]);
    assert_eq!(span_text(&sources, &resolution.diagnostics()[0]), "Missing");
    assert_eq!(span_text(&sources, &resolution.diagnostics()[1]), "unknown");
    assert!(!resolution.references().iter().any(|reference| {
        matches!(
            sources.slice(reference.span()),
            Ok("member" | "deferred" | "Deferred")
        )
    }));
}

#[test]
fn type_and_value_names_can_coexist_and_external_functions_keep_order() {
    let text = "class Entry\nval Entry = 1\nfun invoke(arg: Entry): Int = external(arg)";
    let (sources, parsed) = parsed(text);
    let mut environment = environment();
    let first = environment.declare_function("external").expect("first");
    let second = environment.declare_function("external").expect("second");
    let resolution = resolve_names(&sources, &parsed, &environment).expect("resolve");

    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let reference = resolution
        .references()
        .iter()
        .find(|reference| sources.slice(reference.span()).expect("span") == "external")
        .expect("external reference");
    assert_eq!(
        reference.target(),
        &ReferenceTarget::ExternalOverloadSet(vec![first, second])
    );
}

#[test]
fn no_prelude_is_implicit_and_foreign_source_maps_fail_loudly() {
    let (sources, parsed) = parsed("val answer: Int = 1");
    let resolution = resolve_names(&sources, &parsed, &NameEnvironment::new()).expect("resolve");
    assert_eq!(codes(resolution.diagnostics()), ["L0080"]);

    let foreign = SourceMap::new();
    assert!(resolve_names(&foreign, &parsed, &NameEnvironment::new()).is_err());
}

#[test]
fn generic_bounds_objects_and_each_loop_form_build_the_expected_namespaces() {
    let text = "class Bound\n\
                class Holder<T: Bound>(val item: T)\n\
                object Config { fun get(): Config = Config }\n\
                fun walk(items: Items): Unit {\n\
                    while (ready) { val current = items }\n\
                    loop { return }\n\
                }";
    let (sources, parsed) = parsed(text);
    let mut environment = environment();
    environment.declare_value("ready").expect("ready");
    let resolution = resolve_names(&sources, &parsed, &environment).expect("resolve");

    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let config: Vec<_> = resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "Config")
        .collect();
    assert_eq!(config.len(), 2);
    assert_eq!(config[0].namespace(), Namespace::Type);
    assert_eq!(config[1].namespace(), Namespace::Value);
    assert_eq!(
        resolution
            .scopes()
            .iter()
            .filter(|scope| scope.kind() == ScopeKind::Loop)
            .count(),
        2
    );
    assert!(
        resolution
            .symbols()
            .iter()
            .any(|symbol| symbol.kind() == SymbolKind::TypeParameter)
    );
}

#[test]
fn checked_in_phase2_pass_and_fail_fixtures_are_executed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase2");
    let pass_dir = root.join("name-pass");
    let fail_dir = root.join("name-fail");
    let pass_files: Vec<_> = fs::read_dir(&pass_dir)
        .expect("name-pass fixture directory")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
        .collect();
    let fail_files: Vec<_> = fs::read_dir(&fail_dir)
        .expect("name-fail fixture directory")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
        .collect();
    assert_eq!(pass_files.len(), 1, "zero or unexpected pass fixtures");
    assert_eq!(fail_files.len(), 1, "zero or unexpected fail fixtures");

    for path in pass_files {
        let text = fs::read_to_string(&path).expect("UTF-8 pass fixture");
        let (sources, parsed) = parsed(&text);
        let resolution = resolve_names(&sources, &parsed, &NameEnvironment::new())
            .expect("resolve pass fixture");
        assert!(
            resolution.diagnostics().is_empty(),
            "{path:?}: {:?}",
            resolution.diagnostics()
        );
    }
    for path in fail_files {
        let text = fs::read_to_string(&path).expect("UTF-8 fail fixture");
        let expected = fs::read_to_string(path.with_extension("diag")).expect("diagnostic sidecar");
        let (sources, parsed) = parsed(&text);
        let mut environment = NameEnvironment::new();
        environment.declare_type("Unit").expect("Unit");
        let resolution =
            resolve_names(&sources, &parsed, &environment).expect("resolve fail fixture");
        let actual = resolution
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                let span = diagnostic.primary_span();
                format!("{}\t{}\t{}", diagnostic.code(), span.start(), span.end())
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(actual, expected, "{path:?}");
    }
}
