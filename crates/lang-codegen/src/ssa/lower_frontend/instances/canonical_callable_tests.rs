//! Borrow Fn helper specialization must reuse frontend identities without weakening Fn modes.

use super::*;
use lang_frontend::{
    lexer::lex,
    name_resolution::resolve_names,
    parser::parse_file,
    source::SourceMap,
    type_checking::{BuiltinType, check_types, standard_environments},
};

fn analyze_types(text: &str) -> (ParsedFile, TypedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("canonical-callable.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    (parsed, typed)
}

#[test]
fn canonical_callable_single_specialization_keeps_modes_move_and_container_return() {
    let mut failures = Vec::new();
    for (function_type, concrete_type) in [
        ("(borrow Int) -> T", "(borrow Int) -> Int"),
        ("move (inout Int) -> T", "move (inout Int) -> Int"),
        ("(Int) -> Array<T>", "(Int) -> Array<Int>"),
    ] {
        let text = format!(
            "fun <T> helper(f: {function_type}): Unit {{}}\n\
             fun entry(f: {concrete_type}): Unit {{ helper<Int>(f) }}"
        );
        let (parsed, typed) = analyze_types(&text);
        let helper = &typed.callables()[0];
        let entry = &typed.callables()[1];
        let substitutions = BTreeMap::from([(
            helper.type_parameters()[0],
            typed.types().builtin(BuiltinType::Int).unwrap(),
        )]);
        let len = typed.types().len();
        let resolved = resolve_concrete_type(
            &typed,
            helper.parameters()[0].ty,
            &substitutions,
            parsed.ast().items().get(parsed.roots()[0]).unwrap().span(),
        );
        assert_eq!(
            typed.types().len(),
            len,
            "lowering cannot intern frontend types"
        );
        match resolved {
            Ok(ty) => assert_eq!(ty, entry.parameters()[0].ty, "{text}"),
            Err(error) => failures.push(format!("{function_type}: {error:?}")),
        }
        assert!(matches!(
            typed.types().get(entry.parameters()[0].ty),
            Some(TypeKind::Function { .. })
        ));
    }
    assert!(
        failures.is_empty(),
        "all canonical Function shapes must resolve: {failures:?}"
    );
}

#[test]
fn canonical_callable_single_missing_canonical_target_is_structured_and_readonly() {
    let (parsed, typed) = analyze_types(
        "fun <T> helper(f: (Int) -> T): Unit {}\n\
         fun entry(f: (Int) -> Int): Unit { helper<Int>(f) }",
    );
    let helper = &typed.callables()[0];
    let substitutions = BTreeMap::from([(
        helper.type_parameters()[0],
        typed.types().builtin(BuiltinType::Boolean).unwrap(),
    )]);
    let len = typed.types().len();
    let error = resolve_concrete_type(
        &typed,
        helper.parameters()[0].ty,
        &substitutions,
        parsed.ast().items().get(parsed.roots()[0]).unwrap().span(),
    )
    .expect_err("absent (Int)->Boolean may not be created in codegen");
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(typed.types().len(), len);
}
