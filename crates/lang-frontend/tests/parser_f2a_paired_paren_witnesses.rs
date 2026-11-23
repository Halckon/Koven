//! SPEC-0489: F2a narrow paired-parenthesis recovery evaluation witnesses.
//!
//! Locks the four mandatory counterexample families from `observation-baseline.md` §3
//! across public entrypoints (`parse_expression`, `parse_file`, `parse_block`) without
//! modifying production Parser or `LexedFile`.

use lang_frontend::lexer::lex;
use lang_frontend::parser::{
    Expression, Item, NameMarker, ParsedBlock, ParsedExpression, ParsedFile, Statement,
    parse_block, parse_expression, parse_file,
};
use lang_frontend::source::{SourceId, SourceMap};

fn parse_expression_twice(sources: &SourceMap, id: SourceId) -> ParsedExpression {
    let lexed = lex(sources, id).unwrap();
    let first = parse_expression(sources, &lexed).unwrap();
    let second = parse_expression(sources, &lexed).unwrap();
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    first
}

fn parse_file_twice(sources: &SourceMap, id: SourceId) -> ParsedFile {
    let lexed = lex(sources, id).unwrap();
    let first = parse_file(sources, &lexed).unwrap();
    let second = parse_file(sources, &lexed).unwrap();
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    first
}

fn parse_block_twice(sources: &SourceMap, id: SourceId) -> ParsedBlock {
    let lexed = lex(sources, id).unwrap();
    let first = parse_block(sources, &lexed).unwrap();
    let second = parse_block(sources, &lexed).unwrap();
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    first
}

#[test]
fn f2a_c1_confirmed_local_paired_group_keeps_inner_comma_and_preserves_next_argument() {
    let text = "f((value, other), next)";
    let mut sources = SourceMap::new();
    let id = sources.add_source("f2a-c1.ko", text).unwrap();
    let parsed = parse_expression_twice(&sources, id);

    let codes: Vec<_> = parsed
        .diagnostics()
        .iter()
        .map(|d| {
            (
                d.code().to_string(),
                sources.slice(d.primary_span()).unwrap(),
            )
        })
        .collect();
    assert_eq!(codes, vec![("L0013".to_owned(), ",")]);

    let ast = parsed.ast();
    let root = ast.expressions().get(parsed.root()).unwrap();
    let Expression::Call { arguments, .. } = root.payload() else {
        panic!("expected call expression, got {:?}", root.payload());
    };
    let [first, second] = arguments.as_slice() else {
        panic!("expected 2 call arguments, got {arguments:?}");
    };
    assert_eq!(sources.slice(first.span), Ok("(value, other)"));
    assert_eq!(sources.slice(second.span), Ok("next"));
    let Expression::Group { expression } = ast.expressions().get(first.value).unwrap().payload()
    else {
        panic!("first argument must remain a parenthesized group");
    };
    assert!(matches!(
        ast.expressions().get(*expression).unwrap().payload(),
        Expression::Error
    ));
}

#[test]
fn f2a_c2_ambiguous_single_right_paren_preserves_caller_comma_and_two_arguments() {
    let text = "f((value, next)";
    let mut sources = SourceMap::new();
    let id = sources.add_source("f2a-c2.ko", text).unwrap();
    let parsed = parse_expression_twice(&sources, id);

    let codes: Vec<_> = parsed
        .diagnostics()
        .iter()
        .map(|d| (d.code().to_string(), d.primary_span().is_empty()))
        .collect();
    assert_eq!(codes, vec![("L0010".to_owned(), true)]);

    let ast = parsed.ast();
    let root = ast.expressions().get(parsed.root()).unwrap();
    let Expression::Call { arguments, .. } = root.payload() else {
        panic!("expected call expression, got {:?}", root.payload());
    };
    let [first, second] = arguments.as_slice() else {
        panic!("caller must retain 2 arguments, got {arguments:?}");
    };
    assert_eq!(sources.slice(first.span), Ok("(value"));
    assert_eq!(sources.slice(second.span), Ok("next"));
}

#[test]
fn f2a_c3_physically_paired_group_preserves_caller_file_declaration_else_and_block_statement() {
    // 3a: file-level declaration starter inside a physically paired group.
    let file_text = "val broken = (value junk\nfun saved(): Int = 1)\nval trailing = 2";
    let mut file_sources = SourceMap::new();
    let file_id = file_sources
        .add_source("f2a-c3-file.ko", file_text)
        .unwrap();
    let file = parse_file_twice(&file_sources, file_id);
    assert!(
        file.roots().iter().any(|item_id| {
            let item = file.ast().items().get(*item_id).unwrap();
            matches!(item.payload(), Item::Function { .. })
                && file_sources.slice(item.span()) == Ok("fun saved(): Int = 1")
        }),
        "fun saved must survive physically paired group: {:?}",
        file.diagnostics()
    );

    // 3b: caller `else` inside a physically paired group (direct and nested in `[...]`).
    for else_text in [
        "if (flag) f((value junk else next), suffix)",
        "if (flag) f((value junk [ else next ], rest), suffix)",
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("f2a-c3-else.ko", else_text).unwrap();
        let parsed = parse_expression_twice(&sources, id);
        assert!(
            parsed.ast().expressions().iter().any(|(_, node)| {
                let Expression::If {
                    else_branch: Some(branch),
                    ..
                } = node.payload()
                else {
                    return false;
                };
                let branch = parsed.ast().statements().get(*branch).unwrap();
                sources.slice(branch.span()) == Ok("next")
            }),
            "caller else branch must survive: {else_text}"
        );
    }

    // 3c: block statement separator inside a physically paired poisoned call group.
    let block_text = "{ f((value β; val saved = 1\n))\nval later = saved }";
    let mut block_sources = SourceMap::new();
    let block_id = block_sources
        .add_source("f2a-c3-block.ko", block_text)
        .unwrap();
    let block = parse_block_twice(&block_sources, block_id);
    let Statement::Block { elements } = block
        .ast()
        .statements()
        .get(block.root())
        .unwrap()
        .payload()
    else {
        panic!("expected block root");
    };
    let binding_names: Vec<_> = elements
        .iter()
        .filter_map(|stmt_id| {
            let Statement::LocalVariable { declaration } =
                block.ast().statements().get(*stmt_id).unwrap().payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(name),
                ..
            } = block.ast().items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            Some(block_sources.slice(*name).unwrap())
        })
        .collect();
    assert_eq!(binding_names, vec!["saved", "later"]);
}

#[test]
fn f2a_c4_multi_group_unwind_with_long_poison_preserves_successor_deterministically() {
    let text = format!(
        "val broken = {}value {}\nfun sentinel(): Int = 1",
        "(".repeat(32),
        "β ".repeat(32),
    );
    let mut sources = SourceMap::new();
    let id = sources.add_source("f2a-c4.ko", &text).unwrap();
    let file = parse_file_twice(&sources, id);

    let sentinel = file
        .ast()
        .items()
        .get(*file.roots().last().unwrap())
        .unwrap();
    assert!(matches!(sentinel.payload(), Item::Function { .. }));
    assert_eq!(
        sources.slice(sentinel.span()),
        Ok("fun sentinel(): Int = 1")
    );
    let poison_diags = file
        .diagnostics()
        .iter()
        .filter(|d| d.code().to_string() == "L0001")
        .count();
    assert_eq!(poison_diags, 32);
}
