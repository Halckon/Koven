//! Full source-qualified component delivery and published provider cleanup.
use super::*;
use lang_frontend::ownership_checking::{
    LoanKind, MapIterationComponent, OwnershipBindingKind, UnitIterationCleanupAction as Action,
    UnitIterationExitKind,
};
pub(super) fn check(
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
    drop: UnitDropFact,
) -> Result<(), Failure> {
    let site = super::super::ast::site(p, i)?;
    let unit = one(
        n.names().source_units().iter(),
        "If provider original source unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let statement = UnitStatementId::new(unit, site.statement);
    let plan = o
        .map_iteration(statement)
        .ok_or_else(|| failure("If complete acquired provider plan", statement))?;
    let d = plan.descriptor();
    let target = one(
        n.names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.source_unit() == unit && inside(i.reader, d.name_span()))
            .map(|d| d.id()),
        "If actual reader declaration",
    )?;
    let call = t
        .call(e(site.read))
        .ok_or_else(|| failure("If actual typed reader Call", e(site.read)))?;
    need(
        call.target() == UnitCallTarget::Declaration(target)
            && call.arguments().len() == 2
            && call.receiver().is_none(),
        "If exact actual reader ABI",
        call,
    )?;
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let symbol = root(n, site.bindings[index])?;
        let reference = one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == span(p, site.arguments[index])),
            "If original component reader reference",
        )?;
        let published = &plan.loans()[index];
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == e(site.read) && l.argument() == e(site.arguments[index])),
            "If exact component synchronous loan",
        )?;
        need(
            d.bindings()[index] == symbol
                && o.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
                && reference.target() == &UnitReferenceTarget::Symbol(symbol)
                && published.statement() == statement
                && published.source() == e(site.source)
                && published.component() == component
                && published.binding() == symbol
                && published.kind() == LoanKind::Shared
                && Some(published.origin()) == plan.source()
                && call.arguments()[index].argument_index() == index
                && call.arguments()[index].parameter_index() == index
                && call.arguments()[index].mode() == ParameterMode::Borrow
                && call.arguments()[index].parameter_type()
                    == [d.key_type(), d.value_type()][index]
                && loan.kind() == LoanKind::Shared
                && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==symbol)
                && loan.begin_span() == span(p, site.arguments[index])
                && loan.end_span() == span(p, site.read),
            "If exact K/V canonical source/ordered reader delivery/loan bounds",
            (published, loan),
        )?;
    }
    let mut exhausted = vec![
        Action::FinishProvider(statement),
        Action::EndSource(statement),
    ];
    if matches!(drop.target(), UnitDropTarget::Temporary(_)) {
        exhausted.push(Action::Drop(drop));
    }
    for (kind, point, actions) in [
        (
            UnitIterationExitKind::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
            vec![
                Action::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                Action::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            UnitIterationExitKind::Exhaustion,
            UnitDropPoint::LoopExit(statement),
            exhausted,
        ),
    ] {
        let exit = one(
            plan.exits().iter().filter(|v| v.kind() == kind),
            "If exact normal provider exit",
        )?;
        need(
            plan.exits().len() == 2
                && exit.point() == point
                && exit.actions() == actions
                && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "If complete reverse components/providerEnd/ownerDrop cleanup",
            exit,
        )?;
    }
    Ok(())
}
