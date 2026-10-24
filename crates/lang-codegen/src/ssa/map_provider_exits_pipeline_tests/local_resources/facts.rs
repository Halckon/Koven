//! Independent public FE formation and lexical Resource cleanup facts.
use super::super::local_exits::facts::ast;
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod nominal;
mod owner;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use ast::{Site, expression_span, returned, sites, within};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, IterationExitKind as E, IterationSource, LoanKind,
        MapIterationComponent, OwnershipBindingKind, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId,
        UnitTypeKind,
    },
};

fn qualified(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let span = expression_span(parsed, id)?;
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
        "Resource qualified Name root",
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
    match sites((case.mutable, case.form, case.exit), sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "Resource local original qualified identities",
                    &unit_site((case, sources, parsed), site, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("Resource local original AST", &Err(e)),
    }
}
fn unit_site(
    input: (&Case, &SourceMap, &ParsedFile),
    site: &Site,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let (case, sources, parsed) = input;
    let root = qualified(parsed, names, site.source)?;
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        qualified(parsed, names, site.root.initializer)?
    };
    let unit = root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            marker(names, id, local.marker, SymbolKind::Variable)
                && id.source_unit() == unit
                && owned.formed_owned_locals().contains(&id),
            "Resource actual qualified formation history",
            (id, local),
        )?;
    }
    need(
        (root == original) == (site.root.marker == site.birth.marker)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(original)))
            && !owned.drops().iter().any(|d| {
                d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
            }),
        "Resource qualified original Move obligation not duplicated",
        (root, original),
    )?;
    let statement = UnitStatementId::new(unit, site.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("Resource actual qualified Map plan", statement))?;
    let d = p.descriptor();
    let ty = d.source_type();
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.map_iteration(statement) == Some(d)
            && d.source() == expression(site.source)
            && d.kind() == site.map_kind
            && typed.is_resource_type(ty) == Some(true)
            && typed.is_resource_type(d.value_type()) == Some(true)
            && typed.symbol_type(root) == Some(ty)
            && typed.symbol_type(original) == Some(ty)
            && typed.expression_type(expression(site.source)) == Some(ty)
            && typed.expression_category(expression(site.source))
                == Some(ExpressionCategory::Place)
            && typed.expression_type(expression(site.root.initializer)) == Some(ty)
            && typed.expression_category(expression(site.root.initializer))
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && typed.expression_type(expression(site.birth.initializer)) == Some(ty)
            && typed.expression_category(expression(site.birth.initializer))
                == Some(ExpressionCategory::Temporary)
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
            && typed.types().get(d.key_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            && p.source_access() == UnitIterationSourceAccess::Owned
            && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
        "Resource qualified original type/category/Owned source",
        p,
    )?;
    nominal::unit(case.nullable, sources, parsed, names, typed, d.value_type())?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "Resource selected qualified factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("Resource original qualified factory Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.receiver().is_none()
            && call.arguments().is_empty()
            && call.return_type() == ty
            && call.result_source() == CallableResultSource::Owned,
        "Resource exact qualified factory Call owner",
        call,
    )?;
    for (index, binding) in d.bindings().into_iter().enumerate() {
        need(
            marker(names, binding, site.bindings[index], SymbolKind::ForBinding)
                && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
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
            "Resource exact original qualified K/V loan lineage",
            p.loans(),
        )?;
    }
    if let Some((value, args)) = returned(site, parsed)? {
        let call = typed
            .call(expression(value))
            .ok_or_else(|| failure("Resource qualified actual returned Call", value))?;
        need(
            matches!(call.target(),UnitCallTarget::Declaration(id) if typed.signatures().declarations().iter().any(|d|d.declaration()==id && d.callable().is_some_and(|c|c.name_span()==site.result)))
                && typed.types().get(call.return_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && matches!(call.arguments(),[k,v] if k.mode()==ParameterMode::Borrow && v.mode()==ParameterMode::Borrow && k.parameter_type()==d.key_type() && v.parameter_type()==d.value_type())
                && qualified(parsed, names, args[0])? == d.key_binding()
                && qualified(parsed, names, args[1])? == d.value_binding(),
            "Resource qualified actual Return keeps original K/V loans",
            call,
        )?;
    }
    let point = match site.point {
        DropPoint::AfterStatement(s) => {
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
        }
        DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
        other => return Err(failure("Resource actual unit body point", other)),
    };
    let lexical = match owner::lexical(site, parsed)? {
        DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
        other => return Err(failure("Resource actual lexical Return", other)),
    };
    let kind = match site.kind {
        E::Fallthrough => UE::Fallthrough,
        E::Continue(e) => UE::Continue(expression(e)),
        E::Break(e) => UE::Break(expression(e)),
        E::Return(e) => UE::Return(expression(e)),
        other => return Err(failure("Resource actual unit kind", other)),
    };
    let source = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let returning = matches!(kind, UE::Return(_));
    need(
        source.len() == if returning { 2 } else { 1 }
            && source.iter().filter(|d| d.point() == lexical).count() == 1
            && (!returning || source.iter().filter(|d| d.point() == point).count() == 1)
            && source.iter().all(|d| {
                d.value_origin() == site.root.marker
                    && (d.point() == lexical || returning && d.point() == point)
            }),
        "Resource qualified exact lexical and optional Return Drop points",
        &source,
    )?;
    let guard = one(
        owned
            .formed_owned_locals()
            .iter()
            .copied()
            .filter(|id| marker(names, *id, site.guard.marker, SymbolKind::Variable)),
        "Resource qualified actual Guard formation",
    )?;
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(guard)),
        "Resource qualified actual Guard Drop",
    )?;
    need(
        guard.point() == point && guard.value_origin() == site.guard.marker,
        "Resource qualified original Guard point",
        guard,
    )?;
    need(
        p.exits().len() == 2,
        "Resource qualified complete exits",
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
            "Resource qualified actual exit",
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
        if !body || matches!(kind, UE::Break(_) | UE::Return(_)) {
            expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
        }
        expected.extend(
            source
                .iter()
                .filter(|d| d.point() == point)
                .map(|d| UA::Drop(**d)),
        );
        need(
            exit.actions() == expected
                && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "Resource qualified complete real cleanup sequence",
            (exit, expected),
        )?;
    }
    Ok(())
}
