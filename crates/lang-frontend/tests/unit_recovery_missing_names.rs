//! 普通缺名声明的真实 Parser → Unit recovery；不构造阶段事实或私有身份。

use lang_frontend::{
    analysis::{UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names, analyze_unit_recovery},
    diagnostic::{Diagnostic, Severity},
    name_resolution::UnitReferenceTarget,
    ownership_checking::{
        CompilationUnitOwnership, OwnershipBindingKind, check_compilation_unit_constant_ownership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    parser::{Item, NameMarker},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ParameterMode, TypeEnvironment, UnitTypeKind,
        standard_environments,
    },
};

fn memory(text: &str) -> (UnitNameSnapshot, TypeEnvironment) {
    let mut sources = SourceMap::new();
    let unrelated = sources
        .add_source("not-selected", "#")
        .expect("real unrelated source");
    let source = sources
        .add_source("untitled:MissingNames", text)
        .expect("real source");
    assert_ne!(source, unrelated);
    let (names, types) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        names,
    )
    .expect("ordinary malformed source has a real name snapshot");
    assert_eq!(snapshot.parsed_files().len(), 1);
    assert_eq!(snapshot.parsed_files()[0].source_id(), source);
    assert_eq!(snapshot.sources().source_text(source).unwrap(), text);
    (snapshot, types)
}

// 位置独立取自固定源码，不从被测 diagnostic 或 marker 反推预期。
fn span(snapshot: &UnitNameSnapshot, context: &str, token: &str) -> Span {
    let source = snapshot.descriptors()[0].source_id();
    let text = snapshot.sources().source_text(source).unwrap();
    let contexts = text.match_indices(context).collect::<Vec<_>>();
    assert_eq!(contexts.len(), 1, "unique source context: {context}");
    let tokens = context.match_indices(token).collect::<Vec<_>>();
    assert_eq!(tokens.len(), 1, "unique token: {token}");
    let start = contexts[0].0 + tokens[0].0;
    snapshot
        .sources()
        .span(source, start, start + token.len())
        .unwrap()
}

fn empty_before(snapshot: &UnitNameSnapshot, context: &str, token: &str) -> Span {
    let token = span(snapshot, context, token);
    snapshot
        .sources()
        .span(token.source_id(), token.start(), token.start())
        .unwrap()
}

fn diagnostics(actual: &[Diagnostic], expected: &[(&str, Span)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (actual, (code, primary)) in actual.iter().zip(expected) {
        assert_eq!(actual.severity(), Severity::Error);
        assert_eq!(actual.code().to_string(), *code);
        assert_eq!(actual.primary_span(), *primary);
    }
}

fn rejected(
    snapshot: &UnitNameSnapshot,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) {
    assert!(snapshot.validated_names().is_none());
    assert!(typed.signatures().clone().validate().is_err());
    assert!(typed.clone().validate().is_err());
    assert!(typed.clone().validate_constants().is_err());
    assert!(
        owned.clone().validate().is_err(),
        "quiet ownership cannot erase the parser error"
    );
    assert!(typed.ownership_primitives().is_empty());
    assert!(owned.formed_owned_locals().is_empty());
    assert!(owned.receiver_facts().is_empty());
    assert!(owned.value_deliveries().is_empty());
    assert!(owned.construction_plans().is_empty());
    assert!(owned.drops().is_empty());
}

fn missing_markers(snapshot: &UnitNameSnapshot) -> Vec<NameMarker> {
    snapshot.parsed_files()[0]
        .ast()
        .items()
        .iter()
        .filter_map(|(_, item)| {
            let name = match item.payload() {
                Item::Function { name, .. }
                | Item::Variable { name, .. }
                | Item::Constant { name, .. } => *name,
                Item::Classifier(classifier) => classifier.name,
                _ => return None,
            };
            (!matches!(name, NameMarker::Present(_))).then_some(name)
        })
        .collect()
}

#[test]
fn four_top_level_declarations_keep_missing_and_error_markers_and_body_mismatch() {
    for (text, context, token, code, is_missing) in [
        ("fun (): Int = true", "fun ()", "(", "L0018", true),
        ("fun 1(): Int = true", "fun 1()", "1", "L0018", false),
        ("val : Int = true", "val :", ":", "L0018", true),
        ("val 1: Int = true", "val 1:", "1", "L0018", false),
        ("const val : Int = true", "const val :", ":", "L0018", true),
        (
            "const val 1: Int = true",
            "const val 1:",
            "1",
            "L0018",
            false,
        ),
        (
            "class { fun f(): Int = true }",
            "class {",
            "{",
            "L0067",
            true,
        ),
        (
            "class 1 { fun f(): Int = true }",
            "class 1",
            "1",
            "L0067",
            false,
        ),
    ] {
        let (snapshot, environment) = memory(text);
        let primary = if is_missing {
            empty_before(&snapshot, context, token)
        } else {
            span(&snapshot, context, token)
        };
        let marker = if is_missing {
            NameMarker::Missing(primary)
        } else {
            NameMarker::Error(primary)
        };
        assert_eq!(missing_markers(&snapshot), [marker], "{text}");
        diagnostics(snapshot.parsed_files()[0].diagnostics(), &[(code, primary)]);
        diagnostics(snapshot.names().diagnostics(), &[(code, primary)]);
        let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
            .expect("a real missing-name parser error must not become an internal failure");
        diagnostics(
            typed.diagnostics(),
            &[("L0084", span(&snapshot, "true", "true"))],
        );
        diagnostics(owned.diagnostics(), &[]);
        assert!(owned.deferred().is_empty());
        rejected(&snapshot, &typed, &owned);
    }
}

#[test]
fn unnamed_members_and_companion_constants_keep_real_parameter_types() {
    for name in ["", "1"] {
        let text = format!(
            "class C(val field: Int) {{\nfun {name}(x: Int): Int {{\n\
             val observed: Int = this.field\nval wrong: Int = false\nreturn x\n}}\n\
             companion object {{ const val {name}: Int = true }}\nfun good(): Int = 1\n}}"
        );
        let (snapshot, environment) = memory(&text);
        let function = format!("fun {name}(");
        let constant = format!("const val {name}:");
        let marker = |context: &str, boundary: &str| {
            if name.is_empty() {
                empty_before(&snapshot, context, boundary)
            } else {
                span(&snapshot, context, name)
            }
        };
        let expected = [
            ("L0018", marker(&function, "(")),
            ("L0018", marker(&constant, ":")),
        ];
        diagnostics(snapshot.parsed_files()[0].diagnostics(), &expected);
        diagnostics(snapshot.names().diagnostics(), &expected);
        let (typed, owned) =
            analyze_unit_recovery(&snapshot, &environment).expect("member recovery");
        diagnostics(
            typed.diagnostics(),
            &[
                ("L0084", span(&snapshot, "false", "false")),
                ("L0084", span(&snapshot, "true", "true")),
            ],
        );
        diagnostics(owned.diagnostics(), &[]);
        let reference_span = span(&snapshot, "return x", "x");
        let reference = snapshot
            .names()
            .references()
            .iter()
            .find(|reference| reference.span() == reference_span)
            .expect("actual parameter reference");
        let UnitReferenceTarget::Symbol(symbol) = reference.target() else {
            panic!("parameter must retain its real local identity");
        };
        let ty = typed
            .symbol_type(*symbol)
            .expect("actual unnamed-function parameter type");
        assert_eq!(
            typed.types().get(ty),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(
            typed.body_parameter_mode(*symbol),
            Some(ParameterMode::Borrow)
        );
        let field_span = span(&snapshot, "this.field", "this.field");
        let field_types = typed
            .expression_types()
            .iter()
            .filter(|(expression, _)| {
                snapshot.parsed_files()[0]
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .expect("real field access identity")
                    .span()
                    == field_span
            })
            .collect::<Vec<_>>();
        assert_eq!(field_types.len(), 1);
        assert_eq!(
            typed.types().get(*field_types[0].1),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        );
        assert!(owned.deferred().is_empty());
        rejected(&snapshot, &typed, &owned);
    }
}

#[test]
fn bodyless_unnamed_concrete_member_keeps_l0098_but_interface_does_not_cascade() {
    for classifier in ["class", "interface"] {
        for name in ["", "1"] {
            let text = format!("{classifier} C {{ fun {name}(): Int }}");
            let (snapshot, environment) = memory(&text);
            let context = format!("fun {name}(");
            let primary = if name.is_empty() {
                empty_before(&snapshot, &context, "(")
            } else {
                span(&snapshot, &context, name)
            };
            diagnostics(snapshot.names().diagnostics(), &[("L0018", primary)]);
            let (typed, owned) =
                analyze_unit_recovery(&snapshot, &environment).expect("bodyless member recovery");
            let expected = if classifier == "class" {
                vec![("L0098", primary)]
            } else {
                vec![]
            };
            diagnostics(typed.diagnostics(), &expected);
            diagnostics(owned.diagnostics(), &[]);
            assert!(owned.deferred().is_empty());
            rejected(&snapshot, &typed, &owned);
        }
    }
}

#[test]
fn unnamed_function_real_borrowed_parameter_keeps_independent_ownership_error() {
    for name in ["", "1"] {
        let text = format!(
            "class Resource {{}}\nfun take(own value: Resource): Unit {{}}\n\
             class C {{ fun {name}(value: Resource): Unit {{ take(value) }} }}"
        );
        let (snapshot, environment) = memory(&text);
        let context = format!("fun {name}(");
        let primary = if name.is_empty() {
            empty_before(&snapshot, &context, "(")
        } else {
            span(&snapshot, &context, name)
        };
        diagnostics(snapshot.names().diagnostics(), &[("L0018", primary)]);
        let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
            .expect("ordinary borrowed consumption still reaches ownership");
        diagnostics(typed.diagnostics(), &[]);
        diagnostics(
            owned.diagnostics(),
            &[("L0133", span(&snapshot, "take(value)", "value"))],
        );
        let use_span = span(&snapshot, "take(value)", "value");
        let reference = snapshot
            .names()
            .references()
            .iter()
            .find(|reference| reference.span() == use_span)
            .expect("actual parameter use");
        let UnitReferenceTarget::Symbol(symbol) = reference.target() else {
            panic!("borrowed parameter remains a real symbol");
        };
        assert_eq!(
            typed.body_parameter_mode(*symbol),
            Some(ParameterMode::Borrow)
        );
        let resource = snapshot
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "Resource")
            .expect("real Resource declaration");
        assert_eq!(
            typed.types().get(typed.symbol_type(*symbol).unwrap()),
            Some(&UnitTypeKind::Nominal {
                declaration: resource.id(),
                arguments: Vec::new()
            }),
        );
        let bindings = owned
            .bindings()
            .iter()
            .filter(|binding| binding.symbol() == *symbol)
            .collect::<Vec<_>>();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].kind(), OwnershipBindingKind::Shared);
        assert_eq!(
            bindings[0].declaration_span(),
            span(&snapshot, "(value: Resource)", "value"),
        );
        assert_eq!(typed.calls().len(), 1);
        assert_eq!(owned.call_argument_contracts().len(), 1);
        assert!(owned.deferred().is_empty());
        rejected(&snapshot, &typed, &owned);
    }
}

#[test]
fn unnamed_classifier_error_receiver_does_not_leak_into_companion_or_following_members() {
    for name in ["", "1"] {
        let text = format!(
            "class {name}(val field: Int) {{\nfun read(): Int = this.field\n\
             companion object {{\nfun invalid(): Int = this\nconst val BAD: Int = this\n}}\n\
             fun later(): Int = true\n}}\nfun target(): Int = 1\nfun use(): Int = target()"
        );
        let (snapshot, environment) = memory(&text);
        let context = format!("class {name}(");
        let primary = if name.is_empty() {
            empty_before(&snapshot, &context, "(")
        } else {
            span(&snapshot, &context, name)
        };
        diagnostics(
            snapshot.parsed_files()[0].diagnostics(),
            &[("L0067", primary)],
        );
        diagnostics(
            snapshot.names().diagnostics(),
            &[
                ("L0067", primary),
                (
                    "L0153",
                    span(&snapshot, "const val BAD: Int = this", "this"),
                ),
            ],
        );
        let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
            .expect("actual unnamed classifier recovery");
        diagnostics(
            typed.diagnostics(),
            &[
                (
                    "L0166",
                    span(&snapshot, "fun invalid(): Int = this", "this"),
                ),
                ("L0084", span(&snapshot, "true", "true")),
            ],
        );
        diagnostics(owned.diagnostics(), &[]);
        let this = span(&snapshot, "this.field", "this");
        let member = span(&snapshot, "this.field", "this.field");
        for expected_span in [this, member] {
            let facts = typed
                .expression_types()
                .iter()
                .filter(|(expression, _)| {
                    snapshot.parsed_files()[0]
                        .ast()
                        .expressions()
                        .get(expression.expression())
                        .expect("real expression identity")
                        .span()
                        == expected_span
                })
                .collect::<Vec<_>>();
            assert_eq!(facts.len(), 1, "receiver and member remain checked");
            assert_eq!(typed.types().get(*facts[0].1), Some(&UnitTypeKind::Error));
        }
        let target_span = span(&snapshot, "fun target()", "target");
        let target = snapshot
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name_span() == target_span)
            .expect("healthy target declaration");
        let use_span = span(&snapshot, "= target()", "target");
        let reference = snapshot
            .names()
            .references()
            .iter()
            .find(|reference| reference.span() == use_span)
            .expect("healthy reference after classifier");
        assert_eq!(
            reference.target(),
            &UnitReferenceTarget::Declaration(target.id())
        );
        assert_eq!(typed.calls().len(), 1, "healthy following call survives");
        rejected(&snapshot, &typed, &owned);
    }
}

#[test]
fn repaired_names_regain_the_real_basic_or_constant_qualification() {
    for (text, constant) in [
        ("fun restored(): Int = 1", false),
        ("val restored: Int = 1", false),
        ("const val RESTORED: Int = 1", true),
        ("class Restored { fun f(): Int = 1 }", false),
    ] {
        let (snapshot, environment) = memory(text);
        diagnostics(snapshot.parsed_files()[0].diagnostics(), &[]);
        diagnostics(snapshot.names().diagnostics(), &[]);
        let names = snapshot.validated_names().expect("repaired names");
        let inputs = snapshot.inputs();
        let (typed, owned) =
            analyze_unit_recovery(&snapshot, &environment).expect("repaired recovery");
        diagnostics(typed.diagnostics(), &[]);
        diagnostics(owned.diagnostics(), &[]);
        if constant {
            let typed = typed
                .validate_constants()
                .expect("real constant qualification");
            let owned = check_compilation_unit_constant_ownership(
                snapshot.sources(),
                &inputs,
                names,
                &environment,
                &typed,
            )
            .expect("strict constant ownership")
            .validate()
            .expect("constant ownership qualification");
            const_owned_compilation_unit_view(
                snapshot.sources(),
                &inputs,
                names,
                &environment,
                &typed,
                &owned,
            )
            .expect("sealed constant view");
        } else {
            let typed = typed.validate().expect("real basic qualification");
            assert!(owned.is_compatible_with(&typed));
            let owned = owned.validate().expect("real owned qualification");
            owned_compilation_unit_view(
                snapshot.sources(),
                &inputs,
                names,
                &environment,
                &typed,
                &owned,
            )
            .expect("sealed basic view");
        }
    }
}
