//! 原失败候选、嵌套 postfix 与 recovery 消费者，不执行源程序。
use super::{checked_entries, codes};
use lang_frontend::type_checking::{BuiltinType, UnitTypeKind};

#[test]
fn non_null_assertion_failed_trials_roll_back_and_selected_trial_is_unique() {
    for (body, success) in [("it!!", false), ("it!! + 1", true)] {
        let text = format!(
            "fun choose(callback: (Int?) -> Int): Int = 1\nfun choose(callback: (Boolean?) -> Boolean): Boolean = true\nfun use(): Unit {{ val selected = choose {{ {body} }} }}"
        );
        let (_, _, entries) = checked_entries(&text);
        for (_, _, typed) in entries {
            assert_eq!(
                typed.diagnostics().is_empty(),
                success,
                "{:?}",
                typed.diagnostics()
            );
            assert_eq!(typed.non_null_assertions().len(), usize::from(success));
            if success {
                assert_eq!(
                    typed
                        .types()
                        .get(typed.non_null_assertions()[0].inner_type()),
                    Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                );
            }
        }
    }
}

#[test]
fn non_null_assertion_invalid_nested_postfix_keeps_only_valid_inner_fact() {
    let (sources, _, entries) =
        checked_entries("fun read(x: Int?): Int = x!!!!\nfun bad(): Int = true!!");
    for (_, _, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0085", "L0085"]);
        for diagnostic in typed.diagnostics() {
            assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "!!");
        }
        assert_eq!(typed.non_null_assertions().len(), 1);
    }
}

#[test]
fn non_null_assertion_failed_candidates_preserve_preexisting_fact() {
    let (sources, parsed, entries) = checked_entries(
        "fun first(x: Int?): Int = x!!\nfun choose(callback: (Int?) -> Int): Int = 1\nfun choose(callback: (Boolean?) -> Boolean): Boolean = true\nfun use(): Unit { val selected = choose { it!! + true } }",
    );
    for (_, _, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0123"]);
        let [plan] = typed.non_null_assertions() else {
            panic!("failed candidates must preserve only the earlier committed assertion");
        };
        assert_eq!(
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(plan.expression().expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            "x!!"
        );
    }
}

#[test]
fn non_null_assertion_error_and_deferred_operands_do_not_publish_extraction() {
    let (_, _, entries) = checked_entries(
        "fun bad(): Int = (true + false)!!\nfun deferred(x: Int): Int = (x as Int)!!",
    );
    for (_, _, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0085"],
            "operand error must not gain another assertion diagnostic"
        );
        assert!(typed.non_null_assertions().is_empty());
    }
}
