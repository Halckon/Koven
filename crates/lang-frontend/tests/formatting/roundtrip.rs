use lang_frontend::{
    formatting::format_source,
    lexer::{LexemeKind, TokenKind, lex},
    parser::{
        Expression, FunctionForm, Item, LiteralKind, NameMarker, Statement, VariableKind,
        parse_file,
    },
    source::Span,
};

use super::loaded_source;

type Field = (String, String);
type Projection = Vec<Field>;
type Difference<'a> = (Option<&'a Field>, Option<&'a Field>);

// Visit only the supported rooted file structure; paths encode order and typed edges.
fn project(source: &str) -> Result<Projection, String> {
    let (sources, id) = loaded_source("roundtrip.ko", source);
    let lexed = lex(&sources, id).expect("source can be lexed");
    let parsed = parse_file(&sources, &lexed).expect("source can be parsed");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.source_id(), id);
    assert!(parsed.initial_package_header_is_complete());

    let slice = |span: Span| {
        assert_eq!(span.source_id(), id);
        assert!(!span.is_empty());
        sources.slice(span).expect("valid UTF-8 source span")
    };
    let identifier = |span| {
        assert!(lexed.lexemes().iter().any(|token| {
            token.span() == span && token.kind() == LexemeKind::Token(TokenKind::Identifier)
        }));
        slice(span)
    };
    let bounds = |outer: Span, first: Span, last: Span| {
        assert_eq!(outer.start(), first.start());
        assert_eq!(outer.end(), last.end());
        assert!(first.end() <= last.start());
    };
    let first_token = |outer: Span| {
        lexed
            .lexemes()
            .iter()
            .find(|token| {
                token.span().start() == outer.start()
                    && matches!(token.kind(), LexemeKind::Token(_))
            })
            .expect("node starts on token")
            .span()
    };
    let ordered = |outer: Span, parts: &[Span]| {
        let mut previous_end = outer.start();
        for &part in parts {
            assert!(previous_end <= part.start() && part.end() <= outer.end());
            previous_end = part.end();
        }
    };
    let mut fields = Vec::new();
    let mut push = |path: String, value: &str| fields.push((path, value.to_owned()));
    if let Some(package) = parsed.package() {
        slice(package.span);
        assert_eq!(slice(package.keyword_span), "package");
        bounds(
            package.span,
            package.keyword_span,
            package.segments.last().expect("package segment").span,
        );
        let spans: Vec<_> = std::iter::once(package.keyword_span)
            .chain(package.segments.iter().map(|segment| segment.span))
            .collect();
        ordered(package.span, &spans);
        for (i, segment) in package.segments.iter().enumerate() {
            push(format!("package.segments[{i}]"), identifier(segment.span));
        }
    }
    for (i, import) in parsed.imports().iter().enumerate() {
        let path = format!("imports[{i}]");
        slice(import.span);
        assert_eq!(slice(import.keyword_span), "import");
        if let Some(span) = import.wildcard_span {
            return Err(format!("unsupported {path}.wildcard at {span:?}"));
        }
        let spans: Vec<_> = std::iter::once(import.keyword_span)
            .chain(import.segments.iter().map(|segment| segment.span))
            .chain(
                import
                    .alias
                    .into_iter()
                    .flat_map(|alias| [alias.as_span, alias.name_span]),
            )
            .collect();
        ordered(import.span, &spans);
        for (j, segment) in import.segments.iter().enumerate() {
            push(format!("{path}.segments[{j}]"), identifier(segment.span));
        }
        let alias = import.alias.map_or("", |alias| {
            assert_eq!(slice(alias.as_span), "as");
            identifier(alias.name_span)
        });
        let last = import.alias.map_or_else(
            || import.segments.last().expect("import segment").span,
            |alias| alias.name_span,
        );
        bounds(import.span, import.keyword_span, last);
        push(format!("{path}.alias"), alias);
    }
    let ast = parsed.ast();
    for (i, &root) in parsed.roots().iter().enumerate() {
        let path = format!("roots[{i}]");
        let item = ast.items().get(root).expect("root belongs to AST");
        slice(item.span());
        match item.payload() {
            Item::Variable {
                kind,
                name: NameMarker::Present(name),
                colon_span: None,
                type_ref: None,
                equals_span,
                initializer,
            } => {
                let keyword = match kind {
                    VariableKind::Val => "val",
                    VariableKind::Var => "var",
                    VariableKind::BorrowVal(span) => {
                        return Err(format!("unsupported {path}.kind at {span:?}"));
                    }
                };
                let first = first_token(item.span());
                assert_eq!(slice(first), keyword);
                assert!(first.end() <= name.start() && name.end() <= equals_span.start());
                assert_eq!(slice(*equals_span), "=");
                let init = ast
                    .expressions()
                    .get(*initializer)
                    .expect("valid initializer edge");
                let Expression::Literal(LiteralKind::Integer(kind)) = init.payload() else {
                    return Err(format!(
                        "unsupported {path}.initializer at {:?}",
                        init.span()
                    ));
                };
                use lang_frontend::{
                    lexer::IntegerLiteralSuffix as Suffix, parser::IntegerLiteralKind as Kind,
                };
                let suffix = match kind {
                    Kind::Unsuffixed => Suffix::None,
                    Kind::Long => Suffix::Long,
                    Kind::Unsigned => Suffix::Unsigned,
                    Kind::UnsignedLong => Suffix::UnsignedLong,
                };
                assert!(lexed.lexemes().iter().any(|token| {
                    token.span() == init.span()
                        && token.kind() == LexemeKind::Token(TokenKind::IntegerLiteral(suffix))
                }));
                assert!(equals_span.end() <= init.span().start());
                bounds(item.span(), first, init.span());
                push(format!("{path}.kind"), keyword);
                push(format!("{path}.name"), identifier(*name));
                push(format!("{path}.initializer.spelling"), slice(init.span()));
                push(format!("{path}.initializer.kind"), &format!("{kind:?}"));
            }
            Item::Function {
                name: NameMarker::Present(name),
                extension_receiver: None,
                type_parameters,
                type_parameter_list_span: None,
                parameters,
                form: FunctionForm::ImplicitUnitBlock(body),
            } if type_parameters.is_empty() && parameters.is_empty() => {
                let first = first_token(item.span());
                assert_eq!(slice(first), "fun");
                let body = ast.statements().get(*body).expect("valid body edge");
                if !matches!(body.payload(), Statement::Block { elements } if elements.is_empty()) {
                    return Err(format!("unsupported {path}.body at {:?}", body.span()));
                }
                let block = slice(body.span());
                assert!(block.starts_with('{') && block.ends_with('}'));
                assert!(first.end() <= name.start() && name.end() <= body.span().start());
                bounds(item.span(), first, body.span());
                push(format!("{path}.kind"), "fun");
                push(format!("{path}.name"), identifier(*name));
                push(format!("{path}.body"), "implicit-unit empty block");
            }
            _ => return Err(format!("unsupported {path} at {:?}", item.span())),
        }
    }
    Ok(fields)
}

const CASE: &str = "multiple-declarations.ko";
const FILE: &str = include_str!("../fixtures/phase1/parser-file-pass/multiple-declarations.ko");

fn first_difference<'a>(original: &'a [Field], formatted: &'a [Field]) -> Option<Difference<'a>> {
    (0..original.len().max(formatted.len()))
        .map(|i| (original.get(i), formatted.get(i)))
        .find(|(a, b)| a != b)
}

#[test]
fn formatting_preserves_the_selected_file_syntax() {
    let unformatted = FILE
        .replace("package ", "  package   ")
        .replace(" = ", "   =   ");
    let (sources, id) = loaded_source(CASE, &unformatted);
    let formatted = format_source(&sources, id).expect("fixture formats");
    assert_ne!(
        formatted, unformatted,
        "{CASE}: formatter must change input with redundant horizontal whitespace"
    );
    let original =
        project(&unformatted).unwrap_or_else(|error| panic!("{CASE} original: {error}"));
    let formatted = project(&formatted).unwrap_or_else(|error| panic!("{CASE} formatted: {error}"));
    if let Some((before, after)) = first_difference(&original, &formatted) {
        panic!("{CASE}: first syntax difference: original {before:?}, formatted {after:?}");
    }
}

#[test]
fn projection_ignores_offset_but_keeps_source_identity() {
    let original = project(FILE).expect("baseline projection");
    assert_eq!(project(&format!("// 位移\n{FILE}")).unwrap(), original);
    for (before, after, path) in [
        ("examples", "samples", "package.segments[0]"),
        ("println", "print", "imports[0].segments[2]"),
        ("println", "println as emit", "imports[0].alias"),
        ("answer", "result", "roots[0].name"),
        ("42", "43", "roots[0].initializer.spelling"),
        ("42", "42L", "roots[0].initializer.spelling"),
    ] {
        let changed =
            project(&FILE.replacen(before, after, 1)).expect("mutation remains supported");
        let (left, _) = first_difference(&original, &changed)
            .unwrap_or_else(|| panic!("projection missed {path}"));
        assert_eq!(left.unwrap().0, path);
    }
}

#[test]
fn projection_retains_source_order_and_rejects_unsupported_roots() {
    let first = "import p.a\nimport p.b\nval a = 1\nval b = 2\n";
    let expected = project(first).unwrap();
    for (changed, path) in [
        (
            "import p.b\nimport p.a\nval a = 1\nval b = 2\n",
            "imports[0].segments[1]",
        ),
        (
            "import p.a\nimport p.b\nval b = 2\nval a = 1\n",
            "roots[0].name",
        ),
    ] {
        let changed = project(changed).unwrap();
        let (before, _) = first_difference(&expected, &changed).expect("order must be observable");
        assert_eq!(before.unwrap().0, path);
    }
    assert!(
        project("val a = true\n")
            .unwrap_err()
            .contains("roots[0].initializer")
    );
}
