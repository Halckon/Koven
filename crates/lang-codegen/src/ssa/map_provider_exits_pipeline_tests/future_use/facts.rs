//! Original public FE lifetime/owner handoffs; never call lowering qualification.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, IterationExitKind as E, LoanKind,
        MapIterationComponent, OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropFact,
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
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
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
