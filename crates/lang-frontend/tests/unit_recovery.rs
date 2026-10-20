//! 真实 Unit recovery 的资格传递、完整事实与普通 loop 诊断回归。
//! 仅调用公开阶段入口；不构造阶段产物，不访问私有 recovery facts。

use lang_frontend::{
    analysis::{
        UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names, analyze_unit_recovery,
    },
    diagnostic::{Diagnostic, Severity},
    name_resolution::{SourceUnitInputKey, SourceUnitKey},
    ownership_checking::{
        CompilationUnitOwnership, ConstantMaterializationKind,
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    source::{SourceId, SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, TypeEnvironment, check_compilation_unit_types, standard_environments,
    },
};

const BASIC: &str = "package p\n\
    class Resource {}\n\
    fun inspect(value: Resource): Int = 1\n\
    fun take(own value: Resource): Unit {}\n\
    fun use(own value: Resource): Unit { val count = inspect(value)\ntake(value) }\n\
    fun scan(xs: List<Int>): Unit { for (item in xs) { val copy: Int = item } }";

fn memory(text: &str, locator: &str) -> (UnitNameSnapshot, TypeEnvironment, SourceId) {
    let mut sources = SourceMap::new();
    // 第二次真实登记得到非零 source；未选择源码故意不合法，不能被自动纳入 unit。
    let unselected = sources.add_source("unselected", "# absent").expect("prior real source");
    let source = sources.add_source(locator, text).expect("selected real source");
    assert_ne!(source, unselected);
    let (name_environment, environment) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        name_environment,
    )
    .expect("ordinary memory name prefix");
    assert_eq!(snapshot.sources().source_name(source).unwrap(), locator);
    assert_eq!(snapshot.sources().source_text(source).unwrap(), text);
    assert_eq!(snapshot.sources().source_text(unselected).unwrap(), "# absent");
    assert_eq!(snapshot.descriptors().len(), 1);
    assert_eq!(snapshot.descriptors()[0].source_id(), source);
    assert_eq!(snapshot.descriptors()[0].root_identity(), None);
    assert_eq!(snapshot.descriptors()[0].logical_path(), None);
    assert_eq!(snapshot.parsed_files().len(), 1);
    assert_eq!(snapshot.parsed_files()[0].source_id(), source);
    assert!(snapshot.parsed_files()[0].diagnostics().is_empty());
    let inputs = snapshot.inputs();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].source_id(), source);
    assert_eq!(inputs[0].key(), SourceUnitInputKey::MemoryDocument);
    assert!(std::ptr::eq(inputs[0].parsed(), &snapshot.parsed_files()[0]));
    let units = snapshot.names().index().source_units();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].source_id(), source);
    assert_eq!(units[0].key(), &SourceUnitKey::MemoryDocument);
    (snapshot, environment, source)
}

// 预期位置来自固定源码中的唯一上下文，不取被测 diagnostic 的位置作 oracle。
fn span_in(snapshot: &UnitNameSnapshot, context: &str, token: &str) -> Span {
    let source = snapshot.descriptors()[0].source_id();
    let text = snapshot.sources().source_text(source).unwrap();
    let contexts = text.match_indices(context).collect::<Vec<_>>();
    assert_eq!(contexts.len(), 1, "unique source context: {context}");
    let tokens = context.match_indices(token).collect::<Vec<_>>();
    assert_eq!(tokens.len(), 1, "unique token in context: {token}");
    let start = contexts[0].0 + tokens[0].0;
    snapshot.sources().span(source, start, start + token.len()).unwrap()
}

fn assert_diagnostics(diagnostics: &[Diagnostic], expected: &[(&str, Span)]) {
    assert_eq!(diagnostics.len(), expected.len(), "{diagnostics:?}");
    for (diagnostic, (code, span)) in diagnostics.iter().zip(expected) {
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.code().to_string(), *code);
        assert_eq!(diagnostic.primary_span(), *span);
    }
}

fn assert_no_capability(typed: &CompilationUnitTypes, owned: &CompilationUnitOwnership) {
    assert!(typed.clone().validate().is_err(), "no basic typed capability");
    assert!(typed.clone().validate_constants().is_err(), "no const typed capability");
    assert!(owned.clone().validate().is_err(), "quiet ownership cannot erase upstream failure");
}

fn assert_no_execution_facts(typed: &CompilationUnitTypes, owned: &CompilationUnitOwnership) {
    assert!(typed.sequential_iterations().is_empty());
    assert!(typed.map_iterations().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    assert!(typed.integer_operations().is_empty());
    assert!(typed.integer_range_operations().is_empty());
    assert!(owned.iterations().is_empty());
    assert!(owned.map_iterations().is_empty());
    assert!(owned.formed_owned_locals().is_empty());
    assert!(owned.receiver_facts().is_empty());
    assert!(owned.conditional_receiver_deliveries().is_empty());
    assert!(owned.loans().is_empty());
    assert!(owned.value_deliveries().is_empty());
    assert!(owned.rc_effects().is_empty());
    assert!(owned.construction_plans().is_empty());
    assert!(owned.non_null_assertions().is_empty());
    assert!(owned.ownership_primitives().is_empty());
    assert!(owned.field_replacements().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.conditional_receiver_drops().is_empty());
}

#[test]
fn healthy_memory_matches_full_strict_facts_and_enters_the_sealed_basic_view() {
    for locator in ["untitled:Buffer-7", "file:///not/the/package/F.ko"] {
        let (snapshot, environment, source) = memory(BASIC, locator);
        assert!(snapshot.names().diagnostics().is_empty());
        let names = snapshot.validated_names().expect("healthy names");
        let inputs = snapshot.inputs();
        // 独立调用既有严格入口，不以 recovery 门面再次运行作自证。
        let strict = check_compilation_unit_types(
            snapshot.sources(), &inputs, names, &environment,
        )
        .expect("strict types")
        .validate()
        .expect("strict basic capability");
        let strict_owned = check_compilation_unit_ownership(
            snapshot.sources(), &inputs, names, &environment, &strict,
        )
        .expect("strict ownership");
        let (raw, owned) = analyze_unit_recovery(&snapshot, &environment).expect("recovery");
        assert_eq!(&raw, strict.types(), "all typed facts and diagnostics");
        assert_eq!(owned, strict_owned, "all ownership facts, diagnostics and deferred entries");
        assert!(!raw.is_same_analysis(strict.types()));
        assert!(!owned.is_same_analysis(&strict_owned));
        assert!(!owned.is_compatible_with(&strict));
        assert!(!raw.expression_types().is_empty());
        assert_eq!(raw.sequential_iterations().len(), 1);
        assert!(!owned.call_argument_contracts().is_empty());
        assert!(!owned.loans().is_empty());
        assert!(!owned.value_deliveries().is_empty());
        assert_eq!(owned.iterations().len(), 1);
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());

        let typed = raw.validate().expect("recovery retains genuine basic qualification");
        assert!(owned.is_compatible_with(&typed));
        let owned = owned.validate().expect("recovery retains genuine owned qualification");
        let view = owned_compilation_unit_view(
            snapshot.sources(), &inputs, names, &environment, &typed, &owned,
        )
        .expect("sealed basic view");
        assert!(std::ptr::eq(view.sources(), snapshot.sources()));
        assert!(std::ptr::eq(view.types(), typed.types()));
        assert!(std::ptr::eq(view.ownership(), owned.ownership()));
        assert_eq!(view.inputs()[0].source_id(), source);
        assert_eq!(view.types(), strict.types());
        assert_eq!(view.ownership(), &strict_owned);
    }
}

#[test]
fn name_error_with_quiet_downstream_cannot_validate_but_healthy_neighbor_can() {
    for (text, invalid) in [
        ("fun run(): Unit {}", false),
        ("import absent.VALUE\nfun run(): Unit {}", true),
    ] {
        let (snapshot, environment, _) = memory(text, "untitled:NameQualification");
        let (typed, owned) = analyze_unit_recovery(&snapshot, &environment).expect("ordinary recovery");
        let expected = if invalid {
            vec![("L0148", span_in(&snapshot, "absent.VALUE", "VALUE"))]
        } else {
            Vec::new()
        };
        assert_diagnostics(snapshot.names().diagnostics(), &expected);
        assert_eq!(snapshot.validated_names().is_some(), !invalid);
        assert!(typed.signatures().diagnostics().is_empty());
        assert!(typed.diagnostics().is_empty());
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        assert_eq!(typed.signatures().clone().validate().is_ok(), !invalid);
        if invalid {
            assert_no_capability(&typed, &owned);
        } else {
            let typed = typed.validate().expect("healthy neighbor types");
            assert!(owned.is_compatible_with(&typed));
            owned.validate().expect("healthy neighbor ownership");
        }
    }
}

#[test]
fn type_error_with_quiet_ownership_cannot_validate() {
    let (snapshot, environment, _) = memory(
        "fun run(): Unit { val wrong: Int = true }",
        "untitled:TypeQualification",
    );
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment).expect("ordinary type error");
    assert!(typed.signatures().clone().validate().is_ok());
    assert_diagnostics(
        typed.diagnostics(),
        &[("L0084", span_in(&snapshot, "true", "true"))],
    );
    assert_eq!(typed.body_diagnostics(), typed.diagnostics());
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert_no_capability(&typed, &owned);
    assert_no_execution_facts(&typed, &owned);
}

#[test]
fn healthy_memory_const_keeps_distinct_strict_capabilities_not_basic_ownership() {
    let (snapshot, environment, source) = memory(
        "package p\nconst val TEXT = \"hello\"\nfun read(): String = TEXT",
        "virtual://editor/Const",
    );
    assert!(snapshot.names().diagnostics().is_empty());
    let names = snapshot.validated_names().expect("const names");
    let inputs = snapshot.inputs();
    let strict = check_compilation_unit_types(snapshot.sources(), &inputs, names, &environment)
        .expect("strict constant types");
    let (typed, conservative) = analyze_unit_recovery(&snapshot, &environment).expect("const recovery");
    assert_eq!(typed, strict, "same real typed producer and complete constant facts");
    assert!(typed.diagnostics().is_empty());
    assert!(typed.clone().validate().is_err(), "constant declarations do not become basic types");
    assert!(conservative.diagnostics().is_empty());
    assert!(conservative.clone().validate().is_err(), "conservative raw is not basic ownership");

    let typed = typed.validate_constants().expect("real const capability, not blanket recovery-only");
    assert_eq!(typed.constants().declarations().len(), 1);
    assert_eq!(typed.constants().uses().len(), 1);
    let owned = check_compilation_unit_constant_ownership(
        snapshot.sources(), &inputs, names, &environment, &typed,
    )
    .expect("existing strict constant ownership")
    .validate()
    .expect("distinct const owned capability");
    assert!(owned.is_compatible_with(&typed));
    assert_eq!(owned.materializations().len(), 1);
    assert_eq!(owned.materializations()[0].kind(), ConstantMaterializationKind::StringTemporary);
    assert!(owned.short_circuits().is_empty());
    assert!(owned.ownership().clone().validate().is_err());
    let view = const_owned_compilation_unit_view(
        snapshot.sources(), &inputs, names, &environment, &typed, &owned,
    )
    .expect("sealed constant view");
    assert!(std::ptr::eq(view.types(), typed.types()));
    assert!(std::ptr::eq(view.constant_ownership(), &owned));
    assert_eq!(view.inputs()[0].source_id(), source);
    assert_eq!(view.constant_ownership().materializations(), owned.materializations());
}

fn assert_loop_body_diagnostic(text: &str, consumption: &str, binding: &str) {
    let (snapshot, environment, _) = memory(text, "untitled:LoopRecovery");
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
        .expect("ordinary earlier and body errors must not become internal failure");
    assert!(typed.signatures().diagnostics().is_empty());
    assert_diagnostics(
        typed.diagnostics(),
        &[
            ("L0084", span_in(&snapshot, "true", "true")),
            ("L0084", span_in(&snapshot, "false", "false")),
        ],
    );
    assert_diagnostics(
        owned.diagnostics(),
        &[("L0133", span_in(&snapshot, consumption, binding))],
    );
    assert!(!typed.calls().is_empty(), "the actual ordinary consuming call survives type recovery");
    assert!(!owned.call_argument_contracts().is_empty());
    assert!(owned.deferred().is_empty(), "not a deferred substitution for the real error");
    assert_no_capability(&typed, &owned);
    assert_no_execution_facts(&typed, &owned);
}

#[test]
fn earlier_and_sequential_body_type_errors_do_not_hide_borrowed_consumption() {
    assert_loop_body_diagnostic(
        "fun earlier(): Int = true\n\
         fun take(own item: String): Unit {}\n\
         fun run(xs: List<String>): Unit {\n\
           for (item in xs) { val wrong: Int = false\ntake(item)\nbreak }\n\
         }",
        "take(item)",
        "item",
    );
}

#[test]
fn earlier_and_map_body_type_errors_do_not_hide_borrowed_consumption() {
    assert_loop_body_diagnostic(
        "fun earlier(): Int = true\n\
         fun take(own item: String): Unit {}\n\
         fun run(xs: Map<Int, String>): Unit {\n\
           for ((key, value) in xs) { val wrong: Int = false\ntake(value)\nbreak }\n\
         }",
        "take(value)",
        "value",
    );
}

#[test]
fn effectful_builtin_function_value_has_the_exact_existing_internal_boundary() {
    // 与 source-set last-good snapshot 测试使用完全相同的普通源码。
    let text = "package app\nfun broken(): Unit { val emit = println }";
    let (snapshot, environment, _) = memory(text, "file:///workspace/app/main.ko");
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
    let expected = span_in(&snapshot, "println", "println");
    match analyze_unit_recovery(&snapshot, &environment) {
        Err(lang_frontend::analysis::UnitRecoveryAnalysisError::Type(
            lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(span),
        )) => assert_eq!(span, expected),
        other => panic!("expected only the existing effectful function-value boundary: {other:?}"),
    }
}
