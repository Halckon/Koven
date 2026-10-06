//! Both concrete Function identity and its mode labels remain frontend-owned.

use super::*;

#[test]
fn canonical_callable_unit_specialization_keeps_modes_move_and_container_return() {
    let mut failures = Vec::new();
    for (function_type, concrete_type) in [
        ("(borrow Int) -> T", "(borrow Int) -> Int"),
        ("move (inout Int) -> T", "move (inout Int) -> Int"),
        ("(Int) -> Array<T>", "(Int) -> Array<Int>"),
    ] {
        let mut sources = SourceMap::new();
        let (source, p) = parsed(
            &mut sources,
            "p/callables.ko",
            &format!(
                "package p\nfun <T> helper(f: {function_type}): Unit {{}}\n\
             fun entry(f: {concrete_type}): Unit {{ helper<Int>(f) }}"
            ),
        );
        let inputs = [SourceUnitInput::new("root", "p/callables.ko", source, &p)];
        let (name_environment, environment) = standard_environments();
        let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &environment);
        let helper = typed
            .types()
            .signatures()
            .declaration(declaration(&names, "helper"))
            .unwrap()
            .callable()
            .unwrap();
        let entry = typed
            .types()
            .signatures()
            .declaration(declaration(&names, "entry"))
            .unwrap()
            .callable()
            .unwrap();
        let substitutions = BTreeMap::from([(
            helper.type_parameters()[0],
            typed.types().types().builtin(BuiltinType::Int).unwrap(),
        )]);
        let len = typed.types().types().len();
        let resolved = super::super::unit_plan::resolve_concrete_type(
            typed.types(),
            helper.parameters()[0].ty(),
            &substitutions,
            None,
            helper.parameters()[0].span(),
        );
        assert_eq!(
            typed.types().types().len(),
            len,
            "lowering cannot intern frontend types"
        );
        match resolved {
            Ok(ty) => assert_eq!(ty, entry.parameters()[0].ty()),
            Err(error) => failures.push(format!("{function_type}: {error:?}")),
        }
        assert!(matches!(
            typed.types().types().get(entry.parameters()[0].ty()),
            Some(UnitTypeKind::Function { .. })
        ));
    }
    assert!(
        failures.is_empty(),
        "all canonical Function shapes must resolve: {failures:?}"
    );
}

#[test]
fn canonical_callable_unit_missing_canonical_target_is_structured_and_readonly() {
    let mut sources = SourceMap::new();
    let (source, p) = parsed(
        &mut sources,
        "p/callables.ko",
        "package p\nfun <T> helper(f: (Int) -> T): Unit {}\n\
         fun entry(f: (Int) -> Int): Unit { helper<Int>(f) }",
    );
    let inputs = [SourceUnitInput::new("root", "p/callables.ko", source, &p)];
    let (name_environment, environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &environment);
    let helper = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "helper"))
        .unwrap()
        .callable()
        .unwrap();
    let substitutions = BTreeMap::from([(
        helper.type_parameters()[0],
        typed.types().types().builtin(BuiltinType::Boolean).unwrap(),
    )]);
    let len = typed.types().types().len();
    let error = super::super::unit_plan::resolve_concrete_type(
        typed.types(),
        helper.parameters()[0].ty(),
        &substitutions,
        None,
        helper.parameters()[0].span(),
    )
    .expect_err("absent (Int)->Boolean may not be created in codegen");
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(typed.types().types().len(), len);
}
