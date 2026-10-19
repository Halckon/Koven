//! Same-analysis public Borrow source facts, including legal unacquired dead tails.
mod body;
mod callers;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::*,
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::*,
};
pub(super) type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);
fn symbol(names: &NameResolution, marker: Span) -> Result<SymbolId, Failure> {
    one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == marker && !s.is_synthetic()),
        "scope actual original symbol",
    )
    .map(|s| s.id())
}
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
pub(super) fn single(
    input: Input<'_>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let (c, _, parsed, i) = input;
    callers::single(input, names, typed, owned)?;
    if c.gate == 2 {
        need(
            typed.map_iterations().len() == usize::from(c.provider)
                && owned.map_iterations().is_empty()
                && owned.drops().is_empty()
                && typed.map_constructions().len() == 1
                && i.early_return.is_some(),
            "scope legal dead tail has typed syntax but no acquired plan/drop",
            (
                typed.map_iterations(),
                owned.map_iterations(),
                owned.drops(),
            ),
        )?;
        return Ok(());
    }
    let root = symbol(names, i.scan.parameters[0])?;
    let flag = symbol(names, i.scan.parameters[1])?;
    let sig = one(
        typed
            .callables()
            .iter()
            .filter(|s| s.symbol() == symbol(names, i.scan.name).unwrap()),
        "scope actual scanner signature",
    )?;
    need(
        sig.parameters().len() == 2
            && sig
                .parameters()
                .iter()
                .all(|p| p.mode == ParameterMode::Borrow)
            && sig.parameter_symbols() == [Some(root), Some(flag)]
            && typed.parameter_mode(root) == Some(ParameterMode::Borrow)
            && typed.parameter_mode(flag) == Some(ParameterMode::Borrow)
            && owned.binding_kind(root) == Some(OwnershipBindingKind::Shared)
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Named(root)),
        "scope exact Borrow parameter capabilities and no scanner-owned source cleanup",
        sig,
    )?;
    need(
        typed.map_iterations().len() == usize::from(c.provider)
            && owned.map_iterations().len() == usize::from(c.provider),
        "scope exact actual acquired Map set",
        owned.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("scope actual shared provider plan", site))?;
        let d = p.descriptor();
        if matches!(c.gate, 12 | 13 | 14 | 16) {
            for exit in p.exits() {
                println!(
                    "SCOPE A1 FE EXIT label={} kind={:?} condition={:?} node={:?}",
                    c.label,
                    exit.kind(),
                    exit.condition(),
                    owned.cleanup_conditions().get(exit.condition())
                );
            }
            for incoming in p.closure_phi_incomings() {
                let node = owned.cleanup_conditions().get(incoming.condition());
                println!(
                    "SCOPE A1 FE INCOMING label={} kind={:?} condition={:?} node={:?}",
                    c.label,
                    incoming.kind(),
                    incoming.condition(),
                    node
                );
                if let Some(CleanupCondition::Choice { selector, branches }) = node {
                    println!(
                        "SCOPE A1 FE SELECTION label={} selector={:?} children={:?}",
                        c.label,
                        owned.cleanup_conditions().selector(*selector),
                        branches
                            .iter()
                            .map(|id| (*id, owned.cleanup_conditions().get(*id)))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }

        let reference = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, site.source)),
            "scope actual source reference",
        )?;
        need(
            typed.map_iteration(site.statement) == Some(d)
                && d.source() == site.source
                && typed.symbol_type(root) == Some(d.source_type())
                && reference.target() == &ReferenceTarget::Symbol(root)
                && p.source_access() == MapIterationSourceAccess::Shared
                && matches!(p.source(),Some(LoanTarget::Place(path))if path.is_root()&&path.root()==root)
                && p.closure_phis().is_empty(),
            "scope actual For borrows original source parameter",
            p,
        )?;
        let read = i.read.as_ref().unwrap();
        let call = typed
            .call(read.id)
            .ok_or_else(|| failure("scope actual reader Call", read))?;
        need(
            call.target()
                == CallableTarget::Source(symbol(names, i.reader.as_ref().unwrap().name)?)
                && call.arguments().len() == 2,
            "scope actual selected reader",
            call,
        )?;
        for n in 0..2 {
            let binding = symbol(names, site.bindings[n])?;
            let l = &p.loans()[n];
            let reference = one(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, read.arguments[n])),
                "scope actual component use reference",
            )?;
            need(
                d.bindings()[n] == binding
                    && reference.target() == &ReferenceTarget::Symbol(binding)
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && l.statement() == site.statement
                    && l.source() == site.source
                    && l.binding() == binding
                    && Some(l.origin()) == p.source()
                    && l.kind() == LoanKind::Shared
                    && call.arguments()[n].mode() == ParameterMode::Borrow
                    && call.arguments()[n].argument_index() == n
                    && call.arguments()[n].parameter_index() == n,
                "scope exact ordered Shared component and Borrow consumer",
                l,
            )?;
            let loan = one(
                owned
                    .loans()
                    .iter()
                    .filter(|l| l.call() == read.id && l.argument() == read.arguments[n]),
                "scope actual reader component loan",
            )?;
            let end = one(
                owned
                    .loan_ends()
                    .iter()
                    .filter(|e| e.call() == read.id && e.argument() == read.arguments[n]),
                "scope actual reader loan endpoint",
            )?;
            need(
                loan.kind() == LoanKind::Shared
                    && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                    && end.point() == LoanEndPoint::CallReturn(read.id),
                "scope component original root and synchronous endpoint",
                (loan, end),
            )?;
        }
        need(
            p.exits().len() == 2 && p.closure_phi_incomings().len() == 3,
            "scope complete shared provider exits and incoming",
            p,
        )?;
        for (kind, point, actions) in [
            (
                IterationExitKind::Fallthrough,
                DropPoint::AfterStatement(site.body),
                vec![
                    IterationCleanupAction::EndBinding {
                        statement: site.statement,
                        symbol: d.value_binding(),
                    },
                    IterationCleanupAction::EndBinding {
                        statement: site.statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                IterationExitKind::Exhaustion,
                DropPoint::LoopExit(site.statement),
                vec![
                    IterationCleanupAction::FinishProvider(site.statement),
                    IterationCleanupAction::EndSource(site.statement),
                ],
            ),
        ] {
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "scope actual provider exit",
            )?;
            need(
                exit.point() == point
                    && exit.actions() == actions
                    && incoming_condition(input, owned.cleanup_conditions(), exit.condition())?
                    && owned
                        .cleanup_steps()
                        .iter()
                        .filter_map(|(at, a)| (*at == point).then_some(*a))
                        .collect::<Vec<_>>()
                        == actions,
                "scope full original ordered cleanup without source Drop",
                exit,
            )?;
        }
        for (kind, point, boundary) in [
            (
                IterationPhiIncomingKind::Entry,
                DropPoint::AfterExpression(site.source),
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Fallthrough,
                DropPoint::AfterStatement(site.body),
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Exhaustion,
                DropPoint::LoopExit(site.statement),
                IterationPhiBoundary::Exit,
            ),
        ] {
            let edge = one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|e| e.kind() == kind),
                "scope exact original incoming",
            )?;
            need(
                edge.point() == point
                    && edge.boundary() == boundary
                    && edge.bindings().is_empty()
                    && incoming_condition(input, owned.cleanup_conditions(), edge.condition())?,
                "scope full shared source incoming payload",
                edge,
            )?;
        }
    }
    if let Some(observe) = &i.observe_call {
        let call = typed
            .call(observe.id)
            .ok_or_else(|| failure("scope actual no-For observe", observe))?;
        need(
            call.target()
                == CallableTarget::Source(symbol(names, i.observe.as_ref().unwrap().name)?)
                && matches!(call.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow),
            "scope actual no-For observe target/Borrow ABI",
            call,
        )?;
    }
    body::single(input, names, typed, owned)
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

/// The original enclosing branch is saved once; all three provider boundaries
/// retain its exact selection rather than pretending it is unconditional.
fn incoming_condition(
    input: Input<'_>,
    conditions: &CleanupConditions,
    id: CleanupConditionId,
) -> Result<bool, Failure> {
    let (c, _, parsed, i) = input;
    if !matches!(c.gate, 12 | 14) {
        return Ok(conditions.get(id) == Some(&CleanupCondition::Always));
    }
    let site = i.site.as_ref().unwrap();
    let (control, node) = one(
        parsed.ast().expressions().iter().filter(|(_, n)| {
            ast::inside(i.scan.span, n.span())
                && ast::inside(n.span(), site.span)
                && matches!(
                    (c.gate, n.payload()),
                    (12, Expression::If { .. }) | (14, Expression::When { .. })
                )
        }),
        "scope actual enclosing provider branch",
    )?;
    let Some(CleanupCondition::Choice { selector, branches }) = conditions.get(id) else {
        return Err(failure(
            "scope actual enclosing branch saved Choice",
            conditions.get(id),
        ));
    };
    let selection = conditions
        .selector(*selector)
        .ok_or_else(|| failure("scope actual saved selector", selector))?;
    need(
        selection.control() == Some(control)
            && selection.origin() == node.span()
            && selection.branch_count() == 2
            && selection.selection()
                == if c.gate == 12 {
                    CleanupSelection::Branch
                } else {
                    CleanupSelection::WhenAlternative {
                        entry: 0,
                        alternative: 0,
                    }
                }
            && branches.len() == 2
            && conditions.get(branches[0]) == Some(&CleanupCondition::Always)
            && conditions.get(branches[1]) == Some(&CleanupCondition::Never),
        "scope exact actual enclosing true/matched path and no false provider",
        (selection, branches),
    )?;
    Ok(true)
}
