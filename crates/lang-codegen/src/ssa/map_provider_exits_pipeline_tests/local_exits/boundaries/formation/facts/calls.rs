//! Same-analysis original argument slots, roots, modes and synchronous endpoints.
use super::*;
use lang_frontend::ownership_checking::{LoanKind, UnitCallArgumentOwnershipKind};
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

pub(super) fn unit(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    (site, selected_calls, components): (&Site, &[(Span, Span)], [Span; 2]),
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
            let literal = literal(sources, parsed, argument.value)?;
            let literal_drop = if literal.is_some() {
                need(
                    typed
                        .expression_type(expression(argument.value))
                        .is_some_and(|ty| {
                            typed.types().get(ty)
                                == Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                                    lang_frontend::type_checking::BuiltinType::String,
                                ))
                        })
                        && typed.expression_category(expression(argument.value))
                            == Some(lang_frontend::type_checking::ExpressionCategory::Temporary),
                    "formation qualified original literal String/Temporary",
                    argument,
                )?;
                let loan = one(
                    owned.loans().iter().filter(|loan| {
                        loan.call() == expression(id)
                            && loan.argument() == expression(argument.value)
                    }),
                    "formation qualified literal actual unique temporary loan",
                )?;
                need(
                    root.is_none()
                        && borrow
                        && endpoint == Some((arg, at))
                        && loan.kind() == LoanKind::Shared
                        && loan.target() == &UnitLoanTarget::Temporary(expression(argument.value))
                        && loan.begin_span() == arg
                        && loan.end_span() == at,
                    "formation qualified literal exact Temporary expression and Shared loan",
                    loan,
                )?;
                let drop = one(
                    owned.drops().iter().filter(|drop| {
                        drop.target() == UnitDropTarget::Temporary(expression(argument.value))
                    }),
                    "formation qualified literal actual unique Temporary Drop fact",
                )?;
                need(
                    drop.point() == UnitDropPoint::CallReturn(expression(id))
                        && drop.value_origin() == arg,
                    "formation qualified literal original CallReturn Drop and origin",
                    drop,
                )?;
                Some((at, drop.value_origin()))
            } else {
                None
            };
            args.push(Argument {
                at: arg,
                root,
                borrow,
                birth,
                endpoint,
                literal,
                literal_drop,
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

// All frozen literal-print witnesses use ordinary, non-interpolated text with
// no escapes. Prove that identity before recording decoded UTF-8 bytes; this
// does not borrow the lowering decoder as an oracle.
fn literal(
    sources: &SourceMap,
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<Option<Vec<u8>>, Failure> {
    let node = parsed.ast().expressions().get(id).unwrap();
    let Expression::String { parts } = node.payload() else {
        return Ok(None);
    };
    let mut bytes = vec![];
    for part in parts {
        let lang_frontend::parser::StringPart::Text(at) = part else {
            return Err(failure(
                "formation fixed literal has no interpolation",
                part,
            ));
        };
        let text = sources
            .slice(*at)
            .map_err(|e| failure("formation original literal text bytes", e))?;
        need(
            !text.contains('\\'),
            "formation literal has identity UTF-8 decoding without escapes",
            at,
        )?;
        bytes.extend_from_slice(text.as_bytes());
    }
    let source = sources
        .slice(node.span())
        .map_err(|e| failure("formation original quoted literal bytes", e))?;
    need(
        source
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .is_some_and(|text| text.as_bytes() == bytes),
        "formation decoded bytes exactly cover original quoted String AST",
        node.span(),
    )?;
    Ok(Some(bytes))
}
