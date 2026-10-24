//! Same-analysis public Borrow source facts, including legal unacquired dead tails.
mod body;
mod callers;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::*,
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::*,
};
pub(super) type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);

fn unit_symbol(
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    marker: Span,
) -> Result<UnitSymbolId, Failure> {
    one(
        names.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == marker && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "scope actual qualified original symbol",
    )
    .and_then(|(unit, symbol)| {
        one(
            names
                .names()
                .references()
                .iter()
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(id)
                        if id.source_unit() == unit && id.symbol() == symbol =>
                    {
                        Some(*id)
                    }
                    _ => None,
                })
                .chain(
                    typed
                        .signatures()
                        .declarations()
                        .iter()
                        .filter_map(|d| d.callable())
                        .flat_map(|s| s.parameters())
                        .filter_map(|p| p.symbol())
                        .filter(|id| id.source_unit() == unit && id.symbol() == symbol),
                )
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter(),
            "scope actual qualified referenced symbol",
        )
    })
}
pub(super) fn unit(
    input: Input<'_>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let (c, _, parsed, i) = input;
    let unit = one(
        names.names().source_units().iter(),
        "scope original source unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    callers::unit(input, names, typed, owned)?;
    if c.gate == 2 {
        need(
            typed.map_iterations().len() == usize::from(c.provider)
                && owned.map_iterations().is_empty()
                && owned.drops().is_empty()
                && typed.map_constructions().len() == 1
                && i.early_return.is_some(),
            "scope qualified dead tail is valid and unacquired",
            (typed.map_iterations(), owned.map_iterations()),
        )?;
        return Ok(());
    }
    let root = unit_symbol(names, typed, i.scan.parameters[0])?;
    let flag = unit_symbol(names, typed, i.scan.parameters[1])?;
    let sig = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|s| s.name_span() == i.scan.name),
        "scope actual qualified scanner signature",
    )?;
    need(
        sig.parameters().len() == 2
            && sig
                .parameters()
                .iter()
                .all(|p| p.mode() == ParameterMode::Borrow)
            && sig.parameters()[0].symbol() == Some(root)
            && sig.parameters()[1].symbol() == Some(flag)
            && owned.binding_kind(root) == Some(OwnershipBindingKind::Shared)
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Named(root)),
        "scope qualified exact original Borrow parameter and no owned obligation",
        sig,
    )?;
    need(
        typed.map_iterations().len() == usize::from(c.provider)
            && owned.map_iterations().len() == usize::from(c.provider),
        "scope actual qualified acquired provider count",
        owned.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let statement = UnitStatementId::new(unit, site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("scope actual qualified shared provider", site))?;
        let d = p.descriptor();
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, site.source)),
            "scope original qualified source reference",
        )?;
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == e(site.source)
                && typed.symbol_type(root) == Some(d.source_type())
                && reference.target() == &UnitReferenceTarget::Symbol(root)
                && p.source_access() == UnitIterationSourceAccess::Shared
                && matches!(p.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==root),
            "scope exact qualified original For/parameter source",
            p,
        )?;
        let read = i.read.as_ref().unwrap();
        let call = typed
            .call(e(read.id))
            .ok_or_else(|| failure("scope qualified actual read", read))?;
        let declaration = one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.name_span() == i.reader.as_ref().unwrap().name),
            "scope qualified actual read declaration",
        )?;
        need(
            call.target() == UnitCallTarget::Declaration(declaration.id())
                && call.arguments().len() == 2,
            "scope qualified exact selected reader",
            call,
        )?;
        for n in 0..2 {
            let binding = unit_symbol(names, typed, site.bindings[n])?;
            let l = &p.loans()[n];
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, read.arguments[n])),
                "scope qualified actual component reference",
            )?;
            need(
                d.bindings()[n] == binding
                    && reference.target() == &UnitReferenceTarget::Symbol(binding)
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && l.statement() == statement
                    && l.source() == e(site.source)
                    && l.binding() == binding
                    && Some(l.origin()) == p.source()
                    && call.arguments()[n].mode() == ParameterMode::Borrow
                    && call.arguments()[n].argument_index() == n
                    && call.arguments()[n].parameter_index() == n,
                "scope qualified original ordered Shared/Borrow component",
                l,
            )?;
            let loan = one(
                owned
                    .loans()
                    .iter()
                    .filter(|l| l.call() == e(read.id) && l.argument() == e(read.arguments[n])),
                "scope qualified original component call loan",
            )?;
            need(
                loan.kind() == LoanKind::Shared
                    && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                    && loan.end_span() == ast::span(parsed, read.id),
                "scope qualified original component call/return bound",
                loan,
            )?;
        }
        need(
            p.exits().len() == 2,
            "scope qualified complete normal exits",
            p.exits(),
        )?;
        for (kind, point, actions) in [
            (
                UnitIterationExitKind::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                vec![
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                UnitIterationExitKind::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![
                    UnitIterationCleanupAction::FinishProvider(statement),
                    UnitIterationCleanupAction::EndSource(statement),
                ],
            ),
        ] {
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "scope qualified actual exit",
            )?;
            need(
                exit.point() == point
                    && exit.actions() == actions
                    && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "scope qualified full ordered cleanup without source Drop",
                exit,
            )?;
        }
    }
    if let Some(observe) = &i.observe_call {
        let call = typed
            .call(e(observe.id))
            .ok_or_else(|| failure("scope qualified control observe", observe))?;
        let target = one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.name_span() == i.observe.as_ref().unwrap().name),
            "scope qualified observe target",
        )?;
        need(
            call.target() == UnitCallTarget::Declaration(target.id())
                && matches!(call.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow),
            "scope qualified actual no-For observe Borrow target",
            call,
        )?;
    }
    body::unit(input, names, typed, owned)
}
