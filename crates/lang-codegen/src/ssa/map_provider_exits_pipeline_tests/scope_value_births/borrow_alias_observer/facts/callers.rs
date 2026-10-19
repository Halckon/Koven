//! Original caller owns each make() result through the corresponding scanner invocation.
use super::*;
fn calls(
    sources: &SourceMap,
    p: &ParsedFile,
    i: &Input,
) -> Result<
    Vec<(
        lang_frontend::ast::ExpressionId,
        Vec<lang_frontend::ast::ExpressionId>,
    )>,
    Failure,
> {
    let mut out = p
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, n)| match n.payload() {
            Expression::Call {
                callee, arguments, ..
            } if inside(i.entry, n.span()) && sources.slice(span(p, *callee)) == Ok("scan") => {
                Some((id, arguments.iter().map(|a| a.value).collect::<Vec<_>>()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    out.sort_by_key(|(id, _)| span(p, *id).start());
    need(out.len() == 2, "alias original two entry invocations", &out)?;
    Ok(out)
}
pub(super) fn single(
    sources: &SourceMap,
    p: &ParsedFile,
    i: &Input,
    n: &NameResolution,
    t: &TypedFile,
    o: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let make =
        super::super::super::super::temporary_arguments::ast::function(sources, p, "make", None)?;
    for ((id, args), flag) in calls(sources, p, i)?.into_iter().zip(["true", "false"]) {
        need(
            args.len() == 2 && sources.slice(span(p, args[1])) == Ok(flag),
            "alias original true/false entry operands",
            &args,
        )?;
        let call = t
            .call(id)
            .ok_or_else(|| failure("alias actual scanner entry Call", id))?;
        let birth = t
            .call(args[0])
            .ok_or_else(|| failure("alias actual caller make birth", args[0]))?;
        need(
            call.target() == CallableTarget::Source(symbol(n, i.scan_name)?)
                && call.arguments().len() == 2
                && birth.target() == CallableTarget::Source(symbol(n, make.name)?)
                && birth.arguments().is_empty()
                && birth.result_source() == CallableResultSource::Owned
                && birth.borrow_return().is_none(),
            "alias actual caller selected scanner and concrete owned make",
            (call, birth),
        )?;
        for (index, arg) in args.iter().copied().enumerate() {
            let d = call.arguments()[index];
            let loan = one(
                o.loans()
                    .iter()
                    .filter(|l| l.call() == id && l.argument() == arg),
                "alias actual caller temporary loan",
            )?;
            let end = one(
                o.loan_ends()
                    .iter()
                    .filter(|l| l.call() == id && l.argument() == arg),
                "alias actual caller loan endpoint",
            )?;
            need(
                d.argument_index() == index
                    && d.parameter_index() == index
                    && d.mode() == ParameterMode::Borrow
                    && d.category() == ExpressionCategory::Temporary
                    && t.expression_type(arg) == Some(d.parameter_type())
                    && loan.kind() == LoanKind::Shared
                    && loan.target() == &LoanTarget::Temporary(arg)
                    && loan.begin_span() == span(p, arg)
                    && loan.end_span() == span(p, id)
                    && end.point() == LoanEndPoint::CallReturn(id),
                "alias exact original caller slot/mode/root/CallReturn",
                (d, loan, end),
            )?;
        }
        let drop = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Temporary(args[0])),
            "alias unique actual caller Map cleanup",
        )?;
        need(
            drop.point() == DropPoint::CallReturn(id) && drop.value_origin() == span(p, args[0]),
            "alias caller Map lives until actual scanner returns",
            drop,
        )?;
    }
    Ok(())
}
pub(super) fn unit(
    sources: &SourceMap,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let u = one(
        n.names().source_units().iter(),
        "alias caller original unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(u, id);
    let decl = |marker| {
        one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == u && d.name_span() == marker)
                .map(|d| d.id()),
            "alias qualified caller declaration",
        )
    };
    let make =
        super::super::super::super::temporary_arguments::ast::function(sources, p, "make", None)?;
    for ((id, args), flag) in calls(sources, p, i)?.into_iter().zip(["true", "false"]) {
        need(
            args.len() == 2 && sources.slice(span(p, args[1])) == Ok(flag),
            "alias qualified original true/false operands",
            &args,
        )?;
        let call = t
            .call(e(id))
            .ok_or_else(|| failure("alias qualified actual scanner Call", id))?;
        let birth = t
            .call(e(args[0]))
            .ok_or_else(|| failure("alias qualified caller owned make", args[0]))?;
        need(
            call.target() == UnitCallTarget::Declaration(decl(i.scan_name)?)
                && call.arguments().len() == 2
                && birth.target() == UnitCallTarget::Declaration(decl(make.name)?)
                && birth.arguments().is_empty()
                && birth.result_source() == CallableResultSource::Owned
                && birth.borrow_return().is_none(),
            "alias qualified original caller target/birth",
            (call, birth),
        )?;
        for (index, arg) in args.iter().copied().enumerate() {
            let d = call.arguments()[index];
            let loan = one(
                o.loans()
                    .iter()
                    .filter(|l| l.call() == e(id) && l.argument() == e(arg)),
                "alias qualified actual caller loan",
            )?;
            let contract = one(
                o.call_argument_contracts_for(e(id))
                    .filter(|a| a.argument() == e(arg)),
                "alias qualified actual caller argument contract",
            )?;
            need(
                d.argument_index() == index
                    && d.parameter_index() == index
                    && d.mode() == ParameterMode::Borrow
                    && d.category() == ExpressionCategory::Temporary
                    && t.expression_type(e(arg)) == Some(d.parameter_type())
                    && loan.kind() == LoanKind::Shared
                    && loan.target() == &UnitLoanTarget::Temporary(e(arg))
                    && loan.begin_span() == span(p, arg)
                    && loan.end_span() == span(p, id)
                    && contract.parameter_index() == index
                    && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                    && contract.parameter_type() == d.parameter_type()
                    && contract.category() == ExpressionCategory::Temporary,
                "alias qualified caller exact slot/root/contract/return bounds",
                (d, loan, contract),
            )?;
        }
        let drop = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(e(args[0]))),
            "alias qualified caller sole actual Map cleanup",
        )?;
        need(
            drop.point() == UnitDropPoint::CallReturn(e(id))
                && drop.value_origin() == span(p, args[0]),
            "alias qualified caller current Map ends after scanner",
            drop,
        )?;
    }
    Ok(())
}
