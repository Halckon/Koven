//! SPEC-0226 public capability and source-qualified ownership identity.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        ConstantMaterializationKind, OwnershipCheckingError,
        check_compilation_unit_constant_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        ConstEnabledTypedUnit, TypeEnvironment, check_compilation_unit_types, standard_environments,
    },
};

fn with_unit(
    first: &str,
    second: &str,
    test: impl FnOnce(
        &SourceMap,
        &[SourceUnitInput<'_>],
        &ValidatedCompilationUnitNames,
        &TypeEnvironment,
        &ConstEnabledTypedUnit,
    ),
) {
    let mut sources = SourceMap::new();
    let a = sources.add_source("a.ko", first).unwrap();
    let b = sources.add_source("b.ko", second).unwrap();
    let pa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let pb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
    assert!(pa.diagnostics().is_empty(), "{:?}", pa.diagnostics());
    assert!(pb.diagnostics().is_empty(), "{:?}", pb.diagnostics());
    let inputs = [
        SourceUnitInput::new("root", "a/source.ko", a, &pa),
        SourceUnitInput::new("root", "b/source.ko", b, &pb),
    ];
    let (ne, te) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
        .unwrap()
        .validate_constants()
        .unwrap();
    test(&sources, &inputs, &names, &te, &typed);
}

#[test]
fn constant_owned_capability_preserves_cross_file_plans_and_cannot_reopen_base_validation() {
    with_unit(
        "package a\nimport b.Labels\nfun view(left: String, right: String): Unit {}\nfun read(): Unit { view((Labels.TEXT), Labels.TEXT) }",
        "package b\nobject Labels { const val TEXT = \"hello\" }",
        |sources, inputs, names, te, typed| {
            let recovery =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap();
            assert!(recovery.is_compatible_with(typed));
            assert!(recovery.ownership().clone().validate().is_err());
            let owned = recovery.validate().unwrap();
            assert!(owned.is_compatible_with(&typed.clone()));
            assert_eq!(owned.materializations().len(), 2);
            let loans = owned.ownership().loans();
            assert_eq!(loans.len(), 2);
            assert_eq!(
                loans[0].call(),
                loans[1].call(),
                "the borrows overlap in one call"
            );
            assert_ne!(
                loans[0].target(),
                loans[1].target(),
                "each read owns a distinct temporary"
            );
            assert!(
                owned.materialization_at(loans[0].argument()).is_none(),
                "group is not another materialization"
            );
            assert_eq!(owned.ownership().drops().len(), 2);
            for plan in owned.materializations() {
                assert_eq!(plan.kind(), ConstantMaterializationKind::StringTemporary);
                assert_ne!(
                    plan.descriptor().expression().source_unit(),
                    plan.descriptor().target().source_unit()
                );
                assert_eq!(
                    owned.materialization_at(plan.descriptor().expression()),
                    Some(plan)
                );
            }
            let reversed = [inputs[1], inputs[0]];
            let again =
                check_compilation_unit_constant_ownership(sources, &reversed, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert_eq!(owned.materializations(), again.materializations());
            assert_eq!(owned.ownership().drops(), again.ownership().drops());
            assert_eq!(owned.ownership().loans(), again.ownership().loans());
            assert!(owned.clone().into_ownership().validate().is_ok());
            assert!(owned.ownership().clone().validate().is_err());
        },
    );
}

#[test]
fn constant_entry_retains_its_boundary_even_without_constant_declarations() {
    with_unit(
        "package a\nfun read(): Int = 7",
        "package b",
        |sources, inputs, names, te, typed| {
            assert!(typed.clone().into_types().validate().is_ok());
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert!(owned.materializations().is_empty());
            assert!(
                owned.ownership().clone().validate().is_err(),
                "entry provenance must survive recovery cloning"
            );
        },
    );
}

#[test]
fn constant_entry_rejects_mixed_inputs_names_environment_and_typed_owner() {
    with_unit(
        "package a\nconst val X = 7\nfun read(): Int = X",
        "package b",
        |sources, inputs, names, te, typed| {
            let rejected = |sources, inputs: &[SourceUnitInput<'_>], names, te| {
                assert!(matches!(
                    check_compilation_unit_constant_ownership(sources, inputs, names, te, typed),
                    Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
                ));
            };
            rejected(sources, &[inputs[0], inputs[0]], names, te);
            rejected(sources, &inputs[..1], names, te);
            rejected(
                sources,
                &[
                    SourceUnitInput::new(
                        "other",
                        "a/source.ko",
                        inputs[0].source_id(),
                        inputs[0].parsed(),
                    ),
                    inputs[1],
                ],
                names,
                te,
            );
            let foreign_sources = SourceMap::new();
            rejected(&foreign_sources, inputs, names, te);
            let (ne, foreign_environment) = standard_environments();
            rejected(sources, inputs, names, &foreign_environment);
            let index = index_compilation_unit(sources, inputs).unwrap();
            let foreign_names = resolve_compilation_unit_names(sources, inputs, &index, &ne)
                .unwrap()
                .validate()
                .unwrap();
            rejected(sources, inputs, &foreign_names, te);
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            let fresh = check_compilation_unit_types(sources, inputs, names, te)
                .unwrap()
                .validate_constants()
                .unwrap();
            assert_eq!(typed.constants(), fresh.constants());
            assert!(
                !owned.is_compatible_with(&fresh),
                "equal values do not imply equal analysis identity"
            );
        },
    );
}

#[test]
fn errors_and_deferred_ownership_never_publish_constant_owned_capability() {
    for (body, error) in [
        (
            "fun take(own text: String): Unit {}\nfun read(text: String): Unit { val first = take(TEXT)\nval second = take(text) }",
            true,
        ),
        (
            "class Resource {}\nclass Holder(var payload: Resource)\nfun read(): String = TEXT\nfun deferred(holders: List<Holder>): Unit { val projected = holders[0].payload }",
            false,
        ),
    ] {
        with_unit(
            &format!("package a\nconst val TEXT = \"hello\"\n{body}"),
            "package b",
            |sources, inputs, names, te, typed| {
                let recovery =
                    check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                        .unwrap();
                assert_eq!(!recovery.ownership().diagnostics().is_empty(), error);
                if !error {
                    assert!(!recovery.ownership().deferred().is_empty());
                }
                assert!(recovery.materializations().is_none());
                if error {
                    assert!(recovery.ownership().loans().is_empty());
                    assert!(recovery.ownership().value_deliveries().is_empty());
                    assert!(recovery.ownership().drops().is_empty());
                }
                assert!(recovery.validate().is_err());
            },
        );
    }
}
