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
