//! Public frontend observations from exactly the analysis used for lowering.
mod calls {
    //! Same-analysis original argument slots, roots, modes and synchronous endpoints.
    use super::*;
    use lang_frontend::ownership_checking::{LoanKind, UnitCallArgumentOwnershipKind};
    fn arguments(parsed: &ParsedFile, id: ExpressionId) -> &[lang_frontend::parser::CallArgument] {
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(id).unwrap().payload()
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
                                l.call() == expression(id)
                                    && l.argument() == expression(argument.value)
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
                                == Some(
                                    lang_frontend::type_checking::ExpressionCategory::Temporary,
                                ),
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
                            && loan.target()
                                == &UnitLoanTarget::Temporary(expression(argument.value))
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
}
use super::*;
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    type_checking::{
        CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId,
    },
};

fn unit_point(parsed: &ParsedFile, point: UnitDropPoint) -> Result<Point, Failure> {
    Ok(match point {
        UnitDropPoint::AfterExpression(id) => Point::Expression(span(parsed, id.expression())),
        UnitDropPoint::AfterStatement(id) => Point::Statement(
            parsed
                .ast()
                .statements()
                .get(id.statement())
                .unwrap()
                .span(),
        ),
        UnitDropPoint::CallReturn(id) => Point::Call(span(parsed, id.expression())),
        UnitDropPoint::LoopExit(id) => Point::Loop(
            parsed
                .ast()
                .statements()
                .get(id.statement())
                .unwrap()
                .span(),
        ),
        UnitDropPoint::FunctionEntry(id) => {
            Point::Entry(parsed.ast().items().get(id.item()).unwrap().span())
        }
        other => {
            return Err(failure(
                "formation actual qualified named Drop point",
                other,
            ));
        }
    })
}
fn calls(parsed: &ParsedFile, scope: Span) -> impl Iterator<Item = ExpressionId> + '_ {
    parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(move |(id, n)| {
            (inside(n.span(), scope) && matches!(n.payload(), Expression::Call { .. }))
                .then_some(id)
        })
}

pub(in super::super::super::super) fn unit(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let root = |id| {
        one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span(parsed, id))
                .filter_map(|r| {
                    if let UnitReferenceTarget::Symbol(s) = r.target() {
                        Some(*s)
                    } else {
                        None
                    }
                }),
            "formation original Unit reference",
        )
    };
    let symbol_span = |s: lang_frontend::name_resolution::UnitSymbolId| {
        one(
            names
                .names()
                .source_units()
                .iter()
                .filter(|u| u.source_unit() == s.source_unit())
                .filter_map(|u| {
                    u.resolution()
                        .symbols()
                        .get(s.symbol().index())
                        .map(|v| v.span())
                }),
            "formation actual qualified declaration marker",
        )
    };
    let site = site(sources, parsed, |id| symbol_span(root(id)?))?;
    let source_root = root(site.source)?;
    let unit = source_root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let statement = UnitStatementId::new(unit, site.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("formation original Unit plan", statement))?;
    need(
        typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && typed.map_iteration(statement) == Some(p.descriptor())
            && p.descriptor().source() == expression(site.source)
            && p.source_access() == UnitIterationSourceAccess::Owned
            && matches!(p.source(),Some(UnitLoanTarget::Place(place)) if place.is_root() && place.root()==source_root),
        "formation exact qualified root and same-analysis descriptor",
        p,
    )?;
    for (index, loan) in p.loans().iter().enumerate() {
        need(
            loan.statement() == statement
                && loan.source() == expression(site.source)
                && loan.binding() == p.descriptor().bindings()[index]
                && Some(loan.origin()) == p.source(),
            "formation exact qualified K/V identities",
            loan,
        )?;
    }
    for &(id, marker) in &site.aliases {
        need(
            symbol_span(root(id)?)? == marker
                && typed.expression_type(expression(id)) == Some(p.descriptor().source_type()),
            "formation original qualified Move/root/type",
            id,
        )?;
    }
    if let Some(id) = site.put {
        let put = typed
            .map_put(expression(id))
            .ok_or_else(|| failure("formation actual qualified put descriptor", id))?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            put.expression() == expression(id)
                && root(put.receiver().expression())? == source_root
                && matches!(arguments.as_slice(),[k,v] if put.key()==expression(k.value) && put.value()==expression(v.value))
                && put.key_type() == p.descriptor().key_type()
                && put.value_type() == p.descriptor().value_type(),
            "formation qualified put exact root/ordered key/value types",
            put,
        )?;
    }
    for (_, node) in parsed.ast().expressions().iter() {
        if let Expression::If { condition, .. } = node.payload()
            && inside(node.span(), site.for_span)
        {
            need(
                typed
                    .expression_type(expression(*condition))
                    .is_some_and(|ty| {
                        typed.types().get(ty)
                            == Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                                lang_frontend::type_checking::BuiltinType::Boolean,
                            ))
                    }),
                "formation original qualified If condition is Boolean",
                condition,
            )?;
        }
    }
    let mut selected = vec![];
    for id in calls(parsed, site.scan) {
        let Some(c) = typed.call(expression(id)) else {
            need(
                typed.string_operation(expression(id)).is_some()
                    || typed.map_put(expression(id)).is_some()
                    || typed.construction(expression(id)).is_some(),
                "formation actual qualified builtin/construction descriptor",
                id,
            )?;
            continue;
        };
        if let UnitCallTarget::Declaration(target) = c.target() {
            let signature = typed
                .signatures()
                .declaration(target)
                .and_then(|d| d.callable())
                .ok_or_else(|| failure("formation actual qualified selected signature", target))?;
            let marker = signature.name_span();
            let function=one(parsed.roots().iter().filter_map(|id|{
                let n=parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if *name==marker).then_some(n.span())
            }),"formation selected qualified source callable")?;
            selected.push((span(parsed, id), function));
        }
    }
    if let Some(id) = site.sink {
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        let arg = one(arguments.iter(), "formation qualified sole own transfer")?;
        let delivery = one(
            owned
                .value_deliveries()
                .iter()
                .filter(|d| d.call() == expression(id)),
            "formation actual qualified delivery",
        )?;
        need(
            root(arg.value)? == source_root
                && delivery.argument() == expression(arg.value)
                && delivery
                    .place()
                    .is_some_and(|place| place.is_root() && place.root() == source_root)
                && matches!(typed.call(expression(id)).unwrap().arguments(),[a] if a.argument_index()==0 && a.parameter_index()==0 && a.mode()==ParameterMode::Value)
                && !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(source_root)),
            "formation qualified own transfer and no caller cleanup",
            delivery,
        )?;
    }
    let mut drops = vec![];
    for d in owned.drops() {
        if let UnitDropTarget::Named(root) = d.target() {
            let marker = symbol_span(root)?;
            if inside(marker, site.scan)
                && typed.symbol_type(root) == Some(p.descriptor().source_type())
            {
                drops.push(Drop {
                    root: marker,
                    origin: d.value_origin(),
                    point: unit_point(parsed, d.point())?,
                    birth: drop_birth(
                        parsed,
                        site.scan,
                        marker,
                        unit_point(parsed, d.point())?,
                        &|id| symbol_span(unit_ref(names, parsed, id)?),
                    )?,
                });
            }
        }
    }
    let representation = representation::unit(parsed, names, typed, p.descriptor().value_type())?;
    let guard = representation::unit_guard(parsed, names, typed, owned, &site)?;
    let components = [
        symbol_span(p.descriptor().key_binding())?,
        symbol_span(p.descriptor().value_binding())?,
    ];
    let uses = calls::unit(
        sources,
        parsed,
        names,
        typed,
        owned,
        (&site, &selected, components),
    )?;
    let clones = typed
        .string_operations()
        .iter()
        .filter(|c| inside(span(parsed, c.expression().expression()), site.scan))
        .map(|c| {
            (
                span(parsed, c.expression().expression()),
                span(parsed, c.receiver().expression()),
            )
        })
        .collect();
    let births = calls(parsed, site.scan)
        .map(|id| (id, span(parsed, id)))
        .collect();
    let sizes = typed
        .map_sizes()
        .iter()
        .filter(|d| inside(span(parsed, d.expression().expression()), site.scan))
        .map(|d| {
            Ok((
                span(parsed, d.expression().expression()),
                symbol_span(root(d.receiver().expression())?)?,
            ))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(Inputs {
        site,
        calls: selected,
        drops,
        representation,
        guard,
        kind: p.descriptor().kind(),
        components,
        uses,
        clones,
        births,
        sizes,
    })
}

fn unit_ref(
    names: &ValidatedCompilationUnitNames,
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<lang_frontend::name_resolution::UnitSymbolId, Failure> {
    one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span(parsed, id))
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "formation exact qualified Drop root reference",
    )
}
