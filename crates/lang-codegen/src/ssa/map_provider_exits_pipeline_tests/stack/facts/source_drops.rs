//! Exact global source Drop obligations, including lexical Returns outside For plans.
use super::*;
use lang_frontend::{
    name_resolution::{NameResolution, ValidatedCompilationUnitNames},
    ownership_checking::{
        CleanupOwnerValue as O, CompilationUnitOwnership, DropPoint, DropTarget,
        IterationPhiBoundary as B, OwnershipCheckedFile, UnitDropPoint, UnitDropTarget,
    },
};

pub(super) fn single(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    sites: &[Site],
) -> Result<Vec<SourceDrop>, Failure> {
    let mut result = Vec::new();
    for site in sites {
        let (root, target) = match site.source {
            Source::Parameter {
                marker,
                borrow: false,
                ..
            } => {
                let symbol = one(
                    names.symbols().iter().filter(|s| s.span() == marker),
                    "stack global Drop original source symbol",
                )?;
                (marker, DropTarget::Named(symbol.id()))
            }
            Source::Call { span, .. } => (span, DropTarget::Temporary(site.ast.source)),
            Source::Parameter { borrow: true, .. } => continue,
        };
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == target)
            .collect::<Vec<_>>();
        need(
            !drops.is_empty(),
            "stack source has original global Drop obligations",
            (root, target),
        )?;
        for drop in drops {
            need(
                drop.value_origin() == root
                    && drop.capture_slot().is_none()
                    && drop.instance_address().is_none(),
                "stack global source Drop retains original birth and ordinary owner",
                drop,
            )?;
            let point = single_point(parsed, drop.point(), site.ast.function, sites)?;
            let owner = drop
                .owner()
                .ok_or_else(|| failure("stack global source Drop actual owner", drop))?;
            let definition = owned
                .cleanup_conditions()
                .owner_value(owner)
                .ok_or_else(|| failure("stack global source Drop owner definition", drop))?;
            match target {
                DropTarget::Temporary(expression) => need(
                    matches!(definition,O::Expression{expression:e,origin} if *e==expression&&*origin==root)
                        && drop.condition().is_none(),
                    "stack global temporary Drop exact Call birth/owner/unconditional",
                    (drop, definition),
                )?,
                DropTarget::Named(symbol) => {
                    let current = current(parsed, owned, sites, site.ast.function, &point, symbol)?;
                    need(
                        owner == current.owner()
                            && matches!(definition,O::IterationPhi{statement,symbol:s,boundary,origin} if *s==symbol&&*boundary==current.boundary()&&owned.map_iteration(*statement).is_some_and(|p|p.closure_phis().contains(current))&&parsed.ast().statements().get(*statement).is_ok_and(|n|n.span()==*origin)),
                        "stack global named Drop exact lexical current phi owner",
                        (drop, current, definition),
                    )?;
                    let condition = drop.condition().ok_or_else(|| {
                        failure("stack actual global phi-owner Drop condition", drop)
                    })?;
                    let paths = owned
                        .map_iterations()
                        .iter()
                        .flat_map(|p| p.exits())
                        .filter(|exit| exit.point() == drop.point())
                        .map(|exit| exit.condition())
                        .collect::<std::collections::BTreeSet<_>>();
                    if paths.is_empty() {
                        // This body tail is outside every iteration exit. Its
                        // source remains the exact completed Exit owner.
                        conditions::conjunction(
                            parsed,
                            owned,
                            condition,
                            current.availability_condition(),
                            current.availability_condition(),
                        )?;
                    } else {
                        // Keep every original same-point plan. A terminal path
                        // restriction is conjoined with current phi presence.
                        for path in paths {
                            conditions::conjunction(
                                parsed,
                                owned,
                                condition,
                                path,
                                current.availability_condition(),
                            )?;
                        }
                    }
                }
                _ => return Err(failure("stack original global source target", target)),
            }
            save(
                &mut result,
                SourceDrop {
                    root,
                    function: site.ast.function,
                    point,
                },
            );
        }
    }
    Ok(result)
}

pub(super) fn unit(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    owned: &CompilationUnitOwnership,
    sites: &[Site],
) -> Result<Vec<SourceDrop>, Failure> {
    let source = one(
        names.names().source_units().iter(),
        "stack global Drop original qualified source",
    )?;
    let mut result = Vec::new();
    for site in sites {
        let (root, target) = match site.source {
            Source::Parameter {
                marker,
                borrow: false,
                ..
            } => {
                let symbol = one(
                    owned.bindings().iter().filter_map(|binding| {
                        let id = binding.symbol();
                        (id.source_unit() == source.source_unit()
                            && source
                                .resolution()
                                .symbols()
                                .iter()
                                .any(|s| s.id() == id.symbol() && s.span() == marker))
                        .then_some(id)
                    }),
                    "stack global Drop exact qualified source root",
                )?;
                (marker, UnitDropTarget::Named(symbol))
            }
            Source::Call { span, .. } => (
                span,
                UnitDropTarget::Temporary(lang_frontend::type_checking::UnitExpressionId::new(
                    source.source_unit(),
                    site.ast.source,
                )),
            ),
            Source::Parameter { borrow: true, .. } => continue,
        };
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == target)
            .collect::<Vec<_>>();
        need(
            !drops.is_empty(),
            "stack qualified source global Drop obligations",
            target,
        )?;
        for drop in drops {
            need(
                drop.value_origin() == root,
                "stack qualified global Drop exact source birth",
                drop,
            )?;
            let point = match drop.point() {
                UnitDropPoint::LoopExit(id) => {
                    need(
                        id.source_unit() == source.source_unit(),
                        "stack global Drop original unit",
                        drop,
                    )?;
                    loop_point(parsed, id.statement(), sites)?
                }
                UnitDropPoint::ControlTransfer(id) => {
                    need(
                        id.source_unit() == source.source_unit(),
                        "stack global Return Drop original unit",
                        drop,
                    )?;
                    transfer_point(parsed, id.expression(), site.ast.function)?
                }
                UnitDropPoint::AfterStatement(id) => {
                    need(
                        id.source_unit() == source.source_unit(),
                        "stack global scope Drop original unit",
                        drop,
                    )?;
                    scope_point(parsed, id.statement(), site.ast.function)?
                }
                _ => {
                    return Err(failure(
                        "stack qualified global source Drop original point",
                        drop,
                    ));
                }
            };
            // The public UnitDropFact deliberately has no owner or condition
            // ID. Preserve its real qualified target/point; SSA independently
            // proves the current owner on every incoming path.
            save(
                &mut result,
                SourceDrop {
                    root,
                    function: site.ast.function,
                    point,
                },
            );
        }
    }
    Ok(result)
}
fn save(drops: &mut Vec<SourceDrop>, drop: SourceDrop) {
    if !drops.contains(&drop) {
        drops.push(drop);
    }
}
fn single_point(
    parsed: &ParsedFile,
    point: DropPoint,
    function: Span,
    sites: &[Site],
) -> Result<SourceDropPoint, Failure> {
    match point {
        DropPoint::LoopExit(statement) => loop_point(parsed, statement, sites),
        DropPoint::ControlTransfer(expression) => transfer_point(parsed, expression, function),
        DropPoint::AfterStatement(statement) => scope_point(parsed, statement, function),
        _ => Err(failure("stack global source Drop actual point", point)),
    }
}
fn loop_point(
    parsed: &ParsedFile,
    statement: StatementId,
    sites: &[Site],
) -> Result<SourceDropPoint, Failure> {
    let site = one(
        sites.iter().filter(|s| s.ast.statement == statement),
        "stack global Drop actual provider statement",
    )?;
    need(
        parsed.ast().statements().get(statement).unwrap().span() == site.ast.span,
        "stack global Drop original LoopExit",
        site,
    )?;
    Ok(SourceDropPoint::LoopExit(site.ast.span))
}
fn transfer_point(
    parsed: &ParsedFile,
    expression: ExpressionId,
    function: Span,
) -> Result<SourceDropPoint, Failure> {
    let node = parsed.ast().expressions().get(expression).unwrap();
    need(
        inside(node.span(), function),
        "stack global transfer Drop stays in source callable",
        node,
    )?;
    match node.payload() {
        Expression::Return { value, .. } => Ok(SourceDropPoint::Return {
            span: node.span(),
            operand: value.map(|v| parsed.ast().expressions().get(v).unwrap().span()),
        }),
        Expression::Break { .. } => Ok(SourceDropPoint::Break(node.span())),
        _ => Err(failure("stack actual global finite transfer AST", node)),
    }
}
fn scope_point(
    parsed: &ParsedFile,
    statement: StatementId,
    function: Span,
) -> Result<SourceDropPoint, Failure> {
    let span = parsed.ast().statements().get(statement).unwrap().span();
    let callable = one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            match node.payload() {
                Item::Function {
                    form:
                        lang_frontend::parser::FunctionForm::ImplicitUnitBlock(body)
                        | lang_frontend::parser::FunctionForm::Explicit {
                            body: lang_frontend::parser::FunctionBody::Block(body),
                            ..
                        },
                    ..
                } if node.span() == function => Some(*body),
                _ => None,
            }
        }),
        "stack original implicit Return source scope",
    )?;
    need(
        callable == statement,
        "stack global AfterStatement Drop is exact callable body",
        (statement, callable),
    )?;
    Ok(SourceDropPoint::ScopeEnd(span))
}
fn current<'a>(
    parsed: &ParsedFile,
    owned: &'a OwnershipCheckedFile,
    sites: &[Site],
    function: Span,
    point: &SourceDropPoint,
    root: lang_frontend::name_resolution::SymbolId,
) -> Result<&'a lang_frontend::ownership_checking::IterationClosurePhiBinding, Failure> {
    if let SourceDropPoint::LoopExit(span) = point {
        let statement = one(
            sites.iter().filter(|s| s.ast.span == *span),
            "stack global exhaustion source layer",
        )?
        .ast
        .statement;
        return one(
            owned
                .map_iteration(statement)
                .unwrap()
                .closure_phis()
                .iter()
                .filter(|p| p.symbol() == root && p.boundary() == B::Header),
            "stack global exhaustion consumes exact Header owner",
        );
    }
    let (at, cutoff) = match point {
        SourceDropPoint::Return { span, .. } | SourceDropPoint::Break(span) => {
            (*span, span.start())
        }
        SourceDropPoint::ScopeEnd(span) => (*span, span.end()),
        SourceDropPoint::LoopExit(_) => unreachable!(),
    };
    let active = sites
        .iter()
        .filter(|s| s.ast.function == function && inside(at, s.ast.body_span))
        .filter_map(|s| {
            owned
                .map_iteration(s.ast.statement)?
                .closure_phis()
                .iter()
                .find(|p| p.symbol() == root && p.boundary() == B::Header)
                .map(|phi| (s, phi))
        })
        .min_by_key(|(s, _)| s.ast.body_span.end() - s.ast.body_span.start());
    let completed = sites
        .iter()
        .filter(|s| {
            s.ast.function == function
                && s.ast.span.end() <= cutoff
                && active.is_none_or(|(parent, _)| inside(s.ast.span, parent.ast.body_span))
        })
        .filter_map(|s| {
            owned
                .map_iteration(s.ast.statement)?
                .closure_phis()
                .iter()
                .find(|p| p.symbol() == root && p.boundary() == B::Exit)
                .map(|phi| (s.ast.span.end(), phi))
        })
        .max_by_key(|(end, _)| *end)
        .map(|(_, phi)| phi);
    completed.or(active.map(|(_, phi)| phi)).ok_or_else(|| {
        failure(
            "stack global Drop precise lexical current owner",
            (point, root, parsed.source_id()),
        )
    })
}
