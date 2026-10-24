//! Public frontend observations from exactly the analysis used for lowering.
mod calls;
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
