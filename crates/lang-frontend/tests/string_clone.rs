//! String.clone 的类型与所有权合同。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        LoanEndPoint, LoanKind, check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        Copyability, ExpressionCategory, ParameterMode, StringOperationKind,
        check_compilation_unit_types, check_types, standard_environments,
    },
};

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn check_both(text: &str, type_codes: &[&str], ownership_codes: &[&str]) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("string-clone.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert_eq!(codes(typed.diagnostics()), type_codes, "single-file types");
    if !type_codes.is_empty() {
        assert!(typed.string_operations().is_empty());
    }
    if type_codes.is_empty() {
        let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
        assert_eq!(
            codes(owned.diagnostics()),
            ownership_codes,
            "single-file ownership"
        );
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        if ownership_codes.is_empty() {
            assert_eq!(
                typed.string_operations().len(),
                owned.string_effects().len()
            );
            for operation in typed.string_operations() {
                assert_eq!(operation.kind(), StringOperationKind::Clone);
                assert_eq!(operation.receiver_mode(), ParameterMode::Borrow);
                assert_eq!(operation.result_mode(), ParameterMode::Value);
                assert_eq!(
                    typed.expression_category(operation.expression()),
                    Some(ExpressionCategory::Temporary)
                );
                assert_eq!(
                    typed.copyability(operation.result_type()),
                    Some(Copyability::MoveOnly)
                );
                let effect = owned
                    .string_effect(operation.expression())
                    .expect("approved clone");
                assert_eq!(effect.receiver(), operation.receiver());
                assert_eq!(effect.result_type(), operation.result_type());
                for loan in owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == operation.expression())
                {
                    if let lang_frontend::ownership_checking::LoanTarget::Place(place) =
                        loan.target()
                    {
                        assert!(!owned.drops().iter().any(|drop| drop.target() == lang_frontend::ownership_checking::DropTarget::Named(place.root()) && drop.point() == lang_frontend::ownership_checking::DropPoint::AfterExpression(operation.receiver())), "receiver owner dropped before clone: {:?}", owned.drops());
                    }
                }
                assert!(
                    owned
                        .loans()
                        .iter()
                        .any(|loan| loan.call() == operation.expression()
                            && loan.argument() == operation.receiver()
                            && loan.kind() == LoanKind::Shared)
                );
                assert!(
                    owned
                        .loan_ends()
                        .iter()
                        .any(|end| end.point() == LoanEndPoint::CallReturn(operation.expression()))
                );
            }
        } else {
            assert!(owned.string_effects().is_empty());
        }
    }
    let inputs = [SourceUnitInput::new(
        "root",
        "string-clone.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let typed =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
    assert_eq!(codes(typed.diagnostics()), type_codes, "unit types");
    if !type_codes.is_empty() {
        assert!(typed.string_operations().is_empty());
    }
    if type_codes.is_empty() {
        let typed = typed.validate().expect("valid unit types");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .expect("unit ownership");
        assert_eq!(
            codes(owned.diagnostics()),
            ownership_codes,
            "unit ownership"
        );
        if ownership_codes.is_empty() {
            assert_eq!(
                typed.types().string_operations().len(),
                owned.string_effects().len()
            );
            for operation in typed.types().string_operations() {
                let effect = owned
                    .string_effect(operation.expression())
                    .expect("approved unit clone");
                assert_eq!(effect.receiver(), operation.receiver());
                assert_eq!(effect.result_type(), operation.result_type());
                for loan in owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == operation.expression())
                {
                    if let lang_frontend::ownership_checking::UnitLoanTarget::Place(place) =
                        loan.target()
                    {
                        assert!(!owned.drops().iter().any(|drop| drop.target() == lang_frontend::ownership_checking::UnitDropTarget::Named(place.root()) && drop.point() == lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(operation.receiver())), "receiver owner dropped before clone: {:?}", owned.drops());
                    }
                }
                assert!(
                    owned
                        .loans()
                        .iter()
                        .any(|loan| loan.call() == operation.expression()
                            && loan.argument() == operation.receiver()
                            && loan.kind() == LoanKind::Shared)
                );
            }
            assert!(owned.validate().is_ok());
        } else {
            assert!(owned.string_effects().is_empty());
        }
    }
}

#[test]
fn clone_preserves_borrowed_and_owned_sources() {
    check_both(
        "fun copy(text: String): String = text.clone()\nfun main(): Unit { val source = \"héllo\"; val copy = source.clone(); println(source); println(copy) }",
        &[],
        &[],
    );
}

#[test]
fn clone_borrows_elements_and_temporary_owners() {
    check_both(
        "fun copy(values: List<String>): String = values[0].clone()\nfun main(): Unit { val copy = (\"hello\" + \" world\").clone(); println(copy); val nested = listOf(\"a\")[0].clone(); println(nested) }",
        &[],
        &[],
    );
}

#[test]
fn clone_rejects_arguments_and_type_arguments() {
    check_both(
        "fun bad(text: String): String = text.clone(1)\nfun generic(text: String): String = text.clone<Int>()",
        &["L0121", "L0091"],
        &[],
    );
}

#[test]
fn clone_does_not_make_string_copyable() {
    check_both(
        "fun main(): Unit { val source = \"hello\"; val moved = source; val copy = source.clone(); println(moved) }",
        &[],
        &["L0131"],
    );
}

#[test]
fn clone_result_moves_independently_and_rc_payload_is_borrowed() {
    check_both(
        "fun main(): Unit { val source = \"a\"; val copy = source.clone(); val moved = copy; println(source); println(moved); val shared = Rc(\"payload\"); val detached = shared.value.clone(); println(shared.value); println(detached) }",
        &[],
        &[],
    );
}

#[test]
fn clone_from_inout_and_generic_result_inference() {
    check_both(
        "fun detached(inout text: String): String = text.clone()\nfun <T> identity(own value: T): T = value\nfun main(): Unit { val source = \"a\"; val copy = identity(source.clone()); println(source); println(copy) }",
        &[],
        &[],
    );
}

#[test]
fn clone_rejects_non_string_intrinsics_and_nullable_receiver() {
    check_both(
        "value class Atom(val value: Int)\nfun a(value: Int): Unit { value.clone() }\nfun b(value: List<String>): Unit { value.clone() }\nfun c(value: Array<String>): Unit { value.clone() }\nfun d(value: Rc<String>): Unit { value.clone() }\nfun e(value: Box<Atom>): Unit { value.clone() }\nfun f(value: String?): Unit { value.clone() }",
        &["L0080", "L0080", "L0080", "L0080", "L0080", "L0080"],
        &[],
    );
}

#[test]
fn source_clone_member_is_an_ordinary_member() {
    check_both(
        "class Sample { fun clone(): Int = 1 }\nfun main(): Unit { val sample = Sample(); val result = sample.clone() }",
        &[],
        &[],
    );
}

#[test]
fn clone_overload_trials_keep_only_selected_facts() {
    check_both(
        "fun choose(callback: (Int) -> Int): Int = 1\nfun choose(callback: (String) -> String): String = \"text\"\nfun main(): Unit { val selected = choose({ value -> value.clone() }); println(selected) }",
        &[],
        &[],
    );
}

#[test]
fn clone_holds_field_and_rc_owner_until_return() {
    check_both(
        "class Label(val text: String)\nfun fromField(): String { val label = Label(\"name\"); return label.text.clone() }\nfun fromRc(): String { val shared = Rc(\"name\"); return (shared.value).clone() }",
        &[],
        &[],
    );
}

#[test]
fn failed_clone_overload_trials_publish_no_intrinsic_fact() {
    check_both(
        "fun choose(callback: (Int) -> String): Int = 1\nfun choose(callback: (String) -> String): Int = 2\nfun main(): Unit { val source = \"text\"; val ambiguous = choose({ ignored -> source.clone() }) }",
        &["L0124"],
        &[],
    );
}

#[test]
fn safe_clone_preserves_the_existing_nullable_deferred_boundary() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("safe.ko", "fun nullable(text: String?): String? = text?.clone()\nfun plain(text: String): String? = text?.clone()").expect("source");
    let lexed = lex(&sources, source).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty());
    assert!(typed.string_operations().is_empty());
    let inputs = [SourceUnitInput::new("root", "safe.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("names")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty());
    assert!(typed.string_operations().is_empty());
}

#[test]
fn clone_cannot_read_through_an_active_exclusive_loan() {
    check_both(
        "fun hold(inout text: String, copy: String): Unit {}\nfun main(): Unit { var text = \"source\"; hold(&text, text.clone()) }",
        &[],
        &["L0135"],
    );
}

#[test]
fn unit_clone_last_use_receiver_drops_once_at_call_return() {
    use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget, UnitLoanTarget};
    let mut sources = SourceMap::new();
    let source = sources.add_source("last-use.ko", "class Label(val text: String)\nfun fromField(): String { val label = Label(\"name\"); return label.text.clone() }\nfun fromRc(): String { val shared = Rc(\"name\"); return (shared.value).clone() }").expect("source");
    let lexed = lex(&sources, source).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::new("root", "last-use.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("names")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .expect("types")
        .validate()
        .expect("valid types");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
        .expect("ownership");
    assert!(owned.diagnostics().is_empty());
    assert_eq!(owned.string_effects().len(), 2);
    for operation in typed.types().string_operations() {
        let loan = owned
            .loans()
            .iter()
            .find(|loan| loan.call() == operation.expression())
            .expect("receiver loan");
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("expected owner place")
        };
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(drops, [UnitDropPoint::CallReturn(operation.expression())]);
    }
}
