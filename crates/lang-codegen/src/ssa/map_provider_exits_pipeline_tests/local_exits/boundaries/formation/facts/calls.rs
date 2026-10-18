//! Same-analysis original argument slots, roots, modes and synchronous endpoints.
use super::*;
use lang_frontend::ownership_checking::{LoanEndPoint, LoanKind, UnitCallArgumentOwnershipKind};
fn arguments(parsed: &ParsedFile, id: ExpressionId) -> &[lang_frontend::parser::CallArgument] {
    let Expression::Call { arguments, .. } = parsed.ast().expressions().get(id).unwrap().payload()
    else {
        unreachable!()
    };
    arguments
}
fn selected(at: Span, calls: &[(Span, Span)]) -> Option<Span> {
    calls.iter().find_map(|(s, t)| (*s == at).then_some(*t))
}
pub(super) fn single(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    site: &Site,
    selected_calls: &[(Span, Span)],
    components: [Span; 2],
) -> Result<Vec<CallUse>, Failure> {
    let marker = |id| Ok(names.symbols()[root_ref(names, parsed, id)?.index()].span());
    let mut result = vec![];
    for id in calls(parsed, site.scan) {
        let Some(call) = typed.call(id) else { continue };
        let at = span(parsed, id);
        let target = selected(at, selected_calls);
        if target.is_none() && !call.prints_line() {
            continue;
        }
        need(
            call.receiver().is_none() && call.arguments().len() == arguments(parsed, id).len(),
            "formation actual original source/print arity",
            call,
        )?;
        let mut args = vec![];
        for (index, (argument, actual)) in arguments(parsed, id)
            .iter()
            .zip(call.arguments())
            .enumerate()
        {
            need(
                actual.argument_index() == index
                    && actual.parameter_index() == index
                    && typed.expression_type(argument.value) == Some(actual.parameter_type()),
                "formation source/print actual typed slot",
                (argument, actual),
            )?;
            let arg = span(parsed, argument.value);
            let root = if matches!(
                parsed
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .unwrap()
                    .payload(),
                Expression::Name
            ) {
                Some(marker(argument.value)?)
            } else {
                None
            };
            let borrow = actual.mode() == ParameterMode::Borrow;
            let loans = owned
                .loans()
                .iter()
                .filter(|l| l.call() == id && l.argument() == argument.value)
                .collect::<Vec<_>>();
            need(
                loans.len() <= 1,
                "formation unique original synchronous loan",
                &loans,
            )?;
            let endpoint = if let Some(loan) = loans.first() {
                need(borrow && loan.kind()==LoanKind::Shared && loan.begin_span()==arg && loan.end_span()==at
                    && root.is_none_or(|marker|matches!(loan.target(),LoanTarget::Place(place) if place.is_root() && names.symbols()[place.root().index()].span()==marker)),
                    "formation original argument root and synchronous span",loan)?;
                let end = one(
                    owned
                        .loan_ends()
                        .iter()
                        .filter(|e| e.call() == id && e.argument() == argument.value),
                    "formation actual original CallReturn loan end",
                )?;
                need(
                    end.point() == LoanEndPoint::CallReturn(id),
                    "formation exact synchronous CallReturn point",
                    end,
                )?;
                Some((loan.begin_span(), loan.end_span()))
            } else {
                None
            };
            need(
                !borrow || root.is_none() || endpoint.is_some(),
                "formation named Borrow has original endpoint",
                argument,
            )?;
            let birth = if let Some(root) = root.filter(|root| !components.contains(root)) {
                Some(birth_at(parsed, site.scan, root, at.start(), &marker)?.0)
            } else {
                None
            };
            args.push(Argument {
                at: arg,
                root,
                borrow,
                birth,
                endpoint,
            });
        }
        result.push(CallUse {
            at,
            target,
            print: call.prints_line(),
            args,
        });
    }
    Ok(result)
}
pub(super) fn unit(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    site: &Site,
    selected_calls: &[(Span, Span)],
    components: [Span; 2],
) -> Result<Vec<CallUse>, Failure> {
    let source = one(
        names.names().source_units().iter(),
        "formation use original unit",
    )?;
    let source_unit = source.source_unit();
    let expression = |id| UnitExpressionId::new(source_unit, id);
    let marker = |id| {
        let root = unit_ref(names, parsed, id)?;
        need(
            root.source_unit() == source_unit,
            "formation use original source unit",
            root,
        )?;
        Ok(source.resolution().symbols()[root.symbol().index()].span())
    };
    let mut result = vec![];
    for id in calls(parsed, site.scan) {
        let Some(call) = typed.call(expression(id)) else {
            continue;
        };
        let at = span(parsed, id);
        let target = selected(at, selected_calls);
        if target.is_none() && !call.prints_line() {
            continue;
        }
        need(
            call.receiver().is_none() && call.arguments().len() == arguments(parsed, id).len(),
            "formation qualified source/print actual arity",
            call,
        )?;
        let mut args = vec![];
        for (index, (argument, actual)) in arguments(parsed, id)
            .iter()
            .zip(call.arguments())
            .enumerate()
        {
            need(
                actual.argument_index() == index
                    && actual.parameter_index() == index
                    && typed.expression_type(expression(argument.value))
                        == Some(actual.parameter_type()),
                "formation qualified actual typed argument slot",
                (argument, actual),
            )?;
            let arg = span(parsed, argument.value);
            let root = if matches!(
                parsed
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .unwrap()
                    .payload(),
                Expression::Name
            ) {
                Some(marker(argument.value)?)
            } else {
                None
            };
            let borrow = actual.mode() == ParameterMode::Borrow;
            let contracts = owned
                .call_argument_contracts_for(expression(id))
                .filter(|c| c.argument() == expression(argument.value))
                .collect::<Vec<_>>();
            need(
                contracts.len() <= 1,
                "formation unique original qualified argument contract",
                &contracts,
            )?;
            let endpoint = if let Some(contract) = contracts.first() {
                need(
                    contract.parameter_index() == index
                        && contract.parameter_type() == actual.parameter_type()
                        && contract.argument_span() == arg
                        && contract.call_span() == at
                        && contract.kind()
                            == if borrow {
                                UnitCallArgumentOwnershipKind::SharedLoan
                            } else {
                                UnitCallArgumentOwnershipKind::Value
                            },
                    "formation qualified exact original Borrow/Value slot contract",
                    contract,
                )?;
                if borrow {
                    need(
                        contract.loan_begin_span() == arg,
                        "formation qualified synchronous Borrow start",
                        contract,
                    )?;
                    let loans = owned
                        .loans()
                        .iter()
                        .filter(|l| {
                            l.call() == expression(id) && l.argument() == expression(argument.value)
                        })
                        .collect::<Vec<_>>();
                    need(
                        loans.len() <= 1,
                        "formation qualified exact original loan set",
                        &loans,
                    )?;
                    if let Some(loan) = loans.first() {
                        need(loan.kind()==LoanKind::Shared && loan.begin_span()==arg && loan.end_span()==at
                        && root.is_none_or(|marker|matches!(loan.target(),UnitLoanTarget::Place(place) if place.is_root() && place.root().source_unit()==source_unit && source.resolution().symbols()[place.root().symbol().index()].span()==marker)),
                        "formation qualified original source root/end",loan)?;
                    }
                    Some((contract.loan_begin_span(), contract.call_span()))
                } else {
                    None
                }
            } else {
                None
            };
            need(
                !borrow || root.is_none() || endpoint.is_some(),
                "formation qualified named Borrow has synchronous contract",
                argument,
            )?;
            let birth = if let Some(root) = root.filter(|root| !components.contains(root)) {
                Some(birth_at(parsed, site.scan, root, at.start(), &marker)?.0)
            } else {
                None
            };
            args.push(Argument {
                at: arg,
                root,
                borrow,
                birth,
                endpoint,
            });
        }
        result.push(CallUse {
            at,
            target,
            print: call.prints_line(),
            args,
        });
    }
    Ok(result)
}
