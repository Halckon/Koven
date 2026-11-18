//! Element assignment evaluates its index before consuming the RHS.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn index_move_is_visible_to_assignment_rhs() {
    let mut missing = Vec::new();
    for container in ["Array", "MutableList"] {
        let text = format!(
            "fun take(own value: String): Int = 0\nfun run(inout slots: {container}<String>, own value: String) {{ slots[take(value)] = value }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "assignment.ko",
                rooted,
                standard_environments(),
            );
            if !owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0131")
            {
                missing.push(format!(
                    "{container}/{rooted}: diagnostics={:?}, validate={:?}",
                    owned.diagnostics(),
                    owned.clone().validate().map(|_| ())
                ));
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn index_is_consumed_once_and_rhs_can_mutate_the_container() {
    for text in [
        "fun take(own value: String): Int = 0\nfun run(inout slots: Array<String>, own value: String) { slots[take(value)] = \"new\" }",
        "fun update(inout slots: Array<String>): String { slots[1] = \"inner\"; return \"outer\" }\nfun run(inout slots: Array<String>) { slots[0] = update(&slots) }",
    ] {
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                text,
                "assignment.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn index_exit_does_not_consume_unreachable_rhs() {
    for exit in ["return", "break", "continue"] {
        let text = format!(
            "fun take(own value: String): Int = 0\nfun run(inout slots: Array<String>, own value: String) {{ val consumed = take(value)\nloop {{ slots[{exit}] = value }} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "assignment.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{exit}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn temporary_receiver_prefix_is_evaluated_once_before_index() {
    for container in ["Array", "MutableList"] {
        let accepted = format!(
            "fun make(own first: String): {container}<String>\nfun take(own second: String): Int = 0\nfun run(own first: String, own second: String) {{ make(first)[take(second)] = \"new\" }}"
        );
        let rejected = format!(
            "fun make(own value: String): {container}<String>\nfun take(own value: String): Int = 0\nfun run(own value: String) {{ make(value)[take(value)] = \"new\" }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &accepted,
                "assignment.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{container}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
            let (_, owned) = borrow_rejection_unit::checked(
                &rejected,
                "assignment.ko",
                rooted,
                standard_environments(),
            );
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0131"]
            );
            assert!(owned.validate().is_err());
        }
    }
}

#[test]
fn failed_index_preserves_the_original_move_diagnostic() {
    let text = "fun take(own value: String): Int = 0\nfun run(inout slots: Array<String>, own value: String) { val first = take(value)\nslots[take(value)] = \"new\" }";
    for rooted in [false, true] {
        let (_, owned) =
            borrow_rejection_unit::checked(text, "assignment.ko", rooted, standard_environments());
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0131"]
        );
        assert!(owned.validate().is_err());
    }
}
