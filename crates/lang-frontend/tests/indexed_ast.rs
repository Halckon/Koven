//! SPEC-0004 的 indexed AST 类型、所有权与边界契约测试。

use lang_frontend::{
    ast::{AstError, AstFile, ExpressionId, ItemId, NodeCategory, StatementId, TypeRefId},
    source::{SourceId, SourceMap, Span},
};

#[derive(Debug, PartialEq, Eq)]
enum TestExpression {
    Name(&'static str),
    Pair {
        left: ExpressionId,
        right: ExpressionId,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct TestStatement {
    expression: ExpressionId,
}

#[derive(Debug, PartialEq, Eq)]
struct TestItem {
    body: StatementId,
    return_type: TypeRefId,
}

#[derive(Debug, PartialEq, Eq)]
struct TestTypeRef(&'static str);

type TestAst = AstFile<TestItem, TestStatement, TestExpression, TestTypeRef>;

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources
        .add_source(name, text)
        .expect("test source names must be unique")
}

fn span(sources: &SourceMap, source_id: SourceId, start: usize, end: usize) -> Span {
    sources
        .span(source_id, start, end)
        .expect("test spans must be valid")
}

#[test]
fn private_payloads_form_stable_parent_child_relationships() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "tree.ko", "left right Pair");
    let left_span = span(&sources, source_id, 0, 4);
    let right_span = span(&sources, source_id, 5, 10);
    let pair_span = span(&sources, source_id, 0, 15);
    let mut file = TestAst::new(source_id);

    let type_ref = file
        .add_type_ref(pair_span, TestTypeRef("Pair"))
        .expect("the type-reference span belongs to the file");
    let left = file
        .add_expression(left_span, TestExpression::Name("left"))
        .expect("the left expression belongs to the file");
    let right = file
        .add_expression(right_span, TestExpression::Name("right"))
        .expect("the right expression belongs to the file");
    let pair = file
        .add_expression(pair_span, TestExpression::Pair { left, right })
        .expect("the pair expression belongs to the file");
    let statement = file
        .add_statement(pair_span, TestStatement { expression: pair })
        .expect("the statement belongs to the file");
    let item = file
        .add_item(
            pair_span,
            TestItem {
                body: statement,
                return_type: type_ref,
            },
        )
        .expect("the item belongs to the file");

    assert_eq!(file.source_id(), source_id);
    assert_eq!(
        file.items().get(item).map(|node| node.span()),
        Ok(pair_span)
    );
    assert_eq!(
        file.items().get(item).map(|node| node.payload()),
        Ok(&TestItem {
            body: statement,
            return_type: type_ref,
        })
    );
    assert_eq!(
        file.statements()
            .get(statement)
            .map(|node| node.payload().expression),
        Ok(pair)
    );
    assert_eq!(
        file.statements().get(statement).map(|node| node.span()),
        Ok(pair_span)
    );
    assert_eq!(
        file.expressions().get(pair).map(|node| node.payload()),
        Ok(&TestExpression::Pair { left, right })
    );
    assert_eq!(
        file.expressions().get(left).map(|node| node.span()),
        Ok(left_span)
    );
    assert_eq!(
        file.expressions().get(right).map(|node| node.span()),
        Ok(right_span)
    );
    assert_eq!(
        file.expressions().get(pair).map(|node| node.span()),
        Ok(pair_span)
    );
    assert_eq!(
        file.type_refs().get(type_ref).map(|node| node.payload()),
        Ok(&TestTypeRef("Pair"))
    );
    assert_eq!(
        file.type_refs().get(type_ref).map(|node| node.span()),
        Ok(pair_span)
    );

    let expression_ids: Vec<_> = file.expressions().iter().map(|(id, _)| id).collect();
    assert_eq!(expression_ids, [left, right, pair]);
    assert_eq!(
        file.expressions()
            .iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        expression_ids
    );
    assert_eq!(
        file.items().iter().map(|(id, _)| id).collect::<Vec<_>>(),
        [item]
    );
    assert_eq!(
        file.statements()
            .iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [statement]
    );
    assert_eq!(
        file.type_refs()
            .iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [type_ref]
    );
    assert_eq!((file.items().len(), file.items().is_empty()), (1, false));
    assert_eq!(
        (
            file.statements().len(),
            file.expressions().len(),
            file.type_refs().len()
        ),
        (1, 3, 1)
    );
}

#[test]
fn every_category_rejects_a_span_from_another_source() {
    let mut sources = SourceMap::new();
    let expected = add_source(&mut sources, "expected.ko", "expected");
    let actual = add_source(&mut sources, "actual.ko", "actual");
    let foreign = span(&sources, actual, 0, 1);
    let mut file = AstFile::<&str, &str, &str, &str>::new(expected);

    assert_eq!(
        file.add_item(foreign, "item"),
        Err(AstError::MismatchedSource {
            category: NodeCategory::Item,
            expected,
            actual,
        })
    );
    assert_eq!(
        file.add_statement(foreign, "statement"),
        Err(AstError::MismatchedSource {
            category: NodeCategory::Statement,
            expected,
            actual,
        })
    );
    assert_eq!(
        file.add_expression(foreign, "expression"),
        Err(AstError::MismatchedSource {
            category: NodeCategory::Expression,
            expected,
            actual,
        })
    );
    assert_eq!(
        file.add_type_ref(foreign, "type ref"),
        Err(AstError::MismatchedSource {
            category: NodeCategory::TypeRef,
            expected,
            actual,
        })
    );
    assert!(file.items().is_empty());
    assert!(file.statements().is_empty());
    assert!(file.expressions().is_empty());
    assert!(file.type_refs().is_empty());
}

#[test]
fn out_of_bounds_ids_return_specific_errors() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "source.ko", "x");
    let node_span = span(&sources, source_id, 0, 1);
    let mut origin = AstFile::<(), (), (), ()>::new(source_id);
    let item = origin
        .add_item(node_span, ())
        .expect("the origin item is valid");
    let statement = origin
        .add_statement(node_span, ())
        .expect("the origin statement is valid");
    let expression = origin
        .add_expression(node_span, ())
        .expect("the origin expression is valid");
    let type_ref = origin
        .add_type_ref(node_span, ())
        .expect("the origin type reference is valid");
    let empty = AstFile::<(), (), (), ()>::new(source_id);

    assert_eq!(
        empty
            .items()
            .get(item)
            .expect_err("item ID must be out of bounds"),
        AstError::InvalidNodeId {
            category: NodeCategory::Item,
            index: 0,
            len: 0,
        }
    );
    assert_eq!(
        empty
            .statements()
            .get(statement)
            .expect_err("statement ID must be out of bounds"),
        AstError::InvalidNodeId {
            category: NodeCategory::Statement,
            index: 0,
            len: 0,
        }
    );
    assert_eq!(
        empty
            .expressions()
            .get(expression)
            .expect_err("expression ID must be out of bounds"),
        AstError::InvalidNodeId {
            category: NodeCategory::Expression,
            index: 0,
            len: 0,
        }
    );
    assert_eq!(
        empty
            .type_refs()
            .get(type_ref)
            .expect_err("type-reference ID must be out of bounds"),
        AstError::InvalidNodeId {
            category: NodeCategory::TypeRef,
            index: 0,
            len: 0,
        }
    );
}

#[test]
fn same_category_ids_do_not_claim_file_identity() {
    let mut sources = SourceMap::new();
    let first_source = add_source(&mut sources, "first.ko", "a");
    let second_source = add_source(&mut sources, "second.ko", "b");
    let first_span = span(&sources, first_source, 0, 1);
    let second_span = span(&sources, second_source, 0, 1);
    let mut first = AstFile::<&str, (), (), ()>::new(first_source);
    let first_id = first
        .add_item(first_span, "first file")
        .expect("the first item is valid");
    let mut second = AstFile::<&str, (), (), ()>::new(second_source);
    let second_id = second
        .add_item(second_span, "second file")
        .expect("the second item is valid");

    assert_eq!(format!("{first_id:?}"), format!("{second_id:?}"));
    assert_eq!(
        second.items().get(first_id).map(|node| node.payload()),
        Ok(&"second file")
    );
    assert_eq!(
        second.items().get(first_id).map(|node| node.span()),
        Ok(second_span)
    );
}

#[test]
fn empty_and_eof_spans_remain_valid_ast_locations() {
    let mut sources = SourceMap::new();
    let empty_id = add_source(&mut sources, "empty.ko", "");
    let text_id = add_source(&mut sources, "text.ko", "x");
    let empty_span = span(&sources, empty_id, 0, 0);
    let eof_span = span(&sources, text_id, 1, 1);
    let mut empty_file = AstFile::<(), (), (), ()>::new(empty_id);
    let mut text_file = AstFile::<(), (), (), ()>::new(text_id);

    let empty_node = empty_file
        .add_expression(empty_span, ())
        .expect("an empty-source location is a valid node span");
    let eof_node = text_file
        .add_expression(eof_span, ())
        .expect("an EOF empty range is a valid node span");

    assert_eq!(
        empty_file
            .expressions()
            .get(empty_node)
            .map(|node| node.span()),
        Ok(empty_span)
    );
    assert_eq!(
        text_file
            .expressions()
            .get(eof_node)
            .map(|node| node.span()),
        Ok(eof_span)
    );
}

#[test]
fn debug_representation_is_structural_and_deterministic() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "machine/path/is/not/stored.ko", "xy");
    let first_span = span(&sources, source_id, 0, 1);
    let second_span = span(&sources, source_id, 1, 2);
    let mut file = AstFile::<&str, &str, &str, &str>::new(source_id);
    let expression = file
        .add_expression(first_span, "machine/path/from/payload")
        .expect("the first expression is valid");
    file.add_expression(second_span, "0xaddress-like-payload")
        .expect("the second expression is valid");

    assert_eq!(format!("{expression:?}"), "ExpressionId(0)");
    assert_eq!(
        format!("{file:?}"),
        concat!(
            "AstFile { source_id: SourceId(0), items: ItemTable { nodes: [] }, ",
            "statements: StatementTable { nodes: [] }, expressions: ExpressionTable { ",
            "nodes: [AstNode { span: Span { source_id: SourceId(0), start: 0, end: 1 }, ",
            ".. }, AstNode { span: Span { source_id: SourceId(0), start: 1, end: 2 }, ",
            ".. }] }, type_refs: TypeRefTable { nodes: [] } }",
        )
    );
}

#[test]
fn id_debug_names_make_categories_explicit() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "source.ko", "x");
    let node_span = span(&sources, source_id, 0, 1);
    let mut file = AstFile::<(), (), (), ()>::new(source_id);
    let item: ItemId = file.add_item(node_span, ()).expect("valid item");
    let statement: StatementId = file.add_statement(node_span, ()).expect("valid statement");
    let expression: ExpressionId = file
        .add_expression(node_span, ())
        .expect("valid expression");
    let type_ref: TypeRefId = file.add_type_ref(node_span, ()).expect("valid type ref");

    assert_eq!(format!("{item:?}"), "ItemId(0)");
    assert_eq!(format!("{statement:?}"), "StatementId(0)");
    assert_eq!(format!("{expression:?}"), "ExpressionId(0)");
    assert_eq!(format!("{type_ref:?}"), "TypeRefId(0)");
}
