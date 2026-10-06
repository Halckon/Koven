//! 先验证现有两文件阶段支持，接口红测不把编译夹具错误算行为红测。
use super::*;
use lang_frontend::ownership_checking::{
    OwnedCompilationUnitViewError, owned_compilation_unit_view,
};

#[path = "callable_contract.rs"]
mod contract;

struct Fixture {
    sources: SourceMap,
    provider_source: SourceId,
    provider: ParsedFile,
    consumer_source: SourceId,
    consumer: ParsedFile,
    names: ValidatedCompilationUnitNames,
    typed: ValidatedCompilationUnitTypes,
    owned: CompilationUnitOwnership,
    environment: TypeEnvironment,
}

fn preflight(provider_text: &str, consumer_text: &str) -> Fixture {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/api.ko", provider_text);
    let (consumer_source, consumer) = parsed(&mut sources, "q/use.ko", consumer_text);
    let inputs = [
        SourceUnitInput::new("root", "p/api.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/use.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &environment);
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("unit ownership");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    owned.clone().validate().expect("complete unit ownership");
    Fixture {
        sources,
        provider_source,
        provider,
        consumer_source,
        consumer,
        names,
        typed,
        owned,
        environment,
    }
}

const PROVIDER: &str = "package p\n\
    fun identity(index: Int): Int = index\n\
    fun other(index: Int): Int = index + 1\n\
    fun take(callback: (Int) -> Int): Unit {}\n";
const IMPORTS: &str = "package q\nimport p.identity\nimport p.other\nimport p.take\n";

#[test]
fn callable_provenance_preflight_bare_function_alias_group_and_parameter() {
    let consumer = format!(
        "{IMPORTS}\
        fun relay(forwarded: (Int) -> Int): Unit {{ take((forwarded)) }}\n\
        fun use(): Unit {{\n\
            take(identity)\n\
            val named = identity\nval alias = (named)\ntake(alias)\n\
            val lambda: (Int) -> Int = {{ index -> index }}\n\
            val lambdaAlias = (lambda)\ntake(lambdaAlias)\n\
        }}"
    );
    let fixture = preflight(PROVIDER, &consumer);
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    let callback = symbol_named(&fixture.owned, &fixture.names, unit, "forwarded");
    assert_eq!(
        fixture
            .typed
            .types()
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.callable())
            .flat_map(|callable| callable.parameters())
            .find(|parameter| parameter.symbol() == Some(callback))
            .map(|parameter| parameter.mode()),
        Some(ParameterMode::Borrow)
    );
    assert!(
        fixture
            .owned
            .loans()
            .iter()
            .any(|loan| loan.kind() == LoanKind::Shared)
    );
    // Future facts distinguish imported Declaration, Lambda and real relay Parameter identities.
}

#[test]
fn callable_provenance_preflight_caller_first_pointer_factories() {
    let provider = format!(
        "{PROVIDER}\
        fun factory(): (Int) -> Int {{ println(\"factory\")\nreturn ({{ index -> index }}) }}\n\
        fun namedFactory(): (Int) -> Int = (identity)"
    );
    let consumer = "package q\nimport p.factory\nimport p.namedFactory\nimport p.take\n\
        fun caller(): Unit { val result = factory()\nval alias = (result)\ntake(alias)\ntake(namedFactory()) }";
    let fixture = preflight(&provider, consumer);
    assert!(fixture.owned.captures().is_empty());
    // Canonical input sorting checks provider before this consumer. Production facts must also
    // succeed when the caller sorts first, tested by the extra a/z fixture below.
    let mut sources = SourceMap::new();
    let (caller_source, caller) = parsed(
        &mut sources,
        "a/caller.ko",
        "package a\nfun caller(): Unit { val f = z.factory()\nz.take((f)) }",
    );
    let (factory_source, factory) = parsed(
        &mut sources,
        "z/factory.ko",
        "package z\nfun take(callback: (Int) -> Int): Unit {}\nfun factory(): (Int) -> Int { println(\"factory\")\nreturn ({ index -> index }) }",
    );
    let inputs = [
        SourceUnitInput::new("root", "a/caller.ko", caller_source, &caller),
        SourceUnitInput::new("root", "z/factory.ko", factory_source, &factory),
    ];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &environment);
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("caller-first ownership");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    owned.validate().expect("caller-first unit validates");
}

#[test]
fn callable_provenance_preflight_legal_returns_outside_pointer_summary() {
    let provider = format!(
        "{PROVIDER}\
        fun multiple(flag: Boolean): (Int) -> Int {{ if(flag) {{ return identity }}\nreturn other }}\n\
        fun captured(own label: String): (Int) -> Boolean = move {{ index -> label == \"captured\" && index == 0 }}"
    );
    let fixture = preflight(&provider, "package q\nfun unused(): Unit {}");
    assert_eq!(fixture.owned.captures().len(), 1);
    assert_eq!(
        fixture.owned.captures()[0].mode(),
        ClosureCaptureMode::Owned
    );
    // Both functions remain language-legal; neither gets a pointer-only return summary.
}

#[test]
fn callable_provenance_preflight_same_and_different_origin_cfg() {
    let consumer = format!(
        "{IMPORTS}\
        fun use(flag: Boolean): Unit {{\n\
            val same: (Int) -> Int = if(flag) {{ identity }} else {{ identity }}\ntake(same)\n\
            val different: (Int) -> Int = if(flag) {{ identity }} else {{ other }}\ntake(different)\n\
        }}"
    );
    preflight(PROVIDER, &consumer);
}

#[test]
fn callable_provenance_preflight_loop_alias_stable_overwrite_and_conflict() {
    for (prefix, replacement) in [
        ("", "identity"),
        ("callback = identity\n", "other"),
        ("", "other"),
    ] {
        let consumer = format!(
            "{IMPORTS}\
            fun use(flag: Boolean): Unit {{\n\
                var callback: (Int) -> Int = identity\n\
                while(flag) {{ {prefix}val alias = (callback)\ntake((alias))\ncallback = {replacement}\n }}\n\
            }}"
        );
        preflight(PROVIDER, &consumer);
        // Future origin solving keeps the first two known and permanently marks the third Unknown.
    }
}

#[test]
fn callable_provenance_preflight_ownership_error_suppresses_executable_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "bad-capture.ko",
        "fun invalid(label: String): (Int) -> Boolean = move { index -> label == \"borrowed\" && index == 0 }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "bad-capture.ko",
        source,
        &file,
    )];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &environment);
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership recovery");
    assert_eq!(diagnostic_codes(&owned), ["L0138"]);
    assert!(owned.captures().is_empty());
    assert!(owned.loans().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.validate().is_err());
}

#[test]
fn callable_provenance_preflight_type_error_and_analysis_witness() {
    let fixture = preflight(
        PROVIDER,
        "package q\nimport p.take\nimport p.identity\nfun caller(): Unit { take(identity) }",
    );
    let inputs = [
        SourceUnitInput::new(
            "root",
            "p/api.ko",
            fixture.provider_source,
            &fixture.provider,
        ),
        SourceUnitInput::new(
            "root",
            "q/use.ko",
            fixture.consumer_source,
            &fixture.consumer,
        ),
    ];
    let fresh = validated_types(
        &fixture.sources,
        &inputs,
        &fixture.names,
        &fixture.environment,
    );
    let owned = fixture
        .owned
        .clone()
        .validate()
        .expect("validated ownership");
    assert!(matches!(
        owned_compilation_unit_view(
            &fixture.sources,
            &inputs,
            &fixture.names,
            &fixture.environment,
            &fresh,
            &owned
        ),
        Err(OwnedCompilationUnitViewError::MismatchedAnalysis)
    ));
    assert!(
        owned_compilation_unit_view(
            &fixture.sources,
            &inputs,
            &fixture.names,
            &fixture.environment,
            &fixture.typed,
            &owned
        )
        .is_ok()
    );

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "bad-return.ko",
        "fun invalid(): (Int) -> Int = 1",
    );
    let inputs = [SourceUnitInput::new("root", "bad-return.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("typed recovery");
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084"]
    );
    assert!(
        typed.validate().is_err(),
        "P2 error cannot produce the unit P3 capability"
    );
}
