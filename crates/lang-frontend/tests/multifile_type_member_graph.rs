//! SPEC-0197 compilation-unit member/interface contract graph。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    lexer::lex,
    name_resolution::{
        NameEnvironment, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitSignatures, UnitTypeKind, collect_compilation_unit_signatures,
        standard_environments,
    },
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique test source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

fn signatures(files: &[(&str, &str)]) -> (SourceMap, CompilationUnitSignatures) {
    let mut sources = SourceMap::new();
    let parsed = files
        .iter()
        .map(|(path, text)| parsed(&mut sources, path, text))
        .collect::<Vec<_>>();
    let inputs = files
        .iter()
        .zip(&parsed)
        .map(|((path, _), (source, parsed))| SourceUnitInput::new("root", path, *source, parsed))
        .collect::<Vec<_>>();
    let (name_environment, type_environment) = standard_environments();
    let names = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid compilation-unit names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
            .expect("signature graph succeeds");
    (sources, signatures)
}

fn names<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> lang_frontend::name_resolution::CompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("unit names resolve internally")
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn diagnostic_spans<'a>(sources: &'a SourceMap, diagnostic: &Diagnostic) -> (&'a str, &'a str) {
    let primary = sources
        .slice(diagnostic.primary_span())
        .expect("primary span");
    let label = diagnostic
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("related label");
    (primary, label)
}

#[test]
fn concrete_body_and_cross_file_interface_replacement_keep_l0098_l0099_spans() {
    let (sources, signatures) = signatures(&[
        (
            "p/base.ko",
            "package p\ninterface Base { fun act(input: Int): Int }",
        ),
        (
            "p/child.ko",
            "package p\ninterface Child : Base { fun act(input: Int): Long }",
        ),
        (
            "p/concrete.ko",
            "package p\nclass Concrete { fun absent(): Unit }",
        ),
    ]);

    assert_eq!(codes(signatures.diagnostics()), ["L0099", "L0098"]);
    assert_eq!(
        diagnostic_spans(&sources, &signatures.diagnostics()[0]),
        ("act", "act")
    );
    assert_eq!(
        diagnostic_spans(&sources, &signatures.diagnostics()[1]),
        ("absent", "Concrete")
    );
}

#[test]
fn override_errors_consume_the_same_shape_before_missing_requirement_checks() {
    let (_, signatures) = signatures(&[
        (
            "p/contract.ko",
            "package p\ninterface Required { fun run(input: Int): Int }",
        ),
        (
            "p/implementations.ko",
            "package p\n\
             class Missing : Required {}\n\
             class NeedsOverride : Required { fun run(input: Int): Int = 1 }\n\
             class BadReturn : Required { override fun run(input: Int): Long = 1L }\n\
             class Extra { override fun lone(): Unit {} }\n\
             class Hidden : Required { private override fun run(input: Int): Int = 1 }\n\
             class Good : Required { override fun run(borrow input: Int): Int = 1 }",
        ),
    ]);

    assert_eq!(
        codes(signatures.diagnostics()),
        ["L0101", "L0100", "L0100", "L0100", "L0100"]
    );
}

#[test]
fn generic_substitution_defaults_and_companion_members_preserve_instance_contracts() {
    let (_, signatures) = signatures(&[
        (
            "p/contracts.ko",
            "package p\n\
             interface Required<T> { fun apply(input: T): T }\n\
             interface Left { fun ping(): Int = 1 }\n\
             interface Right { fun ping(): Int = 2 }",
        ),
        (
            "p/implementations.ko",
            "package p\n\
             class Good<U> : Required<U> { override fun apply(input: U): U = input }\n\
             class Conflict : Left, Right {}\n\
             class CompanionOnly : Required<Int> {\n\
                 companion object { fun apply(input: Int): Int = input }\n\
             }",
        ),
    ]);

    assert_eq!(codes(signatures.diagnostics()), ["L0102", "L0101"]);
}

#[test]
fn instance_and_companion_callable_shapes_use_distinct_scopes() {
    let (_, signatures) = signatures(&[(
        "p/scoped.ko",
        "package p\n\
         class Scoped {\n\
             fun same(input: Int): Unit {}\n\
             companion object { fun same(input: Int): Unit {} }\n\
         }",
    )]);

    assert!(
        signatures.diagnostics().is_empty(),
        "{:?}",
        signatures.diagnostics()
    );
    let scoped = signatures
        .declarations()
        .iter()
        .find_map(|declaration| declaration.nominal())
        .expect("Scoped nominal");
    assert_eq!(scoped.members().len(), 1);
    assert_eq!(scoped.companion_members().len(), 1);
    assert_eq!(scoped.members()[0].name(), "same");
    assert_eq!(scoped.companion_members()[0].name(), "same");
}

#[test]
fn delegation_validation_suppresses_derivative_missing_member_diagnostics() {
    let (sources, signatures) = signatures(&[
        (
            "p/contracts.ko",
            "package p\n\
             interface Draw { fun draw(): Unit }\n\
             interface OtherContract { fun draw(): Unit }\n\
             interface Sink<T> { fun send(input: T): Unit }\n\
             class Renderer : Draw { override fun draw(): Unit {} }\n\
             class IntSink : Sink<Int> { override fun send(input: Int): Unit {} }\n\
             class Other {}",
        ),
        (
            "p/delegates.ko",
            "package p\n\
             class Mutable(var renderer: Renderer) : Draw by renderer {}\n\
             class Wrong(val other: Other) : OtherContract by other {}\n\
             class Valid(val sink: IntSink) : Sink<Int> by sink {}",
        ),
    ]);

    assert_eq!(codes(signatures.diagnostics()), ["L0103", "L0104"]);
    let [plan] = signatures.delegations() else {
        panic!("expected exactly one valid delegation plan");
    };
    let owner = signatures
        .declaration(plan.owner())
        .and_then(|declaration| declaration.nominal())
        .expect("delegation owner nominal");
    assert_eq!(plan.interface(), owner.direct_interfaces()[0]);
    assert_eq!(plan.target(), owner.fields()[0].symbol());
    let Some(UnitTypeKind::Nominal { arguments, .. }) = signatures.types().get(plan.interface())
    else {
        panic!("delegated interface must retain its nominal instance");
    };
    assert_eq!(arguments.len(), 1);
    assert_eq!(
        signatures.types().get(arguments[0]),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert_eq!(sources.slice(plan.by_span()).expect("by span"), "by");
    assert_eq!(
        sources
            .slice(plan.delegation_span())
            .expect("delegation span"),
        "by sink"
    );
}

#[test]
fn rejected_duplicate_hierarchy_edge_cannot_supply_a_delegate() {
    let (_, signatures) = signatures(&[
        (
            "p/contracts.ko",
            "package p\n\
             interface Draw { fun draw(): Unit }\n\
             class Renderer : Draw { override fun draw(): Unit {} }",
        ),
        (
            "p/duplicate.ko",
            "package p\n\
             class Duplicate(val renderer: Renderer) : Draw, Draw by renderer {}",
        ),
    ]);

    assert_eq!(codes(signatures.diagnostics()), ["L0101", "L0095"]);
}

#[test]
fn multiple_delegates_and_delegate_plus_foreign_default_emit_l0105() {
    let (sources, signatures) = signatures(&[
        (
            "p/contracts.ko",
            "package p\n\
             interface Draw { fun act(): Unit }\n\
             interface Reset { fun act(): Unit }\n\
             interface Default { fun act(): Unit {} }\n\
             class Drawer : Draw { override fun act(): Unit {} }\n\
             class Resetter : Reset { override fun act(): Unit {} }",
        ),
        (
            "p/delegates.ko",
            "package p\n\
             class Two(val draw: Drawer, val reset: Resetter)\n\
                 : Draw by draw, Reset by reset {}\n\
             class Mixed(val draw: Drawer) : Draw by draw, Default {}\n\
             class Resolved(val draw: Drawer, val reset: Resetter)\n\
                 : Draw by draw, Reset by reset { override fun act(): Unit {} }",
        ),
    ]);

    assert_eq!(codes(signatures.diagnostics()), ["L0105", "L0105"]);
    for diagnostic in signatures.diagnostics() {
        assert_eq!(
            sources
                .slice(diagnostic.primary_span())
                .expect("L0105 primary"),
            "by"
        );
        assert_eq!(diagnostic.details().len(), 2);
    }
}
