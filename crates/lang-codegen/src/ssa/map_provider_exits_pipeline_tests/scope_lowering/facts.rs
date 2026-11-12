//! Same-analysis public Borrow source facts, including legal unacquired dead tails.
mod body {
    //! The two body gates consume actual assignment and builtin String descriptors.
    use super::*;
    use lang_frontend::parser::{AssignmentOperator, BinaryOperator};
    fn assignment(
        parsed: &ParsedFile,
        id: lang_frontend::ast::ExpressionId,
    ) -> Result<
        (
            lang_frontend::ast::ExpressionId,
            lang_frontend::ast::ExpressionId,
            lang_frontend::ast::ExpressionId,
        ),
        Failure,
    > {
        let Expression::Assignment {
            target,
            value,
            operator: AssignmentOperator::Assign,
            ..
        } = parsed.ast().expressions().get(id).unwrap().payload()
        else {
            return Err(failure("scope actual ordinary assignment", id));
        };
        let Expression::Binary {
            left,
            right,
            operator: BinaryOperator::Add,
            ..
        } = parsed.ast().expressions().get(*value).unwrap().payload()
        else {
            return Err(failure("scope actual assigned Add", value));
        };
        Ok((*target, *value, *left)).and_then(|v| {
            need(
                matches!(
                    parsed.ast().expressions().get(*right).unwrap().payload(),
                    Expression::Literal(_)
                ),
                "scope actual increment literal",
                right,
            )?;
            Ok(v)
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
            "scope body original source unit",
        )?
        .source_unit();
        let e = |id| UnitExpressionId::new(unit, id);
        if let Some(id) = i.assignment {
            let (target, rhs, left) = assignment(parsed, id)?;
            let d = typed
                .assignment(e(id))
                .ok_or_else(|| failure("scope actual qualified assignment descriptor", id))?;
            need(
                d.expression() == e(id)
                    && d.target() == e(target)
                    && d.value() == e(rhs)
                    && d.operator() == AssignmentOperator::Assign
                    && d.falls_through()
                    && typed.expression_type(e(target)) == Some(d.target_type())
                    && typed.expression_type(e(left)) == Some(d.target_type())
                    && typed.expression_type(e(rhs)) == Some(d.target_type()),
                "scope qualified exact current assignment typed descriptor",
                d,
            )?;
            let a = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, target)),
                "scope qualified assigned target",
            )?;
            let b = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, left)),
                "scope qualified assigned scalar read",
            )?;
            need(
                a.target() == b.target(),
                "scope qualified assigned scalar root",
                (a, b),
            )?;
            let call = i.count_call.as_ref().unwrap();
            let descriptor = typed
                .call(e(call.id))
                .ok_or_else(|| failure("scope qualified count call", call))?;
            let callee = one(
                names.names().index().declarations().iter().filter(|d| {
                    d.name_span() == i.count.as_ref().unwrap().name && d.source_unit() == unit
                }),
                "scope qualified actual count target",
            )?;
            let arg = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, call.arguments[0])),
                "scope qualified current count reference",
            )?;
            need(
                descriptor.target() == UnitCallTarget::Declaration(callee.id())
                    && matches!(descriptor.arguments(),[a]if a.mode()==ParameterMode::Borrow&&a.argument_index()==0&&a.parameter_index()==0)
                    && arg.target() == a.target(),
                "scope qualified count uses latest assigned root and original Borrow callable",
                descriptor,
            )?;
        }
        if let Some(id) = i.clone {
            let d = typed
                .string_operation(e(id))
                .ok_or_else(|| failure("scope qualified original StringClone descriptor", id))?;
            let effect = owned
                .string_effect(e(id))
                .ok_or_else(|| failure("scope qualified actual StringClone effect", id))?;
            let root = unit_symbol(names, typed, i.cloned.unwrap())?;
            need(
                d.expression() == e(id)
                    && d.kind() == StringOperationKind::Clone
                    && d.receiver_mode() == ParameterMode::Borrow
                    && d.result_mode() == ParameterMode::Value
                    && d.receiver_type() == d.result_type()
                    && typed.expression_type(d.receiver()) == Some(d.receiver_type())
                    && typed.expression_type(e(id)) == Some(d.result_type())
                    && typed.symbol_type(root) == Some(d.result_type())
                    && effect.expression() == e(id)
                    && effect.receiver() == d.receiver()
                    && effect.kind() == d.kind()
                    && effect.result_type() == d.result_type()
                    && owned.formed_owned_locals().contains(&root),
                "scope qualified exact StringClone fresh original owner",
                (d, effect),
            )?;
            if c.provider {
                let site = i.site.as_ref().unwrap();
                let reference = one(
                    names
                        .names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == ast::span(parsed, d.receiver().expression())),
                    "scope qualified clone actual key reference",
                )?;
                need(
                    reference.target()
                        == &UnitReferenceTarget::Symbol(unit_symbol(names, typed, site.bindings[0])?),
                    "scope qualified clone borrows original map key",
                    reference,
                )?;
            }
            let print = i.clone_print.as_ref().unwrap();
            let drop = one(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Named(root)),
                "scope qualified unique cloned cleanup",
            )?;
            need(
                drop.point() == UnitDropPoint::CallReturn(e(print.id))
                    && drop.value_origin() == i.cloned.unwrap(),
                "scope qualified independent clone CallReturn cleanup",
                drop,
            )?;
            let l = one(
                owned
                    .loans()
                    .iter()
                    .filter(|l| l.call() == e(print.id) && l.argument() == e(print.arguments[0])),
                "scope qualified cloned print loan",
            )?;
            need(
                matches!(l.target(),UnitLoanTarget::Place(p)if p.is_root()&&p.root()==root)
                    && l.end_span() == ast::span(parsed, print.id),
                "scope qualified print exact clone loan/return bound",
                l,
            )?;
        }
        Ok(())
    }
}
mod callers {
    //! The original entry owns both real make() results until their corresponding scan returns.
    use super::*;
    pub(super) fn unit(
        input: Input<'_>,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let (c, _, parsed, i) = input;
        let unit = one(
            names.names().source_units().iter(),
            "scope caller original unit",
        )?
        .source_unit();
        let e = |id| UnitExpressionId::new(unit, id);
        let declaration = |marker| {
            one(
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.name_span() == marker && d.source_unit() == unit)
                    .map(|d| d.id()),
                "scope original qualified callable",
            )
        };
        for caller in &i.callers {
            let call = typed
                .call(e(caller.id))
                .ok_or_else(|| failure("scope qualified actual entry Call", caller))?;
            need(
                call.target() == UnitCallTarget::Declaration(declaration(i.scan.name)?)
                    && call.arguments().len() == caller.arguments.len(),
                "scope qualified exact scanner target",
                call,
            )?;
            if c.gate == 2 {
                need(
                    call.arguments().is_empty(),
                    "scope qualified dead-tail scanner has no arguments",
                    call,
                )?;
                continue;
            }
            let source = caller.arguments[0];
            let make = typed
                .call(e(source))
                .ok_or_else(|| failure("scope qualified actual make", source))?;
            need(
                make.target()
                    == UnitCallTarget::Declaration(declaration(i.make.as_ref().unwrap().name)?)
                    && make.arguments().is_empty()
                    && make.result_source() == CallableResultSource::Owned
                    && make.borrow_return().is_none(),
                "scope qualified owned caller map birth",
                make,
            )?;
            for (n, arg) in caller.arguments.iter().copied().enumerate() {
                let d = call.arguments()[n];
                let contract = one(
                    owned
                        .call_argument_contracts_for(e(caller.id))
                        .filter(|a| a.argument() == e(arg)),
                    "scope qualified exact argument ownership contract",
                )?;
                let loan = one(
                    owned
                        .loans()
                        .iter()
                        .filter(|l| l.call() == e(caller.id) && l.argument() == e(arg)),
                    "scope qualified original temporary loan",
                )?;
                need(
                    d.argument_index() == n
                        && d.parameter_index() == n
                        && d.mode() == ParameterMode::Borrow
                        && d.category() == ExpressionCategory::Temporary
                        && typed.expression_type(e(arg)) == Some(d.parameter_type())
                        && contract.parameter_index() == n
                        && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        && contract.parameter_type() == d.parameter_type()
                        && contract.category() == ExpressionCategory::Temporary
                        && loan.kind() == LoanKind::Shared
                        && loan.target() == &UnitLoanTarget::Temporary(e(arg))
                        && loan.begin_span() == ast::span(parsed, arg)
                        && loan.end_span() == ast::span(parsed, caller.id),
                    "scope qualified exact Borrow slot/contract/CallReturn bounds",
                    (d, contract, loan),
                )?;
            }
            let drop = one(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Temporary(e(source))),
                "scope qualified caller unique map Drop",
            )?;
            need(
                drop.point() == UnitDropPoint::CallReturn(e(caller.id))
                    && drop.value_origin() == ast::span(parsed, source),
                "scope qualified actual caller cleanup after scanner",
                drop,
            )?;
        }
        Ok(())
    }
}
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
