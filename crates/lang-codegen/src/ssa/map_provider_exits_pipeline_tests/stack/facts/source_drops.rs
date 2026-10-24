//! Exact global source Drop obligations, including lexical Returns outside For plans.
use super::*;
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget},
};

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
