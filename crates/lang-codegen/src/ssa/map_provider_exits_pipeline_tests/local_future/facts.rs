//! Public local formation, multi-For live owner identity and actual Pure last-use facts.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod ast;
mod loans;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use ast::{Loop, Scanner};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, IterationExitKind as E, IterationSource, LoanKind,
        MapIterationComponent, OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropFact,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn qualified(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let span = ast::span(parsed, id)?;
    one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span)
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "local future actual qualified Name symbol",
    )
}
fn marker(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> bool {
    names.names().source_units().iter().any(|u| {
        u.source_unit() == id.source_unit()
            && u.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == span && s.kind() == kind && !s.is_synthetic()
            })
    })
}

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    report.check(
        "local future complete qualified provider set",
        typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
            && owned.map_iterations().len() == typed.map_iterations().len(),
        (typed.map_iterations(), owned.map_iterations()),
    );
    match ast::scanners((case.binding, case.exit, case.form), sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "local future original qualified formation/cleanup/loans",
                    &unit_site((case, sources, parsed), site, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("local future original AST", &Err(e)),
    }
}
fn unit_site(
    input: (&Case, &SourceMap, &ParsedFile),
    site: &Scanner,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let (case, sources, parsed) = input;
    let root = qualified(parsed, names, site.loops[0].source)?;
    let unit = root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        qualified(parsed, names, site.root.initializer)?
    };
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            id.source_unit() == unit
                && marker(names, id, local.marker, SymbolKind::Variable)
                && owned.formed_owned_locals().contains(&id),
            "local future qualified actual declaration/formation",
            (id, local),
        )?;
    }
    need(
        (root == original) == (case.binding != 2)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(original)))
            && !owned.drops().iter().any(|d| {
                d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
            }),
        "local future qualified alias original has no duplicate Drop",
        (root, original),
    )?;
    let ty = typed
        .symbol_type(root)
        .ok_or_else(|| failure("local future qualified source type", root))?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.symbol_type(original) == Some(ty)
            && typed.is_resource_type(ty) == Some(false)
            && typed.expression_type(expression(site.birth.initializer)) == Some(ty)
            && typed.expression_category(expression(site.birth.initializer))
                == Some(ExpressionCategory::Temporary)
            && typed.expression_type(expression(site.root.initializer)) == Some(ty)
            && typed.expression_category(expression(site.root.initializer))
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && matches!(arguments.as_slice(),[k,v] if k==v && typed.types().get(*k)==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
        "local future qualified exact Pure Map birth/type/category",
        ty,
    )?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "local future qualified original selected factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("local future qualified actual birth Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == ty
            && call.result_source() == CallableResultSource::Owned,
        "local future qualified actual Call formation",
        call,
    )?;
    let scanner = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|c| c.name_span() == site.name),
        "local future qualified original scanner",
    )?;
    need(
        scanner.parameters().is_empty()
            && typed.types().get(scanner.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit)),
        "local future qualified zero-argument Unit scanner",
        scanner,
    )?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root)),
        "local future unique qualified Pure Drop",
    )?;
    need(
        drop.value_origin() == site.root.marker,
        "local future qualified original root Drop marker",
        drop,
    )?;
    for loop_site in &site.loops {
        let statement = UnitStatementId::new(unit, loop_site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("local future actual qualified Map plan", loop_site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == expression(loop_site.source)
                && d.source_type() == ty
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && qualified(parsed, names, loop_site.source)? == root
                && typed.expression_category(expression(loop_site.source))
                    == Some(ExpressionCategory::Place)
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
            "local future original qualified For descriptor/Owned lineage",
            p,
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                marker(
                    names,
                    binding,
                    loop_site.bindings[index],
                    SymbolKind::ForBinding,
                ) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && typed.types().get([d.key_type(), d.value_type()][index])
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && p.loans()[index].statement() == statement
                    && p.loans()[index].source() == d.source()
                    && p.loans()[index].binding() == binding
                    && p.loans()[index].component()
                        == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                    && p.loans()[index].kind() == LoanKind::Shared
                    && Some(p.loans()[index].origin()) == p.source(),
                "local future exact qualified original K/V identities",
                p.loans(),
            )?;
        }
        let point = match loop_site.point {
            DropPoint::AfterStatement(s) => {
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
            }
            DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
            other => return Err(failure("local future qualified actual point", other)),
        };
        let kind = match loop_site.kind {
            E::Fallthrough => UE::Fallthrough,
            E::Continue(e) => UE::Continue(expression(e)),
            E::Break(e) => UE::Break(expression(e)),
            other => return Err(failure("local future actual qualified kind", other)),
        };
        let guard = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.value_origin() == loop_site.guard.marker),
            "local future original qualified Guard Drop",
        )?;
        need(
            guard.point() == point
                && matches!(guard.target(),UnitDropTarget::Named(id) if marker(names,id,loop_site.guard.marker,SymbolKind::Variable) && owned.formed_owned_locals().contains(&id)),
            "local future actual qualified Guard target/point",
            guard,
        )?;
        need(
            p.exits().len() == 2,
            "local future complete qualified exit set",
            p.exits(),
        )?;
        for (kind, point, body) in [
            (kind, point, true),
            (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
        ] {
            let exit = one(
                p.exits()
                    .iter()
                    .filter(|e| e.kind() == kind && e.point() == point),
                "local future original qualified exit",
            )?;
            let mut expected = if body {
                vec![
                    UA::Drop(guard),
                    UA::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ]
            } else {
                vec![]
            };
            if !body || matches!(kind, UE::Break(_)) {
                expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
            }
            need(
                exit.actions() == expected
                    && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                "local future qualified full cleanup never drops still-needed source",
                (exit, expected),
            )?;
        }
    }
    loans::unit(
        (case, sources, parsed, site),
        names,
        typed,
        owned,
        (root, ty),
        &drop,
    )
}
