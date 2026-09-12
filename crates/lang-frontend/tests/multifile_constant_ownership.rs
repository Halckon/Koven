//! SPEC-0226 public capability and source-qualified ownership identity.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        ConstantMaterializationKind, OwnershipCheckingError, UnitDropPoint, UnitDropTarget,
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
fn interpolation_releases_each_inner_owner_before_returning_the_outer_string() {
    with_unit(
        "package a\nimport b.Labels\nfun read(): String = \"${Labels.TEXT}-${(Labels.TEXT)}\"",
        "package b\nobject Labels { const val TEXT = \"hi\" }",
        |sources, inputs, names, te, typed| {
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            let plans = owned.materializations();
            let drops = owned.ownership().drops();
            assert_eq!(plans.len(), 2);
            assert_eq!(
                drops.len(),
                2,
                "inner owners must be released, outer is returned: {drops:?}"
            );
            assert_eq!(drops[0].point(), drops[1].point());
            assert!(drops[0].value_origin().start() > drops[1].value_origin().start());
            for plan in plans {
                let matching = drops
                    .iter()
                    .filter(|drop| {
                        drop.target() == UnitDropTarget::Temporary(plan.descriptor().expression())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(matching.len(), 1);
                assert!(matches!(
                    matching[0].point(),
                    UnitDropPoint::AfterExpression(_)
                ));
                assert_eq!(
                    sources.slice(matching[0].value_origin()).unwrap(),
                    "Labels.TEXT"
                );
            }
        },
    );
}

#[test]
fn interpolation_prefix_owners_follow_control_transfer_and_abort_like_literals() {
    for operand in ["Labels.TEXT", "\"hi\""] {
        for (tail, transfers, normal) in [
            ("if (flag) { return } else { 0 }", 1, 1),
            ("if (flag) { break } else { 0 }", 1, 1),
            ("if (flag) { continue } else { 0 }", 1, 1),
            ("if (flag) { stop() } else { 0 }", 0, 1),
            ("stop()", 0, 0),
        ] {
            with_unit(
                &format!(
                    "package a\nimport b.Labels\nfun stop(): Nothing = stop()\nfun read(flag: Boolean): Unit {{ loop {{ val result = \"${{{operand}}}-${{{tail}}}\"\nbreak }} }}"
                ),
                "package b\nobject Labels { const val TEXT = \"hi\" }",
                |sources, inputs, names, te, typed| {
                    let owned = check_compilation_unit_constant_ownership(
                        sources, inputs, names, te, typed,
                    )
                    .unwrap()
                    .validate()
                    .unwrap();
                    let drops = owned
                        .ownership()
                        .drops()
                        .iter()
                        .filter(|drop| sources.slice(drop.value_origin()).unwrap() == operand)
                        .collect::<Vec<_>>();
                    assert_eq!(
                        drops
                            .iter()
                            .filter(|drop| matches!(
                                drop.point(),
                                UnitDropPoint::ControlTransfer(_)
                            ))
                            .count(),
                        transfers,
                        "{operand}, {tail}: {drops:?}"
                    );
                    assert_eq!(
                        drops
                            .iter()
                            .filter(|drop| matches!(
                                drop.point(),
                                UnitDropPoint::AfterExpression(_)
                            ))
                            .count(),
                        normal,
                        "{operand}, {tail}: {drops:?}"
                    );
                    assert_eq!(drops.len(), transfers + normal);
                },
            );
        }
    }
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
fn nested_interpolation_keeps_outer_inputs_and_live_named_owners_until_their_boundary() {
    with_unit(
        r#"package a
import b.Labels
fun view(text: String): Unit {}
fun read(own kept: String): String {
    val result = "${Labels.TEXT}-${"${Labels.TEXT}-${kept}"}"
    val observed = view(kept)
    return result
}"#,
        "package b\nobject Labels { const val TEXT = \"hi\" }",
        |sources, inputs, names, te, typed| {
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            let plans = owned.materializations();
            assert_eq!(plans.len(), 2);
            let drops = owned.ownership().drops();
            assert_eq!(
                drops.len(),
                4,
                "two constants, inner result, and kept: {drops:?}"
            );
            let constant_drop = |index: usize| {
                drops
                    .iter()
                    .find(|drop| {
                        drop.target()
                            == UnitDropTarget::Temporary(plans[index].descriptor().expression())
                    })
                    .unwrap()
            };
            let outer = constant_drop(0);
            let inner = constant_drop(1);
            assert_ne!(
                outer.point(),
                inner.point(),
                "inner completion must retain the outer prefix"
            );
            assert_eq!(&drops[0], inner);
            assert_eq!(&drops[2], outer);
            assert_eq!(
                drops[1].point(),
                outer.point(),
                "inner result is released with the outer inputs"
            );
            assert!(matches!(outer.point(), UnitDropPoint::AfterExpression(_)));
            assert!(matches!(inner.point(), UnitDropPoint::AfterExpression(_)));
            let UnitDropPoint::AfterExpression(inner_expression) = inner.point() else {
                unreachable!("inner interpolation completion checked above");
            };
            assert_eq!(
                drops[1].target(),
                UnitDropTarget::Temporary(inner_expression)
            );
            assert_eq!(sources.slice(drops[3].value_origin()).unwrap(), "kept");
            assert!(
                matches!(drops[3].point(), UnitDropPoint::CallReturn(_)),
                "kept must survive until its later borrow ends"
            );
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
