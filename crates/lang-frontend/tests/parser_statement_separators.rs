//! Guide 06/07: all block owners require real separators and preserve recovery identity.

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{Statement, SyntaxAst, parse_block, parse_expression, parse_file},
    source::SourceMap,
};

#[derive(Clone, Copy, Debug)]
enum Owner {
    Block,
    Control,
    Lambda,
    File,
}
const OWNERS: [Owner; 4] = [Owner::Block, Owner::Control, Owner::Lambda, Owner::File];

#[derive(Debug)]
struct Observation {
    diagnostics: Vec<(String, usize, usize)>,
    elements: Vec<String>,
    source: String,
}

fn observe(owner: Owner, body: &str) -> Observation {
    let text = match owner {
        Owner::Block => format!("{{ {body} }}"),
        Owner::Control => format!("if (flag) {{ {body} }} else {{}}"),
        Owner::Lambda => format!("({{ -> {body} }})"),
        Owner::File => format!("fun main(): Unit {{ {body} }}"),
    };
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", &text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    let summarize = |ast: &SyntaxAst, diagnostics: &[Diagnostic]| {
        let elements = ast
            .statements()
            .iter()
            .find_map(|(_, node)| {
                if node.span().start() != text.find('{').unwrap() {
                    return None;
                }
                match node.payload() {
                    Statement::Block { elements }
                    | Statement::ControlBody { elements }
                    | Statement::LambdaBody { elements } => Some(elements),
                    _ => None,
                }
            })
            .unwrap();
        Observation {
            diagnostics: diagnostics
                .iter()
                .map(|d| {
                    (
                        d.code().to_string(),
                        d.primary_span().start(),
                        d.primary_span().end(),
                    )
                })
                .collect(),
            elements: elements
                .iter()
                .map(|id| {
                    sources
                        .slice(ast.statements().get(*id).unwrap().span())
                        .unwrap()
                        .to_owned()
                })
                .collect(),
            source: text.clone(),
        }
    };
    match owner {
        Owner::Block => {
            let p = parse_block(&sources, &lexed).unwrap();
            summarize(p.ast(), p.diagnostics())
        }
        Owner::Control | Owner::Lambda => {
            let p = parse_expression(&sources, &lexed).unwrap();
            summarize(p.ast(), p.diagnostics())
        }
        Owner::File => {
            let p = parse_file(&sources, &lexed).unwrap();
            summarize(p.ast(), p.diagnostics())
        }
    }
}

#[test]
fn missing_separator_is_an_error_and_keeps_both_real_elements() {
    for owner in OWNERS {
        for (left, right) in [
            ("x", "y"),
            ("val x = 1", "x"),
            ("val x = 1", "val y = 2"),
            ("{}", "loop { break }"),
            ("{}", "while (true) {}"),
            ("{}", "for (x in xs) {}"),
        ] {
            let result = observe(owner, &format!("{left} {right}"));
            let offset = result.source.rfind(right).unwrap();
            assert_eq!(
                result.diagnostics,
                [("L0169".into(), offset, offset)],
                "{owner:?}: {result:?}"
            );
            assert_eq!(result.elements, [left, right], "{owner:?}: {result:?}");
        }
    }
}

#[test]
fn real_separators_and_multiline_comments_are_accepted_by_every_owner() {
    for owner in OWNERS {
        for separator in ["; ", "\n", "\r\n", " /*\n*/ "] {
            let result = observe(owner, &format!("val x = 1{separator}x"));
            assert!(result.diagnostics.is_empty(), "{owner:?}: {result:?}");
            assert_eq!(result.elements, ["val x = 1", "x"]);
        }
        let result = observe(owner, "val x = 1 /* no newline */ val y = 2");
        assert_eq!(result.diagnostics.len(), 1, "{owner:?}: {result:?}");
        assert_eq!(result.diagnostics[0].0, "L0169");
    }
}

#[test]
fn ambiguous_newline_starts_a_new_element_including_lambda_bodies() {
    for owner in OWNERS {
        for right in ["(y)", "+ y", "- y"] {
            let result = observe(owner, &format!("x\n{right}"));
            assert!(result.diagnostics.is_empty(), "{owner:?}: {result:?}");
            assert_eq!(result.elements, ["x", right]);
        }
    }
}

#[test]
fn incomplete_operands_and_enclosed_expressions_keep_their_continuation() {
    for owner in OWNERS {
        for body in [
            "x +\ny",
            "x\n.member",
            "f(x\n+ y)",
            "(x\n+ y)",
            "xs[x\n+ y]",
        ] {
            let result = observe(owner, body);
            assert!(result.diagnostics.is_empty(), "{owner:?}: {result:?}");
            assert_eq!(result.elements, [body]);
        }
        for body in ["val x = 1 + val y = 2", "val x = #; y", "x async; y"] {
            let result = observe(owner, body);
            assert!(!result.diagnostics.is_empty());
            assert!(
                result.diagnostics.iter().all(|d| d.0 != "L0169"),
                "{owner:?}: {result:?}"
            );
        }
    }
}

#[test]
fn expression_entry_retains_its_existing_trailing_error_contract() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", "x y").unwrap();
    let parsed = parse_expression(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert_eq!(parsed.diagnostics().len(), 1);
    assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0013");
}
