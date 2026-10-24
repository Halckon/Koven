//! Same-analysis FE facts retain the exact owner and event identities.
use super::super::super::{
    fixture::{Failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{sites, span},
};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget},
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{CompilationUnitTypes, ExpressionCategory, UnitExpressionId, UnitStatementId},
};

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    for s in sites(sources, parsed)? {
        let root = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span(parsed, s.source))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(r) => Some(*r),
                    _ => None,
                }),
            "qualified completed source root",
        )?;
        let u = root.source_unit();
        let mut id = s.rhs;
        let ty = typed.expression_type(UnitExpressionId::new(u, s.call));
        let mut depth = 0;
        loop {
            let q = UnitExpressionId::new(u, id);
            need(
                typed.expression_type(q) == ty
                    && typed.expression_category(q) == Some(ExpressionCategory::Temporary),
                "qualified completed Group type/category",
                q,
            )?;
            if let Expression::Group { expression } =
                parsed.ast().expressions().get(id).unwrap().payload()
            {
                depth += 1;
                id = *expression;
            } else {
                break;
            }
        }
        need(
            depth == if case.grouped { 3 } else { 0 } && id == s.call,
            "qualified canonical actual Call",
            id,
        )?;
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root))
            .collect::<Vec<_>>();
        let new = one(
            drops.iter().copied().filter(|d| {
                d.point() == UnitDropPoint::AfterExpression(UnitExpressionId::new(u, s.assignment))
            }),
            "qualified new installation ASAP Drop",
        )?;
        need(
            new.value_origin() == span(parsed, s.assignment),
            "qualified new value origin is complete assignment event",
            new,
        )?;
        let old = one(
            drops.iter().copied().filter(|d| {
                d.point() == UnitDropPoint::LoopExit(UnitStatementId::new(u, s.statement))
            }),
            "qualified old actual provider Drop",
        )?;
        need(
            old.value_origin() == s.binding,
            "qualified old original binding origin",
            old,
        )?;
        for d in drops {
            need(
                d == new
                    || d == old
                    || (d.point()
                        == UnitDropPoint::AfterExpression(UnitExpressionId::new(u, s.rhs))
                        && d.value_origin() == s.binding),
                "qualified old/new exact distinct cleanup events",
                d,
            )?;
        }
        println!("COMPLETED EPOCH UNIT old={old:#?} new={new:#?}");
    }
    Ok(())
}
