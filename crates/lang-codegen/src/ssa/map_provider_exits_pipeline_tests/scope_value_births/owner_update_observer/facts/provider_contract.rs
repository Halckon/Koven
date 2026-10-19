//! Actual typed reader delivery and both complete ordinary provider cleanup exits.
use super::*;
use lang_frontend::ownership_checking::{
    CompilationUnitOwnership, DropPoint, IterationCleanupAction as A, IterationExitKind as K,
    LoanKind, MapIterationComponent, OwnershipBindingKind, UnitDropPoint,
    UnitIterationCleanupAction as UA, UnitIterationExitKind as UK, UnitLoanTarget,
};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, ValidatedCompilationUnitNames},
    type_checking::{CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId},
};
pub(super) fn single(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &NameResolution,
    t: &TypedFile,
    o: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let site = super::super::ast::site(p, i)?;
    let plan = o
        .map_iteration(site.statement)
        .ok_or_else(|| failure("update actual Single delivery plan", site.statement))?;
    let d = plan.descriptor();
    let target = one(
        n.symbols()
            .iter()
            .filter(|s| {
                inside(i.reader.unwrap(), s.span())
                    && s.kind() == lang_frontend::name_resolution::SymbolKind::Function
            })
            .map(|s| s.id()),
        "update original actual reader symbol",
    )?;
    let call = t
        .call(site.read)
        .ok_or_else(|| failure("update actual Single reader Call", site.read))?;
    need(
        call.target() == CallableTarget::Source(target)
            && call.arguments().len() == 2
            && call.receiver().is_none(),
        "update reader actual selected two-Borrow ABI",
        call,
    )?;
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let symbol = super::local(n, site.bindings[index])?;
        let published = &plan.loans()[index];
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == site.read && l.argument() == site.arguments[index]),
            "update exact Single K/V call loan",
        )?;
        let end = one(
            o.loan_ends()
                .iter()
                .filter(|l| l.call() == site.read && l.argument() == site.arguments[index]),
            "update exact Single K/V CallReturn endpoint",
        )?;
        need(
            d.bindings()[index] == symbol
                && o.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
                && super::reference(p, n, site.arguments[index])? == symbol
                && published.statement() == site.statement
                && published.source() == site.source
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
                && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==symbol)
                && loan.begin_span() == span(p, site.arguments[index])
                && loan.end_span() == span(p, site.read)
                && end.point() == LoanEndPoint::CallReturn(site.read),
            "update complete Single K/V canonical source/reader/loan endpoint",
            (published, loan, end),
        )?;
    }
    let mut exhausted = vec![
        A::FinishProvider(site.statement),
        A::EndSource(site.statement),
    ];
    if c.code == 11 {
        exhausted.push(A::Drop(*one(
            o.drops()
                .iter()
                .filter(|d| d.point() == DropPoint::LoopExit(site.statement)),
            "update actual old source LoopExit drop fact",
        )?));
    }
    for (kind, point, actions) in [
        (
            K::Fallthrough,
            DropPoint::AfterStatement(site.body),
            vec![
                A::EndBinding {
                    statement: site.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: site.statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            K::Exhaustion,
            DropPoint::LoopExit(site.statement),
            exhausted,
        ),
    ] {
        let exit = one(
            plan.exits().iter().filter(|e| e.kind() == kind),
            "update exact complete Single normal exit",
        )?;
        need(
            plan.exits().len() == 2
                && exit.point() == point
                && exit.actions() == actions
                && o.cleanup_conditions().get(exit.condition())
                    == Some(&lang_frontend::ownership_checking::CleanupCondition::Always),
            "update exact reverse K/V End, provider/source End and old Drop order",
            exit,
        )?;
    }
    Ok(())
}
pub(super) fn unit(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let site = super::super::ast::site(p, i)?;
    let u = one(
        n.names().source_units().iter(),
        "update reader actual source unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(u, id);
    let statement = UnitStatementId::new(u, site.statement);
    let plan = o
        .map_iteration(statement)
        .ok_or_else(|| failure("update qualified reader provider plan", statement))?;
    let d = plan.descriptor();
    let target = one(
        n.names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.source_unit() == u && inside(i.reader.unwrap(), d.name_span()))
            .map(|d| d.id()),
        "update actual qualified reader declaration",
    )?;
    let call = t
        .call(e(site.read))
        .ok_or_else(|| failure("update qualified actual reader Call", e(site.read)))?;
    need(
        call.target() == UnitCallTarget::Declaration(target)
            && call.arguments().len() == 2
            && call.receiver().is_none(),
        "update qualified selected two-Borrow reader ABI",
        call,
    )?;
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let symbol = one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == span(p, site.arguments[index]))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "update actual qualified component use",
        )?;
        need(
            n.names().source_units().iter().any(|source| {
                source.source_unit() == u
                    && source
                        .resolution()
                        .symbols()
                        .iter()
                        .any(|s| s.id() == symbol.symbol() && s.span() == site.bindings[index])
            }),
            "update qualified component references its exact original binding",
            symbol,
        )?;
        let published = &plan.loans()[index];
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == e(site.read) && l.argument() == e(site.arguments[index])),
            "update qualified actual K/V call loan",
        )?;
        need(
            d.bindings()[index] == symbol
                && o.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
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
            "update qualified complete canonical K/V call delivery bounds",
            (published, loan),
        )?;
    }
    let mut exhausted = vec![UA::FinishProvider(statement), UA::EndSource(statement)];
    if c.code == 11 {
        exhausted.push(UA::Drop(*one(
            o.drops()
                .iter()
                .filter(|d| d.point() == UnitDropPoint::LoopExit(statement)),
            "update qualified actual old source LoopExit Drop",
        )?));
    }
    for (kind, point, actions) in [
        (
            UK::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(u, site.body)),
            vec![
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            UK::Exhaustion,
            UnitDropPoint::LoopExit(statement),
            exhausted,
        ),
    ] {
        let exit = one(
            plan.exits().iter().filter(|e| e.kind() == kind),
            "update exact qualified normal exit",
        )?;
        need(
            plan.exits().len() == 2
                && exit.point() == point
                && exit.actions() == actions
                && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "update qualified full reverse components/provider/source/old Drop cleanup",
            exit,
        )?;
    }
    Ok(())
}
