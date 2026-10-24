//! 同次 FE 公开 facts 到 SSA source origin 的只读桥；不调用生产 qualification。
use super::{
    fixture::{Failure, Fixture, failure, loop_site},
    identity,
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, OwnershipBindingKind, UnitDropPoint,
        UnitDropTarget, UnitIterationCleanupAction as UA, UnitIterationExitKind,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, Item, NameMarker, Statement},
    source::Span,
    type_checking::{
        CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId,
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
    let parameter = signature
        .parameters()
        .get(slot)
        .ok_or_else(|| failure("FE unit original parameter slot", slot))?;
    let source_references = names
        .names()
        .references()
        .iter()
        .filter(|reference| reference.span() == site.source_span)
        .collect::<Vec<_>>();
    need(
        signature.receiver().is_none()
            && signature.parameters().len() == 1
            && path.is_root()
            && symbol.kind() == SymbolKind::ValueParameter
            && !symbol.is_synthetic()
            && symbol.span().source_id() == f.parsed.source_id()
            && parameter.symbol() == Some(source)
            && parameter.ty() == d.source_type()
            && typed.symbol_type(source) == Some(d.source_type())
            && p.descriptor() == d
            && d.source() == UnitExpressionId::new(unit, site.source)
            && matches!(p.source_kind(), IterationSource::BorrowedSource(origin)
                if Some(origin) == p.source())
            && matches!(source_references.as_slice(), [reference]
                if reference.source_unit() == unit
                    && reference.target() == &UnitReferenceTarget::Symbol(source))
            && parameter.mode()
                == if f.map {
                    ParameterMode::Value
                } else {
                    ParameterMode::Borrow
                }
            && owned.binding_kind(source)
                == Some(if f.map {
                    OwnershipBindingKind::Owned
                } else {
                    OwnershipBindingKind::Shared
                })
            && p.source_access()
                == if f.map {
                    UnitIterationSourceAccess::Owned
                } else {
                    UnitIterationSourceAccess::Shared
                },
        "FE unit real parameter seed and source capability",
        (signature, parameter, p, source_references),
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
    // Unit 没有旧 FE closure-phi/selector 表。这里只读取真实 source/drop 事实；
    // 旧 seed/incoming/availability/retirement 义务由 query_owner 的完整 SSA 合同重建。
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
