//! Real source AST anchors for the two original If-owned Map shapes.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
    temporary_arguments::ast::function,
};
use super::Case;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement, VariableKind},
    source::{SourceMap, Span},
};
#[derive(Debug)]
pub(super) struct Input {
    pub scan: Span,
    pub flag: Span,
    pub conditional: ExpressionId,
    pub condition: ExpressionId,
    pub arms: [StatementId; 2],
    pub births: [ExpressionId; 2],
    pub make: Span,
    pub make_name: Span,
    pub observe: Span,
    pub reader: Span,
    pub local: Option<Span>,
    pub site: Option<(StatementId, ExpressionId)>,
    pub observes: Vec<(ExpressionId, ExpressionId)>,
}
pub(super) fn span(p: &ParsedFile, e: ExpressionId) -> Span {
    p.ast().expressions().get(e).unwrap().span()
}
pub(super) fn inside(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
pub(super) fn input(c: &Case, s: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
    let scan = function(s, p, "scan", Some("Boolean"))?;
    let make = function(s, p, "make", None)?;
    let observe = function(s, p, "observe", Some("Map<String,String>"))?;
    let reader = one(
        p.roots().iter().filter_map(|id| {
            let node = p.ast().items().get(*id).ok()?;
            let Item::Function { name: NameMarker::Present(name), parameters, .. } = node.payload() else { return None; };
            (s.slice(*name) == Ok("read") && parameters.len() == 2
                && parameters.iter().zip(["key", "value"]).all(|(parameter, expected)| {
                    parameter.mode_marker.is_none()
                        && matches!(parameter.name, NameMarker::Present(name) if s.slice(name) == Ok(expected))
                        && p.ast().type_refs().get(parameter.type_ref).is_ok_and(|ty| s.slice(ty.span()) == Ok("String"))
                })).then_some(node.span())
        }),
        "If original reader has two ordered String parameters",
    )?;
    let node = one(
        p.roots()
            .iter()
            .map(|id| p.ast().items().get(*id).unwrap())
            .filter(|n| n.span() == scan.span),
        "If original scanner AST",
    )?;
    let Item::Function { parameters, .. } = node.payload() else {
        unreachable!()
    };
    let NameMarker::Present(flag) = parameters[0].name else {
        return Err(failure("If actual flag parameter", parameters));
    };
    let (conditional, condition, arms) = one(
        p.ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::If {
                    condition,
                    then_branch,
                    else_branch: Some(no),
                    ..
                } if inside(scan.span, n.span()) => Some((id, *condition, [*then_branch, *no])),
                _ => None,
            }),
        "If original scanner conditional",
    )?;
    need(
        s.slice(span(p, condition)) == Ok("flag"),
        "If original flag condition",
        condition,
    )?;
    let mut births = Vec::new();
    for arm in arms {
        let n = p.ast().statements().get(arm).unwrap();
        let Statement::Expression { expression } = n.payload() else {
            return Err(failure("If actual direct arm expression", n));
        };
        let Expression::Call {
            callee, arguments, ..
        } = p.ast().expressions().get(*expression).unwrap().payload()
        else {
            return Err(failure("If actual make Call", expression));
        };
        need(
            s.slice(span(p, *callee)) == Ok("make") && arguments.is_empty(),
            "If arm really calls original make",
            expression,
        )?;
        births.push(*expression);
    }
    let local = if c.temporary {
        None
    } else {
        Some(one(
            p.ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        kind: VariableKind::Val,
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if inside(scan.span, n.span()) && *initializer == conditional => Some(*name),
                    _ => None,
                }),
            "If actual named Map initializer",
        )?)
    };
    let site = if c.provider {
        Some(one(
            p.ast()
                .statements()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Statement::For {
                        source,
                        binding: ForBinding::Destructuring { .. },
                        ..
                    } if inside(scan.span, n.span()) => Some((id, *source)),
                    _ => None,
                }),
            "If actual For site",
        )?)
    } else {
        None
    };
    if c.temporary {
        need(
            site.is_some_and(|(_, source)| source == conditional),
            "If original temporary is the full If source",
            site,
        )?;
    }
    let mut observes = p
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, n)| match n.payload() {
            Expression::Call {
                callee, arguments, ..
            } if inside(scan.span, n.span()) && s.slice(span(p, *callee)) == Ok("observe") => {
                Some((id, arguments))
            }
            _ => None,
        })
        .map(|(id, args)| {
            need(
                args.len() == 1,
                "If original observe has one argument",
                args,
            )?;
            Ok((id, args[0].value))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    observes.sort_by_key(|(e, _)| span(p, *e).start());
    need(
        observes.len()
            == if c.temporary {
                0
            } else if c.label == "P03-control" {
                2
            } else {
                1
            },
        "If exact original observe count",
        &observes,
    )?;
    Ok(Input {
        scan: scan.span,
        flag,
        conditional,
        condition,
        arms,
        births: [births[0], births[1]],
        make: make.span,
        make_name: make.name,
        observe: observe.span,
        reader,
        local,
        site,
        observes,
    })
}

pub(super) fn site(
    p: &ParsedFile,
    i: &Input,
) -> Result<super::super::super::births::ast::Site, Failure> {
    let (statement, source) = i
        .site
        .ok_or_else(|| failure("If actual provider AST site", i))?;
    let node = p.ast().statements().get(statement).unwrap();
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        body,
        ..
    } = node.payload()
    else {
        unreachable!()
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("If actual provider two named bindings", names));
    };
    let (read, arguments) = one(
        p.ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call { arguments, .. }
                    if inside(node.span(), n.span()) && id != i.births[0] && id != i.births[1] =>
                {
                    Some((id, arguments))
                }
                _ => None,
            }),
        "If actual provider sole body reader Call",
    )?;
    let [a, b] = arguments.as_slice() else {
        return Err(failure("If actual reader two arguments", arguments));
    };
    Ok(super::super::super::births::ast::Site {
        statement,
        span: node.span(),
        source,
        body: *body,
        bindings: [*key, *value],
        read,
        arguments: [a.value, b.value],
    })
}
