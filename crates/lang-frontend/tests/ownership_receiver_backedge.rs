//! A non-rebindable MoveOnly receiver cannot be consumed on a repeating loop edge.

use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn check(body: &str, copyable: bool, rejected: bool) {
    let classifier = if copyable {
        "value class Resource(val code: Int)"
    } else {
        "class Resource"
    };
    let text = format!(
        "{classifier} {{\n\
         own fun consume(): Unit {{}}\n\
         own fun again(): Boolean = true\n\
         own fun bounds(): IntRange = 0..<2\n\
         own fun run(flag: Boolean): Unit {{ {body} }}\n\
         }}"
    );
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", &text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .unwrap()
        .validate()
        .unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    let codes: Vec<_> = owned
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    if rejected {
        assert_eq!(
            codes,
            ["L0131"],
            "a repeating edge reuses moved this: {text}"
        );
        let span = owned.diagnostics()[0].primary_span();
        assert_eq!(span.source_id(), source);
        assert!(
            sources.slice(span).unwrap().contains("consume")
                || sources.slice(span).unwrap().contains("this")
                || sources.slice(span).unwrap().contains("again")
                || sources.slice(span).unwrap().contains("bounds")
        );
    } else {
        assert!(
            codes.is_empty(),
            "no receiver reuse on this edge: {text}: {codes:?}"
        );
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    }
}

#[test]
fn receiver_moves_are_rejected_on_normal_and_continue_backedges() {
    for movement in ["consume()", "this.consume()", "val moved = this"] {
        for loop_body in [movement.to_owned(), format!("{movement}; continue")] {
            for statement in [
                format!("while (flag) {{ {loop_body} }}"),
                format!("loop {{ {loop_body} }}"),
                format!("for (i in 0..<2) {{ {loop_body} }}"),
            ] {
                check(&statement, false, true);
            }
        }
    }
}

#[test]
fn while_condition_repeats_but_for_source_is_evaluated_once() {
    check("while (again()) {}", false, true);
    check("for (i in bounds()) {}", false, false);
}

#[test]
fn breaks_returns_prior_moves_and_copyable_receivers_are_not_backedge_errors() {
    for body in [
        "while (flag) { consume(); break }",
        "loop { consume(); return }",
        "val moved = this; while (flag) {}",
        "while (flag) { if (flag) { consume(); return } }",
    ] {
        check(body, false, false);
    }
    check("while (flag) { consume() }", true, false);
}

#[test]
fn inner_break_does_not_hide_an_outer_repeating_receiver_move() {
    check("while (flag) { loop { consume(); break } }", false, true);
    check("while (flag) { for (i in bounds()) {} }", false, true);
    check(
        "while (flag) { if (flag) { consume(); continue } else {} }",
        false,
        true,
    );
}
