//! Public local formation, alias provenance and iteration cleanup identities.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod ast;
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
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn unit_reference(
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
        "G2 qualified original Name symbol",
    )
}
fn unit_marker(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    marker: Span,
    kind: SymbolKind,
) -> bool {
    names.names().source_units().iter().any(|u| {
        u.source_unit() == id.source_unit()
            && u.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == marker && s.kind() == kind && !s.is_synthetic()
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
        "G2 exact two original qualified Map plans",
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        (typed.map_iterations(), owned.map_iterations()),
    );
    match sites((case.mutable, case.form, case.exit), sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "G2 original unit local formation and exits",
                    &unit_site(site, parsed, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("G2 original local AST", &Err(e)),
    }
}
fn unit_site(
    site: &Site,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let root = unit_reference(parsed, names, site.source)?;
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        unit_reference(parsed, names, site.root.initializer)?
    };
    let unit = root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            unit_marker(names, id, local.marker, SymbolKind::Variable)
                && id.source_unit() == unit
                && owned.formed_owned_locals().contains(&id),
            "G2 actual qualified formation history",
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
        "G2 unit original Move source has no Drop",
        (root, original),
    )?;
    let statement = UnitStatementId::new(unit, site.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("G2 original qualified local Map plan", statement))?;
    let d = p.descriptor();
    let constructor = if site.map_kind == MapIterationKind::MutableMap {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.map_iteration(statement) == Some(d)
            && d.kind() == site.map_kind
            && d.source() == expression(site.source)
            && typed.sequential_iteration(statement).is_none()
            && owned.iteration(statement).is_none()
            && typed.expression_type(expression(site.source)) == Some(d.source_type())
            && typed.expression_category(expression(site.source))
                == Some(ExpressionCategory::Place)
            && typed.symbol_type(root) == Some(d.source_type())
            && typed.symbol_type(original) == Some(d.source_type())
            && typed.expression_type(expression(site.root.initializer)) == Some(d.source_type())
            && typed.expression_category(expression(site.root.initializer))
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && typed.expression_type(expression(site.birth.initializer)) == Some(d.source_type())
            && typed.expression_category(expression(site.birth.initializer))
                == Some(ExpressionCategory::Temporary)
            && typed.is_resource_type(d.source_type()) == Some(false)
            && matches!(typed.types().get(d.source_type()),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
            && [d.key_type(), d.value_type()]
                .into_iter()
                .all(|t| typed.types().get(t) == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
            && p.source_access() == UnitIterationSourceAccess::Owned
            && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
        "G2 original qualified local type/category/source capability",
        p,
    )?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "G2 qualified selected factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("G2 qualified actual factory Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == d.source_type()
            && call.result_source() == CallableResultSource::Owned,
        "G2 selected qualified Call owner birth",
        call,
    )?;
    for (index, binding) in d.bindings().into_iter().enumerate() {
        need(
            unit_marker(names, binding, site.bindings[index], SymbolKind::ForBinding)
                && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[index].symbol() == binding
                && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                && p.loans()[index].binding() == binding
                && p.loans()[index].statement() == statement
                && p.loans()[index].source() == d.source()
                && p.loans()[index].component()
                    == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                && p.loans()[index].kind() == LoanKind::Shared
                && Some(p.loans()[index].origin()) == p.source(),
            "G2 exact qualified original K/V lineage",
            p.loans(),
        )?;
    }
    let scanner = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.name && c.parameters().is_empty())
        }),
        "G2 actual qualified scanner",
    )?;
    let signature = scanner
        .callable()
        .ok_or_else(|| failure("G2 scanner callable", scanner))?;
    need(
        typed.types().get(signature.return_type())
            == Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        "G2 qualified scanner Int result",
        signature,
    )?;
    let caller = one(
        typed
            .calls()
            .iter()
            .filter(|c| c.target() == UnitCallTarget::Declaration(scanner.declaration())),
        "G2 actual qualified entry caller",
    )?;
    need(
        caller.arguments().is_empty()
            && caller.return_type() == signature.return_type()
            && caller.expression().source_unit() == unit
            && within(
                site.entry,
                expression_span(parsed, caller.expression().expression())?,
            ),
        "G2 entry actual qualified scanner call",
        caller,
    )?;
    if let Some((value, arguments)) = returned(site, parsed)? {
        let operand = typed
            .call(expression(value))
            .ok_or_else(|| failure("G2 actual qualified result Call", value))?;
        need(
            typed.expression_type(expression(value)) == Some(signature.return_type())
                && operand.return_type() == signature.return_type()
                && matches!(operand.target(),UnitCallTarget::Declaration(id) if typed.signatures().declarations().iter().any(|d|d.declaration()==id && d.callable().is_some_and(|c|c.name_span()==site.result)))
                && matches!(operand.arguments(),[k,v] if k.mode()==ParameterMode::Borrow && v.mode()==ParameterMode::Borrow && k.parameter_type()==d.key_type() && v.parameter_type()==d.value_type())
                && unit_reference(parsed, names, arguments[0])? == d.key_binding()
                && unit_reference(parsed, names, arguments[1])? == d.value_binding(),
            "G2 qualified result Call uses actual K/V roots",
            operand,
        )?;
    }
    let point = match site.point {
        DropPoint::AfterStatement(s) => {
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
        }
        DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
        other => return Err(failure("G2 actual qualified point", other)),
    };
    let kind = match site.kind {
        E::Fallthrough => UE::Fallthrough,
        E::Continue(e) => UE::Continue(expression(e)),
        E::Break(e) => UE::Break(expression(e)),
        E::Return(e) => UE::Return(expression(e)),
        other => return Err(failure("G2 actual qualified kind", other)),
    };
    let source = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let terminal = matches!(kind, UE::Break(_) | UE::Return(_));
    need(
        source.len() == if terminal { 2 } else { 1 }
            && source.iter().all(|d| {
                d.value_origin() == site.root.marker
                    && (d.point() == UnitDropPoint::LoopExit(statement)
                        || terminal && d.point() == point)
            })
            && source
                .iter()
                .filter(|d| d.point() == UnitDropPoint::LoopExit(statement))
                .count()
                == 1
            && (!terminal || source.iter().filter(|d| d.point() == point).count() == 1),
        "G2 original qualified source Drop set",
        &source,
    )?;
    let guard = one(
        owned
            .formed_owned_locals()
            .iter()
            .copied()
            .filter(|id| unit_marker(names, *id, site.guard.marker, SymbolKind::Variable)),
        "G2 actual qualified Guard formation",
    )?;
    need(
        typed
            .expression_type(expression(site.guard.initializer))
            .is_some_and(|t| typed.is_resource_type(t) == Some(true)),
        "G2 qualified Guard Resource type",
        guard,
    )?;
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(guard)),
        "G2 actual qualified Guard Drop",
    )?;
    need(
        guard.point() == point && guard.value_origin() == site.guard.marker,
        "G2 original qualified Guard point",
        guard,
    )?;
    need(
        p.exits().len() == 2,
        "G2 unit complete actual exits",
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
            "G2 exact qualified exit",
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
        if !body || terminal {
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
            "G2 original unit complete ordered cleanup",
            (exit, expected),
        )?;
    }
    Ok(())
}
