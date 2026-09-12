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
                assert!(recovery.short_circuits().is_none());
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

#[test]
fn string_binary_prefix_cleanup_matches_literals_on_control_flow_edges() {
    for operand in ["Labels.TEXT", "\"hi\""] {
        for operator in ["+", "==", "!="] {
            for (tail, transfers, normal) in [
                ("if (flag) { return } else { 0 }", 1, 1),
                ("if (flag) { break } else { 0 }", 1, 1),
                ("if (flag) { continue } else { 0 }", 1, 1),
                ("if (flag) { stop() } else { 0 }", 0, 1),
                ("stop()", 0, 0),
                ("return", 1, 0),
                ("break", 1, 0),
                ("continue", 1, 0),
            ] {
                with_unit(
                    &format!(
                        "package a\nimport b.Labels\nfun stop(): Nothing = stop()\nfun read(flag: Boolean): Unit {{ loop {{ val result = {operand} {operator} \"${{{tail}}}\"\nbreak }} }}"
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
                            "{operand} {operator} {tail}: {drops:?}"
                        );
                        assert_eq!(
                            drops
                                .iter()
                                .filter(|drop| matches!(
                                    drop.point(),
                                    UnitDropPoint::AfterBinaryOperands(_)
                                ))
                                .count(),
                            normal,
                            "{operand} {operator} {tail}: {drops:?}"
                        );
                        assert_eq!(drops.len(), transfers + normal);
                        if normal == 0 {
                            assert_eq!(
                                owned.ownership().drops().len(),
                                transfers,
                                "no right/result owner completes on {tail}"
                            );
                        }
                    },
                );
            }
        }
    }
}

#[test]
fn string_binary_nested_prefixes_release_in_reverse_evaluation_order() {
    with_unit(
        "package a\nimport b.Labels\nfun read(flag: Boolean): Unit { val result = Labels.TEXT + (Labels.TEXT + \"${if (flag) { return } else { 0 }}\") }",
        "package b\nobject Labels { const val TEXT = \"hi\" }",
        |sources, inputs, names, te, typed| {
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert_eq!(owned.materializations().len(), 2);
            let transfers = owned
                .ownership()
                .drops()
                .iter()
                .filter(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
                .collect::<Vec<_>>();
            assert_eq!(
                transfers.len(),
                2,
                "both completed prefixes must unwind: {transfers:?}"
            );
            assert_eq!(transfers[0].point(), transfers[1].point());
            assert!(transfers[0].value_origin().start() > transfers[1].value_origin().start());
            for (transfer, plan) in transfers.iter().zip(owned.materializations().iter().rev()) {
                assert_eq!(
                    transfer.target(),
                    UnitDropTarget::Temporary(plan.descriptor().expression())
                );
                assert_eq!(
                    sources.slice(transfer.value_origin()).unwrap(),
                    "Labels.TEXT"
                );
                assert_eq!(
                    owned
                        .ownership()
                        .drops()
                        .iter()
                        .filter(|drop| drop.target() == transfer.target()
                            && matches!(drop.point(), UnitDropPoint::AfterBinaryOperands(_)))
                        .count(),
                    1
                );
            }
        },
    );
}

#[test]
fn string_binary_aborting_left_does_not_visit_the_right_or_following_read() {
    for right in ["Labels.TEXT", "7"] {
        with_unit(
            &format!(
                "package a\nimport b.Labels\nfun stop(): Nothing = stop()\nfun read(): Unit {{ val stopped = stop() == {right}\nval unreachable = Labels.TEXT }}"
            ),
            "package b\nobject Labels { const val TEXT = \"hi\" }",
            |sources, inputs, names, te, typed| {
                let owned =
                    check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                        .unwrap()
                        .validate()
                        .unwrap();
                assert!(owned.materializations().is_empty());
                assert!(owned.ownership().loans().is_empty());
                assert!(
                    owned.ownership().drops().is_empty(),
                    "no evaluated owner exists: {:?}",
                    owned.ownership().drops()
                );
            },
        );
    }
}

#[test]
fn string_binary_named_left_survives_right_branches_calls_and_nested_views() {
    for right in [
        "if (flag) { Labels.TEXT } else { Labels.TEXT }",
        "\"${view(kept)}\"",
        "(kept + Labels.TEXT)",
    ] {
        for later_use in [false, true] {
            let later = if later_use {
                "val observed = view(kept)"
            } else {
                ""
            };
            with_unit(
                &format!(
                    "package a\nimport b.Labels\nfun view(text: String): Int = 7\nfun read(own kept: String, flag: Boolean): Unit {{ val result = kept + {right}\n{later} }}"
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
                        .filter(|drop| sources.slice(drop.value_origin()).unwrap() == "kept")
                        .collect::<Vec<_>>();
                    assert_eq!(
                        drops.len(),
                        1,
                        "kept + {right}, later={later_use}: {drops:?}"
                    );
                    if later_use {
                        assert!(
                            matches!(drops[0].point(), UnitDropPoint::CallReturn(_)),
                            "the later borrow is still live: {drops:?}"
                        );
                    } else {
                        let UnitDropPoint::AfterBinaryOperands(binary) = drops[0].point() else {
                            panic!(
                                "left view is needed until the outer binary finishes: {drops:?}"
                            );
                        };
                        let node = inputs[0]
                            .parsed()
                            .ast()
                            .expressions()
                            .get(binary.expression())
                            .unwrap();
                        assert_eq!(
                            sources.slice(node.span()).unwrap(),
                            format!("kept + {right}")
                        );
                    }
                },
            );
        }
    }
}

#[test]
fn acceptance_scalar_repeated_delivery_has_no_runtime_owner_loan_or_drop() {
    for (ty, literal) in [
        ("Boolean", "true"),
        ("Byte", "1"),
        ("Short", "2"),
        ("Int", "3"),
        ("Long", "4"),
        ("UByte", "5u"),
        ("UShort", "6u"),
        ("UInt", "7u"),
        ("ULong", "8uL"),
        ("Char", "'文'"),
    ] {
        with_unit(
            &format!(
                "package a\nimport b.VALUE\nfun take(own payload: {ty}): Unit {{}}\nfun read(): Unit {{ val first = take(VALUE)\nval second = take(VALUE) }}"
            ),
            &format!("package b\nconst val VALUE: {ty} = {literal}"),
            |sources, inputs, names, te, typed| {
                let owned =
                    check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                        .unwrap()
                        .validate()
                        .unwrap();
                assert_eq!(owned.materializations().len(), 2, "{ty}");
                assert_eq!(owned.ownership().value_deliveries().len(), 2, "{ty}");
                for (plan, delivery) in owned
                    .materializations()
                    .iter()
                    .zip(owned.ownership().value_deliveries())
                {
                    assert_eq!(plan.kind(), ConstantMaterializationKind::InlineCopy, "{ty}");
                    assert_eq!(
                        delivery.source(),
                        &lang_frontend::ownership_checking::UnitValueDeliverySource::Temporary(
                            plan.descriptor().expression()
                        ),
                        "{ty}: never deliver the declaration place"
                    );
                    assert_ne!(
                        delivery.kind(),
                        lang_frontend::ownership_checking::UnitValueDeliveryKind::Move,
                        "{ty}: reading again cannot move the declaration"
                    );
                }
                assert!(owned.ownership().loans().is_empty(), "{ty}");
                assert!(owned.ownership().drops().is_empty(), "{ty}");
                assert!(owned.ownership().captures().is_empty(), "{ty}");
            },
        );
    }
}

#[test]
fn acceptance_colliding_local_ids_keep_capture_and_drop_sources_stable() {
    let body = "const val TEXT = \"hi\"\nfun view(text: String): Unit {}\nfun read(own kept: String): Unit { val f = { val first = view(kept)\nval second = view(OTHER.TEXT) }\nf() }";
    with_unit(
        &format!("package a\n{}", body.replace("OTHER", "b")),
        &format!("package b\n{}", body.replace("OTHER", "a")),
        |sources, inputs, names, te, typed| {
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            let plans = owned.materializations();
            assert_eq!(plans.len(), 2);
            assert_eq!(
                plans[0].descriptor().expression().expression(),
                plans[1].descriptor().expression().expression()
            );
            assert_ne!(
                plans[0].descriptor().expression(),
                plans[1].descriptor().expression()
            );
            let captures = owned.ownership().captures();
            assert_eq!(captures.len(), 2, "only kept is captured in each source");
            for capture in captures {
                assert_eq!(sources.slice(capture.reference_span()).unwrap(), "kept");
            }
            let closures = owned.ownership().closures();
            assert_eq!(closures.len(), 2);
            assert_eq!(
                closures[0].expression().expression(),
                closures[1].expression().expression()
            );
            assert_ne!(closures[0].expression(), closures[1].expression());
            for closure in closures {
                let matching = captures
                    .iter()
                    .filter(|capture| capture.lambda() == closure.expression())
                    .collect::<Vec<_>>();
                assert_eq!(matching.len(), 1);
                let lang_frontend::ownership_checking::UnitClosureCaptureSource::Symbol(symbol) =
                    matching[0].source()
                else {
                    panic!("kept is a local symbol")
                };
                assert_eq!(symbol.source_unit(), closure.expression().source_unit());
            }
            for plan in plans {
                let owner = plan.descriptor().expression();
                let drops = owned
                    .ownership()
                    .drops()
                    .iter()
                    .filter(|drop| drop.target() == UnitDropTarget::Temporary(owner))
                    .collect::<Vec<_>>();
                assert_eq!(drops.len(), 1, "same local ID must not merge source owners");
                let UnitDropPoint::CallReturn(call) = drops[0].point() else {
                    panic!("borrowed constant lives until view returns")
                };
                assert_eq!(call.source_unit(), owner.source_unit());
                assert_ne!(
                    plan.descriptor().target().source_unit(),
                    owner.source_unit()
                );
            }
            let reversed = [inputs[1], inputs[0]];
            let again =
                check_compilation_unit_constant_ownership(sources, &reversed, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert_eq!(owned.materializations(), again.materializations());
            assert_eq!(owned.ownership().captures(), again.ownership().captures());
            assert_eq!(owned.ownership().closures(), again.ownership().closures());
            assert_eq!(owned.ownership().loans(), again.ownership().loans());
            assert_eq!(owned.ownership().drops(), again.ownership().drops());
        },
    );
}

#[test]
fn short_circuit_skipped_rhs_has_no_constant_materialization_or_cleanup() {
    for (left, operator) in [("false", "&&"), ("true", "||")] {
        with_unit(
            &format!(
                "package a\nimport b.TEXT\nfun view(text: String): Boolean = true\nfun read(): Boolean = {left} {operator} view(TEXT)"
            ),
            "package b\nconst val TEXT = \"hi\"",
            |sources, inputs, names, te, typed| {
                let owned =
                    check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                        .unwrap()
                        .validate()
                        .unwrap();
                assert!(
                    owned.materializations().is_empty(),
                    "{left} {operator}: the RHS is not evaluated: {:?}",
                    owned.materializations()
                );
                assert!(owned.ownership().loans().is_empty());
                assert!(owned.ownership().drops().is_empty());
            },
        );
    }
}

#[test]
fn short_circuit_empty_constant_entry_keeps_base_behavior_separate() {
    with_unit(
        "package a\nfun view(text: String): Boolean = true\nfun read(): Boolean = false && view(\"hi\")",
        "package b",
        |sources, inputs, names, te, typed| {
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert!(owned.ownership().loans().is_empty());
            assert!(owned.ownership().drops().is_empty());
            let base_typed = typed.clone().into_types().validate().unwrap();
            let base = lang_frontend::ownership_checking::check_compilation_unit_ownership(
                sources,
                inputs,
                names,
                te,
                &base_typed,
            )
            .unwrap()
            .validate()
            .unwrap();
            assert_eq!(
                base.ownership().loans().len(),
                1,
                "old SSA contract remains explicit"
            );
        },
    );
}

#[test]
fn short_circuit_dynamic_rhs_exit_preserves_the_skip_successor() {
    for operator in ["&&", "||"] {
        for exit in ["return", "stop()"] {
            with_unit(
                &format!(
                    "package a\nimport b.TEXT\nfun stop(): Nothing = stop()\nfun view(text: String): Unit {{}}\nfun read(flag: Boolean): Unit {{ val result = flag {operator} (\"${{{exit}}}\" == TEXT)\nval after = view(TEXT) }}"
                ),
                "package b\nconst val TEXT = \"hi\"",
                |sources, inputs, names, te, typed| {
                    let owned = check_compilation_unit_constant_ownership(
                        sources, inputs, names, te, typed,
                    )
                    .unwrap()
                    .validate()
                    .unwrap();
                    assert_eq!(
                        owned.materializations().len(),
                        1,
                        "{operator}, {exit}: skip reaches after, RHS tail does not"
                    );
                    assert_eq!(owned.ownership().loans().len(), 1);
                    let call = owned.ownership().loans()[0].call();
                    let node = inputs[0]
                        .parsed()
                        .ast()
                        .expressions()
                        .get(call.expression())
                        .unwrap();
                    assert_eq!(sources.slice(node.span()).unwrap(), "view(TEXT)");
                    assert_eq!(owned.ownership().drops().len(), 1);
                    assert_eq!(
                        owned.ownership().drops()[0].point(),
                        UnitDropPoint::CallReturn(call)
                    );
                },
            );
        }
    }
}

#[test]
fn short_circuit_conditional_move_cleans_only_the_skipped_edge() {
    for (operator, skipped_branch) in [("&&", 1), ("||", 0)] {
        with_unit(
            &format!(
                "package a\nfun take(own payload: String): Boolean = true\nfun read(flag: Boolean, own kept: String): Unit {{ val result = flag {operator} take(kept) }}"
            ),
            "package b",
            |sources, inputs, names, te, typed| {
                let owned =
                    check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                        .unwrap()
                        .validate()
                        .unwrap();
                let drops = owned
                    .ownership()
                    .drops()
                    .iter()
                    .filter(|drop| sources.slice(drop.value_origin()).unwrap() == "kept")
                    .collect::<Vec<_>>();
                assert_eq!(drops.len(), 1);
                let UnitDropPoint::BranchExit { control, branch } = drops[0].point() else {
                    panic!("skip must release kept: {drops:?}")
                };
                assert_eq!(branch, skipped_branch);
                let node = inputs[0]
                    .parsed()
                    .ast()
                    .expressions()
                    .get(control.expression())
                    .unwrap();
                assert_eq!(
                    sources.slice(node.span()).unwrap(),
                    format!("flag {operator} take(kept)")
                );
            },
        );
    }
}

#[test]
fn short_circuit_rhs_exit_preserves_outer_pending_operand_on_skip() {
    for mode in ["", "own "] {
        for operator in ["&&", "||"] {
            for exit in ["return", "stop()"] {
                with_unit(
                    &format!(
                        "package a\nimport b.TEXT\nfun stop(): Nothing = stop()\nfun view({mode}text: String, flag: Boolean): Unit {{}}\nfun read(flag: Boolean): Unit {{ val result = view(TEXT, flag {operator} (\"${{{exit}}}\" == \"x\")) }}"
                    ),
                    "package b\nconst val TEXT = \"hi\"",
                    |sources, inputs, names, te, typed| {
                        let owned = check_compilation_unit_constant_ownership(
                            sources, inputs, names, te, typed,
                        )
                        .unwrap()
                        .validate()
                        .unwrap();
                        assert_eq!(owned.materializations().len(), 1);
                        let owner = owned.materializations()[0].descriptor().expression();
                        let drops = owned
                            .ownership()
                            .drops()
                            .iter()
                            .filter(|drop| drop.target() == UnitDropTarget::Temporary(owner))
                            .collect::<Vec<_>>();
                        let transfers = usize::from(exit == "return");
                        let returns = usize::from(mode.is_empty());
                        assert_eq!(
                            drops
                                .iter()
                                .filter(|drop| matches!(
                                    drop.point(),
                                    UnitDropPoint::ControlTransfer(_)
                                ))
                                .count(),
                            transfers,
                            "{mode}, {operator}, {exit}: {drops:?}"
                        );
                        assert_eq!(
                            drops
                                .iter()
                                .filter(|drop| matches!(drop.point(), UnitDropPoint::CallReturn(_)))
                                .count(),
                            returns,
                            "{mode}, {operator}, {exit}: {drops:?}"
                        );
                        assert_eq!(drops.len(), transfers + returns);
                    },
                );
            }
        }
    }
}

#[test]
fn short_circuit_public_plans_preserve_sources_decisions_and_recovery() {
    use lang_frontend::ownership_checking::UnitShortCircuitRhs;
    let body = "const val FLAG = false\nconst val TEXT = \"hi\"\nfun view(text: String): Boolean = true\nfun read(flag: Boolean): Boolean { val skipped = (OTHER.FLAG) && view(OTHER.TEXT)\nval evaluated = true && view(OTHER.TEXT)\nreturn flag || view(OTHER.TEXT) }";
    with_unit(
        &format!("package a\n{}", body.replace("OTHER", "b")),
        &format!("package b\n{}", body.replace("OTHER", "a")),
        |sources, inputs, names, te, typed| {
            let recovery =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap();
            assert_eq!(recovery.short_circuits().unwrap().len(), 6);
            let owned = recovery.validate().unwrap();
            let plans = owned.short_circuits();
            assert_eq!(plans.len(), 6);
            for (index, plan) in plans.iter().enumerate() {
                assert_eq!(
                    plan.rhs(),
                    [
                        UnitShortCircuitRhs::Never,
                        UnitShortCircuitRhs::Always,
                        UnitShortCircuitRhs::Conditional
                    ][index % 3]
                );
                assert_eq!(plan.rhs_branch(), usize::from(index % 3 == 2));
                assert_eq!(plan.left().source_unit(), plan.expression().source_unit());
                assert_eq!(plan.right().source_unit(), plan.expression().source_unit());
                assert_eq!(owned.short_circuit_at(plan.expression()), Some(plan));
                assert!(owned.short_circuit_at(plan.right()).is_none());
            }
            assert_eq!(
                plans[0].expression().expression(),
                plans[3].expression().expression()
            );
            assert_ne!(plans[0].expression(), plans[3].expression());
            let reversed = [inputs[1], inputs[0]];
            let again =
                check_compilation_unit_constant_ownership(sources, &reversed, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert_eq!(plans, again.short_circuits());
            assert_eq!(
                owned.clone().into_ownership().short_circuits().unwrap(),
                plans
            );
        },
    );
}

#[test]
fn unreachable_lambda_does_not_publish_orphan_constant_cleanup() {
    for body in [
        "return\nval f = { view(TEXT) }",
        "val stopped = stop()\nval f = { view(TEXT) }",
        "val skipped = false && invoke({ view(TEXT) })",
    ] {
        with_unit(
            &format!(
                "package a\nimport b.TEXT\nfun stop(): Nothing = stop()\nfun view(text: String): Boolean = true\nfun invoke(action: () -> Boolean): Boolean = action()\nfun read(): Unit {{ {body} }}"
            ),
            "package b\nconst val TEXT = \"hi\"",
            |sources, inputs, names, te, typed| {
                let owned =
                    check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                        .unwrap()
                        .validate()
                        .unwrap();
                assert!(owned.materializations().is_empty(), "{body}");
                assert!(owned.ownership().loans().is_empty(), "{body}");
                assert!(
                    owned.ownership().drops().is_empty(),
                    "{body}: {:?}",
                    owned.ownership().drops()
                );
                assert!(
                    owned.ownership().closures().is_empty(),
                    "no unreachable closure entry"
                );
            },
        );
    }
}
