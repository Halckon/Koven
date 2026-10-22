//! 无 receiver 的真实 this、合法实例邻控与既有错误恢复；只使用公开 Unit 入口。

use lang_frontend::{
    analysis::{UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names, analyze_unit_recovery},
    diagnostic::{Diagnostic, Severity},
    ownership_checking::{
        CompilationUnitOwnership, check_compilation_unit_ownership, owned_compilation_unit_view,
    },
    parser::Expression,
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, TypeEnvironment, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types, standard_environments,
    },
};

fn memory(text: &str) -> (UnitNameSnapshot, TypeEnvironment) {
    let mut sources = SourceMap::new();
    let prior = sources
        .add_source("unselected", "# absent")
        .expect("real prior registration");
    let source = sources
        .add_source("untitled:ThisRecovery", text)
        .expect("real selected source");
    assert_ne!(source, prior);
    let (names, environment) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        names,
    )
    .expect("ordinary parsed memory input");
    assert_eq!(snapshot.descriptors().len(), 1);
    assert_eq!(snapshot.descriptors()[0].source_id(), source);
    assert_eq!(snapshot.descriptors()[0].root_identity(), None);
    assert_eq!(snapshot.descriptors()[0].logical_path(), None);
    assert_eq!(snapshot.parsed_files().len(), 1);
    assert_eq!(snapshot.parsed_files()[0].source_id(), source);
    assert!(snapshot.parsed_files()[0].diagnostics().is_empty());
    assert_eq!(snapshot.sources().source_text(source).unwrap(), text);
    assert_eq!(snapshot.sources().source_text(prior).unwrap(), "# absent");
    (snapshot, environment)
}

fn unique_span(snapshot: &UnitNameSnapshot, token: &str) -> Span {
    let source = snapshot.descriptors()[0].source_id();
    let text = snapshot.sources().source_text(source).unwrap();
    let positions = text.match_indices(token).collect::<Vec<_>>();
    assert_eq!(positions.len(), 1, "unique expected source token {token}");
    let start = positions[0].0;
    snapshot
        .sources()
        .span(source, start, start + token.len())
        .unwrap()
}

fn assert_diagnostics(diagnostics: &[Diagnostic], expected: &[(&str, Span)]) {
    assert_eq!(diagnostics.len(), expected.len(), "{diagnostics:?}");
    for (diagnostic, (code, span)) in diagnostics.iter().zip(expected) {
        assert_eq!(diagnostic.code().to_string(), *code);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.primary_span(), *span);
    }
}

fn this_expression(snapshot: &UnitNameSnapshot) -> UnitExpressionId {
    let file = &snapshot.parsed_files()[0];
    let ids = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::This).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "one actual parser-produced this expression");
    let unit = snapshot
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == file.source_id())
        .expect("original selected source");
    UnitExpressionId::new(unit.id(), ids[0])
}

fn assert_error_this(snapshot: &UnitNameSnapshot, typed: &CompilationUnitTypes) {
    let ty = typed
        .expression_type(this_expression(snapshot))
        .expect("real this type fact");
    assert_eq!(typed.types().get(ty), Some(&UnitTypeKind::Error));
}

fn assert_unqualified(typed: &CompilationUnitTypes, owned: &CompilationUnitOwnership) {
    assert!(typed.clone().validate().is_err());
    assert!(typed.clone().validate_constants().is_err());
    assert!(owned.clone().validate().is_err());
    assert!(owned.drops().is_empty());
    assert!(owned.iterations().is_empty());
    assert!(owned.map_iterations().is_empty());
}

fn assert_without_receiver(text: &str) {
    assert_without_receiver_after(text, &[]);
}

/// `leading` are diagnostics the unit reports before the `this` rejection, such as the
/// Guide v0.46 L0167 on a top-level runtime value.
fn assert_without_receiver_after(text: &str, leading: &[(&str, &str)]) {
    let (snapshot, environment) = memory(text);
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
        .expect("ordinary receiver error is not an internal failure");
    assert!(typed.signatures().diagnostics().is_empty());
    let mut expected = leading
        .iter()
        .map(|(code, slice)| (*code, unique_span(&snapshot, slice)))
        .collect::<Vec<_>>();
    expected.push(("L0166", unique_span(&snapshot, "this")));
    assert_diagnostics(typed.diagnostics(), &expected);
    assert_eq!(typed.body_diagnostics(), typed.diagnostics());
    assert_error_this(&snapshot, &typed);
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert_unqualified(&typed, &owned);
}

#[test]
fn top_level_function_this_reports_exact_l0166_and_has_no_capability() {
    assert_without_receiver("fun invalid(): Unit { this }");
}

#[test]
fn top_level_initializer_this_reports_exact_l0166_and_has_no_capability() {
    assert_without_receiver_after("val bad = this", &[("L0167", "val")]);
}

#[test]
fn companion_function_this_reports_exact_l0166_and_has_no_capability() {
    assert_without_receiver("class Tools { companion { fun invalid(): Unit { this } } }");
}

#[test]
fn instance_this_retains_real_basic_types_ownership_and_sealed_view() {
    let (snapshot, environment) = memory(
        "class Holder(val value: Int) { fun read(): Int = this.value }\n\
         fun use(holder: Holder): Int = holder.read()",
    );
    assert!(snapshot.names().diagnostics().is_empty());
    let names = snapshot.validated_names().expect("healthy instance names");
    let inputs = snapshot.inputs();
    let strict = check_compilation_unit_types(snapshot.sources(), &inputs, names, &environment)
        .expect("strict instance types")
        .validate()
        .expect("strict basic types");
    let strict_owned =
        check_compilation_unit_ownership(snapshot.sources(), &inputs, names, &environment, &strict)
            .expect("strict instance ownership");
    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment).expect("healthy instance");
    assert_eq!(&typed, strict.types());
    assert_eq!(owned, strict_owned);
    assert!(typed.diagnostics().is_empty());
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    let this_ty = typed
        .expression_type(this_expression(&snapshot))
        .expect("typed actual this");
    let holder = snapshot
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "Holder")
        .expect("real Holder declaration");
    assert!(matches!(
        typed.types().get(this_ty),
        Some(UnitTypeKind::Nominal { declaration, arguments })
            if *declaration == holder.id() && arguments.is_empty()
    ));
    let typed = typed.validate().expect("not blanket recovery-only");
    assert!(owned.is_compatible_with(&typed));
    let owned = owned.validate().expect("healthy instance owned capability");
    let view = owned_compilation_unit_view(
        snapshot.sources(),
        &inputs,
        names,
        &environment,
        &typed,
        &owned,
    )
    .expect("genuine basic sealed view");
    assert!(std::ptr::eq(view.types(), typed.types()));
    assert!(std::ptr::eq(view.ownership(), owned.ownership()));
}

#[test]
fn companion_constant_same_token_l0153_does_not_gain_l0166() {
    for declaration in ["const val BAD = this", "const val BAD: Int = this"] {
        let (snapshot, environment) =
            memory(&format!("class Holder {{ companion {{ {declaration} }} }}",));
        assert_diagnostics(
            snapshot.names().diagnostics(),
            &[("L0153", unique_span(&snapshot, "this"))],
        );
        assert!(snapshot.validated_names().is_none());
        let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
            .expect("existing constant-context rejection remains ordinary recovery");
        assert_error_this(&snapshot, &typed);
        // 这里只锁新码不重复；显式 Int 的既有 L0156 常量边界不在本片改写或抹除。
        assert!(
            typed
                .diagnostics()
                .iter()
                .all(|diagnostic| { diagnostic.code().to_string() != "L0166" })
        );
        assert!(typed.constants().is_none());
        assert!(owned.diagnostics().is_empty());
        assert_unqualified(&typed, &owned);
    }
}

#[test]
fn rejected_real_extension_receiver_keeps_l0164_without_false_l0166() {
    let (snapshot, environment) = memory("borrow fun List<Int>.probe(): Unit { this }");
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
        .expect("already diagnosed real extension is ordinary Error recovery");
    assert_diagnostics(
        typed.diagnostics(),
        &[("L0164", unique_span(&snapshot, "."))],
    );
    assert_eq!(typed.signatures().diagnostics(), typed.diagnostics());
    assert!(typed.body_diagnostics().is_empty());
    let callables = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.callable())
        .collect::<Vec<_>>();
    assert_eq!(callables.len(), 1);
    assert!(
        callables[0].receiver().is_some(),
        "actual declared receiver, not absence"
    );
    assert!(
        callables[0].range_extension().is_none(),
        "no forged trusted authority"
    );
    assert_error_this(&snapshot, &typed);
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert_unqualified(&typed, &owned);
}
