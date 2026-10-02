//! 整数 inv 的类型、静态身份与所有权合同。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn check_both(text: &str, expected: &[&str], ownership: &[&str]) {
    check_both_with_count(text, expected, ownership, None);
}

fn check_both_with_count(
    text: &str,
    expected: &[&str],
    ownership: &[&str],
    fact_count: Option<usize>,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("inv.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>(),
        expected,
        "single: {text}"
    );
    if !expected.is_empty() {
        assert!(typed.integer_operations().is_empty());
    }
    if let Some(count) = fact_count {
        assert_eq!(typed.integer_operations().len(), count);
    }
    assert!(
        typed
            .integer_operations()
            .windows(2)
            .all(|pair| pair[0].expression().index() < pair[1].expression().index())
    );
    for operation in typed.integer_operations() {
        assert_eq!(
            operation.kind(),
            lang_frontend::type_checking::IntegerOperationKind::Invert
        );
        assert_eq!(
            typed.expression_type(operation.expression()),
            Some(operation.result_type())
        );
        assert_eq!(
            typed.expression_type(operation.receiver()),
            Some(operation.receiver_type())
        );
        assert_eq!(
            typed.copyability(operation.result_type()),
            Some(lang_frontend::type_checking::Copyability::Copyable)
        );
    }
    if expected.is_empty() {
        let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ownership,
            "single ownership: {text}"
        );
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    }
    let inputs = [SourceUnitInput::new("root", "inv.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>(),
        expected,
        "unit: {text}"
    );
    if !expected.is_empty() {
        assert!(typed.integer_operations().is_empty());
    }
    if let Some(count) = fact_count {
        assert_eq!(typed.integer_operations().len(), count);
    }
    assert!(
        typed
            .integer_operations()
            .windows(2)
            .all(|pair| pair[0].expression() < pair[1].expression())
    );
    for operation in typed.integer_operations() {
        assert_eq!(
            operation.kind(),
            lang_frontend::type_checking::IntegerOperationKind::Invert
        );
        assert_eq!(
            typed.expression_type(operation.expression()),
            Some(operation.result_type())
        );
        assert_eq!(
            typed.expression_type(operation.receiver()),
            Some(operation.receiver_type())
        );
    }
    if expected.is_empty() {
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ownership,
            "unit ownership: {text}"
        );
        if ownership.is_empty() {
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn inv_preserves_all_eight_integer_types() {
    for ty in [
        "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
    ] {
        check_both(
            &format!("fun invert(value: {ty}): {ty} = value.inv()"),
            &[],
            &[],
        );
    }
}
#[test]
fn inv_rejects_arguments_type_arguments_and_non_integer_receivers() {
    check_both(
        "fun a(value: Int): Int = value.inv(1)\nfun b(value: Int): Int = value.inv<Int>()",
        &["L0121", "L0091"],
        &[],
    );
    for ty in [
        "Boolean",
        "Char",
        "String",
        "Float",
        "Double",
        "Int?",
        "List<Int>",
    ] {
        check_both(
            &format!("fun invalid(value: {ty}): Unit {{ value.inv() }}"),
            &["L0080"],
            &[],
        );
    }
}
#[test]
fn ordinary_inv_member_is_not_the_integer_intrinsic() {
    check_both_with_count(
        "class Sample { fun inv(): Int = 7 }\nfun main(): Unit { val result = Sample().inv() }",
        &[],
        &[],
        Some(0),
    );
}
#[test]
fn inv_remains_outside_the_const_call_whitelist() {
    check_both("const val MASK: Int = 1.inv()", &["L0156"], &[]);
}
#[test]
fn inv_reads_fields_elements_and_borrowed_parameters() {
    check_both(
        "class Number(val number: Int)\nfun field(): Int { val value = Number(1); return value.number.inv() }\nfun element(values: List<Int>): Int = values[0].inv()\nfun invert(value: Int): Int = value.inv()",
        &[],
        &[],
    );
}
#[test]
fn inv_cannot_read_through_an_active_exclusive_loan() {
    check_both(
        "fun hold(inout value: Int, inverted: Int): Unit {}\nfun main(): Unit { var value: Int = 1; hold(&value, value.inv()) }",
        &[],
        &["L0135"],
    );
}
#[test]
fn inv_literal_chains_and_overload_trials_keep_only_selected_facts() {
    check_both_with_count(
        "fun choose(callback: (Int) -> Int): Int = 1\nfun choose(callback: (String) -> String): String = \"text\"\nfun main(): Unit { val value: Int = 1.inv().inv(); val chosen = choose({ value -> value.inv() }) }",
        &[],
        &[],
        Some(3),
    );
}

#[test]
fn failed_inv_trial_publishes_no_intrinsic_fact() {
    check_both_with_count(
        "fun choose(callback: (Int) -> Int): Int = 1\nfun choose(callback: (String) -> Int): Int = 2\nfun main(): Unit { val number: Int = 1; val result = choose({ unused -> number.inv() }) }",
        &["L0124"],
        &[],
        Some(0),
    );
}

#[test]
fn bitwise_binary_rejects_nonintegers_and_typed_width_mismatches() {
    for operator in ["and", "or", "xor", "shl", "shr", "ushr"] {
        for ty in ["Boolean", "Char", "Float", "Double", "String", "Int?"] {
            check_both(
                &format!("fun invalid(left: {ty}, right: {ty}): Unit {{ left {operator} right }}"),
                &["L0085"],
                &[],
            );
        }
        check_both(
            &format!("fun invalid(left: Byte, right: Int): Unit {{ left {operator} right }}"),
            &["L0085"],
            &[],
        );
        check_both(
            &format!("fun invalid(left: UInt, right: Int): Unit {{ left {operator} right }}"),
            &["L0085"],
            &[],
        );
    }
}

#[test]
fn inv_unit_facts_keep_source_identity_when_local_ids_collide() {
    let mut sources = SourceMap::new();
    let a = sources
        .add_source("a.ko", "fun first(value: Byte): Byte = value.inv()")
        .unwrap();
    let b = sources
        .add_source("b.ko", "fun second(value: ULong): ULong = value.inv()")
        .unwrap();
    let a_file = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let b_file = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
    let mut snapshot = None;
    for reversed in [false, true] {
        let mut inputs = vec![
            SourceUnitInput::new("root", "a.ko", a, &a_file),
            SourceUnitInput::new("root", "b.ko", b, &b_file),
        ];
        if reversed {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let operations = typed.integer_operations();
        assert_eq!(operations.len(), 2);
        assert_eq!(
            operations[0].expression().expression(),
            operations[1].expression().expression()
        );
        assert_ne!(
            operations[0].expression().source_unit(),
            operations[1].expression().source_unit()
        );
        let actual = operations
            .iter()
            .map(|op| {
                (
                    op.expression(),
                    op.receiver(),
                    format!("{:?}", typed.types().get(op.receiver_type())),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual[0].2, "Some(Builtin(Byte))");
        assert_eq!(actual[1].2, "Some(Builtin(ULong))");
        if let Some(previous) = &snapshot {
            assert_eq!(&actual, previous);
        }
        snapshot = Some(actual);
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.validate().is_ok());
    }
}
