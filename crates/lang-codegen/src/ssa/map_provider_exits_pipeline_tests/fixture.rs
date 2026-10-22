//! 普通源码、真实 AST 和阶段错误；不制造或改写 frontend facts。
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, VariableKind, parse_file,
    },
    source::{SourceMap, Span},
    type_checking::{FinalTypeAdmissionError, FinalTypeAdmissionInternalError},
};

use crate::ssa::{LoweringError, model::Origin, verify::VerifyErrors};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exit {
    Continue,
    Break,
    Return,
}

impl Exit {
    pub fn callee(self) -> &'static str {
        if self == Self::Return {
            "operand"
        } else {
            "read"
        }
    }
}

#[derive(Debug)]
pub(in crate::ssa) struct Failure {
    pub stage: &'static str,
    pub original: String,
}

pub(super) fn failure(stage: &'static str, error: impl std::fmt::Debug) -> Failure {
    Failure {
        stage,
        original: format!("{error:?}"),
    }
}

pub(super) fn location(sources: &SourceMap, span: Span) -> String {
    format!(
        "span={span:?} path={:?} text={:?}",
        sources.source_name(span.source_id()),
        sources.slice(span)
    )
}

pub(super) fn at(
    sources: &SourceMap,
    stage: &'static str,
    error: impl std::fmt::Debug,
    span: Option<Span>,
) -> Failure {
    failure(stage, (error, span.map(|span| location(sources, span))))
}

fn diagnostics(sources: &SourceMap, diagnostics: &[Diagnostic]) -> String {
    format!(
        "{:#?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic, location(sources, diagnostic.primary_span())))
            .collect::<Vec<_>>()
    )
}

pub(super) fn clean(
    sources: &SourceMap,
    stage: &'static str,
    items: &[Diagnostic],
) -> Result<(), Failure> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(failure(stage, diagnostics(sources, items)))
    }
}

pub(super) fn complete_failure(sources: &SourceMap, error: FinalTypeAdmissionError) -> Failure {
    let locations = match &error {
        FinalTypeAdmissionError::Diagnostics(items) => diagnostics(sources, items),
        FinalTypeAdmissionError::Internal(
            FinalTypeAdmissionInternalError::MissingRequiredFact { span, .. },
        ) => location(sources, *span),
        _ => "no source span supplied by the original error".to_owned(),
    };
    failure("complete admission", (error, locations))
}

pub(super) fn lower_failure(sources: &SourceMap, error: LoweringError) -> Failure {
    at(sources, "lower", error, error.span)
}

pub(super) fn verify_failure(sources: &SourceMap, errors: VerifyErrors) -> Failure {
    let locations = errors
        .errors
        .iter()
        .filter_map(|error| match &error.origin {
            Some(Origin::Source(span)) => Some(location(sources, *span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    failure("SSA verifier", (errors, locations))
}

pub(super) struct Fixture {
    pub sources: SourceMap,
    pub parsed: ParsedFile,
    pub path: &'static str,
    pub map: bool,
    pub exit: Exit,
}

impl Fixture {
    pub fn new(label: &str, text: &str) -> Result<Self, Failure> {
        let mut sources = SourceMap::new();
        let path = "provider.ko";
        let source = sources
            .add_source(path, text)
            .map_err(|error| failure("source infrastructure", error))?;
        let lexed = lex(&sources, source).map_err(|error| failure("lex infrastructure", error))?;
        clean(&sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed =
            parse_file(&sources, &lexed).map_err(|error| failure("parse infrastructure", error))?;
        clean(&sources, "parse diagnostics", parsed.diagnostics())?;
        let (map, exit) = match label {
            "map-continue" => (true, Exit::Continue),
            "map-break" => (true, Exit::Break),
            "map-return" => (true, Exit::Return),
            "mutable-map-continue" => (false, Exit::Continue),
            "mutable-map-break" => (false, Exit::Break),
            "mutable-map-return" => (false, Exit::Return),
            _ => return Err(failure("fixture identity", label)),
        };
        Ok(Self {
            sources,
            parsed,
            path,
            map,
            exit,
        })
    }
}

#[derive(Debug)]
pub(super) struct LoopSite {
    pub statement: StatementId,
    pub body: StatementId,
    pub span: Span,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
    pub transfer: ExpressionId,
    pub transfer_span: Span,
    pub call: ExpressionId,
    pub call_span: Span,
    pub arguments: [Span; 2],
    pub entry_calls: [ExpressionId; 2],
    pub local: Span,
    pub guard: ExpressionId,
    pub guard_span: Span,
}

pub(super) fn loop_site(fixture: &Fixture) -> Result<LoopSite, Failure> {
    let ast = fixture.parsed.ast();
    let sites = ast
        .statements()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Statement::For { .. }))
        .collect::<Vec<_>>();
    let [(statement, node)] = sites.as_slice() else {
        return Err(failure(
            "AST Map input query",
            ("expected one For", sites.len()),
        ));
    };
    let Statement::For {
        binding,
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("AST Map input query", "not a real For"));
    };
    let ForBinding::Destructuring { names, .. } = binding else {
        return Err(failure("AST Map input query", "not destructuring"));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure(
            "AST Map input query",
            "expected two named bindings",
        ));
    };
    let expression = ast
        .expressions()
        .get(*source)
        .map_err(|error| failure("AST Map input query", error))?;
    if !matches!(expression.payload(), Expression::Name) {
        return Err(at(
            &fixture.sources,
            "AST Map input query",
            "source must be Name",
            Some(expression.span()),
        ));
    }
    let scan = function_body(fixture, "scan")?;
    let scan_elements = block_elements(fixture, scan)?;
    need(
        scan_elements.first() == Some(statement),
        "AST For is direct scan first statement",
        scan_elements,
    )?;
    let elements = block_elements(fixture, *body)?;
    let expected_len = if fixture.exit == Exit::Return { 2 } else { 3 };
    need(
        elements.len() == expected_len,
        "AST exact straightline transfer body",
        elements,
    )?;
    let first = ast
        .statements()
        .get(elements[0])
        .map_err(|e| failure("AST local", e))?;
    let Statement::LocalVariable { declaration } = first.payload() else {
        return Err(failure("AST real Guard local declaration", first));
    };
    let local = ast
        .items()
        .get(*declaration)
        .map_err(|e| failure("AST local item", e))?;
    let Item::Variable {
        kind: VariableKind::Val,
        name: NameMarker::Present(local_name),
        initializer: guard,
        ..
    } = local.payload()
    else {
        return Err(failure("AST Guard val initializer", local));
    };
    need(
        fixture.sources.slice(*local_name) == Ok("local"),
        "AST local name",
        local,
    )?;
    let guard_node = ast
        .expressions()
        .get(*guard)
        .map_err(|e| failure("AST Guard call", e))?;
    direct_call(fixture, *guard, "Guard", &[])?;
    let transfer = expression_statement(
        fixture,
        *elements
            .last()
            .ok_or_else(|| failure("AST tail", elements))?,
    )?;
    let transfer_node = ast
        .expressions()
        .get(transfer)
        .map_err(|e| failure("AST tail expression", e))?;
    let call = match (fixture.exit, transfer_node.payload()) {
        (Exit::Continue, Expression::Continue { .. }) | (Exit::Break, Expression::Break { .. }) => {
            expression_statement(fixture, elements[1])?
        }
        (
            Exit::Return,
            Expression::Return {
                value: Some(value), ..
            },
        ) => *value,
        _ => {
            return Err(failure(
                "AST exact actual terminal transfer",
                (fixture.exit, transfer_node),
            ));
        }
    };
    let call_node = ast
        .expressions()
        .get(call)
        .map_err(|e| failure("AST component call", e))?;
    let arguments = direct_call(fixture, call, fixture.exit.callee(), &["key", "value"])?;
    let [key_arg, value_arg] = arguments.as_slice() else {
        return Err(failure("AST two actual component arguments", arguments));
    };
    // Entry stays executable: one populated call and one empty call, with distinct actual operands.
    let entry_body = function_body(fixture, "entry")?;
    let entry_span = ast
        .statements()
        .get(entry_body)
        .map_err(|e| failure("AST entry body", e))?
        .span();
    let scan_calls = ast
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Call { callee, .. } = node.payload() else {
                return None;
            };
            let target = ast.expressions().get(*callee).ok()?;
            (contains(entry_span, node.span())
                && matches!(target.payload(), Expression::Name)
                && fixture.sources.slice(target.span()) == Ok("scan"))
            .then_some(id)
        })
        .collect::<Vec<_>>();
    need(
        scan_calls.len() == 2,
        "AST entry exactly two real scan calls",
        &scan_calls,
    )?;
    direct_call(fixture, scan_calls[0], "scan", &["populated"])?;
    direct_call(fixture, scan_calls[1], "scan", &["empty"])?;
    if fixture.exit == Exit::Return {
        // Exact source bytes are frozen separately; additionally inspect actual expression identity.
        for (name, expected) in [("operand", "7"), ("scan", "0")] {
            let body = function_body(fixture, name)?;
            let elements = block_elements(fixture, body)?;
            let tail = expression_statement(
                fixture,
                *elements
                    .last()
                    .ok_or_else(|| failure("AST return tail", name))?,
            )?;
            let node = ast
                .expressions()
                .get(tail)
                .map_err(|e| failure("AST return expression", e))?;
            let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            else {
                return Err(failure("AST real scalar return", node));
            };
            let value = ast
                .expressions()
                .get(*value)
                .map_err(|e| failure("AST scalar result", e))?;
            need(
                matches!(value.payload(), Expression::Literal(_))
                    && fixture.sources.slice(value.span()) == Ok(expected),
                "AST exact 7/0 result",
                value,
            )?;
        }
        let text = fixture
            .sources
            .slice(entry_span)
            .map_err(|e| failure("AST entry text", e))?;
        need(
            text.contains("if (result == 7)")
                && text.contains("if (emptyResult == 0)")
                && text.contains("println(\"unexpected\")"),
            "AST entry exact nonempty/empty result checks",
            text,
        )?;
    }
    Ok(LoopSite {
        statement: *statement,
        body: *body,
        span: node.span(),
        source: *source,
        source_span: expression.span(),
        bindings: [*key, *value],
        transfer,
        transfer_span: transfer_node.span(),
        call,
        call_span: call_node.span(),
        arguments: [*key_arg, *value_arg],
        entry_calls: [scan_calls[0], scan_calls[1]],
        local: *local_name,
        guard: *guard,
        guard_span: guard_node.span(),
    })
}

pub(super) fn need(
    ok: bool,
    stage: &'static str,
    detail: impl std::fmt::Debug,
) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(failure(stage, detail))
    }
}

pub(super) fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

pub(super) fn function_body(f: &Fixture, expected: &str) -> Result<StatementId, Failure> {
    let mut bodies = Vec::new();
    for root in f.parsed.roots() {
        let node = f
            .parsed
            .ast()
            .items()
            .get(*root)
            .map_err(|e| failure("AST function root", e))?;
        if let Item::Function {
            name: NameMarker::Present(name),
            form,
            ..
        } = node.payload()
            && f.sources.slice(*name) == Ok(expected)
        {
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => return Err(failure("AST actual function block", node)),
            };
            bodies.push(body);
        }
    }
    let [body] = bodies.as_slice() else {
        return Err(failure("AST unique actual function", (expected, bodies)));
    };
    Ok(*body)
}

fn block_elements(f: &Fixture, body: StatementId) -> Result<&[StatementId], Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|e| failure("AST actual block", e))?;
    let Statement::Block { elements } = node.payload() else {
        return Err(failure("AST actual Block", node));
    };
    Ok(elements)
}

fn expression_statement(f: &Fixture, statement: StatementId) -> Result<ExpressionId, Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("AST expression statement", e))?;
    let Statement::Expression { expression } = node.payload() else {
        return Err(failure("AST actual expression statement", node));
    };
    Ok(*expression)
}

fn direct_call(
    f: &Fixture,
    id: ExpressionId,
    name: &str,
    expected: &[&str],
) -> Result<Vec<Span>, Failure> {
    let node = f
        .parsed
        .ast()
        .expressions()
        .get(id)
        .map_err(|e| failure("AST direct call", e))?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = node.payload()
    else {
        return Err(failure("AST direct Call", node));
    };
    let target = f
        .parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|e| failure("AST callee", e))?;
    need(
        matches!(target.payload(), Expression::Name)
            && f.sources.slice(target.span()) == Ok(name)
            && type_arguments.is_empty()
            && arguments.len() == expected.len(),
        "AST exact direct callee and arity",
        node,
    )?;
    let mut spans = Vec::new();
    for (argument, expected) in arguments.iter().zip(expected) {
        let value = f
            .parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|e| failure("AST actual argument", e))?;
        need(
            matches!(value.payload(), Expression::Name)
                && f.sources.slice(value.span()) == Ok(*expected),
            "AST ordered named argument",
            value,
        )?;
        spans.push(value.span());
    }
    Ok(spans)
}
