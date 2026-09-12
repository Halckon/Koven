//! SPEC-0210: independently validated unit constant facts cannot enter the base pipeline.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{BuiltinType, ConstValue, check_compilation_unit_types, standard_environments},
};

#[test]
fn constant_facts_are_exact_and_separate_from_base_validation() {
    let mut sources = SourceMap::new();
    let a = sources
        .add_source(
            "a.ko",
            "package a\nimport b.B\nobject A { const val X = B.X + 2 }\nfun read(): Int = A.X",
        )
        .unwrap();
    let b = sources
        .add_source("b.ko", "package b\nobject B { const val X = 40 }")
        .unwrap();
    let fa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let fb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
    let inputs = [
        SourceUnitInput::new("root", "a/source.ko", a, &fa),
        SourceUnitInput::new("root", "b/source.ko", b, &fb),
    ];
    let (ne, te) = standard_environments();
    let mut prior = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.clone().validate().is_err());
        let enabled = typed.validate_constants().unwrap();
        let facts = enabled.constants();
        assert_eq!(facts.declarations().len(), 2);
        assert_eq!(
            facts.declarations()[0].value(),
            &ConstValue::Integer {
                ty: BuiltinType::Int,
                value: 42
            }
        );
        assert_eq!(
            facts.declarations()[1].value(),
            &ConstValue::Integer {
                ty: BuiltinType::Int,
                value: 40
            }
        );
        assert_eq!(
            facts.declarations()[0].dependencies(),
            &[facts.declarations()[1].symbol()]
        );
        assert_eq!(
            facts.uses().len(),
            2,
            "{facts:#?}\ntypes={:?}",
            enabled.types().expression_types()
        );
        if let Some(prior) = &prior {
            assert_eq!(facts, prior);
        }
        for (usage, declaration) in facts
            .uses()
            .iter()
            .zip([&facts.declarations()[1], &facts.declarations()[0]])
        {
            assert_eq!(usage.target(), declaration.symbol());
            assert_eq!(usage.ty(), declaration.ty());
            assert_eq!(usage.value(), declaration.value());
            assert_eq!(
                enabled.types().expression_type(usage.expression()),
                Some(usage.ty())
            );
        }
        prior = Some(facts.clone());
        assert!(enabled.into_types().validate().is_err());
    }
}

fn analyze(text: &str) -> lang_frontend::type_checking::CompilationUnitTypes {
    let mut sources = SourceMap::new();
    let source = sources.add_source("a.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::new("root", "a/source.ko", source, &parsed)];
    let (ne, te) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
        .unwrap()
        .validate()
        .unwrap();
    check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap()
}

#[test]
fn unused_constants_also_require_the_separate_capability() {
    let typed = analyze("package a\nconst val X = 7");
    assert!(typed.diagnostics().is_empty());
    assert!(typed.clone().validate().is_err());
    let enabled = typed.validate_constants().unwrap();
    assert_eq!(enabled.constants().declarations().len(), 1);
    assert!(enabled.constants().uses().is_empty());
    assert!(
        analyze("package a\nfun ordinary(): Int = 7")
            .validate()
            .is_ok()
    );
}

#[test]
fn any_constant_or_runtime_error_prevents_partial_publication() {
    for failure in [
        "const val BAD = 1 / 0",
        "const val BAD = BAD",
        "const val BAD: Double = 1.5",
        "fun call(): Int = 1\nconst val BAD = call()",
        "fun bad(): Int = true",
        "const val BAD: Array<Int<String>>? = null",
    ] {
        let typed = analyze(&format!("package a\nconst val GOOD = 42\n{failure}"));
        assert!(!typed.diagnostics().is_empty(), "{failure}");
        assert!(typed.constants().is_none(), "partial facts: {failure}");
        assert!(typed.clone().validate_constants().is_err());
        assert!(typed.validate().is_err());
    }
}

#[test]
fn parameter_shadowing_does_not_select_the_classifier_constant() {
    let typed = analyze(
        "package a\nobject A { const val X = 1 }\nclass Holder(val X: Int)\nfun read(A: Holder): Int = A.X",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let enabled = typed.validate_constants().unwrap();
    assert!(enabled.constants().uses().is_empty());
}
