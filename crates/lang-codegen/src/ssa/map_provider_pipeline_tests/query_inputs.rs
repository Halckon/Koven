//! 同次 FE 公开 facts 到 SSA source origin 的只读桥；不调用生产 qualification。
use super::{
    fixture::{Failure, Fixture, failure, loop_site},
    identity,
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, ReferenceTarget, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition, CleanupConditions, CleanupOwnerValue, CleanupSelection,
        CleanupSelectorSource, CompilationUnitOwnership, DropPoint, DropTarget,
        IterationCleanupAction as A, IterationClosurePhiBinding, IterationExitKind,
        IterationPhiBoundary, IterationPhiIncomingKind, LoanTarget, OwnershipCheckedFile,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA, UnitIterationExitKind,
        UnitLoanTarget,
    },
    parser::{Expression, Item, NameMarker, Statement},
    source::Span,
    type_checking::{
        CallableTarget, CompilationUnitTypes, ParameterMode, TypedFile, UnitCallTarget,
        UnitExpressionId, UnitStatementId,
    },
};

pub(super) fn need(
    ok: bool,
    stage: &'static str,
    detail: impl std::fmt::Debug,
) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(failure(stage, detail))
    }
}

pub(super) fn one<T>(
    mut values: impl Iterator<Item = T>,
    stage: &'static str,
) -> Result<T, Failure> {
    let result = values
        .next()
        .ok_or_else(|| failure(stage, "missing unique entity"))?;
    need(values.next().is_none(), stage, "multiple matching entities")?;
    Ok(result)
}

#[derive(Clone, Debug)]
pub(super) struct Inputs {
    pub scan: Span,
    pub read: Span,
    pub loop_span: Span,
    pub parameter: Span,
    pub parameter_slot: usize,
    pub call: Span,
    pub drop_origin: Option<Span>,
    pub owned: bool,
}

fn function_span(f: &Fixture, name: Span) -> Result<Span, Failure> {
    one(f.parsed.roots().iter().filter_map(|root| {
        let node = f.parsed.ast().items().get(*root).ok()?;
        matches!(node.payload(), Item::Function { name: NameMarker::Present(actual), .. } if *actual == name)
            .then_some(node.span())
    }), "FE exact function root")
}

fn call(f: &Fixture, outer: Span) -> Result<(ExpressionId, Span, [Span; 2]), Failure> {
    let (id, node) = one(
        f.parsed.ast().expressions().iter().filter(|(_, node)| {
            let s = node.span();
            s.source_id() == outer.source_id()
                && outer.start() <= s.start()
                && s.end() <= outer.end()
                && matches!(node.payload(), Expression::Call { .. })
        }),
        "FE body actual call",
    )?;
    let Expression::Call { arguments, .. } = node.payload() else {
        return Err(failure("FE actual call shape", node));
    };
    need(arguments.len() == 2, "FE body arguments", arguments)?;
    let mut spans = Vec::new();
    for argument in arguments {
        let node = f
            .parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|e| failure("FE call argument AST", e))?;
        need(
            matches!(node.payload(), Expression::Name),
            "FE component argument is Name",
            node,
        )?;
        spans.push(node.span());
    }
    Ok((id, node.span(), [spans[0], spans[1]]))
}

pub(super) fn single(
    f: &Fixture,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Inputs, Failure> {
    let site = loop_site(f)?;
    let d = typed
        .map_iteration(site.statement)
        .ok_or_else(|| failure("FE single descriptor", &site))?;
    let p = owned
        .map_iteration(site.statement)
        .ok_or_else(|| failure("FE single plan", &site))?;
    let Some(LoanTarget::Place(path)) = p.source() else {
        return Err(failure("FE source root", p));
    };
    let source = path.root();
    let symbol = one(
        names.symbols().iter().filter(|s| s.id() == source),
        "FE source parameter symbol",
    )?;
    let (signature, slot) = one(
        typed.callables().iter().flat_map(|c| {
            c.parameter_symbols()
                .iter()
                .enumerate()
                .filter_map(move |(i, s)| (*s == Some(source)).then_some((c, i)))
        }),
        "FE source actual parameter slot",
    )?;
    let scan_name = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.id() == signature.symbol()),
        "FE scan declaration",
    )?
    .span();
    need(
        signature.receiver().is_none(),
        "FE scan has no receiver",
        signature,
    )?;
    let (call_id, call_span, argument_spans) = call(f, site.span)?;
    let c = typed
        .call(call_id)
        .ok_or_else(|| failure("FE body call descriptor", call_id))?;
    let CallableTarget::Source(read_symbol) = c.target() else {
        return Err(failure("FE read target", c));
    };
    let read_name = one(
        names.symbols().iter().filter(|s| s.id() == read_symbol),
        "FE actual read declaration",
    )?
    .span();
    need(
        c.arguments().len() == 2 && c.receiver().is_none(),
        "FE read actual argument map",
        c,
    )?;
    for (i, argument) in c.arguments().iter().enumerate() {
        let refs = names
            .references()
            .iter()
            .filter(|r| r.span() == argument_spans[i])
            .collect::<Vec<_>>();
        need(
            matches!(refs.as_slice(), [r] if r.target() == &ReferenceTarget::Symbol(d.bindings()[i]))
                && argument.argument_index() == i
                && argument.parameter_index() == i
                && argument.mode() == ParameterMode::Borrow
                && argument.parameter_type() == [d.key_type(), d.value_type()][i],
            "FE read argument binding/type/Borrow",
            (argument, refs),
        )?;
    }
    let Statement::For { body, .. } = f
        .parsed
        .ast()
        .statements()
        .get(site.statement)
        .map_err(|e| failure("FE For body", e))?
        .payload()
    else {
        return Err(failure("FE actual For shape", site.statement));
    };
    need(p.exits().len() == 2, "FE only normal exits", p.exits())?;
    let fall = one(
        p.exits()
            .iter()
            .filter(|e| e.kind() == IterationExitKind::Fallthrough),
        "FE fallthrough",
    )?;
    let end = one(
        p.exits()
            .iter()
            .filter(|e| e.kind() == IterationExitKind::Exhaustion),
        "FE exhaustion",
    )?;
    let expected = [
        A::EndBinding {
            statement: site.statement,
            symbol: d.value_binding(),
        },
        A::EndBinding {
            statement: site.statement,
            symbol: d.key_binding(),
        },
    ];
    need(
        fall.point() == DropPoint::AfterStatement(*body)
            && fall.actions() == expected
            && end.point() == DropPoint::LoopExit(site.statement)
            && owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| (*point == fall.point()).then_some(*action))
                .collect::<Vec<_>>()
                == fall.actions()
            && owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| (*point == end.point()).then_some(*action))
                .collect::<Vec<_>>()
                == end.actions(),
        "FE actual cleanup point and value/key order",
        (fall, end),
    )?;
    let drops = owned
        .drops()
        .iter()
        .filter(|d| d.target() == DropTarget::Named(source))
        .collect::<Vec<_>>();
    let drop_origin = if f.map {
        let drop = *one(drops.iter().copied(), "FE single source drop")?;
        need(
            drop.point() == end.point()
                && drop.value_origin() == symbol.span()
                && end.actions()
                    == [
                        A::FinishProvider(site.statement),
                        A::EndSource(site.statement),
                        A::Drop(drop),
                    ],
            "FE actual source drop action",
            drop,
        )?;
        // Query each published edge relationally. No owner/condition/selector number is an oracle.
        let header = one(
            p.closure_phis()
                .iter()
                .filter(|v| v.symbol() == source && v.boundary() == IterationPhiBoundary::Header),
            "FE source header phi",
        )?;
        let exit = one(
            p.closure_phis()
                .iter()
                .filter(|v| v.symbol() == source && v.boundary() == IterationPhiBoundary::Exit),
            "FE source exit phi",
        )?;
        need(
            p.closure_phis().len() == 2
                && p.closure_phi_incomings().len() == 3
                && header.owner() != exit.owner()
                && drop.owner() == Some(header.owner())
                && drop.condition() == Some(header.availability_condition()),
            "FE source owner phi/drop identity",
            (header, exit, drop),
        )?;
        for kind in [
            IterationPhiIncomingKind::Entry,
            IterationPhiIncomingKind::Fallthrough,
            IterationPhiIncomingKind::Exhaustion,
        ] {
            one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|incoming| incoming.kind() == kind),
                "FE exact owner edge kind",
            )?;
        }
        let conditions = owned.cleanup_conditions();
        for phi in [header, exit] {
            let owner = conditions.owner_value(phi.owner());
            need(
                matches!(owner, Some(CleanupOwnerValue::IterationPhi { statement, boundary, symbol, origin })
                if *statement == site.statement && *boundary == phi.boundary() && *symbol == source && *origin == site.span),
                "FE actual owner phi source identity",
                owner,
            )?;
            phi_condition(conditions, phi, site.statement, site.span)?;
        }

        for incoming in p.closure_phi_incomings() {
            let binding = one(incoming.bindings().iter(), "FE owner incoming binding")?;
            match incoming.kind() {
                IterationPhiIncomingKind::Entry => {
                    let value = one(binding.values().iter(), "FE parameter owner seed")?;
                    need(
                        incoming.boundary() == IterationPhiBoundary::Header
                            && incoming.point() == DropPoint::AfterExpression(site.source)
                            && binding.target() == header.owner()
                            && value.condition() == binding.available_when()
                            && conditions.get(binding.available_when())
                                == Some(&CleanupCondition::Always)
                            && matches!(conditions.owner_value(value.source()), Some(CleanupOwnerValue::Parameter { symbol: s, origin }) if *s == source && *origin == symbol.span()),
                        "FE owner seed is actual parameter",
                        incoming,
                    )?;
                }
                IterationPhiIncomingKind::Fallthrough => {
                    let value = one(binding.values().iter(), "FE current header owner backedge")?;
                    need(
                        incoming.boundary() == IterationPhiBoundary::Header
                            && incoming.point() == fall.point()
                            && binding.target() == header.owner()
                            && value.source() == header.owner()
                            && value.condition() == header.availability_condition()
                            && binding.available_when() == header.availability_condition(),
                        "FE current owner backedge",
                        incoming,
                    )?;
                }
                IterationPhiIncomingKind::Exhaustion => {
                    need(
                        incoming.boundary() == IterationPhiBoundary::Exit
                            && incoming.point() == end.point()
                            && binding.target() == exit.owner()
                            && binding.values().is_empty()
                            && conditions.get(binding.available_when())
                                == Some(&CleanupCondition::Never),
                        "FE owner retired at exhaustion",
                        incoming,
                    )?;
                }
                _ => return Err(failure("FE unexpected owner incoming", incoming)),
            }
        }
        Some(drop.value_origin())
    } else {
        need(
            drops.is_empty()
                && end.actions()
                    == [
                        A::FinishProvider(site.statement),
                        A::EndSource(site.statement),
                    ],
            "FE borrowed source has no drop",
            (drops, end),
        )?;
        None
    };
    finish(
        f,
        Inputs {
            scan: function_span(f, scan_name)?,
            read: function_span(f, read_name)?,
            loop_span: site.span,
            parameter: symbol.span(),
            parameter_slot: slot,
            call: call_span,
            drop_origin,
            owned: f.map,
        },
    )
}

pub(super) fn unit(
    f: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let site = loop_site(f)?;
    let local = one(names.names().source_units().iter(), "FE unit source")?;
    let unit = local.source_unit();
    let statement = UnitStatementId::new(unit, site.statement);
    let d = typed
        .map_iteration(statement)
        .ok_or_else(|| failure("FE unit descriptor", statement))?;
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("FE unit plan", statement))?;
    let Some(UnitLoanTarget::Place(path)) = p.source() else {
        return Err(failure("FE unit source root", p));
    };
    let source = path.root();
    need(
        source.source_unit() == unit,
        "FE parameter source-unit identity",
        source,
    )?;
    let symbol = one(
        local
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.id() == source.symbol()),
        "FE unit parameter symbol",
    )?;
    let (signature, slot) = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .flat_map(|c| {
                c.parameters()
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, p)| (p.symbol() == Some(source)).then_some((c, i)))
            }),
        "FE unit source actual parameter slot",
    )?;
    need(
        signature.receiver().is_none(),
        "FE unit scan has no receiver",
        signature,
    )?;
    let (call_id, call_span, argument_spans) = call(f, site.span)?;
    let c = typed
        .call(UnitExpressionId::new(unit, call_id))
        .ok_or_else(|| failure("FE unit body call", call_id))?;
    let UnitCallTarget::Declaration(read) = c.target() else {
        return Err(failure("FE unit read target", c));
    };
    let declaration = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.id() == read),
        "FE unit read declaration",
    )?;
    need(
        declaration.source_unit() == unit && c.arguments().len() == 2 && c.receiver().is_none(),
        "FE unit read argument map",
        c,
    )?;
    for (i, argument) in c.arguments().iter().enumerate() {
        let refs = names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == argument_spans[i])
            .collect::<Vec<_>>();
        need(
            matches!(refs.as_slice(), [r] if r.target() == &UnitReferenceTarget::Symbol(d.bindings()[i]))
                && argument.argument_index() == i
                && argument.parameter_index() == i
                && argument.mode() == ParameterMode::Borrow
                && argument.parameter_type() == [d.key_type(), d.value_type()][i],
            "FE unit read binding/type/Borrow",
            (argument, refs),
        )?;
    }
    let Statement::For { body, .. } = f
        .parsed
        .ast()
        .statements()
        .get(site.statement)
        .map_err(|e| failure("FE unit For body", e))?
        .payload()
    else {
        return Err(failure("FE actual For shape", site.statement));
    };
    need(p.exits().len() == 2, "FE unit only normal exits", p.exits())?;
    let fall = one(
        p.exits()
            .iter()
            .filter(|e| e.kind() == UnitIterationExitKind::Fallthrough),
        "FE unit fallthrough",
    )?;
    let end = one(
        p.exits()
            .iter()
            .filter(|e| e.kind() == UnitIterationExitKind::Exhaustion),
        "FE unit exhaustion",
    )?;
    let expected = [
        UA::EndBinding {
            statement,
            symbol: d.value_binding(),
        },
        UA::EndBinding {
            statement,
            symbol: d.key_binding(),
        },
    ];
    need(
        fall.point() == UnitDropPoint::AfterStatement(UnitStatementId::new(unit, *body))
            && fall.actions() == expected
            && end.point() == UnitDropPoint::LoopExit(statement)
            && owned.iteration_cleanup_at(fall.point()) == Some(fall.actions())
            && owned.iteration_cleanup_at(end.point()) == Some(end.actions()),
        "FE unit actual cleanup points/order",
        (fall, end),
    )?;
    let drops = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(source))
        .collect::<Vec<_>>();
    let drop_origin = if f.map {
        let drop = *one(drops.iter().copied(), "FE unit source drop")?;
        need(
            drop.point() == end.point()
                && drop.value_origin() == symbol.span()
                && end.actions()
                    == [
                        UA::FinishProvider(statement),
                        UA::EndSource(statement),
                        UA::Drop(drop),
                    ],
            "FE unit actual source drop action",
            drop,
        )?;
        Some(drop.value_origin())
    } else {
        need(
            drops.is_empty()
                && end.actions() == [UA::FinishProvider(statement), UA::EndSource(statement)],
            "FE unit borrowed source no drop",
            (drops, end),
        )?;
        None
    };
    finish(
        f,
        Inputs {
            scan: function_span(f, signature.name_span())?,
            read: function_span(f, declaration.name_span())?,
            loop_span: site.span,
            parameter: symbol.span(),
            parameter_slot: slot,
            call: call_span,
            drop_origin,
            owned: f.map,
        },
    )
}

fn finish(f: &Fixture, inputs: Inputs) -> Result<Inputs, Failure> {
    let functions = identity::source_functions(f)?;
    need(
        functions.contains(&("scan", inputs.scan)) && functions.contains(&("read", inputs.read)),
        "FE actual symbols map to original scan/read roots",
        &inputs,
    )?;
    println!("SPEC0331 PROVIDER IDENTITY FE {inputs:#?}");
    Ok(inputs)
}

fn phi_condition(
    conditions: &CleanupConditions,
    phi: &IterationClosurePhiBinding,
    statement: lang_frontend::ast::StatementId,
    origin: Span,
) -> Result<(), Failure> {
    let selector = conditions
        .selector(phi.availability_selector())
        .ok_or_else(|| failure("FE phi availability selector", phi))?;
    need(
        matches!(selector.source(), CleanupSelectorSource::IterationPhi { statement: s, boundary, .. } if s == statement && boundary == phi.boundary())
            && selector.selection() == CleanupSelection::IterationPresence
            && selector.origin() == origin
            && selector.branch_count() == 2,
        "FE actual phi selector source and presence domain",
        selector,
    )?;
    let Some(CleanupCondition::Choice { selector, branches }) =
        conditions.get(phi.availability_condition())
    else {
        return Err(failure("FE actual phi availability Choice", phi));
    };
    need(
        *selector == phi.availability_selector()
            && branches.len() == 2
            && conditions.get(branches[0]) == Some(&CleanupCondition::Never)
            && conditions.get(branches[1]) == Some(&CleanupCondition::Always),
        "FE phi Choice selector/branches",
        phi,
    )
}
