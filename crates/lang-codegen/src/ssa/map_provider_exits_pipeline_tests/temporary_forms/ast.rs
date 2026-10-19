//! Original same-scope source Calls, component markers and following output statements.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
    },
    source::{SourceMap, Span},
};

#[derive(Debug)]
pub(super) struct Function {
    pub item: ItemId,
    pub name: Span,
    pub span: Span,
    pub body: StatementId,
}

#[derive(Debug)]
pub(super) struct Site {
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub source: ExpressionId,
    pub call: Span,
    pub factory: Function,
    pub bindings: [Span; 2],
    pub read: ExpressionId,
    pub arguments: [ExpressionId; 2],
    pub after: Span,
    pub marker: &'static str,
}

pub(super) struct Input {
    pub entry: Function,
    pub reader: Function,
    pub sites: [Site; 2],
}

pub(super) fn expression(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}

pub(super) fn function(
    sources: &SourceMap,
    parsed: &ParsedFile,
    wanted: &str,
) -> Result<Function, Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            let Item::Function {
                name: NameMarker::Present(name),
                form,
                ..
            } = node.payload()
            else {
                return None;
            };
            if sources.slice(*name) != Ok(wanted) {
                return None;
            }
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => return None,
            };
            Some(Function {
                item: *id,
                name: *name,
                span: node.span(),
                body,
            })
        }),
        "temporary forms original function",
    )
}

fn elements(parsed: &ParsedFile, block: StatementId) -> Result<&[StatementId], Failure> {
    match parsed.ast().statements().get(block).unwrap().payload() {
        Statement::Block { elements } => Ok(elements),
        other => Err(failure("temporary forms original block", other)),
    }
}

fn statement_expression(
    parsed: &ParsedFile,
    statement: StatementId,
) -> Result<ExpressionId, Failure> {
    match parsed.ast().statements().get(statement).unwrap().payload() {
        Statement::Expression { expression } => Ok(*expression),
        other => Err(failure(
            "temporary forms original expression statement",
            other,
        )),
    }
}

fn call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    id: ExpressionId,
    name: &str,
    arity: usize,
) -> Result<Vec<ExpressionId>, Failure> {
    let node = parsed.ast().expressions().get(id).unwrap();
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = node.payload()
    else {
        return Err(failure("temporary forms original direct Call", node));
    };
    need(
        matches!(
            parsed.ast().expressions().get(*callee).unwrap().payload(),
            Expression::Name
        ) && sources.slice(expression(parsed, *callee)) == Ok(name)
            && type_arguments.is_empty()
            && arguments.len() == arity
            && arguments
                .iter()
                .all(|a| a.named_prefix.is_none() && a.mode_marker.is_none()),
        "temporary forms exact source Call and operand order",
        node,
    )?;
    Ok(arguments.iter().map(|argument| argument.value).collect())
}

fn marker(
    sources: &SourceMap,
    parsed: &ParsedFile,
    id: StatementId,
    expected: &str,
) -> Result<Span, Failure> {
    let id = statement_expression(parsed, id)?;
    let argument = call(sources, parsed, id, "println", 1)?[0];
    need(
        sources.slice(expression(parsed, argument)) == Ok(format!("\"{expected}\"").as_str()),
        "temporary forms actual output marker",
        argument,
    )?;
    Ok(expression(parsed, id))
}

fn site(
    sources: &SourceMap,
    parsed: &ParsedFile,
    statement: StatementId,
    factory: &str,
    after: Span,
    marker: &'static str,
) -> Result<Site, Failure> {
    let node = parsed.ast().statements().get(statement).unwrap();
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("temporary forms original destructuring For", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("temporary forms actual K/V markers", names));
    };
    need(
        sources.slice(*key) == Ok("key") && sources.slice(*value) == Ok("value"),
        "temporary forms ordered original component names",
        names,
    )?;
    call(sources, parsed, *source, factory, 0)?;
    let [read] = elements(parsed, *body)? else {
        return Err(failure("temporary forms sole body reader", body));
    };
    let read = statement_expression(parsed, *read)?;
    let arguments = call(sources, parsed, read, "read", 2)?;
    let arguments = [arguments[0], arguments[1]];
    for (argument, name) in arguments.into_iter().zip(["key", "value"]) {
        need(
            matches!(
                parsed.ast().expressions().get(argument).unwrap().payload(),
                Expression::Name
            ) && sources.slice(expression(parsed, argument)) == Ok(name),
            "temporary forms actual read Name operand",
            argument,
        )?;
    }
    Ok(Site {
        statement,
        span: node.span(),
        body: *body,
        source: *source,
        call: expression(parsed, *source),
        factory: function(sources, parsed, factory)?,
        bindings: [*key, *value],
        read,
        arguments,
        after,
        marker,
    })
}

pub(super) fn input(sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
    let entry = function(sources, parsed, "entry")?;
    let reader = function(sources, parsed, "read")?;
    let [nonempty, first, after_first, empty, second, after_second] = elements(parsed, entry.body)?
    else {
        return Err(failure(
            "temporary forms six same-scope entry statements",
            entry,
        ));
    };
    marker(sources, parsed, *nonempty, "nonempty")?;
    marker(sources, parsed, *empty, "empty")?;
    let first_after = marker(sources, parsed, *after_first, "after-nonempty")?;
    let second_after = marker(sources, parsed, *after_second, "after-empty")?;
    let sites = [
        site(
            sources,
            parsed,
            *first,
            "produce",
            first_after,
            "after-nonempty",
        )?,
        site(
            sources,
            parsed,
            *second,
            "produceEmpty",
            second_after,
            "after-empty",
        )?,
    ];
    need(
        parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
            .count()
            == 2
            && sites[0].span.end() < sites[1].span.start()
            && sites[0].source != sites[1].source
            && sites[0]
                .bindings
                .iter()
                .all(|binding| !sites[1].bindings.contains(binding)),
        "temporary forms exactly two distinct same-scope providers",
        &sites,
    )?;
    Ok(Input {
        entry,
        reader,
        sites,
    })
}

pub(super) fn nominal(
    sources: &SourceMap,
    parsed: &ParsedFile,
    name: &str,
) -> Result<(Span, Span), Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(class) = node.payload() else {
                return None;
            };
            matches!(class.name, NameMarker::Present(marker) if sources.slice(marker) == Ok(name))
                .then(|| {
                    let NameMarker::Present(marker) = class.name else {
                        unreachable!()
                    };
                    (node.span(), marker)
                })
        }),
        "temporary forms original nominal declaration",
    )
}
