//! 原单次求值、重复检查和 error 遮蔽消费者，只观察类型描述符。
use super::super::{environments, parse};
use super::checked_entries;
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::Expression,
    type_checking::{BuiltinType, UnitExpressionId, UnitTypeKind, check_compilation_unit_types},
};

#[test]
fn non_null_assertion_describes_one_evaluation_without_a_synthetic_call() {
    let text = "class Node {}\nfun make(): Node? = null\nfun read(): Node = make()!!";
    let (sources, parsed) = parse(text);
    let (environment, types) = environments();
    for rooted in [false, true] {
        let input = if rooted {
            SourceUnitInput::new("root", "types.ko", parsed.source_id(), &parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), parsed.source_id());
        match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "types.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid unit names");

        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let [plan] = typed.non_null_assertions() else {
            panic!("one assertion must publish exactly one extraction and abort fact");
        };
        let node = parsed
            .ast()
            .expressions()
            .get(plan.expression().expression())
            .unwrap();
        let Expression::NonNullAssert {
            operand,
            operator_span,
        } = node.payload()
        else {
            panic!("descriptor must retain the assertion AST identity");
        };
        assert_eq!(plan.expression().source_unit(), source.id());
        assert_eq!(plan.operand(), UnitExpressionId::new(source.id(), *operand));
        assert_eq!(plan.operator_span(), *operator_span);
        assert_eq!(
            typed.expression_type(UnitExpressionId::new(source.id(), *operand)),
            Some(plan.nullable_type())
        );
        assert_eq!(
            typed.expression_type(plan.expression()),
            Some(plan.inner_type())
        );
        assert_eq!(
            typed.types().get(plan.nullable_type()),
            Some(&UnitTypeKind::Nullable(plan.inner_type()))
        );
        assert_eq!(typed.non_null_assertion(plan.expression()), Some(*plan));
        assert_eq!(
            typed.calls().len(),
            1,
            "only the source make() call is resolved"
        );
        let repeated = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .expect("repeated types");
        assert_eq!(typed.non_null_assertions(), repeated.non_null_assertions());
    }
}

#[test]
fn non_null_assertion_abort_is_independent_of_shadowed_error() {
    use lang_frontend::type_checking::{AssertionFailureEffect, UnitCallTarget};
    let (_, _, entries) = checked_entries(
        "fun error(message: String): Int = 7\nfun read(x: Int?): Int { val ordinary = error(\"local\")\n return x!! }",
    );
    for (source, names, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let [plan] = typed.non_null_assertions() else {
            panic!("one assertion")
        };
        assert_eq!(plan.failure_effect(), AssertionFailureEffect::Abort);
        let [call] = typed.calls() else {
            panic!("only explicit error resolves a callable")
        };
        let UnitCallTarget::Declaration(declaration) = call.target() else {
            panic!("local error must win")
        };
        let declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|candidate| candidate.id() == declaration)
            .expect("source callable");
        assert_eq!(declaration.source_unit(), source);
        assert_eq!(declaration.name(), "error");
        assert_eq!(
            typed.types().get(call.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
}
