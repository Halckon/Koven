//! Original AST anchors plus normalized snapshots from the same public FE products.
mod incoming;
mod representation;
mod single;
mod unit;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};
pub(super) use single::collect as single;
pub(super) use unit::collect as unit;

#[derive(Clone, Debug)]
pub(super) struct AstSite {
    pub statement: StatementId,
    pub source: ExpressionId,
    pub body: StatementId,
    pub span: Span,
    pub body_span: Span,
    pub function: Span,
    pub names: [Span; 2],
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Source {
    Parameter {
        slot: usize,
        marker: Span,
        borrow: bool,
    },
    Call {
        span: Span,
        callee: Span,
        arity: usize,
    },
}
#[derive(Clone, Debug)]
pub(super) struct Site {
    pub ast: AstSite,
    pub source: Source,
    pub mutable: bool,
    pub key_type: String,
    pub value_type: String,
    pub owner_markers: Vec<Span>,
    pub representation: Representation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Value(usize),
    Key(usize),
    Finish(usize),
    End(usize),
    Drop(Span),
}
#[derive(Clone, Debug)]
pub(super) struct Cleanup {
    pub point: Span,
    pub kind: u8, // 0 fallthrough, 1 exhaustion, 2 Return, 3 Break
    pub actions: Vec<Action>,
}
#[derive(Clone, Debug)]
pub(super) struct Call {
    pub span: Span,
    pub callee: Span,
    pub arguments: Vec<(Span, Option<Span>, bool)>,
}
#[derive(Clone, Debug)]
pub(super) struct Representation {
    pub nullable: bool,
    pub deinit: Option<Span>,
    pub strings: usize,
}
#[derive(Clone, Debug)]
pub(super) struct GuardInput {
    pub marker: Span,
    pub initializer: Span,
    pub deinit: Span,
}
#[derive(Clone, Debug)]
pub(super) struct NameInput {
    pub span: Span,
    pub marker: Span,
}
#[derive(Clone, Debug)]
pub(super) struct Inputs {
    pub sites: Vec<Site>,
    pub cleanups: Vec<Cleanup>,
    pub calls: Vec<Call>,
    pub guards: Vec<GuardInput>,
    pub names: Vec<NameInput>,
}

pub(super) fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() >= outer.start()
        && inner.end() <= outer.end()
}

pub(super) fn ast(parsed: &ParsedFile) -> Result<Vec<AstSite>, Failure> {
    let mut sites = Vec::new();
    for (statement, node) in parsed.ast().statements().iter() {
        let Statement::For {
            source,
            body,
            binding,
            ..
        } = node.payload()
        else {
            continue;
        };
        let function = one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                (matches!(n.payload(), Item::Function { .. }) && inside(node.span(), n.span()))
                    .then_some(n.span())
            }),
            "stack For exact enclosing callable",
        )?;
        let ForBinding::Destructuring { names, .. } = binding else {
            return Err(failure("stack original K/V binding pair", binding));
        };
        let names = names
            .iter()
            .map(|name| match name {
                NameMarker::Present(s) => Ok(*s),
                _ => Err(failure("stack original present binding", name)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let names = names
            .try_into()
            .map_err(|v| failure("stack exact K/V arity", v))?;
        sites.push(AstSite {
            statement,
            source: *source,
            body: *body,
            span: node.span(),
            body_span: parsed.ast().statements().get(*body).unwrap().span(),
            function,
            names,
        });
    }
    sites.sort_by_key(|s| s.span.start());
    need(!sites.is_empty(), "stack complete actual For set", &sites)?;
    Ok(sites)
}

pub(super) fn parameter(
    parsed: &ParsedFile,
    function: Span,
    marker: Span,
) -> Result<usize, Failure> {
    let parameters = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            match n.payload() {
                Item::Function { parameters, .. } if n.span() == function => Some(parameters),
                _ => None,
            }
        }),
        "stack original callable parameter list",
    )?;
    one(
        parameters
            .iter()
            .enumerate()
            .filter_map(|(i, p)| matches!(p.name,NameMarker::Present(s) if s==marker).then_some(i)),
        "stack exact source parameter slot",
    )
}

pub(super) fn callable(parsed: &ParsedFile, marker: Span) -> Result<Span, Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(),Item::Function{name:NameMarker::Present(s),..} if *s==marker)
                .then_some(n.span())
        }),
        "stack original selected callable declaration",
    )
}

pub(super) fn layer(sites: &[AstSite], statement: StatementId) -> Result<usize, Failure> {
    one(
        sites
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (s.statement == statement).then_some(i)),
        "stack action actual provider statement",
    )
}

pub(super) fn exits(parsed: &ParsedFile, sites: &[AstSite], index: usize) -> Vec<(u8, Span)> {
    let site = &sites[index];
    let mut expected = vec![(0, site.body_span), (1, site.span)];
    for (_, node) in parsed.ast().expressions().iter() {
        if !inside(node.span(), site.body_span) {
            continue;
        }
        let kind = match node.payload() {
            Expression::Return { .. } => 2,
            Expression::Break { .. }
                if !sites.iter().any(|s| {
                    s.statement != site.statement
                        && inside(s.span, site.body_span)
                        && inside(node.span(), s.body_span)
                }) =>
            {
                3
            }
            _ => continue,
        };
        expected.push((kind, node.span()));
    }
    expected
}

pub(super) fn save(
    cleanups: &mut Vec<Cleanup>,
    point: Span,
    kind: u8,
    actions: Vec<Action>,
) -> Result<(), Failure> {
    if let Some(prior) = cleanups.iter().find(|c| c.point == point && c.kind == kind) {
        need(
            prior.actions == actions,
            "stack same-point shared Return actions identical",
            (prior, actions),
        )?;
    } else {
        cleanups.push(Cleanup {
            point,
            kind,
            actions,
        });
    }
    Ok(())
}

pub(super) fn validate_order(actions: &[Action]) -> Result<(), Failure> {
    for (index, action) in actions.iter().enumerate() {
        match action {
            Action::Value(site) => need(
                actions.get(index + 1) == Some(&Action::Key(*site)),
                "stack FE exact VEnd/KEnd adjacency",
                actions,
            )?,
            Action::Finish(site) => need(
                actions.get(index + 1) == Some(&Action::End(*site)),
                "stack FE exact Finish/EndSource adjacency",
                actions,
            )?,
            _ => {}
        }
    }
    Ok(())
}
