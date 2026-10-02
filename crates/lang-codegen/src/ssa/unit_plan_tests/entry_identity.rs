use super::*;

#[test]
fn nested_runtime_layout_requires_the_exact_frontend_owner_descriptor() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/layout.ko",
        "package p\n\
         class Dependent<T>(val items: List<T>)\n\
         fun entry(input: Dependent<Int>): Unit {}",
    );
    let inputs = [SourceUnitInput::new("root", "p/layout.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let dependent = declaration(&names, "Dependent");
    let nominal = typed
        .types()
        .signatures()
        .declaration(dependent)
        .and_then(|signature| signature.nominal())
        .expect("Dependent signature exists");
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int is seeded");
    let string = typed
        .types()
        .types()
        .builtin(BuiltinType::String)
        .expect("String is seeded");
    let owner = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: dependent,
            arguments: vec![int],
        })
        .expect("Dependent<Int> is canonical");

    let fields = resolve_nominal_runtime_field_types(typed.types(), owner, nominal, &[int])
        .expect("exact owner descriptor resolves nested List<Int>");
    assert!(matches!(
        fields.as_slice(),
        [field]
            if matches!(
                typed.types().types().get(*field),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments,
                }) if arguments == &[int]
            )
    ));
    let error = resolve_nominal_runtime_field_types(typed.types(), owner, nominal, &[string])
        .expect_err("owner arguments cannot be replaced by another global canonical type");
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
}

#[test]
fn rejects_non_callable_entries_and_foreign_ownership_products() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\nclass NotCallable {}\nfun entry(): Unit {}",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "NotCallable"),
    )
    .expect_err("classifier cannot be a native entry");
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert!(error.span.is_some());

    let (_, foreign_typed, foreign_owned) =
        analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(!foreign_owned.ownership().is_compatible_with(&typed));
    assert!(foreign_owned.ownership().is_compatible_with(&foreign_typed));
    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &foreign_owned,
        declaration(&names, "entry"),
    )
    .expect_err("foreign ownership must fail before planning");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
    assert!(error.span.is_none());
}

#[test]
fn recovery_owner_without_a_layout_cannot_become_executable_ssa() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/recovery.ko",
        "package p\nclass Broken(val prefix: Int, val poison: Opaque)\nfun entry(input: Broken): Unit {}",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/recovery.ko",
        source,
        &parsed,
    )];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment.declare_type("Opaque").unwrap();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(typed.types().runtime_field_layouts().is_empty());
    let error = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .err()
    .expect("typed recovery does not authorize lowering a poisoned field");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}
