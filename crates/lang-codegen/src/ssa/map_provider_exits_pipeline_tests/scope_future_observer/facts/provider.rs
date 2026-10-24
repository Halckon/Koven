//! Both frontend entrypoints publish the exact original reader and complete normal exits.
use super::*;
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, MapIterationComponent, UnitDropPoint,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UK, UnitLoanTarget,
    },
    type_checking::{CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId},
};
pub(super) fn unit(
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let s = ast::site(p, i)?;
    let root = super::unit::symbol(n, i.source)?;
    let u = root.source_unit();
    let e = |id| UnitExpressionId::new(u, id);
    let statement = UnitStatementId::new(u, s.statement);
    let plan = o
        .map_iteration(statement)
        .ok_or_else(|| failure("D mandatory qualified full provider", statement))?;
    let d = plan.descriptor();
    let target = one(
        n.names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.source_unit() == u && d.name_span() == s.reader_name)
            .map(|d| d.id()),
        "D exact qualified reader declaration",
    )?;
    let call = t
        .call(e(s.read))
        .ok_or_else(|| failure("D qualified actual reader Call", e(s.read)))?;
    need(
        call.target() == UnitCallTarget::Declaration(target)
            && call.receiver().is_none()
            && call.arguments().len() == 2
            && t.types().get(call.return_type())
                == Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                    BuiltinType::Unit,
                )),
        "D qualified exact reader target and Unit/two-Borrow ABI",
        call,
    )?;
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = super::unit::symbol(n, s.bindings[index])?;
        let published = &plan.loans()[index];
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == e(s.read) && l.argument() == e(s.arguments[index])),
            "D qualified original reader component loan",
        )?;
        let ty = [d.key_type(), d.value_type()][index];
        need(
            d.bindings()[index] == binding
                && super::unit::reference(p, n, s.arguments[index])? == binding
                && o.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && published.statement() == statement
                && published.source() == e(s.source)
                && published.binding() == binding
                && published.component() == component
                && published.kind() == LoanKind::Shared
                && Some(published.origin()) == plan.source()
                && call.arguments()[index].argument_index() == index
                && call.arguments()[index].parameter_index() == index
                && call.arguments()[index].mode() == ParameterMode::Borrow
                && call.arguments()[index].parameter_type() == ty
                && t.types().get(ty)
                    == Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                        BuiltinType::String,
                    ))
                && loan.kind() == LoanKind::Shared
                && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                && loan.begin_span() == ast::span(p, s.arguments[index])
                && loan.end_span() == ast::span(p, s.read),
            "D qualified canonical source/components/exact slots/loan cleanup",
            (published, loan),
        )?;
    }
    for (kind, point, actions) in [
        (
            UK::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(u, s.body)),
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
            vec![UA::FinishProvider(statement), UA::EndSource(statement)],
        ),
    ] {
        let exit = one(
            plan.exits().iter().filter(|e| e.kind() == kind),
            "D exact qualified normal provider exit",
        )?;
        need(
            plan.exits().len() == 2
                && exit.point() == point
                && exit.actions() == actions
                && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "D qualified full reverse components/provider/source exit actions",
            exit,
        )?;
    }
    Ok(())
}
