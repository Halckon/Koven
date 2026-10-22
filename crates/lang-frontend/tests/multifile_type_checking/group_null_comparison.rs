//! Comparison-only Group(null) regression matrix; every row uses the raw Unit API.
#[path = "group_null_comparison/diagnostics.rs"]
mod diagnostics;

use super::*;
use diagnostics::{assert_diagnostics, cannot_infer, diagnostic, mismatch};
use lang_frontend::{
    parser::{BinaryOperator, LiteralKind},
    source::Span,
    type_checking::UnitTypeId,
};
use std::collections::BTreeSet;

const ORIGINAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit { var x = input; when (x) { null -> {}; else -> { x = replacement; if (x == null) {} else { observe(x) } } } }
"#;

const NOT_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if (((x) != null)) { observe((x)) } else {}
        }
    }
}
"#;

const REVERSED_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if ((null) == (x)) {} else { observe((x)) }
        }
    }
}
"#;

const REVERSED_NOT_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if (((null) != ((x)))) { observe(((x))) } else {}
        }
    }
}
"#;

const FORWARD_GROUP_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if ((x) == (null)) {} else { observe((x)) }
        }
    }
}
"#;

const FORWARD_GROUP_NOT_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if (((x) != ((null)))) { observe(((x))) } else {}
        }
    }
}
"#;

const BARE_REVERSE_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if (null == (x)) {} else { observe((x)) }
        }
    }
}
"#;

const BARE_REVERSE_NOT_EQUAL: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            if ((null != ((x)))) { observe(((x))) } else {}
        }
    }
}
"#;

const STANDALONE_GROUP_NULL: &str = r#"fun entry(): Unit { val missing = ((null)) }
"#;

const BOTH_GROUP_NULL_EQUAL: &str = r#"fun entry(): Boolean = (null) == ((null))
"#;

const BOTH_GROUP_NULL_NOT_EQUAL: &str = r#"fun entry(): Boolean = ((null)) != (null)
"#;

const WRITE_WITHOUT_RECHECK: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> {};
        else -> {
            x = replacement
            observe((x))
        }
    }
}
"#;

const WRITE_AND_OPPOSITE_ARM: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    if (x != null) {
        observe((x))
        x = replacement
        observe((x))
    } else {
        observe((x))
    }
    observe((x))
}
"#;

const CONTAINER_GROUP_NULL: &str = r#"fun entry(): Unit { val missing = listOf((null)) }
"#;

const CONTAINER_BARE_NULL: &str = r#"fun entry(): Unit { val missing = listOf(null) }
"#;

const GROUP_WRITE_AND_OPPOSITE_ARM: &str = r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit {
    var x = input
    if (((x) != (null))) {
        observe((x))
        x = replacement
        observe((x))
    } else {
        observe((x))
    }
    observe((x))
}
"#;

struct Fixture {
    label: &'static str,
    sources: SourceMap,
    file: ParsedFile,
    names: ValidatedCompilationUnitNames,
    typed: CompilationUnitTypes,
    unit: SourceUnitId,
}

fn checked(label: &'static str, text: &str) -> Fixture {
    let mut sources = SourceMap::new();
    let source = sources.add_source(label, text).expect("unique source");
    let lexed = lex(&sources, source).expect("lexing succeeds internally");
    assert!(
        lexed.diagnostics().is_empty(),
        "{label}: {:?}",
        lexed.diagnostics()
    );
    let file = parse_file(&sources, &lexed).expect("parsing succeeds internally");
    assert!(
        file.diagnostics().is_empty(),
        "{label}: {:?}",
        file.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", label, source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("ordinary source returns a raw Unit product");
    let unit = source_unit(&names, source);
    assert!(typed.signatures().diagnostics().is_empty());
    for ty in typed
        .expression_types()
        .values()
        .chain(typed.body_symbol_types().values())
        .chain(typed.type_ref_types().values())
    {
        assert_no_deferred(&typed, *ty);
    }
    Fixture {
        label,
        sources,
        file,
        names,
        typed,
        unit,
    }
}

fn assert_no_deferred(typed: &CompilationUnitTypes, ty: UnitTypeId) {
    match typed.types().get(ty).expect("recorded type exists") {
        UnitTypeKind::Deferred(reason) => panic!("no deferred escape: {reason:?}"),
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => {
            assert_no_deferred(typed, *inner);
        }
        UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        } => {
            for parameter in parameters {
                assert_no_deferred(typed, parameter.ty());
            }
            assert_no_deferred(typed, *return_type);
        }
        UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. } => {
            for argument in arguments {
                assert_no_deferred(typed, *argument);
            }
        }
        UnitTypeKind::EnumCase { root, .. } => assert_no_deferred(typed, *root),
        _ => {}
    }
}

fn key(fixture: &Fixture, expression: ExpressionId) -> UnitExpressionId {
    UnitExpressionId::new(fixture.unit, expression)
}

fn span(fixture: &Fixture, expression: ExpressionId) -> Span {
    fixture
        .file
        .ast()
        .expressions()
        .get(expression)
        .expect("actual AST expression")
        .span()
}

fn payload(fixture: &Fixture, expression: ExpressionId) -> &Expression {
    fixture
        .file
        .ast()
        .expressions()
        .get(expression)
        .expect("actual AST expression")
        .payload()
}

fn statement_span(fixture: &Fixture, statement: StatementId) -> Span {
    fixture
        .file
        .ast()
        .statements()
        .get(statement)
        .expect("actual AST statement")
        .span()
}

fn chain(fixture: &Fixture, mut expression: ExpressionId) -> Vec<ExpressionId> {
    let mut result = vec![expression];
    while let Expression::Group { expression: inner } = payload(fixture, expression) {
        expression = *inner;
        result.push(expression);
    }
    result
}

fn leaf(fixture: &Fixture, expression: ExpressionId) -> ExpressionId {
    *chain(fixture, expression).last().expect("nonempty chain")
}

fn resolved_symbol(fixture: &Fixture, expression: ExpressionId) -> UnitSymbolId {
    let expression = leaf(fixture, expression);
    assert!(matches!(payload(fixture, expression), Expression::Name));
    let references = fixture
        .names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == fixture.unit
                && reference.span() == span(fixture, expression)
                && reference.namespace() == Some(Namespace::Value)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        references.len(),
        1,
        "one real source-qualified Name reference"
    );
    let UnitReferenceTarget::Symbol(symbol) = references[0].target() else {
        panic!("the actual Name resolves to a body symbol");
    };
    assert_eq!(symbol.source_unit(), fixture.unit);
    *symbol
}

fn node_types(fixture: &Fixture) -> (UnitTypeId, UnitTypeId) {
    let declaration = declaration(&fixture.names, "Node");
    let node = fixture
        .typed
        .signatures()
        .declaration(declaration)
        .expect("Node signature")
        .ty();
    assert_eq!(
        fixture.typed.types().get(node),
        Some(&UnitTypeKind::Nominal {
            declaration,
            arguments: Vec::new(),
        })
    );
    let nullable = fixture
        .typed
        .types()
        .find(&UnitTypeKind::Nullable(node))
        .expect("actual Node? type");
    (node, nullable)
}

fn assert_type(fixture: &Fixture, expression: ExpressionId, ty: UnitTypeId) {
    assert_eq!(
        fixture.typed.expression_type(key(fixture, expression)),
        Some(ty),
        "{}: {:?}",
        fixture.label,
        span(fixture, expression)
    );
}

fn assert_uses(fixture: &Fixture, symbol: UnitSymbolId, expressions: &[ExpressionId]) {
    let (node, nullable) = node_types(fixture);
    let expected = expressions
        .iter()
        .map(|expression| key(fixture, *expression))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        expressions.len(),
        expected.len(),
        "test expectation has no duplicate use"
    );
    assert_eq!(
        fixture.typed.non_null_uses().len(),
        expected.len(),
        "{}",
        fixture.label
    );
    assert_eq!(
        fixture
            .typed
            .non_null_uses()
            .iter()
            .map(|fact| fact.expression())
            .collect::<BTreeSet<_>>(),
        expected,
        "{}",
        fixture.label
    );
    for expression in expressions {
        let fact = fixture
            .typed
            .non_null_use(key(fixture, *expression))
            .expect("actual Name/Group NonNullUse");
        assert_eq!(fact.expression(), key(fixture, *expression));
        assert_eq!(fact.symbol(), symbol);
        assert_eq!(resolved_symbol(fixture, *expression), symbol);
        assert_eq!(fact.declared_type(), nullable);
        assert_eq!(fact.narrowed_type(), node);
        assert_type(fixture, *expression, node);
        assert_eq!(
            fixture.typed.expression_category(key(fixture, *expression)),
            Some(ExpressionCategory::Place)
        );
    }
}

fn observe_arguments(fixture: &Fixture) -> Vec<ExpressionId> {
    let mut arguments = fixture
        .file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            if fixture.sources.slice(span(fixture, *callee)) != Ok("observe") {
                return None;
            }
            assert_eq!(arguments.len(), 1);
            Some(arguments[0].value)
        })
        .collect::<Vec<_>>();
    arguments.sort_by_key(|argument| span(fixture, *argument).start());
    arguments
}

fn assignment_targets(fixture: &Fixture) -> Vec<ExpressionId> {
    fixture
        .file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::Assignment {
                target, operator, ..
            } = node.payload()
            else {
                return None;
            };
            assert_eq!(*operator, AssignmentOperator::Assign);
            Some(*target)
        })
        .collect()
}

fn assert_comparison(
    fixture: &Fixture,
    binary: ExpressionId,
    symbol: UnitSymbolId,
    non_null_when_true: bool,
) {
    let descriptor = fixture
        .typed
        .null_comparison(key(fixture, binary))
        .expect("actual binary descriptor");
    let Expression::Binary { operator, .. } = payload(fixture, binary) else {
        panic!("real binary");
    };
    assert!(matches!(
        operator,
        BinaryOperator::Equal | BinaryOperator::NotEqual
    ));
    assert_eq!(descriptor.expression(), key(fixture, binary));
    assert_eq!(descriptor.expression().source_unit(), fixture.unit);
    assert_eq!(descriptor.symbol(), symbol);
    assert_eq!(descriptor.nullable_type(), node_types(fixture).1);
    assert_eq!(descriptor.non_null_when_true(), non_null_when_true);
    assert_eq!(*operator == BinaryOperator::NotEqual, non_null_when_true);
    assert_eq!(fixture.typed.null_comparisons(), &[descriptor]);
    assert_type(
        fixture,
        binary,
        fixture.typed.types().builtin(BuiltinType::Boolean).unwrap(),
    );
}

struct Positive {
    name: &'static str,
    text: &'static str,
    binary: &'static str,
    grouped_null: bool,
    non_null_when_true: bool,
}

fn positive(row: &Positive, gaps: &mut Vec<&'static str>) {
    let fixture = checked(row.name, row.text);
    let controls = fixture
        .file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } = node.payload()
            else {
                return None;
            };
            Some((
                *condition,
                *then_branch,
                else_branch.expect("explicit else"),
            ))
        })
        .collect::<Vec<_>>();
    let [(condition, then_branch, else_branch)] = controls.as_slice() else {
        panic!("one actual If");
    };
    let binary = leaf(&fixture, *condition);
    assert_eq!(
        fixture.sources.slice(span(&fixture, binary)),
        Ok(row.binary)
    );
    let Expression::Binary {
        left,
        operator,
        right,
        ..
    } = payload(&fixture, binary)
    else {
        panic!("actual comparison");
    };
    assert_eq!(
        *operator,
        if row.non_null_when_true {
            BinaryOperator::NotEqual
        } else {
            BinaryOperator::Equal
        }
    );
    let (null, candidate) = if matches!(
        payload(&fixture, leaf(&fixture, *left)),
        Expression::Literal(LiteralKind::Null)
    ) {
        (*left, *right)
    } else {
        (*right, *left)
    };
    assert_eq!(chain(&fixture, null).len() > 1, row.grouped_null);
    assert_eq!(
        fixture.sources.slice(span(&fixture, leaf(&fixture, null))),
        Ok("null")
    );
    assert_eq!(
        fixture
            .sources
            .slice(span(&fixture, leaf(&fixture, candidate))),
        Ok("x")
    );
    let symbol = resolved_symbol(&fixture, candidate);
    let (node, nullable) = node_types(&fixture);
    assert_eq!(
        fixture.typed.body_symbol_types().get(&symbol),
        Some(&nullable)
    );
    for expression in chain(&fixture, candidate) {
        assert_type(&fixture, expression, nullable);
        assert!(
            fixture
                .typed
                .non_null_use(key(&fixture, expression))
                .is_none(),
            "no stale pre-write proof on the new comparison"
        );
    }
    let arguments = observe_arguments(&fixture);
    let [argument] = arguments.as_slice() else {
        panic!("one actual observe call");
    };
    let targets = assignment_targets(&fixture);
    let [target] = targets.as_slice() else {
        panic!("one preceding write");
    };
    assert!(span(&fixture, *target).end() < span(&fixture, binary).start());
    assert_eq!(resolved_symbol(&fixture, *target), symbol);
    let good = if row.non_null_when_true {
        *then_branch
    } else {
        *else_branch
    };
    let bad = if row.non_null_when_true {
        *else_branch
    } else {
        *then_branch
    };
    let good_span = statement_span(&fixture, good);
    let bad_span = statement_span(&fixture, bad);
    assert_eq!(good_span.source_id(), span(&fixture, *argument).source_id());
    assert!(
        good_span.start() <= span(&fixture, *argument).start()
            && span(&fixture, *argument).end() <= good_span.end()
    );
    assert_eq!(fixture.sources.slice(bad_span), Ok("{}"));
    let missing = !fixture.typed.diagnostics().is_empty();
    let mut expected_uses = vec![*target];
    if missing {
        assert!(
            row.grouped_null,
            "only a frozen Group(null) producer gap is collectable"
        );
        assert_diagnostics(
            &fixture,
            &[cannot_infer(&fixture, null), mismatch(&fixture, *argument)],
        );
        assert!(fixture.typed.null_comparisons().is_empty());
        let error = fixture.typed.types().find(&UnitTypeKind::Error).unwrap();
        for expression in chain(&fixture, null)
            .into_iter()
            .chain(chain(&fixture, *argument))
        {
            assert_type(&fixture, expression, error);
        }
        gaps.push(row.name);
    } else {
        assert_diagnostics(&fixture, &[]);
        assert_comparison(&fixture, binary, symbol, row.non_null_when_true);
        for expression in chain(&fixture, null) {
            assert_type(&fixture, expression, nullable);
        }
        for expression in chain(&fixture, *argument) {
            assert_type(&fixture, expression, node);
            expected_uses.push(expression);
        }
    }
    let boolean = fixture.typed.types().builtin(BuiltinType::Boolean).unwrap();
    for expression in chain(&fixture, *condition) {
        assert_type(&fixture, expression, boolean);
    }
    assert_uses(&fixture, symbol, &expected_uses);
}

fn inference_negative(name: &'static str, text: &str, null_count: usize, bare_container: bool) {
    let fixture = checked(name, text);
    let mut nulls = fixture
        .file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Literal(LiteralKind::Null)).then_some(id)
        })
        .collect::<Vec<_>>();
    nulls.sort_by_key(|expression| span(&fixture, *expression).start());
    assert_eq!(nulls.len(), null_count);
    let expected = nulls
        .iter()
        .map(|expression| {
            if bare_container {
                diagnostic(
                    &fixture,
                    "L0126",
                    "cannot infer a storable element type from a bottom or null first element",
                    span(&fixture, *expression),
                )
            } else {
                cannot_infer(&fixture, *expression)
            }
        })
        .collect::<Vec<_>>();
    assert_diagnostics(&fixture, &expected);
    assert!(fixture.typed.null_comparisons().is_empty());
    assert!(fixture.typed.non_null_uses().is_empty());
    assert!(fixture.typed.container_constructions().is_empty());
    if !bare_container {
        for null in nulls {
            assert_type(
                &fixture,
                null,
                fixture.typed.types().find(&UnitTypeKind::Error).unwrap(),
            );
        }
    }
}

fn no_recheck_negative() {
    let fixture = checked("write_without_recheck.ko", WRITE_WITHOUT_RECHECK);
    let arguments = observe_arguments(&fixture);
    let [argument] = arguments.as_slice() else {
        panic!("one rejected use");
    };
    let targets = assignment_targets(&fixture);
    let [target] = targets.as_slice() else {
        panic!("one killing write");
    };
    assert!(span(&fixture, *target).end() < span(&fixture, *argument).start());
    let symbol = resolved_symbol(&fixture, *target);
    assert_eq!(resolved_symbol(&fixture, *argument), symbol);
    assert_diagnostics(&fixture, &[mismatch(&fixture, *argument)]);
    assert!(fixture.typed.null_comparisons().is_empty());
    assert_uses(&fixture, symbol, &[*target]);
    for expression in chain(&fixture, *argument) {
        assert!(
            fixture
                .typed
                .non_null_use(key(&fixture, expression))
                .is_none()
        );
        assert_type(
            &fixture,
            expression,
            fixture.typed.types().find(&UnitTypeKind::Error).unwrap(),
        );
    }
}

fn boundary_negative(name: &'static str, text: &str, grouped: bool, gaps: &mut Vec<&'static str>) {
    let fixture = checked(name, text);
    let arguments = observe_arguments(&fixture);
    assert_eq!(arguments.len(), 4);
    let targets = assignment_targets(&fixture);
    let [target] = targets.as_slice() else {
        panic!("one write inside the nonnull arm");
    };
    assert!(span(&fixture, arguments[0]).end() < span(&fixture, *target).start());
    assert!(span(&fixture, *target).end() < span(&fixture, arguments[1]).start());
    let binary = expression_with_text(
        &fixture.sources,
        &fixture.file,
        if grouped {
            "(x) != (null)"
        } else {
            "x != null"
        },
    );
    let Expression::Binary { left, right, .. } = payload(&fixture, binary) else {
        panic!("actual boundary comparison");
    };
    let controls = fixture
        .file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } = node.payload()
            else {
                return None;
            };
            Some((
                id,
                *condition,
                *then_branch,
                else_branch.expect("actual else arm"),
            ))
        })
        .collect::<Vec<_>>();
    let [(control, condition, then_branch, else_branch)] = controls.as_slice() else {
        panic!("one actual boundary If");
    };
    assert_eq!(leaf(&fixture, *condition), binary);
    let then_span = statement_span(&fixture, *then_branch);
    let else_span = statement_span(&fixture, *else_branch);
    for expression in [arguments[0], *target, arguments[1]] {
        assert!(
            then_span.start() <= span(&fixture, expression).start()
                && span(&fixture, expression).end() <= then_span.end()
        );
    }
    assert!(
        else_span.start() <= span(&fixture, arguments[2]).start()
            && span(&fixture, arguments[2]).end() <= else_span.end()
    );
    assert!(span(&fixture, *control).end() < span(&fixture, arguments[3]).start());
    let symbol = resolved_symbol(&fixture, *left);
    assert_eq!(resolved_symbol(&fixture, *target), symbol);
    for argument in &arguments {
        assert_eq!(resolved_symbol(&fixture, *argument), symbol);
    }
    let missing = fixture.typed.diagnostics().len() == 5;
    let mut expected_uses = Vec::new();
    let mut expected = Vec::new();
    if missing {
        assert!(
            grouped,
            "only the exact grouped boundary gap may be collected"
        );
        expected.push(cannot_infer(&fixture, *right));
        expected.push(mismatch(&fixture, arguments[0]));
        assert!(fixture.typed.null_comparisons().is_empty());
        gaps.push(name);
    } else {
        assert_comparison(&fixture, binary, symbol, true);
        expected_uses.extend(chain(&fixture, arguments[0]));
        expected_uses.push(*target);
        for expression in chain(&fixture, *right) {
            assert_type(&fixture, expression, node_types(&fixture).1);
        }
    }
    expected.extend(
        arguments[1..]
            .iter()
            .map(|argument| mismatch(&fixture, *argument)),
    );
    assert_diagnostics(&fixture, &expected);
    assert_uses(&fixture, symbol, &expected_uses);
    for argument in &arguments[if missing { 0 } else { 1 }..] {
        for expression in chain(&fixture, *argument) {
            assert!(
                fixture
                    .typed
                    .non_null_use(key(&fixture, expression))
                    .is_none(),
                "no opposite-arm, post-write, or post-If fact"
            );
            assert_type(
                &fixture,
                expression,
                fixture.typed.types().find(&UnitTypeKind::Error).unwrap(),
            );
        }
    }
}

#[test]
fn grouped_null_comparisons_keep_expected_types_and_edge_facts() {
    let rows = [
        ("original.ko", ORIGINAL, "x == null", false, false),
        ("not_equal.ko", NOT_EQUAL, "(x) != null", false, true),
        (
            "reversed_equal.ko",
            REVERSED_EQUAL,
            "(null) == (x)",
            true,
            false,
        ),
        (
            "reversed_not_equal.ko",
            REVERSED_NOT_EQUAL,
            "(null) != ((x))",
            true,
            true,
        ),
        (
            "forward_group_equal.ko",
            FORWARD_GROUP_EQUAL,
            "(x) == (null)",
            true,
            false,
        ),
        (
            "forward_group_not_equal.ko",
            FORWARD_GROUP_NOT_EQUAL,
            "(x) != ((null))",
            true,
            true,
        ),
        (
            "bare_reverse_equal.ko",
            BARE_REVERSE_EQUAL,
            "null == (x)",
            false,
            false,
        ),
        (
            "bare_reverse_not_equal.ko",
            BARE_REVERSE_NOT_EQUAL,
            "null != ((x))",
            false,
            true,
        ),
    ];
    let mut gaps = Vec::new();
    let mut completed = Vec::new();
    for (name, text, binary, grouped_null, non_null_when_true) in rows {
        positive(
            &Positive {
                name,
                text,
                binary,
                grouped_null,
                non_null_when_true,
            },
            &mut gaps,
        );
        completed.push(name);
    }
    for (name, text, null_count, bare_container) in [
        ("standalone_group_null.ko", STANDALONE_GROUP_NULL, 1, false),
        ("both_group_null_equal.ko", BOTH_GROUP_NULL_EQUAL, 2, false),
        (
            "both_group_null_not_equal.ko",
            BOTH_GROUP_NULL_NOT_EQUAL,
            2,
            false,
        ),
        ("container_group_null.ko", CONTAINER_GROUP_NULL, 1, false),
        ("container_bare_null.ko", CONTAINER_BARE_NULL, 1, true),
    ] {
        inference_negative(name, text, null_count, bare_container);
        completed.push(name);
    }
    no_recheck_negative();
    completed.push("write_without_recheck.ko");
    for (name, text, grouped) in [
        ("write_and_opposite_arm.ko", WRITE_AND_OPPOSITE_ARM, false),
        (
            "group_write_and_opposite_arm.ko",
            GROUP_WRITE_AND_OPPOSITE_ARM,
            true,
        ),
    ] {
        boundary_negative(name, text, grouped, &mut gaps);
        completed.push(name);
    }
    assert_eq!(
        completed.len(),
        16,
        "all eight positive and eight negative documents ran"
    );
    assert!(
        gaps.is_empty(),
        "all 16 rows reached their exact contracts; completed={completed:?}; missing Group(null) comparison support={gaps:?}"
    );
}
