//! SPEC-0254 常量 owned-unit 交接：精确借用、完整专用事实与 basic 隔离。

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        ConstEnabledOwnedUnit, ConstOwnedCompilationUnitView, ConstantMaterializationKind,
        OwnedCompilationUnitViewError, UnitShortCircuitRhs,
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
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
        &ConstEnabledOwnedUnit,
    ),
) {
    let mut sources = SourceMap::new();
    let a = sources.add_source("a.ko", first).expect("first source");
    let b = sources.add_source("b.ko", second).expect("second source");
    let pa = parse_file(&sources, &lex(&sources, a).expect("first lex")).expect("first parse");
    let pb = parse_file(&sources, &lex(&sources, b).expect("second lex")).expect("second parse");
    assert!(pa.diagnostics().is_empty(), "{:?}", pa.diagnostics());
    assert!(pb.diagnostics().is_empty(), "{:?}", pb.diagnostics());
    let inputs = [
        SourceUnitInput::new("root", "a/source.ko", a, &pa),
        SourceUnitInput::new("root", "b/source.ko", b, &pb),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("names")
        .validate()
        .expect("validated names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("types")
        .validate_constants()
        .expect("constant typed capability");
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .expect("ownership")
            .validate()
            .expect("constant owned capability");
    test(&sources, &inputs, &names, &environment, &typed, &owned);
}

#[test]
fn const_view_borrows_exact_products_and_keeps_complete_constant_facts() {
    for (value, operator, rhs) in [
        ("false", "&&", UnitShortCircuitRhs::Never),
        ("true", "||", UnitShortCircuitRhs::Never),
        ("true", "&&", UnitShortCircuitRhs::Always),
        ("false", "||", UnitShortCircuitRhs::Always),
    ] {
        with_unit(
            &format!(
                "package a\nimport b.FLAG\nimport b.TEXT\nfun view(text: String): Boolean = true\nfun read(): Boolean = (FLAG) {operator} view(TEXT)\nfun dead(): Unit {{ return\nval unused = false && view(TEXT) }}"
            ),
            &format!("package b\nconst val FLAG = {value} && true\nconst val TEXT = \"hi\""),
            |sources, inputs, names, environment, typed, owned| {
                let view: ConstOwnedCompilationUnitView<'_, '_> =
                    const_owned_compilation_unit_view(
                        sources,
                        inputs,
                        names,
                        environment,
                        typed,
                        owned,
                    )
                    .expect("matching constant view");
                assert!(std::ptr::eq(view.sources(), sources));
                assert!(std::ptr::eq(view.inputs(), inputs));
                assert!(std::ptr::eq(view.names(), names));
                assert!(std::ptr::eq(view.types(), typed.types()));
                assert!(std::ptr::eq(view.ownership(), owned.ownership()));
                assert!(std::ptr::eq(view.constant_ownership(), owned));
                assert_eq!(view.types(), typed.types());
                assert_eq!(view.ownership(), owned.ownership());

                let expected_materializations = owned.materializations().to_vec();
                let expected_short_circuits = owned.short_circuits().to_vec();
                let constant = view.constant_ownership();
                assert_eq!(constant.materializations(), expected_materializations);
                assert_eq!(constant.short_circuits(), expected_short_circuits);
                assert!(std::ptr::eq(
                    constant.materializations(),
                    owned.materializations(),
                ));
                assert!(std::ptr::eq(
                    constant.short_circuits(),
                    owned.short_circuits()
                ));
                assert_eq!(constant.short_circuits().len(), 1);
                let short = &constant.short_circuits()[0];
                assert_eq!(short.rhs(), rhs);
                assert_eq!(short.rhs_branch(), usize::from(operator == "||"));
                assert_eq!(short.left().source_unit(), short.expression().source_unit());
                assert_eq!(
                    short.right().source_unit(),
                    short.expression().source_unit()
                );
                assert_eq!(
                    constant.materializations().len(),
                    if rhs == UnitShortCircuitRhs::Never {
                        1
                    } else {
                        2
                    },
                );
                assert_eq!(
                    constant.materializations()[0].kind(),
                    ConstantMaterializationKind::InlineCopy
                );
                assert_eq!(
                    constant
                        .materializations()
                        .iter()
                        .filter(|plan| {
                            plan.kind() == ConstantMaterializationKind::StringTemporary
                        })
                        .count(),
                    usize::from(rhs == UnitShortCircuitRhs::Always),
                );
                for plan in constant.materializations() {
                    let expression = plan.descriptor().expression();
                    assert!(std::ptr::eq(
                        constant
                            .materialization_at(expression)
                            .expect("constant lookup"),
                        plan,
                    ));
                    assert_ne!(
                        expression.source_unit(),
                        plan.descriptor().target().source_unit()
                    );
                }
                for plan in constant.short_circuits() {
                    assert!(std::ptr::eq(
                        constant
                            .short_circuit_at(plan.expression())
                            .expect("short-circuit lookup"),
                        plan,
                    ));
                }
                assert!(constant.materialization_at(short.expression()).is_none());
                assert!(constant.short_circuit_at(short.left()).is_none());
            },
        );
    }
}

#[test]
fn const_view_retains_reordered_inputs_clones_and_same_typed_rechecks() {
    with_unit(
        "package a\nimport b.TEXT\nfun read(): String = TEXT",
        "package b\nconst val TEXT = \"hi\"",
        |sources, inputs, names, environment, typed, owned| {
            let reversed = [inputs[1], inputs[0]];
            let names_clone = names.clone();
            let environment_clone = environment.clone();
            let typed_clone = typed.clone();
            let owned_clone = owned.clone();
            let view = const_owned_compilation_unit_view(
                sources,
                &reversed,
                &names_clone,
                &environment_clone,
                &typed_clone,
                &owned_clone,
            )
            .expect("legal clones and input order retain identity");
            assert!(std::ptr::eq(view.inputs(), reversed.as_slice()));
            assert!(std::ptr::eq(view.names(), &names_clone));
            assert!(std::ptr::eq(view.types(), typed_clone.types()));
            assert!(std::ptr::eq(view.constant_ownership(), &owned_clone));
            assert_eq!(view.ownership(), owned.ownership());
            assert_eq!(
                view.constant_ownership().materializations(),
                owned.materializations()
            );
            assert_eq!(
                view.constant_ownership().short_circuits(),
                owned.short_circuits()
            );

            let rechecked = check_compilation_unit_constant_ownership(
                sources,
                inputs,
                names,
                environment,
                typed,
            )
            .expect("same typed ownership recheck")
            .validate()
            .expect("rechecked owned");
            let rechecked_view = const_owned_compilation_unit_view(
                sources,
                inputs,
                names,
                environment,
                typed,
                &rechecked,
            )
            .expect("same typed owner remains compatible");
            assert!(std::ptr::eq(
                rechecked_view.constant_ownership(),
                &rechecked
            ));
            assert_eq!(rechecked_view.ownership(), owned.ownership());
            assert_eq!(
                rechecked_view.constant_ownership().materializations(),
                owned.materializations()
            );
            assert_eq!(
                rechecked_view.constant_ownership().short_circuits(),
                owned.short_circuits()
            );
        },
    );
}

#[test]
fn const_view_reports_source_errors_before_analysis_mismatches() {
    with_unit(
        "package a\nimport b.TEXT\nfun read(): String = TEXT",
        "package b\nconst val TEXT = \"hi\"",
        |sources, inputs, names, environment, typed, owned| {
            let duplicate = [inputs[0], inputs[0]];
            let changed = [
                SourceUnitInput::new(
                    "other",
                    inputs[0].logical_path(),
                    inputs[0].source_id(),
                    inputs[0].parsed(),
                ),
                inputs[1],
            ];
            let foreign = SourceMap::new();
            for (map, candidate, expected) in [
                (
                    &foreign,
                    inputs,
                    OwnedCompilationUnitViewError::MismatchedSource,
                ),
                (
                    sources,
                    duplicate.as_slice(),
                    OwnedCompilationUnitViewError::MismatchedSource,
                ),
                (
                    sources,
                    changed.as_slice(),
                    OwnedCompilationUnitViewError::MismatchedAnalysis,
                ),
                (
                    sources,
                    [].as_slice(),
                    OwnedCompilationUnitViewError::MismatchedAnalysis,
                ),
            ] {
                assert_eq!(
                    const_owned_compilation_unit_view(
                        map,
                        candidate,
                        names,
                        environment,
                        typed,
                        owned
                    )
                    .err()
                    .expect("mismatch rejected"),
                    expected,
                );
            }
        },
    );
}

#[test]
fn empty_constant_entry_keeps_short_circuit_facts_and_cannot_bypass_basic_validation() {
    // Frozen distinction from multifile_constant_ownership: the dedicated path skips
    // this literal RHS, while the basic checker intentionally retains its loan.
    with_unit(
        "package a\nfun view(text: String): Boolean = true\nfun read(): Boolean = false && view(\"hi\")",
        "package b",
        |sources, inputs, names, environment, typed, owned| {
            let view = const_owned_compilation_unit_view(
                sources,
                inputs,
                names,
                environment,
                typed,
                owned,
            )
            .expect("empty-constant unit still uses the dedicated view");
            assert!(view.constant_ownership().materializations().is_empty());
            assert_eq!(view.constant_ownership().short_circuits().len(), 1);
            assert_eq!(
                view.constant_ownership().short_circuits()[0].rhs(),
                UnitShortCircuitRhs::Never
            );
            assert!(view.ownership().loans().is_empty());
            assert!(view.ownership().drops().is_empty());
            assert!(view.ownership().clone().validate().is_err());
            assert!(
                view.constant_ownership()
                    .clone()
                    .into_ownership()
                    .ownership()
                    .clone()
                    .validate()
                    .is_err(),
                "constant origin survives both recovery projections with no constant declarations",
            );

            let basic_typed = typed
                .clone()
                .into_types()
                .validate()
                .expect("no declarations permits basic types");
            let basic_owned =
                check_compilation_unit_ownership(sources, inputs, names, environment, &basic_typed)
                    .expect("independent basic check")
                    .validate()
                    .expect("basic owned");
            let basic_view = owned_compilation_unit_view(
                sources,
                inputs,
                names,
                environment,
                &basic_typed,
                &basic_owned,
            )
            .expect("independent basic view");
            assert_eq!(basic_view.ownership().loans().len(), 1);
            assert_eq!(view.ownership().loans().len(), 0);
            assert!(std::ptr::eq(view.constant_ownership(), owned));
        },
    );
}
