//! Immutable, loop-free alias composition across controls and subsequent bindings.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

const BODIES: &[&str] = &[
    "val a=optional; val b=a; val c=b; return (c)!!",
    "return (if(again) (when(third) { true -> optional; else -> null }) else null)!!",
    "return (when(again) { true -> (if(third) optional else null); else -> null })!!",
    "val result=if(again) optional else null; return result!!",
    "val result=when(again) { true -> optional; else -> null }; val next=result; return next!!",
    "val result=if(again) (when(third) { true -> optional; else -> null }) else null; val next=(result); return next!!",
];

#[test]
fn borrowed_alias_compositions_retain_exact_rejection() {
    let mut failures = Vec::new();
    for body in BODIES {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag:Boolean, again:Boolean, third:Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; val optional=if(flag) f else null; {body} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "composition.ko",
                rooted,
                standard_environments(),
            );
            let codes = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>();
            if codes != ["L0137"] {
                failures.push(format!("{body}/{rooted}: {:?}", owned.diagnostics()));
            }
            if codes == ["L0137"] {
                assert!(owned.non_null_assertions().is_empty());
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.validate().is_err());
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn capture_free_and_owned_compositions_remain_legal() {
    for closure in ["{}", "move { read(xs) }"] {
        for body in BODIES {
            let text = format!(
                "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag:Boolean, again:Boolean, third:Boolean): () -> Unit {{ val f: () -> Unit = {closure}; val optional=if(flag) f else null; {body} }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "composition.ko",
                    rooted,
                    standard_environments(),
                );
                assert!(
                    owned.diagnostics().is_empty(),
                    "{closure}/{body}/{rooted}: {:?}",
                    owned.diagnostics()
                );
                assert!(owned.validate().is_ok());
            }
        }
    }
}
