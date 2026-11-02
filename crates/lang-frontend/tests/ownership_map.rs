//! SPEC-0288: Map 参数交付与 receiver 权限的 MemoryDocument/Rooted 合同。
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::Expression,
    type_checking::UnitExpressionId,
};

#[path = "ownership_map/consume.rs"]
mod consume;
#[path = "ownership_map/unit_contract.rs"]
mod unit_contract;
use unit_contract::{check_each, expression};

#[test]
fn map_queries_borrow_string_keys_and_allow_reuse() {
    check_each(
        "fun run(m: Map<String, Int>, key: String): Unit {\n m.contains(key)\n m.get(key)\n val v = m[key]\n println(key)\n}",
        |_, _, _, unit| {
            assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        },
    );
}

#[test]
fn map_subscript_releases_a_temporary_string_key_after_query() {
    check_each(
        "fun run(m: Map<String, Int>): Unit {\n val result = m[\"key\"]\n}",
        |_, _, _, unit| {
            assert!(unit.drops().iter().any(|drop| matches!(
                drop.point(),
                UnitDropPoint::CallReturn(_)
            ) && matches!(
                drop.target(),
                UnitDropTarget::Temporary(_)
            )));
        },
    );
}

#[test]
fn map_mutation_requires_mutable_receiver() {
    for operation in ["m.put(1, 2)", "m.remove(1)", "m[1] = 2"] {
        check_each(
            &format!("fun run(): Unit {{\n val m = mutableMapOf<Int, Int>()\n {operation}\n}}"),
            |_, _, _, unit| {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0134"),
                    "{operation}: {:?}",
                    unit.diagnostics()
                );
            },
        );
    }
}

#[test]
fn map_put_owns_string_key_and_move_only_value() {
    for operation in ["m.put(key, value)", "m[key] = value"] {
        check_each(
            &format!(
                "class Resource {{}}\nfun run(key: String, value: Resource): Unit {{\n var m = mutableMapOf<String, Resource>()\n {operation}\n}}"
            ),
            |_, _, _, unit| {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0133"),
                    "{operation}: {:?}",
                    unit.diagnostics()
                );
            },
        );
    }
}

#[test]
fn map_remove_borrows_string_key() {
    check_each(
        "fun run(key: String): Unit {\n var m = mutableMapOf<String, Int>()\n m.remove(key)\n println(key)\n}",
        |_, _, _, unit| {
            assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        },
    );
}

#[test]
fn map_temporary_null_comparisons_publish_one_result_cleanup() {
    for (ty, value) in [
        ("String", "\"text\".clone()"),
        ("Text", "Text(\"text\".clone())"),
        ("Resource", "Resource()"),
    ] {
        for condition in [
            "m.remove(1) != null",
            "null == m.remove(1)",
            "((m.remove(1))) == null",
            "null != ((m.remove(1)))",
        ] {
            let source = format!(
                "value class Text(val text: String)\nclass Resource {{ deinit() {{ println(\"drop\") }} }}\nfun run(): Unit {{ var m = mutableMapOf<Int, {ty}>(); m.put(1, {value}); val present = {condition}; if (present) {{ println(\"yes\") }} }}"
            );
            check_each(&source, |sources, file, source_unit, unit| {
                assert!(
                    unit.diagnostics().is_empty(),
                    "{source}: {:?}",
                    unit.diagnostics()
                );
                let call = expression(sources, file, "m.remove(1)", |node| {
                    matches!(node, Expression::Call { .. })
                });
                let binary = expression(sources, file, condition, |node| {
                    matches!(node, Expression::Binary { .. })
                });
                let drops = unit
                    .drops()
                    .iter()
                    .filter(|fact| matches!(fact.point(), UnitDropPoint::AfterBinaryOperands(_)))
                    .collect::<Vec<_>>();
                let [drop] = drops.as_slice() else {
                    panic!("exactly the actual result, never the null literal: {source}")
                };
                assert_eq!(
                    drop.target(),
                    UnitDropTarget::Temporary(UnitExpressionId::new(source_unit, call))
                );
                assert_eq!(
                    drop.point(),
                    UnitDropPoint::AfterBinaryOperands(UnitExpressionId::new(source_unit, binary))
                );
                assert_eq!(
                    drop.value_origin(),
                    file.ast().expressions().get(call).unwrap().span()
                );
            });
        }
    }
}

#[test]
fn map_temporary_null_cleanup_excludes_copyable_named_borrowed_and_incomplete_results() {
    for source in [
        "fun run() { var m = mutableMapOf<Int, Int>(); val present = m.remove(1) != null }",
        "fun run() { var m = mutableMapOf<Int, String>(); val value = m.remove(1); val present = value != null }",
        "class Resource {}\nfun view(source: Resource?): borrow Resource? from source = source\nfun run(source: Resource?) { borrow val value = view(source); val present = value != null }",
        "fun run() { var m = mutableMapOf<Int, String>(); val present = m.remove(error(\"stop\")) != null }",
    ] {
        check_each(source, |_, _, _, unit| {
            assert!(
                unit.diagnostics().is_empty(),
                "{source}: {:?}",
                unit.diagnostics()
            );

            assert!(
                !unit
                    .drops()
                    .iter()
                    .any(|fact| matches!(fact.point(), UnitDropPoint::AfterBinaryOperands(_))),
                "{source}"
            );
        });
    }
}

#[test]
fn map_temporary_null_direct_borrow_result_keeps_rejection() {
    let source = "class Resource {}\nfun view(source: Resource?): borrow Resource? from source = source\nfun run(source: Resource?) { val present = view(source) != null }";
    check_each(source, |_, _, _, unit| {
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164")
        );

        assert!(unit.drops().is_empty());
    });
}

#[test]
fn map_temporary_null_cleanup_keeps_key_call_return_separate() {
    let source = "fun run() { var m = mutableMapOf<String, String>(); val present = m.remove(\"key\".clone()) != null }";
    check_each(source, |sources, file, source_unit, unit| {
        let result = expression(sources, file, "m.remove(\"key\".clone())", |node| {
            matches!(node, Expression::Call { .. })
        });
        let key = expression(sources, file, "\"key\".clone()", |node| {
            matches!(node, Expression::Call { .. })
        });
        let binary = expression(sources, file, "m.remove(\"key\".clone()) != null", |node| {
            matches!(node, Expression::Binary { .. })
        });
        let results = unit
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::AfterBinaryOperands(_)))
            .collect::<Vec<_>>();
        let [drop] = results.as_slice() else {
            panic!("one result cleanup")
        };
        assert_eq!(
            drop.target(),
            UnitDropTarget::Temporary(UnitExpressionId::new(source_unit, result))
        );
        assert_eq!(
            drop.point(),
            UnitDropPoint::AfterBinaryOperands(UnitExpressionId::new(source_unit, binary))
        );
        assert_eq!(
            drop.value_origin(),
            file.ast().expressions().get(result).unwrap().span()
        );
        let key_span = file.ast().expressions().get(key).unwrap().span();
        let keys = unit
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), UnitDropTarget::Temporary(_))
                    && fact.value_origin() == key_span
            })
            .collect::<Vec<_>>();
        let [drop] = keys.as_slice() else {
            panic!("one key cleanup")
        };
        assert_eq!(
            drop.target(),
            UnitDropTarget::Temporary(UnitExpressionId::new(source_unit, key))
        );
        assert_eq!(
            drop.point(),
            UnitDropPoint::CallReturn(UnitExpressionId::new(source_unit, result))
        );
    });
}

#[test]
fn map_temporary_null_comparison_keeps_inout_receiver_place_rejection() {
    check_each(
        "fun run() { val present = mutableMapOf<Int, String>().remove(1) != null }",
        |_, _, _, unit| {
            assert!(
                unit.diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0134")
            );

            assert!(unit.drops().is_empty());
        },
    );
}
