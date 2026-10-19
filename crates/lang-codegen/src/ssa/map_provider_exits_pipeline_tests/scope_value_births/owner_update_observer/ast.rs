//! Original Calls, assignments, conditional writes and Name moves without synthetic source.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::Case;
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, Item, NameMarker, ParsedFile, Statement, VariableKind},
    source::{SourceMap, Span},
};
#[derive(Debug)]
pub(super) struct Birth {
    pub expression: ExpressionId,
    pub factory: Span,
    pub name: Span,
}
#[derive(Debug)]
pub(super) struct Local {
    pub marker: Span,
    pub initializer: ExpressionId,
    pub mutable: bool,
}
#[derive(Debug)]
pub(super) struct Input {
    pub scan: Span,
    pub flag: Option<Span>,
    pub locals: Vec<Local>,
    pub source: Span,
    pub births: Vec<Birth>,
    pub conditional: Option<(ExpressionId, ExpressionId)>,
    pub assignment: Option<(ExpressionId, ExpressionId, ExpressionId)>,
    pub put: Option<ExpressionId>,
    pub observes: Vec<(ExpressionId, ExpressionId)>,
    pub observer: Span,
    pub reader: Option<Span>,
    pub site: Option<(lang_frontend::ast::StatementId, ExpressionId)>,
}
pub(super) fn span(p: &ParsedFile, e: ExpressionId) -> Span {
    p.ast().expressions().get(e).unwrap().span()
}
pub(super) fn inside(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
fn function(
    s: &SourceMap,
    p: &ParsedFile,
    wanted: &str,
    arity: usize,
) -> Result<(Span, Span), Failure> {
    one(
        p.roots().iter().filter_map(|id| {
            let n = p.ast().items().get(*id).ok()?;
            match n.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } if s.slice(*name) == Ok(wanted) && parameters.len() == arity => {
                    Some((n.span(), *name))
                }
                _ => None,
            }
        }),
        "update original actual-arity function",
    )
}
pub(super) fn input(c: &Case, s: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
    let scan = function(s, p, "scan", usize::from(c.code <= 9))?.0;
    let flag = if c.code <= 9 {
        Some(one(
            p.roots().iter().filter_map(|id| {
                let node = p.ast().items().get(*id).ok()?;
                let Item::Function { parameters, .. } = node.payload() else {
                    return None;
                };
                if node.span() != scan {
                    return None;
                }
                let [parameter] = parameters.as_slice() else {
                    return None;
                };
                match parameter.name {
                    NameMarker::Present(name)
                        if p.ast()
                            .type_refs()
                            .get(parameter.type_ref)
                            .is_ok_and(|ty| s.slice(ty.span()) == Ok("Boolean")) =>
                    {
                        Some(name)
                    }
                    _ => None,
                }
            }),
            "update actual original Boolean flag parameter",
        )?)
    } else {
        None
    };
    let reader = if c.provider {
        Some(function(s, p, "read", 2)?.0)
    } else {
        None
    };
    let observer = function(
        s,
        p,
        if c.code == 9 {
            "observeMutable"
        } else {
            "observe"
        },
        1,
    )?
    .0;
    let mut locals = p
        .ast()
        .items()
        .iter()
        .filter_map(|(_, n)| match n.payload() {
            Item::Variable {
                name: NameMarker::Present(marker),
                initializer,
                kind,
                ..
            } if inside(scan, n.span()) => Some(Local {
                marker: *marker,
                initializer: *initializer,
                mutable: *kind == VariableKind::Var,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    locals.sort_by_key(|l| l.marker.start());
    let source = one(
        locals.iter().filter(|l| s.slice(l.marker) == Ok("source")),
        "update original var source",
    )?
    .marker;
    need(
        locals.len() == if c.code >= 10 { 2 } else { 1 }
            && locals
                .iter()
                .find(|l| l.marker == source)
                .is_some_and(|l| l.mutable),
        "update exact original owning locals",
        &locals,
    )?;
    let mut births = Vec::new();
    let mut observes = Vec::new();
    let mut puts = Vec::new();
    let mut assignments = Vec::new();
    let mut conditions = Vec::new();
    for (id, n) in p.ast().expressions().iter() {
        if !inside(scan, n.span()) {
            continue;
        }
        match n.payload() {
            Expression::Call {
                callee, arguments, ..
            } => {
                let name = s
                    .slice(span(p, *callee))
                    .map_err(|e| failure("update original callee", e))?;
                if ["make", "makeMutable"].contains(&name) {
                    need(arguments.is_empty(), "update actual factory arity", n)?;
                    let (factory, name) = function(s, p, name, 0)?;
                    births.push(Birth {
                        expression: id,
                        factory,
                        name,
                    });
                }
                if name
                    == if c.code == 9 {
                        "observeMutable"
                    } else {
                        "observe"
                    }
                {
                    need(
                        arguments.len() == 1,
                        "update actual observe arity",
                        arguments,
                    )?;
                    observes.push((id, arguments[0].value));
                }
                if matches!(
                    p.ast().expressions().get(*callee).unwrap().payload(),
                    Expression::Member { .. }
                ) && s
                    .slice(span(p, *callee))
                    .is_ok_and(|text| text.ends_with(".put"))
                {
                    puts.push(id);
                }
            }
            Expression::Assignment { target, value, .. } => assignments.push((id, *target, *value)),
            Expression::If {
                condition,
                else_branch: None,
                ..
            } => conditions.push((id, *condition)),
            _ => {}
        }
    }
    births.sort_by_key(|b| span(p, b.expression).start());
    observes.sort_by_key(|(e, _)| span(p, *e).start());
    need(
        births.len() == if c.code == 9 { 1 } else { 2 }
            && observes.len() == if c.provider { 1 } else { 2 },
        "update exact original Calls/observes",
        (&births, &observes),
    )?;
    let conditional = if c.code <= 9 {
        Some(one(
            conditions.into_iter(),
            "update actual conditional write",
        )?)
    } else {
        need(
            conditions.is_empty(),
            "update Name moves have no invented conditional",
            conditions,
        )?;
        None
    };
    let assignment = if c.code == 9 {
        need(
            assignments.is_empty(),
            "put is no root assignment",
            &assignments,
        )?;
        None
    } else {
        Some(one(
            assignments.into_iter(),
            "update actual source assignment",
        )?)
    };
    if let Some((_, target, rhs)) = assignment {
        need(
            s.slice(span(p, target)) == Ok("source")
                && matches!(
                    p.ast().expressions().get(target).unwrap().payload(),
                    Expression::Name
                ),
            "update actual assignment target source",
            target,
        )?;
        need(
            if c.code == 11 {
                s.slice(span(p, rhs)) == Ok("next")
            } else {
                births.iter().any(|b| b.expression == rhs)
            },
            "update actual complete RHS Call or future Name",
            rhs,
        )?;
    }
    let put = if c.code == 9 {
        Some(one(puts.into_iter(), "update actual conditional put")?)
    } else {
        need(puts.is_empty(), "update no unrelated put", &puts)?;
        None
    };
    if c.code == 10 {
        let local = locals.iter().find(|l| l.marker == source).unwrap();
        need(
            s.slice(span(p, local.initializer)) == Ok("old")
                && matches!(
                    p.ast()
                        .expressions()
                        .get(local.initializer)
                        .unwrap()
                        .payload(),
                    Expression::Name
                ),
            "update prior MoveName old initializer",
            local,
        )?;
    }
    let sites = p
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, n)| match n.payload() {
            Statement::For { source, .. } if inside(scan, n.span()) => Some((id, *source)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let site = if c.provider {
        Some(one(sites.into_iter(), "update actual acquired For AST")?)
    } else {
        need(sites.is_empty(), "update control has no For", &sites)?;
        None
    };
    if let Some((_, source)) = site {
        need(
            s.slice(span(p, source)) == Ok("source"),
            "update For reads actual current source Name",
            source,
        )?;
    }
    Ok(Input {
        scan,
        flag,
        locals,
        source,
        births,
        conditional,
        assignment,
        put,
        observes,
        observer,
        reader,
        site,
    })
}

pub(super) fn declaration(
    p: &ParsedFile,
    marker: Span,
) -> Result<lang_frontend::ast::StatementId, Failure> {
    let item=one(p.ast().items().iter().filter_map(|(id,n)|matches!(n.payload(),Item::Variable{name:NameMarker::Present(name),..}if *name==marker).then_some(id)),"update actual local declaration item")?;
    one(
        p.ast().statements().iter().filter_map(|(id, n)| {
            matches!(n.payload(),Statement::LocalVariable{declaration}if *declaration==item)
                .then_some(id)
        }),
        "update actual local declaration statement",
    )
}
pub(super) fn site(
    p: &ParsedFile,
    i: &Input,
) -> Result<super::super::super::births::ast::Site, Failure> {
    let (statement, source) = i.site.ok_or_else(|| failure("update actual For AST", i))?;
    let node = p.ast().statements().get(statement).unwrap();
    let Statement::For {
        binding: lang_frontend::parser::ForBinding::Destructuring { names, .. },
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("update actual destructuring For", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("update actual K/V names", names));
    };
    let (read, arguments) = one(
        p.ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call { arguments, .. } if inside(node.span(), n.span()) => {
                    Some((id, arguments))
                }
                _ => None,
            }),
        "update actual For sole reader Call",
    )?;
    let [a, b] = arguments.as_slice() else {
        return Err(failure("update actual reader argument arity", arguments));
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

pub(super) fn clone_receivers(p: &ParsedFile, i: &Input) -> Result<Vec<ExpressionId>, Failure> {
    let Some(put) = i.put else {
        return Ok(vec![]);
    };
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        unreachable!()
    };
    arguments
        .iter()
        .map(|arg| {
            let Expression::Call { callee, .. } =
                p.ast().expressions().get(arg.value).unwrap().payload()
            else {
                return Err(failure("update actual put clone Call", arg));
            };
            let Expression::Member { receiver, .. } =
                p.ast().expressions().get(*callee).unwrap().payload()
            else {
                return Err(failure("update actual clone receiver", callee));
            };
            Ok(*receiver)
        })
        .collect()
}
