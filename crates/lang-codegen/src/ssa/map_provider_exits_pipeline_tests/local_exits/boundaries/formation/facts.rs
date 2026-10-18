//! Public frontend observations from exactly the analysis used for lowering.
use super::*;
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CleanupSelection, CleanupSelectorSource,
        CompilationUnitOwnership, DropPoint, DropTarget, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, IterationPhiPresenceSource, LoanTarget,
        MapIterationSourceAccess, OwnershipCheckedFile, UnitDropPoint, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{
        CallableTarget, CompilationUnitTypes, ParameterMode, TypedFile, UnitCallTarget,
        UnitExpressionId, UnitStatementId,
    },
};
fn single_point(parsed: &ParsedFile, point: DropPoint) -> Result<Point, Failure> {
    Ok(match point {
        DropPoint::AfterExpression(id) => Point::Expression(span(parsed, id)),
        DropPoint::AfterStatement(id) => {
            Point::Statement(parsed.ast().statements().get(id).unwrap().span())
        }
        DropPoint::CallReturn(id) => Point::Call(span(parsed, id)),
        DropPoint::LoopExit(id) => Point::Loop(parsed.ast().statements().get(id).unwrap().span()),
        DropPoint::FunctionEntry(id) => Point::Entry(parsed.ast().items().get(id).unwrap().span()),
        other => return Err(failure("formation actual named Drop point", other)),
    })
}
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
pub(in super::super::super::super) fn single(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Inputs, Failure> {
    let root = |id| {
        let symbol = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == span(parsed, id))
                .filter_map(|r| {
                    if let ReferenceTarget::Symbol(s) = r.target() {
                        Some(*s)
                    } else {
                        None
                    }
                }),
            "formation original single reference",
        )?;
        Ok(symbol)
    };
    let marker = |id| Ok(names.symbols()[root(id)?.index()].span());
    let site = site(sources, parsed, marker)?;
    let source_root = root(site.source)?;
    let p = owned
        .map_iteration(site.statement)
        .ok_or_else(|| failure("formation same-analysis plan", site.statement))?;
    need(
        typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && typed.map_iteration(site.statement) == Some(p.descriptor())
            && p.descriptor().source() == site.source
            && p.source_access() == MapIterationSourceAccess::Owned
            && matches!(p.source(), Some(LoanTarget::Place(place)) if place.is_root() && place.root()==source_root)
            && p.capture_graph().nodes().is_empty(),
        "formation actual named source descriptor",
        p,
    )?;
    for (index, loan) in p.loans().iter().enumerate() {
        need(
            loan.statement() == site.statement
                && loan.source() == site.source
                && loan.binding() == p.descriptor().bindings()[index]
                && Some(loan.origin()) == p.source(),
            "formation ordered original component identities",
            loan,
        )?;
    }
    for &(expression, original) in &site.aliases {
        need(
            marker(expression)? == original
                && typed.expression_type(expression) == Some(p.descriptor().source_type()),
            "formation exact original Move Name/root/type",
            (expression, original),
        )?;
    }
    let table = owned.cleanup_conditions();
    // Some ownership-dead-at-For sources publish no owner phi. When present,
    // every public incoming must describe this precise original birth and root.
    if !p.closure_phis().is_empty() {
        need(
            p.closure_phi_incomings().len() == 3,
            "formation complete noncapturing owner phi set",
            p,
        )?;
        let roots = p
            .closure_phis()
            .iter()
            .map(|phi| phi.symbol())
            .collect::<std::collections::BTreeSet<_>>();
        need(
            roots.contains(&source_root) && p.closure_phis().len() == roots.len() * 2,
            "formation complete distinct live-root owner pairs",
            p.closure_phis(),
        )?;
        for &current_root in &roots {
            let current_marker = names.symbols()[current_root.index()].span();
            let (birth, _) = birth_at(
                parsed,
                site.scan,
                current_marker,
                site.for_span.start(),
                &marker,
            )?;
            let birth_matches = |id| match (table.owner_value(id), birth) {
                (Some(O::Expression { expression, origin }), Birth::Call(call)) => {
                    *expression == call && *origin == span(parsed, call)
                }
                (Some(O::Parameter { symbol, origin }), Birth::Parameter(marker)) => {
                    names.symbols()[symbol.index()].span() == marker && *origin == marker
                }
                _ => false,
            };
            let h = one(
                p.closure_phis()
                    .iter()
                    .filter(|v| v.symbol() == current_root && v.boundary() == B::Header),
                "formation actual Header owner",
            )?;
            let x = one(
                p.closure_phis()
                    .iter()
                    .filter(|v| v.symbol() == current_root && v.boundary() == B::Exit),
                "formation actual Exit owner",
            )?;
            need(
                h.owner() != x.owner() && h.availability_selector() != x.availability_selector(),
                "formation distinct current owner boundaries",
                (h, x),
            )?;
            for phi in [h, x] {
                let selector = table
                    .selector(phi.availability_selector())
                    .ok_or_else(|| failure("formation actual presence selector", phi))?;
                need(
                    matches!(table.owner_value(phi.owner()),Some(O::IterationPhi {statement,boundary,symbol,origin}) if *statement==site.statement && *boundary==phi.boundary() && *symbol==current_root && *origin==site.for_span)
                        && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..} if statement==site.statement && boundary==phi.boundary())
                        && selector.origin() == site.for_span
                        && selector.selection() == CleanupSelection::IterationPresence
                        && selector.branch_count() == 2
                        && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches}) if *selector==phi.availability_selector() && matches!(branches.as_slice(),[no,yes] if table.get(*no)==Some(&C::Never) && table.get(*yes)==Some(&C::Always))),
                    "formation exact owner/presence definition",
                    phi,
                )?;
            }
            for (kind, phi, point) in [
                (K::Entry, h, DropPoint::AfterExpression(site.source)),
                (K::Fallthrough, h, DropPoint::AfterStatement(site.body)),
                (K::Exhaustion, x, DropPoint::LoopExit(site.statement)),
            ] {
                let incoming = one(
                    p.closure_phi_incomings()
                        .iter()
                        .filter(|i| i.kind() == kind),
                    "formation complete actual incoming",
                )?;
                need(
                    incoming.bindings().len() == roots.len(),
                    "formation complete incoming live-root set",
                    incoming,
                )?;
                let binding = one(
                    incoming
                        .bindings()
                        .iter()
                        .filter(|b| b.target() == phi.owner()),
                    "formation exact current root incoming binding",
                )?;
                need(
                    incoming.point() == point
                        && incoming.boundary() == phi.boundary()
                        && table.get(incoming.condition()) == Some(&C::Always)
                        && binding.target() == phi.owner()
                        && binding.availability_selector() == phi.availability_selector()
                        && binding.capture_slots_to_clear().is_empty()
                        && binding.root_sources().is_empty()
                        && binding.selector_writes().is_empty()
                        && binding.origins().is_empty()
                        && binding.presence_source()
                            == IterationPhiPresenceSource::StaticConditions,
                    "formation full incoming layout and real boundary",
                    incoming,
                )?;
                if binding.values().is_empty() {
                    need(
                        kind == K::Exhaustion
                            && table.get(binding.available_when()) == Some(&C::Never)
                            && owned.drops().iter().any(|d| {
                                d.target() == DropTarget::Named(current_root)
                                    && d.point() == point
                                    && d.owner() == Some(h.owner())
                                    && d.condition() == Some(h.availability_condition())
                            }),
                        "formation absent Exit only after actual Header Drop",
                        incoming,
                    )?;
                } else {
                    let v = one(
                        binding.values().iter(),
                        "formation unique actual incoming owner",
                    )?;
                    need(
                        v.condition() == binding.available_when()
                            && if kind == K::Entry {
                                table.get(binding.available_when()) == Some(&C::Always)
                                    && birth_matches(v.source())
                            } else {
                                v.source() == h.owner()
                                    && binding.available_when() == h.availability_condition()
                            },
                        "formation Entry actual RHS/Move birth and live Header transport",
                        incoming,
                    )?;
                }
            }
        }
    }
    if let Some(id) = site.put {
        let put = typed
            .map_put(id)
            .ok_or_else(|| failure("formation actual typed put descriptor", id))?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            put.expression() == id
                && root(put.receiver())? == source_root
                && matches!(arguments.as_slice(),[k,v] if put.key()==k.value && put.value()==v.value)
                && put.key_type() == p.descriptor().key_type()
                && put.value_type() == p.descriptor().value_type(),
            "formation typed put exact root/ordered key/value types",
            put,
        )?;
    }
    let mut selected = vec![];
    for id in calls(parsed, site.scan) {
        let Some(call) = typed.call(id) else {
            need(
                typed.string_operation(id).is_some()
                    || typed.map_put(id).is_some()
                    || typed.construction(id).is_some(),
                "formation actual typed builtin/construction descriptor",
                id,
            )?;
            continue;
        };
        if let CallableTarget::Source(target) = call.target() {
            let marker = names.symbols()[target.index()].span();
            let selected_function = one(parsed.roots().iter().filter_map(|id| {
                let n=parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if *name==marker).then_some(n.span())
            }), "formation exact selected source function")?;
            selected.push((span(parsed, id), selected_function));
        }
    }
    if let Some(id) = site.sink {
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        let arg = one(arguments.iter(), "formation sole own transfer argument")?;
        need(
            root(arg.value)? == source_root
                && matches!(typed.call(id).unwrap().arguments(),[a] if a.argument_index()==0 && a.parameter_index()==0 && a.mode()==ParameterMode::Value)
                && !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == DropTarget::Named(source_root)),
            "formation exact own source transfer has no caller Drop",
            id,
        )?;
    }
    for d in owned.drops() {
        let DropTarget::Named(root) = d.target() else {
            continue;
        };
        if !inside(names.symbols()[root.index()].span(), site.scan)
            || typed.symbol_type(root) != Some(p.descriptor().source_type())
        {
            continue;
        }
        let owner = d
            .owner()
            .ok_or_else(|| failure("formation actual Drop owner identity", d))?;
        if let Some(O::IterationPhi {
            statement,
            symbol,
            boundary,
            origin,
        }) = table.owner_value(owner)
        {
            let phi = one(
                p.closure_phis().iter().filter(|phi| phi.owner() == owner),
                "formation Drop actual current owner phi",
            )?;
            need(
                *statement == site.statement
                    && *symbol == root
                    && *origin == site.for_span
                    && *boundary == phi.boundary()
                    && d.condition() == Some(phi.availability_condition()),
                "formation Drop exact root/current boundary/presence condition",
                d,
            )?;
        } else {
            let expected = drop_birth(
                parsed,
                site.scan,
                names.symbols()[root.index()].span(),
                single_point(parsed, d.point())?,
                &marker,
            )?;
            let original = match (table.owner_value(owner), expected) {
                (Some(O::Expression { expression, origin }), Birth::Call(id)) => {
                    *expression == id && *origin == span(parsed, id)
                }
                (Some(O::Parameter { symbol, origin }), Birth::Parameter(at)) => {
                    names.symbols()[symbol.index()].span() == at && *origin == at
                }
                _ => false,
            };
            need(
                original
                    && d.condition()
                        .is_none_or(|c| table.get(c) == Some(&C::Always)),
                "formation non-phi Drop exact original live epoch",
                d,
            )?;
        }
    }
    let mut drops = vec![];
    for d in owned.drops() {
        if let DropTarget::Named(root) = d.target() {
            let marker = names.symbols()[root.index()].span();
            if inside(marker, site.scan)
                && typed.symbol_type(root) == Some(p.descriptor().source_type())
            {
                need(
                    d.capture_slot().is_none() && d.instance_address().is_none(),
                    "formation no captured Drop substitution",
                    d,
                )?;
                drops.push(Drop {
                    root: marker,
                    origin: d.value_origin(),
                    point: single_point(parsed, d.point())?,
                    birth: drop_birth(
                        parsed,
                        site.scan,
                        marker,
                        single_point(parsed, d.point())?,
                        &|id| Ok(names.symbols()[root_ref(names, parsed, id)?.index()].span()),
                    )?,
                });
            }
        }
    }
    let representation = representation::single(parsed, names, typed, p.descriptor().value_type())?;
    let guard = representation::single_guard(parsed, names, typed, owned, &site)?;
    Ok(Inputs {
        site,
        calls: selected,
        drops,
        representation,
        guard,
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
    Ok(Inputs {
        site,
        calls: selected,
        drops,
        representation,
        guard,
    })
}

fn root_ref(
    names: &NameResolution,
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<lang_frontend::name_resolution::SymbolId, Failure> {
    one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == span(parsed, id))
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "formation exact Drop root reference",
    )
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
