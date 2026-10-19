//! Original Int storage, scalar payloads, reader comparisons and For/control syntax.
use super::super::super::{
    births::ast::Site,
    fixture::{Failure, failure, need},
    query_inputs::one,
    temporary_arguments::ast::function,
};
use super::Case;
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};
#[derive(Debug)]
pub(super) struct Input {
    pub entry: Span,
    pub scan: Span,
    pub make: Span,
    pub reader: Span,
    pub make_name: Span,
    pub scan_name: Span,
    pub reader_name: Span,
    pub reader_params: [Span; 2],
    pub scanner_call: ExpressionId,
    pub source: Span,
    pub birth: ExpressionId,
    pub initial: Span,
    pub construction: ExpressionId,
    pub put: ExpressionId,
    pub payload: [ExpressionId; 2],
    pub consume: ExpressionId,
    pub returned: ExpressionId,
    pub site: Option<Site>,
    pub size: Option<ExpressionId>,
    pub compares: [ExpressionId; 2],
    pub compared: [ExpressionId; 2],
    pub literals: [ExpressionId; 2],
}
pub(super) fn span(p: &ParsedFile, id: ExpressionId) -> Span {
    p.ast().expressions().get(id).unwrap().span()
}
pub(super) fn inside(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
pub(super) fn input(c: &Case, sources: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
    let entry = function(sources, p, "entry", None)?.span;
    let scanner = function(sources, p, "scan", None)?;
    let factory = function(sources, p, "makeInt", None)?;
    let (reader, reader_name, reader_params) = one(
        p.roots()
            .iter()
            .filter_map(|id| p.ast().items().get(*id).ok())
            .filter_map(|n| match n.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } if sources.slice(*name) == Ok("readInt") => Some((n.span(), *name, parameters)),
                _ => None,
            }),
        "Int actual reader declaration",
    )?;
    let reader_params = reader_params
        .iter()
        .map(|p| match p.name {
            NameMarker::Present(s) => Ok(s),
            _ => Err(failure("Int actual reader parameter", p)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let reader_params: [Span; 2] = reader_params
        .try_into()
        .map_err(|v| failure("Int exact two reader parameters", v))?;
    let call = |scope, wanted| {
        one(
            p.ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Call { callee, .. }
                        if inside(scope, n.span())
                            && sources.slice(span(p, *callee)) == Ok(wanted) =>
                    {
                        Some(id)
                    }
                    _ => None,
                }),
            "Int actual ordinary source Call",
        )
    };
    let local = |scope, wanted| {
        one(
            p.ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if inside(scope, n.span()) && sources.slice(*name) == Ok(wanted) => {
                        Some((*name, *initializer))
                    }
                    _ => None,
                }),
            "Int actual named owner initializer",
        )
    };
    let (source, birth) = local(scanner.span, "source")?;
    need(
        birth == call(scanner.span, "makeInt")?,
        "Int source actual makeInt birth",
        birth,
    )?;
    let (initial, construction) = local(factory.span, "m")?;
    let member = |wanted| {
        one(p.ast().expressions().iter().filter_map(|(id,n)|match n.payload(){Expression::Call{callee,..}if inside(factory.span,n.span())&&matches!(p.ast().expressions().get(*callee).unwrap().payload(),Expression::Member{name_span,..}if sources.slice(*name_span)==Ok(wanted))=>Some(id),_=>None}),"Int original factory member Call")
    };
    let put = member("put")?;
    let consume = member("consume")?;
    let returned = one(
        p.ast().expressions().iter().filter_map(|(id, n)| {
            matches!(n.payload(),Expression::Return{value:Some(v),..}if *v==consume).then_some(id)
        }),
        "Int actual owned consume Return",
    )?;
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        unreachable!()
    };
    let [a, b] = arguments.as_slice() else {
        return Err(failure("Int actual two put payloads", arguments));
    };
    let payload = [a.value, b.value];
    for (id, text) in payload.into_iter().zip(["1", "2"]) {
        need(
            sources.slice(span(p, id)) == Ok(text),
            "Int fixed actual put payload literal",
            id,
        )?;
    }
    let site = if c.provider {
        let (statement, n, names, source, body) = one(
            p.ast()
                .statements()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Statement::For {
                        binding: ForBinding::Destructuring { names, .. },
                        source,
                        body,
                        ..
                    } if inside(scanner.span, n.span()) => Some((id, n, names, *source, *body)),
                    _ => None,
                }),
            "Int actual original For",
        )?;
        let [NameMarker::Present(k), NameMarker::Present(v)] = names.as_slice() else {
            return Err(failure("Int actual K/V markers", names));
        };
        let read = call(n.span(), "readInt")?;
        let Expression::Call { arguments, .. } = p.ast().expressions().get(read).unwrap().payload()
        else {
            unreachable!()
        };
        let [karg, varg] = arguments.as_slice() else {
            return Err(failure("Int reader original slots", arguments));
        };
        Some(Site {
            statement,
            span: n.span(),
            source,
            body,
            bindings: [*k, *v],
            read,
            arguments: [karg.value, varg.value],
        })
    } else {
        None
    };
    let size = if c.provider {
        None
    } else {
        Some(one(p.ast().expressions().iter().filter_map(|(id,n)|matches!(n.payload(),Expression::Member{name_span,..}if inside(scanner.span,n.span())&&sources.slice(*name_span)==Ok("size")).then_some(id)),"Int actual control MapSize")?)
    };
    let mut comparison = p
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, n)| match n.payload() {
            Expression::Binary {
                left,
                right,
                operator: lang_frontend::parser::BinaryOperator::Equal,
                ..
            } if inside(reader, n.span()) => Some((id, *left, *right)),
            _ => None,
        })
        .collect::<Vec<_>>();
    comparison.sort_by_key(|(id, _, _)| span(p, *id).start());
    let [a, b] = comparison.as_slice() else {
        return Err(failure("Int exact reader comparisons", comparison));
    };
    Ok(Input {
        entry,
        scan: scanner.span,
        make: factory.span,
        reader,
        make_name: factory.name,
        scan_name: scanner.name,
        reader_name,
        reader_params,
        scanner_call: call(entry, "scan")?,
        source,
        birth,
        initial,
        construction,
        put,
        payload,
        consume,
        returned,
        site,
        size,
        compares: [a.0, b.0],
        compared: [a.1, b.1],
        literals: [a.2, b.2],
    })
}
