//! Cross-check local mutable evidence with escaping Elvis and field delivery.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

fn check(field: bool) {
    for (update, rejected) in [
        ("", true),
        ("optional=null;", false),
        ("optional=if(flag) ({}) else null;", false),
        ("if(again) { optional=make() };", true),
        ("optional=null; optional=if(flag) f2 else null;", true),
    ] {
        let (result, delivery) = if field {
            ("Unit", "holder.callback=((optional ?: ({})))")
        } else {
            ("() -> Unit", "return ((optional ?: ({})))")
        };
        let text = format!(
            "class Holder(var callback: () -> Unit)\nfun read(xs: List<Int>) {{}}\nfun make(): () -> Unit {{ return ({{}}) }}\nfun run(own xs: List<Int>, flag:Boolean, again:Boolean): {result} {{ val holder=Holder({{}}); val f: () -> Unit = {{read(xs)}}; val f2: () -> Unit = {{read(xs)}}; var optional=if(flag) f else null; {update} {delivery} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "combined.ko",
                rooted,
                standard_environments(),
            );
            let codes = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(
                codes,
                if rejected { vec!["L0137"] } else { vec![] },
                "{field}/{update}/{rooted}: {:?}",
                owned.diagnostics()
            );
            if rejected {
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.field_replacements().is_empty());
            }
            assert_eq!(owned.validate().is_ok(), !rejected);
        }
    }
}
#[test]
fn mutable_left_delivery_follows_committed_values() {
    check(false);
}
#[test]
fn mutable_left_field_storage_follows_committed_values() {
    check(true);
}
