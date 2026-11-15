//! Constant-owned short-circuit edge, source and pending-operand plans.
use super::*;

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
