//! Nullable alias diagnostics must not retain closure environments or source loans.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn immutable_nullable_aliases_preserve_borrowed_escape_sources() {
    let mut missing = Vec::new();
    for body in [
        "val optional = if (flag) f else null; return optional!!",
        "val optional = if (flag) null else f; return optional!!",
        "val optional = when (flag) { true -> f; else -> null }; return optional!!",
        "val optional = (if (flag) f else null); val alias = optional; return (alias)!!",
        "val optional = if (flag) { val local: () -> Unit = { read(xs) }; local } else null; return optional!!",
        "val optional = if (flag) { val local: () -> Unit = { read(xs) }; val wrapped: () -> Unit = move { local() }; wrapped } else null; return optional!!",
        "val optional = if (flag) { var local: () -> Unit = { read(xs) }; val wrapped: () -> Unit = move { local() }; local = {}; wrapped } else null; return optional!!",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; {body} }}"
        );
        for rooted in [false, true] {
            let (sources, owned) =
                borrow_rejection_unit::checked(&text, "alias.ko", rooted, standard_environments());
            if !owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0137")
            {
                missing.push(format!("{body}/{rooted}: {:?}", owned.diagnostics()));
            }
            for diagnostic in owned
                .diagnostics()
                .iter()
                .filter(|d| d.code().to_string() == "L0137")
            {
                let expected = body.rsplit("return ").next().unwrap();
                assert_eq!(
                    sources.slice(diagnostic.primary_span()),
                    Ok(expected),
                    "{body}/{rooted}"
                );
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn nullable_alias_metadata_does_not_retain_resources_or_taint_other_bindings() {
    for text in [
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional = if (flag) { return ({}); f } else null; return optional!! }",
        "fun run(flag: Boolean) { while (flag) { var n = 1; val f: () -> Int = { n }; val optional = if (flag) f else null; n = 3; break } }",
        "fun run(flag: Boolean) { var n = 1; val f: () -> Int = { n }; val optional = if (flag) f else null; n = 3 }",
        "fun run(flag: Boolean): () -> Unit { val f: () -> Unit = {}; val optional = if (flag) f else null; val alias = optional; return alias!! }",
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional = if (flag) f else null; if (flag) { val optional = if (flag) ({}) else null; return optional!! }; return ({}) }",
    ] {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "alias.ko", rooted, standard_environments());
            assert!(
                owned.diagnostics().is_empty(),
                "{text}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn existing_nonnullable_constant_branch_keeps_its_conservative_rejection() {
    let text = "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>): () -> Unit { val f: () -> Unit = { read(xs) }; val optional = if (false) f else ({}); return optional }";
    for rooted in [false, true] {
        let (_, owned) =
            borrow_rejection_unit::checked(text, "alias.ko", rooted, standard_environments());
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0137")
        );
    }
}
