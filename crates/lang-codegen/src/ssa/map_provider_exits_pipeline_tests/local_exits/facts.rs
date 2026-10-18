//! Public local formation, alias provenance and iteration cleanup identities.
mod ast;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use ast::{Site, expression_span, returned, sites, within};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CleanupSelection, CleanupSelectorSource,
        CompilationUnitOwnership, DropFact, DropPoint, DropTarget, IterationCleanupAction as A,
        IterationExitKind as E, IterationPhiBoundary as B, IterationPhiIncomingKind as K,
        IterationPhiPresenceSource, IterationSource, LoanKind, LoanTarget, MapIterationComponent,
        MapIterationOwnershipPlan, MapIterationSourceAccess, OwnershipBindingKind,
        OwnershipCheckedFile, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, CompilationUnitTypes,
        ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind, ParameterMode, TypeKind,
        TypedFile, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn single_reference(
    parsed: &ParsedFile,
    names: &NameResolution,
    id: ExpressionId,
) -> Result<SymbolId, Failure> {
    let span = expression_span(parsed, id)?;
    one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == span)
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "G2 original Name symbol",
    )
}
fn unit_reference(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let span = expression_span(parsed, id)?;
    one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span)
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "G2 qualified original Name symbol",
    )
}
fn unit_marker(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    marker: Span,
    kind: SymbolKind,
) -> bool {
    names.names().source_units().iter().any(|u| {
        u.source_unit() == id.source_unit()
            && u.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == marker && s.kind() == kind && !s.is_synthetic()
            })
    })
}
fn single_phi(
    site: &Site,
    root: SymbolId,
    p: &MapIterationOwnershipPlan,
    drops: &[&DropFact],
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let table = owned.cleanup_conditions();
    let h = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Header),
        "G2 actual local Header phi",
    )?;
    let x = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Exit),
        "G2 actual local Exit phi",
    )?;
    need(
        p.closure_phis().len() == 2
            && h.owner() != x.owner()
            && h.availability_selector() != x.availability_selector(),
        "G2 independent local owner boundaries",
        p.closure_phis(),
    )?;
    for phi in [h, x] {
        let selector = table
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("G2 actual selector", phi))?;
        need(
            matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement,boundary,symbol,origin}) if *statement==site.statement && *boundary==phi.boundary() && *symbol==root && *origin==site.span)
                && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..} if statement==site.statement && boundary==phi.boundary())
                && selector.origin() == site.span
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.branch_count() == 2
                && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches}) if *selector==phi.availability_selector() && matches!(branches.as_slice(),[no,yes] if table.get(*no)==Some(&C::Never) && table.get(*yes)==Some(&C::Always))),
            "G2 full public owner/selector/Choice identity",
            (phi, selector),
        )?;
    }
    need(
        drops.iter().all(|d| {
            d.owner() == Some(h.owner())
                && d.condition() == Some(h.availability_condition())
                && d.capture_slot().is_none()
                && d.instance_address().is_none()
        }),
        "G2 actual current Header Drop identity",
        drops,
    )?;
    let mut edges = vec![
        (K::Entry, DropPoint::AfterExpression(site.source), B::Header),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ];
    match site.kind {
        E::Fallthrough => edges.push((K::Fallthrough, site.point, B::Header)),
        E::Continue(e) => edges.push((K::Continue(e), site.point, B::Header)),
        E::Break(e) => edges.push((K::Break(e), site.point, B::Exit)),
        E::Return(_) => {}
        _ => return Err(failure("G2 actual phi exit kind", site.kind)),
    }
    need(
        p.closure_phi_incomings().len() == edges.len(),
        "G2 complete actual incoming set and no Return incoming",
        p.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in edges {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "G2 actual unique incoming kind",
        )?;
        let phi = if boundary == B::Header { h } else { x };
        let binding = one(edge.bindings().iter(), "G2 exact root incoming binding")?;
        need(
            edge.point() == point
                && edge.boundary() == boundary
                && table.get(edge.condition()) == Some(&C::Always)
                && binding.target() == phi.owner()
                && binding.availability_selector() == phi.availability_selector()
                && binding.capture_slots_to_clear().is_empty()
                && binding.root_sources().is_empty()
                && binding.selector_writes().is_empty()
                && binding.origins().is_empty()
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions,
            "G2 original incoming boundary and full payload",
            edge,
        )?;
        if drops.iter().any(|d| d.point() == point) {
            need(
                binding.values().is_empty()
                    && table.get(binding.available_when()) == Some(&C::Never),
                "G2 cleaned original owner writes absent Exit",
                edge,
            )?;
        } else {
            let value = one(binding.values().iter(), "G2 original live incoming value")?;
            need(
                value.condition() == binding.available_when()
                    && if kind == K::Entry {
                        table.get(binding.available_when()) == Some(&C::Always)
                            && matches!(table.owner_value(value.source()),Some(O::Expression{expression,origin}) if *expression==site.birth.initializer && *origin==site.birth_span)
                    } else {
                        value.source() == h.owner()
                            && binding.available_when() == h.availability_condition()
                    },
                "G2 exact factory Call birth or current Header delivery",
                edge,
            )?;
        }
    }
    Ok(())
}
fn single_cleanup(
    site: &Site,
    root: SymbolId,
    guard: SymbolId,
    p: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let source = owned
        .drops()
        .iter()
        .filter(|d| d.target() == DropTarget::Named(root))
        .collect::<Vec<_>>();
    let terminal = matches!(site.kind, E::Break(_) | E::Return(_));
    need(
        source.len() == if terminal { 2 } else { 1 }
            && source.iter().all(|d| {
                d.value_origin() == site.root.marker
                    && (d.point() == DropPoint::LoopExit(site.statement)
                        || terminal && d.point() == site.point)
            })
            && source
                .iter()
                .filter(|d| d.point() == DropPoint::LoopExit(site.statement))
                .count()
                == 1
            && (!terminal || source.iter().filter(|d| d.point() == site.point).count() == 1),
        "G2 exact original local source Drop set",
        &source,
    )?;
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(guard)),
        "G2 actual Guard Drop",
    )?;
    need(guard.point()==site.point && guard.value_origin()==site.guard.marker && guard.condition().is_none()
        && guard.owner().is_some_and(|owner|matches!(owned.cleanup_conditions().owner_value(owner),Some(O::Expression{expression,origin}) if *expression==site.guard.initializer && *origin==site.guard_span)),"G2 actual Guard construction/cleanup identity",guard)?;
    need(p.exits().len() == 2, "G2 complete local exits", p.exits())?;
    for (kind, point, body) in [
        (site.kind, site.point, true),
        (E::Exhaustion, DropPoint::LoopExit(site.statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == kind && e.point() == point),
            "G2 original exact exit",
        )?;
        let mut expected = if body {
            vec![
                A::Drop(guard),
                A::EndBinding {
                    statement: site.statement,
                    symbol: p.descriptor().value_binding(),
                },
                A::EndBinding {
                    statement: site.statement,
                    symbol: p.descriptor().key_binding(),
                },
            ]
        } else {
            vec![]
        };
        if !body || terminal {
            expected.extend([
                A::FinishProvider(site.statement),
                A::EndSource(site.statement),
            ]);
        }
        expected.extend(
            source
                .iter()
                .filter(|d| d.point() == point)
                .map(|d| A::Drop(**d)),
        );
        let full = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, a)| (*at == point).then_some(*a))
            .collect::<Vec<_>>();
        need(
            exit.actions() == expected
                && full == expected
                && owned.cleanup_conditions().get(exit.condition()) == Some(&C::Always),
            "G2 full real Guard/components/provider/source cleanup",
            (exit, full, expected),
        )?;
    }
    single_phi(site, root, p, &source, owned)
}

pub(super) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    report.check(
        "G2 exact two original single Map plans",
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        (typed.map_iterations(), owned.map_iterations()),
    );
    match sites(case, sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "G2 original single local formation and exits",
                    &single_site(site, parsed, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("G2 original local AST", &Err(e)),
    }
}
fn single_site(
    site: &Site,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let root = single_reference(parsed, names, site.source)?;
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        single_reference(parsed, names, site.root.initializer)?
    };
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            names.symbols().iter().any(|s| {
                s.id() == id
                    && s.span() == local.marker
                    && s.kind() == SymbolKind::Variable
                    && !s.is_synthetic()
            }) && owned.formed_owned_locals().contains(&id),
            "G2 actual original local formation history",
            (id, local),
        )?;
    }
    need(
        (root == original) == (site.root.marker == site.birth.marker)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == DropTarget::Named(original)))
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Temporary(site.birth.initializer)),
        "G2 moved original has no duplicate Drop obligation",
        (root, original),
    )?;
    let p = owned
        .map_iteration(site.statement)
        .ok_or_else(|| failure("G2 actual local Map plan", site))?;
    let d = p.descriptor();
    let mutable = site.map_kind == MapIterationKind::MutableMap;
    let constructor = if mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.map_iteration(site.statement) == Some(d)
            && d.kind() == site.map_kind
            && d.source() == site.source
            && typed.sequential_iteration(site.statement).is_none()
            && owned.iteration(site.statement).is_none()
            && typed.expression_type(site.source) == Some(d.source_type())
            && typed.expression_category(site.source) == Some(ExpressionCategory::Place)
            && typed.symbol_type(root) == Some(d.source_type())
            && typed.symbol_type(original) == Some(d.source_type())
            && typed.expression_type(site.root.initializer) == Some(d.source_type())
            && typed.expression_category(site.root.initializer)
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && typed.expression_type(site.birth.initializer) == Some(d.source_type())
            && typed.expression_category(site.birth.initializer)
                == Some(ExpressionCategory::Temporary)
            && typed.is_resource_type(d.source_type()) == Some(false)
            && matches!(typed.types().get(d.source_type()),Some(TypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
            && [d.key_type(), d.value_type()]
                .into_iter()
                .all(|t| typed.types().get(t) == Some(&TypeKind::Builtin(BuiltinType::String)))
            && p.source_access() == MapIterationSourceAccess::Owned
            && matches!(p.source_kind(),IterationSource::BorrowedSource(LoanTarget::Place(path)) if path.is_root() && path.root()==root),
        "G2 actual local type/category/Owned root lineage",
        p,
    )?;
    let factory = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == site.factory && s.kind() == SymbolKind::Function),
        "G2 original selected factory symbol",
    )?;
    let call = typed
        .call(site.birth.initializer)
        .ok_or_else(|| failure("G2 actual factory Call descriptor", site))?;
    need(
        call.target() == CallableTarget::Source(factory.id())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == d.source_type()
            && call.result_source() == CallableResultSource::Owned,
        "G2 exact selected factory Call birth",
        call,
    )?;
    let guard = one(
        owned.formed_owned_locals().iter().copied().filter(|id| {
            names.symbols().iter().any(|s| {
                s.id() == *id && s.span() == site.guard.marker && s.kind() == SymbolKind::Variable
            })
        }),
        "G2 original formed Guard",
    )?;
    need(
        typed
            .expression_type(site.guard.initializer)
            .is_some_and(|t| typed.is_resource_type(t) == Some(true)),
        "G2 actual Guard Resource",
        guard,
    )?;
    for (index, binding) in d.bindings().into_iter().enumerate() {
        need(
            names.symbols().iter().any(|s| {
                s.id() == binding
                    && s.span() == site.bindings[index]
                    && s.kind() == SymbolKind::ForBinding
            }) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[index].symbol() == binding
                && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                && p.loans()[index].binding() == binding
                && p.loans()[index].statement() == site.statement
                && p.loans()[index].source() == site.source
                && p.loans()[index].component()
                    == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                && p.loans()[index].kind() == LoanKind::Shared
                && Some(p.loans()[index].origin()) == p.source(),
            "G2 exact original single K/V Shared capability",
            p.loans(),
        )?;
    }
    let scanner = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == site.name && s.kind() == SymbolKind::Function),
        "G2 original scanner symbol",
    )?;
    let signature = one(
        typed
            .callables()
            .iter()
            .filter(|c| c.symbol() == scanner.id()),
        "G2 actual scanner signature",
    )?;
    need(
        signature.parameters().is_empty()
            && signature.receiver().is_none()
            && typed.types().get(signature.return_type())
                == Some(&TypeKind::Builtin(BuiltinType::Int)),
        "G2 original zero-parameter Int scanner",
        signature,
    )?;
    let caller = one(
        typed
            .calls()
            .iter()
            .filter(|c| c.target() == CallableTarget::Source(scanner.id())),
        "G2 actual entry scanner call",
    )?;
    need(
        caller.arguments().is_empty()
            && caller.return_type() == signature.return_type()
            && within(site.entry, expression_span(parsed, caller.expression())?)
            && within(site.scan, site.span),
        "G2 entry calls this actual scanner once",
        caller,
    )?;
    if let Some((value, arguments)) = returned(site, parsed)? {
        let operand = typed
            .call(value)
            .ok_or_else(|| failure("G2 actual original result Call", value))?;
        need(
            typed.expression_type(value) == Some(signature.return_type())
                && operand.return_type() == signature.return_type()
                && matches!(operand.target(),CallableTarget::Source(s) if names.symbols().iter().any(|v|v.id()==s && v.span()==site.result))
                && matches!(operand.arguments(),[k,v] if k.mode()==ParameterMode::Borrow && v.mode()==ParameterMode::Borrow && k.parameter_type()==d.key_type() && v.parameter_type()==d.value_type())
                && single_reference(parsed, names, arguments[0])? == d.key_binding()
                && single_reference(parsed, names, arguments[1])? == d.value_binding(),
            "G2 actual result Call uses the original K/V loans",
            operand,
        )?;
    }
    single_cleanup(site, root, guard, p, owned)
}

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    report.check(
        "G2 exact two original qualified Map plans",
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        (typed.map_iterations(), owned.map_iterations()),
    );
    match sites(case, sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "G2 original unit local formation and exits",
                    &unit_site(site, parsed, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("G2 original local AST", &Err(e)),
    }
}
fn unit_site(
    site: &Site,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let root = unit_reference(parsed, names, site.source)?;
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        unit_reference(parsed, names, site.root.initializer)?
    };
    let unit = root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            unit_marker(names, id, local.marker, SymbolKind::Variable)
                && id.source_unit() == unit
                && owned.formed_owned_locals().contains(&id),
            "G2 actual qualified formation history",
            (id, local),
        )?;
    }
    need(
        (root == original) == (site.root.marker == site.birth.marker)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(original)))
            && !owned.drops().iter().any(|d| {
                d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
            }),
        "G2 unit original Move source has no Drop",
        (root, original),
    )?;
    let statement = UnitStatementId::new(unit, site.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("G2 original qualified local Map plan", statement))?;
    let d = p.descriptor();
    let constructor = if site.map_kind == MapIterationKind::MutableMap {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.map_iteration(statement) == Some(d)
            && d.kind() == site.map_kind
            && d.source() == expression(site.source)
            && typed.sequential_iteration(statement).is_none()
            && owned.iteration(statement).is_none()
            && typed.expression_type(expression(site.source)) == Some(d.source_type())
            && typed.expression_category(expression(site.source))
                == Some(ExpressionCategory::Place)
            && typed.symbol_type(root) == Some(d.source_type())
            && typed.symbol_type(original) == Some(d.source_type())
            && typed.expression_type(expression(site.root.initializer)) == Some(d.source_type())
            && typed.expression_category(expression(site.root.initializer))
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && typed.expression_type(expression(site.birth.initializer)) == Some(d.source_type())
            && typed.expression_category(expression(site.birth.initializer))
                == Some(ExpressionCategory::Temporary)
            && typed.is_resource_type(d.source_type()) == Some(false)
            && matches!(typed.types().get(d.source_type()),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
            && [d.key_type(), d.value_type()]
                .into_iter()
                .all(|t| typed.types().get(t) == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
            && p.source_access() == UnitIterationSourceAccess::Owned
            && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
        "G2 original qualified local type/category/source capability",
        p,
    )?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "G2 qualified selected factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("G2 qualified actual factory Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == d.source_type()
            && call.result_source() == CallableResultSource::Owned,
        "G2 selected qualified Call owner birth",
        call,
    )?;
    for (index, binding) in d.bindings().into_iter().enumerate() {
        need(
            unit_marker(names, binding, site.bindings[index], SymbolKind::ForBinding)
                && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[index].symbol() == binding
                && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                && p.loans()[index].binding() == binding
                && p.loans()[index].statement() == statement
                && p.loans()[index].source() == d.source()
                && p.loans()[index].component()
                    == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                && p.loans()[index].kind() == LoanKind::Shared
                && Some(p.loans()[index].origin()) == p.source(),
            "G2 exact qualified original K/V lineage",
            p.loans(),
        )?;
    }
    let scanner = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.name && c.parameters().is_empty())
        }),
        "G2 actual qualified scanner",
    )?;
    let signature = scanner
        .callable()
        .ok_or_else(|| failure("G2 scanner callable", scanner))?;
    need(
        typed.types().get(signature.return_type())
            == Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        "G2 qualified scanner Int result",
        signature,
    )?;
    let caller = one(
        typed
            .calls()
            .iter()
            .filter(|c| c.target() == UnitCallTarget::Declaration(scanner.declaration())),
        "G2 actual qualified entry caller",
    )?;
    need(
        caller.arguments().is_empty()
            && caller.return_type() == signature.return_type()
            && caller.expression().source_unit() == unit
            && within(
                site.entry,
                expression_span(parsed, caller.expression().expression())?,
            ),
        "G2 entry actual qualified scanner call",
        caller,
    )?;
    if let Some((value, arguments)) = returned(site, parsed)? {
        let operand = typed
            .call(expression(value))
            .ok_or_else(|| failure("G2 actual qualified result Call", value))?;
        need(
            typed.expression_type(expression(value)) == Some(signature.return_type())
                && operand.return_type() == signature.return_type()
                && matches!(operand.target(),UnitCallTarget::Declaration(id) if typed.signatures().declarations().iter().any(|d|d.declaration()==id && d.callable().is_some_and(|c|c.name_span()==site.result)))
                && matches!(operand.arguments(),[k,v] if k.mode()==ParameterMode::Borrow && v.mode()==ParameterMode::Borrow && k.parameter_type()==d.key_type() && v.parameter_type()==d.value_type())
                && unit_reference(parsed, names, arguments[0])? == d.key_binding()
                && unit_reference(parsed, names, arguments[1])? == d.value_binding(),
            "G2 qualified result Call uses actual K/V roots",
            operand,
        )?;
    }
    let point = match site.point {
        DropPoint::AfterStatement(s) => {
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
        }
        DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
        other => return Err(failure("G2 actual qualified point", other)),
    };
    let kind = match site.kind {
        E::Fallthrough => UE::Fallthrough,
        E::Continue(e) => UE::Continue(expression(e)),
        E::Break(e) => UE::Break(expression(e)),
        E::Return(e) => UE::Return(expression(e)),
        other => return Err(failure("G2 actual qualified kind", other)),
    };
    let source = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let terminal = matches!(kind, UE::Break(_) | UE::Return(_));
    need(
        source.len() == if terminal { 2 } else { 1 }
            && source.iter().all(|d| {
                d.value_origin() == site.root.marker
                    && (d.point() == UnitDropPoint::LoopExit(statement)
                        || terminal && d.point() == point)
            })
            && source
                .iter()
                .filter(|d| d.point() == UnitDropPoint::LoopExit(statement))
                .count()
                == 1
            && (!terminal || source.iter().filter(|d| d.point() == point).count() == 1),
        "G2 original qualified source Drop set",
        &source,
    )?;
    let guard = one(
        owned
            .formed_owned_locals()
            .iter()
            .copied()
            .filter(|id| unit_marker(names, *id, site.guard.marker, SymbolKind::Variable)),
        "G2 actual qualified Guard formation",
    )?;
    need(
        typed
            .expression_type(expression(site.guard.initializer))
            .is_some_and(|t| typed.is_resource_type(t) == Some(true)),
        "G2 qualified Guard Resource type",
        guard,
    )?;
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(guard)),
        "G2 actual qualified Guard Drop",
    )?;
    need(
        guard.point() == point && guard.value_origin() == site.guard.marker,
        "G2 original qualified Guard point",
        guard,
    )?;
    need(
        p.exits().len() == 2,
        "G2 unit complete actual exits",
        p.exits(),
    )?;
    for (kind, point, body) in [
        (kind, point, true),
        (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == kind && e.point() == point),
            "G2 exact qualified exit",
        )?;
        let mut expected = if body {
            vec![
                UA::Drop(guard),
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ]
        } else {
            vec![]
        };
        if !body || terminal {
            expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
        }
        expected.extend(
            source
                .iter()
                .filter(|d| d.point() == point)
                .map(|d| UA::Drop(**d)),
        );
        need(
            exit.actions() == expected
                && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "G2 original unit complete ordered cleanup",
            (exit, expected),
        )?;
    }
    Ok(())
}
