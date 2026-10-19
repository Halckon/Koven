//! Original public FE lifetime/owner handoffs; never call lowering qualification.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition as C, CleanupConditionId, CleanupOwnerValue as O, CleanupOwnerValueId,
        CleanupSelection, CleanupSelectorSource, CompilationUnitOwnership, DropFact, DropPoint,
        DropTarget, IterationCleanupAction as A, IterationExitKind as E, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, IterationPhiPresenceSource, LoanKind, LoanTarget,
        MapIterationComponent, MapIterationOwnershipPlan, MapIterationSourceAccess,
        OwnershipBindingKind, OwnershipCheckedFile, UnitCallArgumentOwnershipKind, UnitDropFact,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitIterationSourceAccess, UnitLoanTarget,
        UnitMapIterationOwnershipPlan, UnitValueDeliveryKind,
    },
    parser::{
        Expression, FunctionBody, FunctionForm, Item, NameMarker, ParameterModeMarker, ParsedFile,
        Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableTarget, CompilationUnitTypes, IntrinsicTypeConstructor,
        MapIterationKind, ParameterMode, TypeKind, TypedFile, UnitCallTarget, UnitExpressionId,
        UnitStatementId, UnitTypeKind,
    },
};

#[derive(Debug)]
struct Site {
    statement: StatementId,
    source: ExpressionId,
    body: StatementId,
    span: Span,
    transfer: Option<(ExpressionId, bool)>,
    local: Span,
    initializer: ExpressionId,
    initializer_span: Span,
}
impl Site {
    fn body_point(&self) -> DropPoint {
        self.transfer
            .map_or(DropPoint::AfterStatement(self.body), |(e, _)| {
                DropPoint::ControlTransfer(e)
            })
    }
    fn kind(&self) -> E {
        match self.transfer {
            None => E::Fallthrough,
            Some((e, true)) => E::Break(e),
            Some((e, false)) => E::Continue(e),
        }
    }
    fn incoming(&self) -> K {
        match self.transfer {
            None => K::Fallthrough,
            Some((e, true)) => K::Break(e),
            Some((e, false)) => K::Continue(e),
        }
    }
}
#[derive(Debug)]
struct Ast {
    scan: Span,
    name: Span,
    parameter: Span,
    sites: Vec<Site>,
}
fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
fn expression_span(parsed: &ParsedFile, id: ExpressionId) -> Result<Span, Failure> {
    parsed
        .ast()
        .expressions()
        .get(id)
        .map(|n| n.span())
        .map_err(|e| failure("G9 actual expression", e))
}
fn ast(case: &Case, sources: &SourceMap, parsed: &ParsedFile) -> Result<Ast, Failure> {
    let node=one(parsed.roots().iter().filter_map(|id|{
        let node=parsed.ast().items().get(*id).ok()?;
        matches!(node.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("scan")).then_some(node)
    }),"G9 original unique scan")?;
    let Item::Function {
        name: NameMarker::Present(name),
        parameters,
        form,
        ..
    } = node.payload()
    else {
        return Err(failure("G9 original scan", node));
    };
    let [parameter] = parameters.as_slice() else {
        return Err(failure("G9 original parameter arity", parameters));
    };
    let NameMarker::Present(parameter_name) = parameter.name else {
        return Err(failure("G9 original parameter name", parameter));
    };
    need(
        sources.slice(parameter_name) == Ok("source")
            && matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))),
        "G9 original own source declaration",
        parameter,
    )?;
    let scope = match form {
        FunctionForm::ImplicitUnitBlock(b)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(b),
            ..
        } => *b,
        _ => return Err(failure("G9 original scan block", form)),
    };
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(scope)
        .map_err(|e| failure("G9 scope", e))?
        .payload()
    else {
        return Err(failure("G9 original scope Block", scope));
    };
    let mut sites = Vec::new();
    for &statement in elements {
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|e| failure("G9 original statement", e))?;
        let Statement::For { source, body, .. } = node.payload() else {
            continue;
        };
        let source_node = parsed
            .ast()
            .expressions()
            .get(*source)
            .map_err(|e| failure("G9 original source", e))?;
        need(
            matches!(source_node.payload(), Expression::Name)
                && sources.slice(source_node.span()) == Ok("source"),
            "G9 original ungrouped source Name",
            source_node,
        )?;
        let body_span = parsed
            .ast()
            .statements()
            .get(*body)
            .map_err(|e| failure("G9 original body", e))?
            .span();
        let transfers = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| {
                let breaking = match n.payload() {
                    Expression::Break { .. } => true,
                    Expression::Continue { .. } => false,
                    _ => return None,
                };
                contains(body_span, n.span()).then_some((id, breaking))
            })
            .collect::<Vec<_>>();
        need(
            transfers.len() <= 1,
            "G9 finite actual transfer",
            &transfers,
        )?;
        let (local, initializer) = one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if contains(body_span, n.span()) => Some((*name, *initializer)),
                    _ => None,
                }),
            "G9 original sole Guard local",
        )?;
        need(
            sources.slice(local) == Ok("local")
                && sources.slice(expression_span(parsed, initializer)?) == Ok("Guard()"),
            "G9 actual Guard initializer",
            (local, initializer),
        )?;
        sites.push(Site {
            statement,
            source: *source,
            body: *body,
            span: node.span(),
            transfer: transfers.first().copied(),
            local,
            initializer,
            initializer_span: expression_span(parsed, initializer)?,
        });
    }
    need(
        sites.len() == if case.form == 3 { 2 } else { 1 },
        "G9 exact original source-order For set",
        &sites,
    )?;
    Ok(Ast {
        scan: node.span(),
        name: *name,
        parameter: parameter_name,
        sites,
    })
}
fn last_use(
    case: &Case,
    parsed: &ParsedFile,
    original: &Ast,
    last: Span,
    point: DropPoint,
) -> Result<Option<ExpressionId>, Failure> {
    if case.form == 3 {
        let final_site = original
            .sites
            .last()
            .ok_or_else(|| failure("G9 final For", original))?;
        need(
            point == DropPoint::LoopExit(final_site.statement)
                && expression_span(parsed, final_site.source)? == last,
            "G9 repeat Drop at actual second LoopExit",
            (point, last),
        )?;
        return Ok(None);
    }
    let DropPoint::CallReturn(expression) = point else {
        return Err(failure("G9 source Drop at actual final read return", point));
    };
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|e| failure("G9 final read expression", e))?;
    need(
        contains(node.span(), last) && node.span().start() > original.sites[0].span.end(),
        "G9 final read contains original last root reference",
        (node, last),
    )?;
    if case.form != 0 {
        need(
            expression_span(parsed, argument(parsed, expression)?)? == last,
            "G9 final call exact original root argument",
            node,
        )?;
    }
    Ok(Some(expression))
}

fn argument(parsed: &ParsedFile, call: ExpressionId) -> Result<ExpressionId, Failure> {
    let node = parsed
        .ast()
        .expressions()
        .get(call)
        .map_err(|e| failure("G9 original caller AST", e))?;
    let Expression::Call { arguments, .. } = node.payload() else {
        return Err(failure("G9 actual caller Call", node));
    };
    let [argument] = arguments.as_slice() else {
        return Err(failure("G9 actual caller arity", arguments));
    };
    Ok(argument.value)
}

fn single_cleanup(
    site: &Site,
    p: &MapIterationOwnershipPlan,
    names: &NameResolution,
    drop: &DropFact,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.value_origin() == site.local),
        "G9 single actual Guard Drop",
    )?;
    need(guard.point()==site.body_point() && guard.condition().is_none()
        && guard.owner().is_some_and(|o|matches!(owned.cleanup_conditions().owner_value(o),Some(O::Expression{expression,origin}) if *expression==site.initializer && *origin==site.initializer_span)),"G9 single original Guard owner",guard)?;
    need(
        p.exits().len() == 2,
        "G9 single complete two exits",
        p.exits(),
    )?;
    for (kind, point, body) in [
        (site.kind(), site.body_point(), true),
        (E::Exhaustion, DropPoint::LoopExit(site.statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == kind && e.point() == point),
            "G9 single actual exit kind and point",
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
        if !body || matches!(site.kind(), E::Break(_)) {
            expected.extend([
                A::FinishProvider(site.statement),
                A::EndSource(site.statement),
            ]);
        }
        if drop.point() == point {
            expected.push(A::Drop(*drop));
        }
        let full = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, a)| (*at == point).then_some(*a))
            .collect::<Vec<_>>();
        need(
            exit.actions() == expected && full == expected,
            "G9 single full cleanup and exact final-only source Drop",
            (exit, full, expected),
        )?;
    }
    need(
        matches!(guard.target(),DropTarget::Named(local) if names.symbols().iter().any(|s|s.id()==local && s.span()==site.local)),
        "G9 Guard actual declaration target",
        guard,
    )
}

fn single_phis(
    site: &Site,
    p: &MapIterationOwnershipPlan,
    root: (SymbolId, Span),
    previous: Option<(CleanupOwnerValueId, CleanupConditionId)>,
    drop: &DropFact,
    owned: &OwnershipCheckedFile,
) -> Result<(CleanupOwnerValueId, CleanupConditionId), Failure> {
    let conditions = owned.cleanup_conditions();
    let h = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root.0 && v.boundary() == B::Header),
        "G9 original Header owner",
    )?;
    let x = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root.0 && v.boundary() == B::Exit),
        "G9 original Exit owner",
    )?;
    need(
        p.closure_phis().len() == 2
            && h.owner() != x.owner()
            && h.availability_selector() != x.availability_selector(),
        "G9 independent source owner boundaries",
        p.closure_phis(),
    )?;
    for phi in [h, x] {
        let selector = conditions
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("G9 original availability selector", phi))?;
        need(
            matches!(conditions.owner_value(phi.owner()),Some(O::IterationPhi{statement,symbol,boundary,origin}) if *statement==site.statement && *symbol==root.0 && *boundary==phi.boundary() && *origin==site.span)
                && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..} if statement==site.statement && boundary==phi.boundary())
                && selector.origin() == site.span
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.branch_count() == 2
                && matches!(conditions.get(phi.availability_condition()),Some(C::Choice{selector,branches}) if *selector==phi.availability_selector() && matches!(branches.as_slice(),[no,yes] if conditions.get(*no)==Some(&C::Never) && conditions.get(*yes)==Some(&C::Always))),
            "G9 full owner/selector/Choice identity",
            (phi, selector),
        )?;
    }
    need(
        p.closure_phi_incomings().len() == 3,
        "G9 full three incoming set",
        p.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in [
        (K::Entry, DropPoint::AfterExpression(site.source), B::Header),
        (
            site.incoming(),
            site.body_point(),
            if matches!(site.kind(), E::Break(_)) {
                B::Exit
            } else {
                B::Header
            },
        ),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ] {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "G9 each incoming kind",
        )?;
        let phi = if boundary == B::Header { h } else { x };
        let binding = one(edge.bindings().iter(), "G9 exactly one source binding")?;
        need(
            edge.point() == point
                && edge.boundary() == boundary
                && conditions.get(edge.condition()) == Some(&C::Always)
                && binding.target() == phi.owner()
                && binding.availability_selector() == phi.availability_selector()
                && binding.capture_slots_to_clear().is_empty()
                && binding.root_sources().is_empty()
                && binding.selector_writes().is_empty()
                && binding.origins().is_empty()
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions,
            "G9 exact target and no hidden incoming transport",
            edge,
        )?;
        if drop.point() == point {
            need(
                binding.values().is_empty()
                    && conditions.get(binding.available_when()) == Some(&C::Never),
                "G9 final immediately dropped owner writes Never to Exit",
                edge,
            )?;
        } else {
            let value = one(binding.values().iter(), "G9 actual live incoming owner")?;
            let correct = if kind == K::Entry {
                if let Some((owner, available)) = previous {
                    value.source() == owner && binding.available_when() == available
                } else {
                    conditions.get(binding.available_when()) == Some(&C::Always)
                        && matches!(conditions.owner_value(value.source()),Some(O::Parameter{symbol,origin}) if *symbol==root.0 && *origin==root.1)
                }
            } else {
                value.source() == h.owner()
                    && binding.available_when() == h.availability_condition()
            };
            need(
                correct && value.condition() == binding.available_when(),
                "G9 exact Parameter/previous Exit/current Header handoff",
                (edge, previous),
            )?;
        }
    }
    if drop.point() == DropPoint::LoopExit(site.statement) {
        need(
            drop.owner() == Some(h.owner()) && drop.condition() == Some(h.availability_condition()),
            "G9 immediate final Drop exact current Header",
            drop,
        )?;
    } else if matches!(drop.point(), DropPoint::CallReturn(_)) {
        need(
            drop.owner() == Some(x.owner()) && drop.condition() == Some(x.availability_condition()),
            "G9 final read Drop exact Exit owner",
            drop,
        )?;
    }
    Ok((x.owner(), x.availability_condition()))
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
    let snapshots = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            A::SaveOwnerSnapshot { owner, .. } => Some((
                point,
                action,
                owned.cleanup_conditions().owner_snapshot(*owner),
                owned.cleanup_conditions().owner_value(*owner),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    println!(
        "G9 ORIGINAL SNAPSHOTS case={} steps={snapshots:#?}",
        case.label
    );
    report.query(
        "G9 original single future-use owner facts",
        &single_query(case, sources, parsed, names, typed, owned),
    );
}
fn single_query(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let original = ast(case, sources, parsed)?;
    let root = one(
        names.symbols().iter().filter(|s| {
            s.span() == original.parameter
                && s.kind() == SymbolKind::ValueParameter
                && !s.is_synthetic()
        }),
        "G9 actual own parameter symbol",
    )?;
    let signature = one(
        typed
            .callables()
            .iter()
            .filter(|c| c.parameter_symbols() == [Some(root.id())]),
        "G9 actual scan signature",
    )?;
    need(
        names
            .symbols()
            .iter()
            .any(|s| s.id() == signature.symbol() && s.span() == original.name)
            && signature.receiver().is_none(),
        "G9 scan declaration and parameter root",
        signature,
    )?;
    let source_type = one(signature.parameters().iter(), "G9 sole source type")?.ty;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        matches!(signature.parameters(),[p] if p.mode==ParameterMode::Value)
            && typed.is_resource_type(source_type) == Some(false)
            && matches!(typed.types().get(source_type),Some(TypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && matches!(arguments.as_slice(),[k,v] if typed.types().get(*k)==Some(&TypeKind::Builtin(BuiltinType::String)) && k==v)),
        "G9 Pure exact owned Map<String,String> signature",
        signature,
    )?;
    let refs = names
        .references()
        .iter()
        .filter(|r| {
            r.target() == &ReferenceTarget::Symbol(root.id()) && contains(original.scan, r.span())
        })
        .map(|r| r.span())
        .collect::<Vec<_>>();
    let last = *refs
        .iter()
        .max_by_key(|s| (s.start(), s.end()))
        .ok_or_else(|| failure("G9 original last source reference", &refs))?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(root.id())),
        "G9 unique Pure source Drop",
    )?;
    need(
        drop.value_origin() == original.parameter
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none(),
        "G9 source Drop actual declaration identity",
        drop,
    )?;
    if let Some(id) = last_use(case, parsed, &original, last, drop.point())? {
        if case.form == 0 {
            let size = typed
                .map_size(id)
                .ok_or_else(|| failure("G9 original final size descriptor", id))?;
            need(
                expression_span(parsed, size.receiver())? == last && size.map_type() == source_type,
                "G9 exact final size root and type",
                size,
            )?;
        } else {
            let call = typed
                .call(id)
                .ok_or_else(|| failure("G9 final readMap call", id))?;
            need(
                matches!(call.target(),CallableTarget::Source(s) if names.symbols().iter().any(|n|n.id()==s && sources.slice(n.span())==Ok("readMap")))
                    && matches!(call.arguments(),[a] if a.mode()==ParameterMode::Borrow && a.parameter_type()==source_type),
                "G9 final real readMap Borrow signature",
                call,
            )?;
        }
    }
    need(
        typed.map_iterations().len() == original.sites.len()
            && owned.map_iterations().len() == original.sites.len(),
        "G9 complete original For descriptor set",
        (&original, typed.map_iterations()),
    )?;
    let mut previous = None;
    for site in &original.sites {
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("G9 actual single Map plan", site))?;
        let d = p.descriptor();
        let source_span = expression_span(parsed, site.source)?;
        need(
            typed.map_iteration(site.statement) == Some(d)
                && d.source() == site.source
                && d.source_type() == source_type
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && typed.types().get(d.key_type()) == Some(&TypeKind::Builtin(BuiltinType::String))
                && d.key_type() == d.value_type()
                && p.source_access() == MapIterationSourceAccess::Owned
                && matches!(p.source(),Some(LoanTarget::Place(path)) if path.is_root() && path.root()==root.id())
                && names.references().iter().any(|r| {
                    r.span() == source_span && r.target() == &ReferenceTarget::Symbol(root.id())
                }),
            "G9 same original For source, Pure root, kind and K/V",
            p,
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && p.loans()[index].statement() == site.statement
                    && p.loans()[index].source() == site.source
                    && p.loans()[index].binding() == binding
                    && p.loans()[index].component()
                        == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                    && p.loans()[index].kind() == LoanKind::Shared
                    && Some(p.loans()[index].origin()) == p.source(),
                "G9 exact original separate Shared components",
                p.loans(),
            )?;
        }
        single_cleanup(site, p, names, &drop, owned)?;
        previous = Some(single_phis(
            site,
            p,
            (root.id(), original.parameter),
            previous,
            &drop,
            owned,
        )?);
    }
    let calls = typed
        .calls()
        .iter()
        .filter(|c| c.target() == CallableTarget::Source(signature.symbol()))
        .collect::<Vec<_>>();
    need(
        calls.len() == 2,
        "G9 both actual caller Value sites",
        &calls,
    )?;
    for call in calls {
        let argument = argument(parsed, call.expression())?;
        let span = expression_span(parsed, argument)?;
        let caller_root = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == span)
                .filter_map(|r| match r.target() {
                    ReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "G9 original caller root",
        )?;
        need(
            matches!(call.arguments(),[a] if a.mode()==ParameterMode::Value && a.argument_index()==0 && a.parameter_index()==0 && a.parameter_type()==source_type)
                && typed.expression_type(argument) == Some(source_type)
                && !owned.loans().iter().any(|l| l.call() == call.expression()),
            "G9 original caller Value contract/root reference without loan",
            (call, caller_root),
        )?;
    }
    Ok(())
}

fn unit_cleanup(
    site: &Site,
    p: &UnitMapIterationOwnershipPlan,
    drop: &UnitDropFact,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let unit = p.descriptor().statement().source_unit();
    let statement = p.descriptor().statement();
    let point = match site.body_point() {
        DropPoint::AfterStatement(s) => {
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
        }
        DropPoint::ControlTransfer(e) => {
            UnitDropPoint::ControlTransfer(UnitExpressionId::new(unit, e))
        }
        other => return Err(failure("G9 original unit body point", other)),
    };
    let kind = match site.kind() {
        E::Fallthrough => UE::Fallthrough,
        E::Continue(e) => UE::Continue(UnitExpressionId::new(unit, e)),
        E::Break(e) => UE::Break(UnitExpressionId::new(unit, e)),
        other => return Err(failure("G9 actual unit body kind", other)),
    };
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.value_origin() == site.local),
        "G9 unit actual Guard Drop",
    )?;
    need(
        guard.point() == point,
        "G9 unit Guard Drop exact original body exit",
        guard,
    )?;
    need(p.exits().len() == 2, "G9 unit complete exits", p.exits())?;
    for (kind, point, body) in [
        (kind, point, true),
        (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == kind && e.point() == point),
            "G9 unit original exit",
        )?;
        let mut expected = if body {
            vec![
                UA::Drop(guard),
                UA::EndBinding {
                    statement,
                    symbol: p.descriptor().value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: p.descriptor().key_binding(),
                },
            ]
        } else {
            vec![]
        };
        if !body || matches!(kind, UE::Break(_)) {
            expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
        }
        if drop.point() == point {
            expected.push(UA::Drop(*drop));
        }
        need(
            exit.actions() == expected
                && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "G9 unit complete ordered exit and final-only source Drop",
            (exit, expected),
        )?;
    }
    Ok(())
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
    report.query(
        "G9 original unit future-use source facts",
        &unit_query(case, sources, parsed, names, typed, owned),
    );
}
fn unit_query(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let original = ast(case, sources, parsed)?;
    let source = one(
        names.names().source_units().iter().filter(|u| {
            u.resolution()
                .symbols()
                .iter()
                .any(|s| s.span() == original.parameter)
        }),
        "G9 actual parameter source unit",
    )?;
    let symbol = one(
        source.resolution().symbols().iter().filter(|s| {
            s.span() == original.parameter
                && s.kind() == SymbolKind::ValueParameter
                && !s.is_synthetic()
        }),
        "G9 qualified original parameter",
    )?;
    let first_source_span = expression_span(parsed, original.sites[0].source)?;
    let root = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == first_source_span)
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "G9 actual qualified source root",
    )?;
    need(
        root.symbol() == symbol.id() && root.source_unit() == source.source_unit(),
        "G9 root source-unit identity",
        root,
    )?;
    let declaration = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| matches!(c.parameters(),[p] if p.symbol()==Some(root)))
        }),
        "G9 actual qualified scan signature",
    )?;
    let signature = declaration
        .callable()
        .ok_or_else(|| failure("G9 unit scan callable", declaration))?;
    let source_type = one(signature.parameters().iter(), "G9 unit sole source type")?.ty();
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        signature.name_span() == original.name
            && signature.receiver().is_none()
            && matches!(signature.parameters(),[p] if p.mode()==ParameterMode::Value)
            && typed.is_resource_type(source_type) == Some(false)
            && matches!(typed.types().get(source_type),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && matches!(arguments.as_slice(),[k,v] if typed.types().get(*k)==Some(&UnitTypeKind::Builtin(BuiltinType::String)) && k==v)),
        "G9 unit Pure exact Value Map<String,String> signature",
        signature,
    )?;
    let last = names
        .names()
        .references()
        .iter()
        .filter(|r| {
            r.target() == &UnitReferenceTarget::Symbol(root) && contains(original.scan, r.span())
        })
        .map(|r| r.span())
        .max_by_key(|s| (s.start(), s.end()))
        .ok_or_else(|| failure("G9 unit original last reference", root))?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root)),
        "G9 unique qualified Pure source Drop",
    )?;
    need(
        drop.value_origin() == original.parameter,
        "G9 unit original source Drop origin",
        drop,
    )?;
    let point = match drop.point() {
        UnitDropPoint::CallReturn(e) => DropPoint::CallReturn(e.expression()),
        UnitDropPoint::LoopExit(s) => DropPoint::LoopExit(s.statement()),
        other => return Err(failure("G9 unit precise last-use Drop point", other)),
    };
    if let Some(id) = last_use(case, parsed, &original, last, point)? {
        let id = UnitExpressionId::new(root.source_unit(), id);
        if case.form == 0 {
            let size = typed
                .map_size(id)
                .ok_or_else(|| failure("G9 actual qualified final size", id))?;
            need(
                size.receiver().source_unit() == root.source_unit()
                    && expression_span(parsed, size.receiver().expression())? == last
                    && size.map_type() == source_type,
                "G9 unit exact final size root/type",
                size,
            )?;
        } else {
            let call = typed
                .call(id)
                .ok_or_else(|| failure("G9 unit original final Borrow call", id))?;
            need(
                matches!(call.target(),UnitCallTarget::Declaration(target) if names.names().index().declarations().iter().any(|d|d.id()==target && sources.slice(d.name_span())==Ok("readMap")))
                    && matches!(call.arguments(),[a] if a.mode()==ParameterMode::Borrow && a.parameter_type()==source_type),
                "G9 unit exact final readMap Borrow result",
                call,
            )?;
        }
    }
    need(
        typed.map_iterations().len() == original.sites.len()
            && owned.map_iterations().len() == original.sites.len(),
        "G9 unit complete original For set",
        typed.map_iterations(),
    )?;
    for site in &original.sites {
        let statement = UnitStatementId::new(root.source_unit(), site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("G9 actual qualified Map plan", statement))?;
        let d = p.descriptor();
        let source_span = expression_span(parsed, site.source)?;
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == UnitExpressionId::new(root.source_unit(), site.source)
                && d.source_type() == source_type
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && typed.types().get(d.key_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && d.key_type() == d.value_type()
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source(),Some(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root)
                && names.names().references().iter().any(|r| {
                    r.span() == source_span && r.target() == &UnitReferenceTarget::Symbol(root)
                }),
            "G9 unit exact original For source/capability/type",
            p,
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && p.loans()[index].statement() == statement
                    && p.loans()[index].source() == d.source()
                    && p.loans()[index].binding() == binding
                    && p.loans()[index].component()
                        == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                    && p.loans()[index].kind() == LoanKind::Shared
                    && Some(p.loans()[index].origin()) == p.source(),
                "G9 unit original independent K/V loan transport",
                p.loans(),
            )?;
        }
        unit_cleanup(site, p, &drop, owned)?;
    }
    let calls = typed
        .calls()
        .iter()
        .filter(|c| c.target() == UnitCallTarget::Declaration(declaration.declaration()))
        .collect::<Vec<_>>();
    need(calls.len() == 2, "G9 unit both actual caller sites", &calls)?;
    for call in calls {
        let argument = argument(parsed, call.expression().expression())?;
        let arg = UnitExpressionId::new(root.source_unit(), argument);
        let span = expression_span(parsed, argument)?;
        let caller_root = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span)
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "G9 qualified original caller root",
        )?;
        let contracts = owned
            .call_argument_contracts_for(call.expression())
            .collect::<Vec<_>>();
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|d| d.call() == call.expression())
            .collect::<Vec<_>>();
        need(
            matches!(call.arguments(),[a] if a.mode()==ParameterMode::Value && a.argument_index()==0 && a.parameter_index()==0 && a.parameter_type()==source_type)
                && typed.expression_type(arg) == Some(source_type)
                && matches!(contracts.as_slice(),[c] if c.argument()==arg && c.parameter_index()==0 && c.kind()==UnitCallArgumentOwnershipKind::Value)
                && matches!(deliveries.as_slice(),[d] if d.argument()==arg && d.kind()==UnitValueDeliveryKind::Move && d.place().is_some_and(|p|p.is_root() && p.root()==caller_root))
                && !owned.loans().iter().any(|l| l.call() == call.expression()),
            "G9 unit exact caller Value Move identity",
            (call, caller_root, contracts, deliveries),
        )?;
    }
    Ok(())
}
