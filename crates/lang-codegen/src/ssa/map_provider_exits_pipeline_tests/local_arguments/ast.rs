//! Select the actual Call initializer, lexical scope, parameter syntax and For use independently.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::Case;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParameterModeMarker,
        ParsedFile, Statement,
    },
    source::{SourceMap, Span},
    type_checking::ParameterMode,
};
#[derive(Debug)]
pub(super) struct Parameter {
    pub name: Span,
    pub span: Span,
    pub mode: ParameterMode,
}
#[derive(Debug)]
pub(super) struct Function {
    pub span: Span,
    pub name: Span,
    pub body: StatementId,
    pub parameters: Vec<Parameter>,
}
#[derive(Debug)]
pub(super) struct Argument {
    pub span: Span,
    pub expression: ExpressionId,
    pub parameter: usize,
}
#[derive(Debug)]
pub(super) struct Input {
    pub scope: Function,
    pub factory: Function,
    pub reader: Function,
    pub birth_span: Span,
    pub call: ExpressionId,
    pub arguments: Vec<Argument>,
    pub root: Option<Span>,
    pub statement: StatementId,
    pub span: Span,
    pub source: ExpressionId,
    pub body: StatementId,
    pub bindings: [Span; 2],
    pub read: ExpressionId,
    pub read_arguments: [ExpressionId; 2],
    pub after: Option<Span>,
}
pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
pub(super) fn within(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
fn function(
    parsed: &ParsedFile,
    node: &lang_frontend::ast::AstNode<Item>,
) -> Result<Function, Failure> {
    let Item::Function {
        name: NameMarker::Present(name),
        form,
        parameters,
        ..
    } = node.payload()
    else {
        return Err(failure("local argument original function", node));
    };
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return Err(failure("local argument actual block", form)),
    };
    let parameters = parameters
        .iter()
        .map(|p| {
            let NameMarker::Present(name) = p.name else {
                return Err(failure("local argument original parameter", p));
            };
            let mode = match p.mode_marker {
                None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
                Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
                _ => return Err(failure("local argument parameter mode", p)),
            };
            Ok(Parameter {
                name,
                span: p.span,
                mode,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let _ = parsed;
    Ok(Function {
        span: node.span(),
        name: *name,
        body,
        parameters,
    })
}
pub(super) fn input(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<Input, Failure> {
    let (statement, node) = one(
        parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, n)| matches!(n.payload(), Statement::For { .. })),
        "local argument sole original For",
    )?;
    let Statement::For {
        source,
        body,
        binding: ForBinding::Destructuring { names, .. },
        ..
    } = node.payload()
    else {
        return Err(failure("local argument actual destructuring", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("local argument actual K/V", names));
    };
    let scope_node = one(
        parsed
            .roots()
            .iter()
            .filter_map(|id| parsed.ast().items().get(*id).ok())
            .filter(|n| {
                matches!(n.payload(), Item::Function { .. }) && within(n.span(), node.span())
            }),
        "local argument actual enclosing callable",
    )?;
    let scope = function(parsed, scope_node)?;
    let (root, call) = if case.direct {
        (None, *source)
    } else {
        need(
            matches!(
                parsed.ast().expressions().get(*source).unwrap().payload(),
                Expression::Name
            ) && sources.slice(span(parsed, *source)) == Ok("source"),
            "local argument For names actual local",
            source,
        )?;
        let declaration = one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if within(scope.span, n.span()) && sources.slice(*name) == Ok("source") => {
                        Some((*name, *initializer))
                    }
                    _ => None,
                }),
            "local argument actual current local initializer",
        )?;
        need(
            span(parsed, declaration.1).end() < node.span().start(),
            "local argument birth precedes For",
            declaration,
        )?;
        (Some(declaration.0), declaration.1)
    };
    let Expression::Call {
        callee, arguments, ..
    } = parsed.ast().expressions().get(call).unwrap().payload()
    else {
        return Err(failure("local argument actual birth Call", call));
    };
    let wanted = if case.route { "other" } else { "produce" };
    need(
        sources
            .slice(span(parsed, *callee))
            .is_ok_and(|s| s.ends_with(wanted)),
        "local argument original callable spelling",
        callee,
    )?;
    let class = if case.route {
        Some(one(parsed.roots().iter().filter_map(|id| parsed.ast().items().get(*id).ok()).filter(|n| matches!(n.payload(), Item::Classifier(c) if matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Factory")))), "local argument actual Factory")?.span())
    } else {
        None
    };
    let factory_node = one(
        parsed.ast().items().iter().filter_map(|(_, n)| {
            let Item::Function {
                name: NameMarker::Present(name),
                parameters,
                ..
            } = n.payload()
            else {
                return None;
            };
            (sources.slice(*name) == Ok(wanted)
                && parameters.len() == arguments.len()
                && class.is_none_or(|scope| within(scope, n.span()))
                && !parameters.iter().any(|p| {
                    sources.slice(parsed.ast().type_refs().get(p.type_ref).unwrap().span())
                        == Ok("Boolean")
                }))
            .then_some(n)
        }),
        "local argument exact selected source implementation",
    )?;
    let factory = function(parsed, factory_node)?;
    let arguments = arguments
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let slot = if let Some(prefix) = a.named_prefix {
                one(
                    factory.parameters.iter().enumerate().filter_map(|(i, p)| {
                        (sources.slice(p.name) == sources.slice(prefix.name_span)).then_some(i)
                    }),
                    "local argument original named slot",
                )?
            } else {
                i
            };
            need(
                a.mode_marker.is_none(),
                "local argument ordinary argument syntax",
                a,
            )?;
            Ok(Argument {
                span: a.span,
                expression: a.value,
                parameter: slot,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let reader_node = one(parsed.roots().iter().filter_map(|id| parsed.ast().items().get(*id).ok()).filter(|n| matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("read"))), "local argument actual reader declaration")?;
    let reader = function(parsed, reader_node)?;
    let read = one(
        parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call { callee, .. }
                    if within(node.span(), n.span())
                        && sources.slice(span(parsed, *callee)) == Ok("read") =>
                {
                    Some(id)
                }
                _ => None,
            }),
        "local argument actual reader Call",
    )?;
    let Expression::Call {
        arguments: read_args,
        ..
    } = parsed.ast().expressions().get(read).unwrap().payload()
    else {
        unreachable!()
    };
    need(
        read_args.len() == 2,
        "local argument real reader arity",
        read_args,
    )?;
    let after = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            (within(scope.span, n.span()) && sources.slice(n.span()) == Ok("println(\"after\")"))
                .then_some(n.span())
        })
        .next();
    Ok(Input {
        birth_span: span(parsed, call),
        scope,
        factory,
        reader,
        call,
        arguments,
        root,
        statement,
        span: node.span(),
        source: *source,
        body: *body,
        bindings: [*key, *value],
        read,
        read_arguments: [read_args[0].value, read_args[1].value],
        after,
    })
}
