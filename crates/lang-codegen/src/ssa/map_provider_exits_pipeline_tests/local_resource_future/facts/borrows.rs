//! Real future caller loans end normally while the Resource owner remains live.
use super::*;
use lang_frontend::{
    parser::{Item, NameMarker},
    type_checking::{TypeId, UnitTypeId},
};
fn reader(sources: &SourceMap, parsed: &ParsedFile) -> Result<(Span, Span), Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            match n.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } if sources.slice(*name) == Ok("readMap") && parameters.len() == 1 => {
                    Some((*name, parameters[0].span))
                }
                _ => None,
            }
        }),
        "Resource future original readMap root/parameter",
    )
}
pub(super) fn single(
    input: (&Case, &SourceMap, &ParsedFile, &Scanner),
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    source: (SymbolId, TypeId),
) -> Result<(), Failure> {
    let (case, sources, parsed, site) = input;
    let (root, ty) = source;
    let name = reader(sources, parsed)?.0;
    let target = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == name && s.kind() == SymbolKind::Function),
        "Resource future actual reader symbol",
    )?;
    let calls = typed
        .calls()
        .iter()
        .filter(|c| {
            c.target() == CallableTarget::Source(target.id())
                && ast::span(parsed, c.expression()).is_ok_and(|s| ast::within(site.span, s))
        })
        .collect::<Vec<_>>();
    let sizes = typed
        .map_sizes()
        .iter()
        .filter(|s| ast::span(parsed, s.expression()).is_ok_and(|s| ast::within(site.span, s)))
        .collect::<Vec<_>>();
    need(
        calls.len() == if case.form == 0 { 2 } else { 1 }
            && sizes.len() == usize::from(case.form == 0),
        "Resource future exact actual read set",
        (&calls, &sizes),
    )?;
    let mut uses = Vec::new();
    for call in calls {
        let arg = ast::argument(parsed, call.expression())?;
        need(
            reference(parsed, names, arg)? == root
                && matches!(call.arguments(),[a] if a.mode()==ParameterMode::Borrow && a.argument_index()==0 && a.parameter_index()==0 && a.parameter_type()==ty),
            "Resource future real Borrow caller/root",
            call,
        )?;
        uses.push((call.expression(), arg));
    }
    for size in sizes {
        need(
            size.map_type() == ty && reference(parsed, names, size.receiver())? == root,
            "Resource future exact size receiver/type",
            size,
        )?;
        uses.push((size.expression(), size.receiver()));
    }
    let loans = owned
        .loans()
        .iter()
        .filter(|l| matches!(l.target(),LoanTarget::Place(p) if p.is_root() && p.root()==root))
        .collect::<Vec<_>>();
    need(
        loans.len() == uses.len(),
        "Resource future complete original source call loans",
        &loans,
    )?;
    for (call, arg) in uses {
        let loan = one(
            loans
                .iter()
                .copied()
                .filter(|l| l.call() == call && l.argument() == arg),
            "Resource future actual loan",
        )?;
        need(
            loan.kind() == LoanKind::Shared
                && loan.begin_span() == ast::span(parsed, arg)?
                && loan.end_span() == ast::span(parsed, call)?,
            "Resource future original loan begin/end",
            loan,
        )?;
        let end = one(
            owned.loan_ends().iter().filter(|e| {
                e.call() == call
                    && e.argument() == arg
                    && e.point() == LoanEndPoint::CallReturn(call)
            }),
            "Resource future same original loan ends at actual CallReturn",
        )?;
        let steps = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, a)| (*at == DropPoint::CallReturn(call)).then_some(*a))
            .collect::<Vec<_>>();
        need(
            steps == [A::EndCallLoan(*end)],
            "Resource future call End restores source without Pure ASAP Drop",
            steps,
        )?;
    }
    Ok(())
}
pub(super) fn unit(
    input: (&Case, &SourceMap, &ParsedFile, &Scanner),
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    source: (UnitSymbolId, UnitTypeId),
) -> Result<(), Failure> {
    let (case, sources, parsed, site) = input;
    let (root, ty) = source;
    let unit = root.source_unit();
    let (name, parameter) = reader(sources, parsed)?;
    let target = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| d.callable().is_some_and(|c| c.name_span() == name)),
        "Resource future qualified reader declaration",
    )?;
    let calls = typed
        .calls()
        .iter()
        .filter(|c| {
            c.target() == UnitCallTarget::Declaration(target.declaration())
                && ast::span(parsed, c.expression().expression())
                    .is_ok_and(|s| ast::within(site.span, s))
        })
        .collect::<Vec<_>>();
    let sizes = typed
        .map_sizes()
        .iter()
        .filter(|s| {
            s.expression().source_unit() == unit
                && ast::span(parsed, s.expression().expression())
                    .is_ok_and(|s| ast::within(site.span, s))
        })
        .collect::<Vec<_>>();
    need(
        calls.len() == if case.form == 0 { 2 } else { 1 }
            && sizes.len() == usize::from(case.form == 0),
        "Resource future qualified actual read set",
        (&calls, &sizes),
    )?;
    let mut uses = Vec::new();
    for call in calls {
        let arg =
            UnitExpressionId::new(unit, ast::argument(parsed, call.expression().expression())?);
        let contracts = owned
            .call_argument_contracts_for(call.expression())
            .collect::<Vec<_>>();
        need(
            qualified(parsed, names, arg.expression())? == root
                && matches!(call.arguments(),[a] if a.mode()==ParameterMode::Borrow && a.argument_index()==0 && a.parameter_index()==0 && a.parameter_type()==ty)
                && matches!(contracts.as_slice(),[c] if c.argument()==arg && c.parameter_index()==0 && c.kind()==UnitCallArgumentOwnershipKind::SharedLoan)
                && !owned
                    .value_deliveries()
                    .iter()
                    .any(|d| d.call() == call.expression()),
            "Resource future qualified true Shared caller contract without Move",
            (call, contracts),
        )?;
        uses.push((call.expression(), arg, Some(parameter)));
    }
    for size in sizes {
        need(
            size.map_type() == ty
                && qualified(parsed, names, size.receiver().expression())? == root,
            "Resource future qualified true size",
            size,
        )?;
        uses.push((size.expression(), size.receiver(), None));
    }
    let loans = owned
        .loans()
        .iter()
        .filter(|l| matches!(l.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==root))
        .collect::<Vec<_>>();
    need(
        loans.len() == uses.len(),
        "Resource future qualified complete actual loan set",
        &loans,
    )?;
    for (call, arg, parameter) in uses {
        let loan = one(
            loans
                .iter()
                .copied()
                .filter(|l| l.call() == call && l.argument() == arg),
            "Resource future qualified actual loan",
        )?;
        need(
            loan.kind() == LoanKind::Shared
                && loan.begin_span() == ast::span(parsed, arg.expression())?
                && loan.end_span() == ast::span(parsed, call.expression())?
                && loan.parameter_span() == parameter,
            "Resource future qualified original CallReturn span/parameter",
            loan,
        )?;
    }
    Ok(())
}
