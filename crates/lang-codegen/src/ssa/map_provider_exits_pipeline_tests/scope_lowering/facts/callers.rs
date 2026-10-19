//! The original entry owns both real make() results until their corresponding scan returns.
use super::*;
pub(super) fn single(
    input: Input<'_>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let (c, _, parsed, i) = input;
    let scanner = symbol(names, i.scan.name)?;
    for caller in &i.callers {
        let call = typed
            .call(caller.id)
            .ok_or_else(|| failure("scope actual entry scanner Call", caller))?;
        need(
            call.target() == CallableTarget::Source(scanner)
                && call.arguments().len() == caller.arguments.len(),
            "scope actual entry selects original scanner ABI",
            call,
        )?;
        if c.gate == 2 {
            need(
                call.arguments().is_empty(),
                "scope dead-tail scanner takes no hidden substitute",
                call,
            )?;
            continue;
        }
        let source = caller.arguments[0];
        let make = typed
            .call(source)
            .ok_or_else(|| failure("scope actual make result", source))?;
        need(
            make.target() == CallableTarget::Source(symbol(names, i.make.as_ref().unwrap().name)?)
                && make.arguments().is_empty()
                && make.result_source() == CallableResultSource::Owned
                && make.borrow_return().is_none(),
            "scope actual owned make result",
            make,
        )?;
        for (n, arg) in caller.arguments.iter().copied().enumerate() {
            let d = call.arguments()[n];
            need(
                d.argument_index() == n
                    && d.parameter_index() == n
                    && d.mode() == ParameterMode::Borrow
                    && d.category() == ExpressionCategory::Temporary
                    && typed.expression_type(arg) == Some(d.parameter_type()),
                "scope actual entry slot/mode/type/category",
                d,
            )?;
            let loan = one(
                owned
                    .loans()
                    .iter()
                    .filter(|l| l.call() == caller.id && l.argument() == arg),
                "scope actual entry argument loan",
            )?;
            let end = one(
                owned
                    .loan_ends()
                    .iter()
                    .filter(|e| e.call() == caller.id && e.argument() == arg),
                "scope original entry loan end",
            )?;
            need(
                loan.kind() == LoanKind::Shared
                    && loan.target() == &LoanTarget::Temporary(arg)
                    && loan.begin_span() == ast::span(parsed, arg)
                    && loan.end_span() == ast::span(parsed, caller.id)
                    && end.point() == LoanEndPoint::CallReturn(caller.id),
                "scope entry exact temporary loan/CallReturn",
                (loan, end),
            )?;
        }
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Temporary(source)),
            "scope caller unique original map Drop",
        )?;
        need(
            drop.point() == DropPoint::CallReturn(caller.id)
                && drop.value_origin() == ast::span(parsed, source),
            "scope map temporary remains caller-owned through actual scan",
            drop,
        )?;
    }
    Ok(())
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
