//! Ordinary group recovery and its enclosing syntax owners.

use super::*;
use lang_frontend::parser::NameMarker;

#[test]
fn a_group_recovery_leaves_the_block_declaration_after_a_bad_index() {
    let text = "{ val broken = (value junk [key\nval saved = 1\n], rest)\nval later = saved }";
    let (sources, block) = parsed(text);
    let ast = block.ast();
    let has_binding = |expected| {
        block_elements(ast, block.root()).iter().any(|statement| {
            let statement = ast.statements().get(*statement).unwrap();
            let Statement::LocalVariable { declaration } = statement.payload() else {
                return false;
            };
            let declaration = ast.items().get(*declaration).unwrap();
            matches!(declaration.payload(), Item::Variable { name: NameMarker::Present(span), .. }
                if sources.slice(*span) == Ok(expected))
        })
    };
    assert!(
        has_binding("saved") && has_binding("later"),
        "{text}: {:?}",
        block.diagnostics()
    );
}

#[test]
fn a_group_recovery_leaves_the_next_block_after_a_bad_index() {
    let text = "{ val broken = (value junk [key\n{ sentinel() }\n], rest)\nval later = 1 }";
    let (sources, block) = parsed(text);
    let ast = block.ast();
    assert!(
        block_elements(ast, block.root()).iter().any(|statement| {
            let statement = ast.statements().get(*statement).unwrap();
            matches!(statement.payload(), Statement::Block { .. })
                && sources.slice(statement.span()) == Ok("{ sentinel() }")
        }),
        "{:?}",
        block.diagnostics()
    );
}

#[test]
fn paired_malformed_group_keeps_the_following_call_argument() {
    for (group, unexpected) in [
        ("(value, other)", ","),
        ("(value other)", "other"),
        ("(value, (if (flag) a else b))", ","),
    ] {
        let text = format!("f({group}, next)");
        let mut sources = SourceMap::new();
        let source = sources.add_source("malformed-group.ko", &text).unwrap();
        let parsed = parse_expression_twice(&sources, source, &text);
        assert_group_diagnostics(&sources, parsed.diagnostics(), &[("L0013", unexpected)]);
        let root = parsed.ast().expressions().get(parsed.root()).unwrap();
        assert_eq!(sources.slice(root.span()), Ok(text.as_str()));
        assert_error_group_call(&sources, parsed.ast(), parsed.root(), group);
    }
}

#[test]
fn a_group_recovery_does_not_jump_past_a_file_declaration_or_else() {
    let mut sources = SourceMap::new();
    let text = "val broken = (value junk\nfun saved(): Int = 1)\nval trailing = 2";
    let source = sources.add_source("group-file-boundary.ko", text).unwrap();
    let file = parse_file_twice(&sources, source, text);
    assert!(file.roots().iter().any(|item| {
        let item = file.ast().items().get(*item).unwrap();
        matches!(item.payload(), Item::Function { .. })
            && sources.slice(item.span()) == Ok("fun saved(): Int = 1")
    }));

    for text in [
        "if (flag) f((value junk else next), suffix)",
        "if (flag) f((value junk [ else next), suffix)",
        "if (flag) f((value junk [ else next ], rest), suffix)",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("group-else-boundary.ko", text).unwrap();
        let parsed = parse_expression_twice(&sources, source, text);
        assert!(parsed.ast().expressions().iter().any(|(_, node)| {
            let Expression::If {
                else_branch: Some(branch),
                ..
            } = node.payload()
            else {
                return false;
            };
            let branch = parsed.ast().statements().get(*branch).unwrap();
            sources.slice(branch.span()) == Ok("next")
        }));
    }
}

#[test]
fn ambiguous_group_closer_leaves_the_caller_separator() {
    let text = "f((value, next)";
    let mut sources = SourceMap::new();
    let source = sources.add_source("unclosed-group.ko", text).unwrap();
    let parsed = parse_expression_twice(&sources, source, text);
    assert_group_diagnostics(&sources, parsed.diagnostics(), &[("L0010", "")]);
    let root = parsed.ast().expressions().get(parsed.root()).unwrap();
    let Expression::Call { arguments, .. } = root.payload() else {
        panic!("expected outer call, got {:?}", root.payload());
    };
    let [first, next] = arguments.as_slice() else {
        panic!("expected two arguments, got {arguments:?}");
    };
    assert_eq!(sources.slice(first.span), Ok("(value"));
    assert_eq!(sources.slice(next.span), Ok("next"));
}

fn assert_call_recovery_keeps_saved_and_later(sources: &SourceMap, block: &ParsedBlock) {
    let ast = block.ast();
    let names = block_elements(ast, block.root())
        .iter()
        .filter_map(|id| {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*id).unwrap().payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(name),
                ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            Some(sources.slice(*name).unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(names, ["saved", "later"]);
}

#[test]
fn a_malformed_call_group_leaves_a_declaration_after_a_paired_nested_index() {
    let head = "f((value, junk[key";
    let text = format!("{{ {head}; val saved = 1\n]))\nval later = saved }}");
    let (sources, block) = parsed(&text);
    let ast = block.ast();
    let first = ast
        .statements()
        .get(block_elements(ast, block.root())[0])
        .unwrap();
    assert_eq!(sources.slice(first.span()), Ok(head));
    assert_call_recovery_keeps_saved_and_later(&sources, &block);
}

#[test]
fn a_poisoned_call_group_leaves_the_proven_block_separator() {
    for key in ["β", "value β", "β +"] {
        let head = format!("f(({key}");
        let text = format!("{{ {head}; val saved = 1\n))\nval later = saved }}");
        let (sources, block) = parsed(&text);
        let ast = block.ast();
        let elements: Vec<_> = block_elements(ast, block.root())
            .iter()
            .map(|id| {
                sources
                    .slice(ast.statements().get(*id).unwrap().span())
                    .unwrap()
            })
            .collect();
        assert_eq!(elements.first().copied(), Some(head.as_str()), "{text}");
        assert_call_recovery_keeps_saved_and_later(&sources, &block);
        let poison = block
            .diagnostics()
            .iter()
            .filter(|diagnostic| sources.slice(diagnostic.primary_span()) == Ok("β"))
            .collect::<Vec<_>>();
        let [poison] = poison.as_slice() else {
            panic!(
                "expected one lexical root: {text}: {:?}",
                block.diagnostics()
            );
        };
        assert_eq!(poison.code().to_string(), "L0001");
        assert!(block.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().to_string() == "L0010"
                && diagnostic.primary_span().is_empty()
                && diagnostic.primary_span().start() == text.find(';').unwrap()
        }));
    }
}

#[test]
fn a_call_group_with_cross_line_poison_keeps_the_block_declarations() {
    let text = "{ f((value\nβ; val saved = 1\nval later = saved }";
    let (sources, block) = parsed(text);
    assert_group_diagnostics(
        &sources,
        block.diagnostics(),
        &[("L0001", "β"), ("L0010", "")],
    );
    assert!(block.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().to_string() == "L0010"
            && diagnostic.primary_span().start() == text.find(';').unwrap()
    }));
    assert_call_recovery_keeps_saved_and_later(&sources, &block);
}

#[test]
fn a_call_group_keeps_local_semicolons_and_the_next_argument() {
    for (group, expected) in [
        ("(value β; 2)", ("L0001", "β")),
        (
            "(value β { -> val first = 1; val second = 2 })",
            ("L0001", "β"),
        ),
        (
            "(value, nested({ -> val first = 1; val second = 2 }))",
            ("L0013", ","),
        ),
    ] {
        let text = format!("{{ f({group}, next); tail }}");
        let (sources, block) = parsed(&text);
        let ast = block.ast();
        assert_group_diagnostics(&sources, block.diagnostics(), &[expected]);
        let [call, tail] = block_elements(ast, block.root()) else {
            panic!("expected call and tail: {text}");
        };
        let Statement::Expression { expression } = ast.statements().get(*call).unwrap().payload()
        else {
            panic!("expected call expression: {text}");
        };
        assert_error_group_call(&sources, ast, *expression, group);
        assert_eq!(
            sources.slice(ast.statements().get(*tail).unwrap().span()),
            Ok("tail")
        );
    }
}
