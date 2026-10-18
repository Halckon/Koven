//! Actual synchronous source loans and their original CallReturn endpoints.
use super::*;
use lang_frontend::{
    parser::{Item, NameMarker},
    type_checking::{TypeId, UnitTypeId},
};
fn reader(sources: &SourceMap, parsed: &ParsedFile) -> Result<(Span, Span), Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Function {
                name: NameMarker::Present(name),
                parameters,
                ..
            } = n.payload()
            else {
                return None;
            };
            (sources.slice(*name) == Ok("readMap") && parameters.len() == 1)
                .then(|| (*name, parameters[0].span))
        }),
        "local future original readMap declaration/parameter",
    )
}
pub(super) fn single(
    input: (&Case, &SourceMap, &ParsedFile, &Scanner),
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    source: (SymbolId, TypeId),
    drop: &DropFact,
) -> Result<(), Failure> {
    let (case, sources, parsed, site) = input;
    let (root, ty) = source;
    let read = reader(sources, parsed)?.0;
    let read = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == read && s.kind() == SymbolKind::Function),
        "local future actual readMap symbol",
    )?;
    let calls = typed
        .calls()
        .iter()
        .filter(|c| {
            c.target() == CallableTarget::Source(read.id())
                && ast::span(parsed, c.expression()).is_ok_and(|s| ast::within(site.span, s))
        })
        .collect::<Vec<_>>();
    let sizes = typed
        .map_sizes()
        .iter()
        .filter(|s| {
            ast::span(parsed, s.expression()).is_ok_and(|span| ast::within(site.span, span))
        })
        .collect::<Vec<_>>();
    need(
        calls.len() == if case.form == 0 { 2 } else { 1 }
            && sizes.len() == usize::from(case.form != 2),
        "local future all actual future source reads",
        (&calls, &sizes),
    )?;
    let mut uses = Vec::new();
    for call in calls {
        let arg = ast::argument(parsed, call.expression())?;
        need(
            reference(parsed, names, arg)? == root
                && matches!(call.arguments(),[a] if a.mode()==ParameterMode::Borrow && a.argument_index()==0 && a.parameter_index()==0 && a.parameter_type()==ty),
            "local future actual Borrow call original root/mode",
            call,
        )?;
        uses.push((call.expression(), arg));
    }
    for size in sizes {
        need(
            size.map_type() == ty && reference(parsed, names, size.receiver())? == root,
            "local future actual size original root/type",
            size,
        )?;
        uses.push((size.expression(), size.receiver()));
    }
    let last = names
        .references()
        .iter()
        .filter(|r| {
            r.target() == &ReferenceTarget::Symbol(root) && ast::within(site.span, r.span())
        })
        .map(|r| r.span())
        .max_by_key(|s| (s.start(), s.end()))
        .ok_or_else(|| failure("local future true last root reference", root))?;
    let final_call = one(
        uses.iter().filter_map(|(call, arg)| {
            ast::span(parsed, *arg)
                .is_ok_and(|s| s == last)
                .then_some(*call)
        }),
        "local future actual final source read",
    )?;
    need(
        drop.point() == DropPoint::CallReturn(final_call)
            && if case.form == 0 {
                typed.map_size(final_call).is_some()
            } else {
                typed.call(final_call).is_some()
            },
        "local future Pure Drop at exact final size/Borrow return",
        drop,
    )?;
    let root_loans = owned
        .loans()
        .iter()
        .filter(|l| matches!(l.target(),LoanTarget::Place(p) if p.is_root() && p.root()==root))
        .collect::<Vec<_>>();
    need(
        root_loans.len() == uses.len(),
        "local future full source call-loan set",
        &root_loans,
    )?;
    for (call, arg) in uses {
        let loan = one(
            root_loans
                .iter()
                .copied()
                .filter(|l| l.call() == call && l.argument() == arg),
            "local future exact original call loan",
        )?;
        need(
            loan.kind() == LoanKind::Shared
                && loan.begin_span() == ast::span(parsed, arg)?
                && loan.end_span() == ast::span(parsed, call)?,
            "local future real call/argument loan begin/end",
            loan,
        )?;
        let end = one(
            owned
                .loan_ends()
                .iter()
                .filter(|e| e.call() == call && e.argument() == arg),
            "local future original loan-end fact",
        )?;
        need(
            end.point() == LoanEndPoint::CallReturn(call),
            "local future same loan ends at actual synchronous return",
            end,
        )?;
        let mut expected = vec![A::EndCallLoan(*end)];
        if call == final_call {
            expected.push(A::Drop(*drop));
        }
        let full = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, a)| (*at == DropPoint::CallReturn(call)).then_some(*a))
            .collect::<Vec<_>>();
        need(
            full == expected,
            "local future complete loan-end then unique ASAP Drop",
            (full, expected),
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
    drop: &UnitDropFact,
) -> Result<(), Failure> {
    let (case, sources, parsed, site) = input;
    let (root, ty) = source;
    let unit = root.source_unit();
    let (read_name, parameter) = reader(sources, parsed)?;
    let read = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| d.callable().is_some_and(|c| c.name_span() == read_name)),
        "local future qualified original readMap",
    )?;
    let calls = typed
        .calls()
        .iter()
        .filter(|c| {
            c.target() == UnitCallTarget::Declaration(read.declaration())
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
                    .is_ok_and(|span| ast::within(site.span, span))
        })
        .collect::<Vec<_>>();
    need(
        calls.len() == if case.form == 0 { 2 } else { 1 }
            && sizes.len() == usize::from(case.form != 2),
        "local future qualified actual read set",
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
            "local future qualified real caller Borrow contract without Move",
            (call, contracts),
        )?;
        uses.push((call.expression(), arg, Some(parameter)));
    }
    for size in sizes {
        need(
            size.map_type() == ty
                && qualified(parsed, names, size.receiver().expression())? == root,
            "local future qualified original size root/type",
            size,
        )?;
        uses.push((size.expression(), size.receiver(), None));
    }
    let last = names
        .names()
        .references()
        .iter()
        .filter(|r| {
            r.target() == &UnitReferenceTarget::Symbol(root) && ast::within(site.span, r.span())
        })
        .map(|r| r.span())
        .max_by_key(|s| (s.start(), s.end()))
        .ok_or_else(|| failure("local future qualified actual last root reference", root))?;
    let final_call = one(
        uses.iter().filter_map(|(call, arg, _)| {
            ast::span(parsed, arg.expression())
                .is_ok_and(|s| s == last)
                .then_some(*call)
        }),
        "local future qualified actual final read",
    )?;
    need(
        drop.point() == UnitDropPoint::CallReturn(final_call)
            && if case.form == 0 {
                typed.map_size(final_call).is_some()
            } else {
                typed.call(final_call).is_some()
            },
        "local future qualified Drop at true final read",
        drop,
    )?;
    let root_loans = owned
        .loans()
        .iter()
        .filter(|l| matches!(l.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==root))
        .collect::<Vec<_>>();
    need(
        root_loans.len() == uses.len(),
        "local future qualified full real loan set",
        &root_loans,
    )?;
    for (call, arg, parameter) in uses {
        let loan = one(
            root_loans
                .iter()
                .copied()
                .filter(|l| l.call() == call && l.argument() == arg),
            "local future qualified exact original call loan",
        )?;
        need(
            loan.kind() == LoanKind::Shared
                && loan.begin_span() == ast::span(parsed, arg.expression())?
                && loan.end_span() == ast::span(parsed, call.expression())?
                && loan.parameter_span() == parameter,
            "local future qualified real loan end retains actual call span/parameter",
            loan,
        )?;
    }
    Ok(())
}
