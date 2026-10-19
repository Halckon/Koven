//! Source-qualified concrete Int facts; current owner identity is closed by the SSA query.
use super::*;
fn root(n: &ValidatedCompilationUnitNames, span: Span) -> Result<UnitSymbolId, Failure> {
    let (unit, symbol) = one(
        n.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == span && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "Int qualified actual original symbol",
    )?;
    one(
        n.names()
            .references()
            .iter()
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(id)
                    if id.source_unit() == unit && id.symbol() == symbol =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter(),
        "Int actual qualified local reference identity",
    )
}
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_value_births::int_storage) fn check(
    input: Input<'_>,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let (c, _, p, i) = input;
    let unit = one(n.names().source_units().iter(), "Int original unit")?.source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let decl = |marker| {
        one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && d.name_span() == marker)
                .map(|d| d.id()),
            "Int actual original qualified declaration",
        )
    };
    let initial = root(n, i.initial)?;
    let source = root(n, i.source)?;
    let constructed = t
        .map_construction(e(i.construction))
        .ok_or_else(|| failure("Int qualified actual construction", i.construction))?;
    let put = t
        .map_put(e(i.put))
        .ok_or_else(|| failure("Int qualified actual put", i.put))?;
    let consumed = t
        .map_consume(e(i.consume))
        .ok_or_else(|| failure("Int qualified actual consume", i.consume))?;
    let int = constructed.key_type();
    let mutable = constructed.map_type();
    let map = consumed.result_type();
    need(
        constructed.expression() == e(i.construction)
            && constructed.callable() == IntrinsicCallable::MutableMapOf
            && constructed.value_type() == int
            && t.types().get(int) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            && t.copyability(int) == Copyability::Copyable
            && t.symbol_type(initial) == Some(mutable)
            && o.formed_owned_locals().contains(&initial)
            && put.expression() == e(i.put)
            && put.receiver() == e(receiver(p, i.put))
            && put.key() == e(i.payload[0])
            && put.value() == e(i.payload[1])
            && put.key_type() == int
            && put.value_type() == int
            && i.payload
                .iter()
                .all(|id| t.expression_type(e(*id)) == Some(int))
            && consumed.expression() == e(i.consume)
            && consumed.receiver() == e(receiver(p, i.consume))
            && consumed.source() == Some((initial, mutable))
            && consumed.receiver_type() == mutable
            && consumed.key_type() == int
            && consumed.value_type() == int
            && t.expression_type(e(i.consume)) == Some(map)
            && t.types().get(map)
                == Some(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map,
                    arguments: vec![int, int],
                }),
        "Int qualified exact Copy construction/put(1,2)/consume storage",
        (constructed, put, consumed),
    )?;
    for id in [put.receiver(), consumed.receiver()] {
        let r = one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(p, id.expression())),
            "Int qualified factory receiver",
        )?;
        need(
            r.target() == &UnitReferenceTarget::Symbol(initial),
            "Int qualified put/consume current same m root",
            r,
        )?;
    }
    need(
        !o.drops().iter().any(|d| {
            d.target() == UnitDropTarget::Named(initial)
                || d.target() == UnitDropTarget::Temporary(e(i.consume))
        }),
        "Int qualified transferred initial/return not duplicated cleanup",
        o.drops(),
    )?;
    let call = t
        .call(e(i.birth))
        .ok_or_else(|| failure("Int qualified actual makeInt Call", i.birth))?;
    need(
        call.target() == UnitCallTarget::Declaration(decl(i.make_name)?)
            && call.arguments().is_empty()
            && call.return_type() == map
            && call.result_source() == CallableResultSource::Owned
            && call.borrow_return().is_none()
            && t.symbol_type(source) == Some(map)
            && o.formed_owned_locals().contains(&source)
            && !o
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Temporary(e(i.birth))),
        "Int qualified exact concrete named source birth",
        call,
    )?;
    let reader = t
        .signatures()
        .declaration(decl(i.reader_name)?)
        .and_then(|d| d.callable())
        .ok_or_else(|| failure("Int qualified actual reader signature", i.reader_name))?;
    need(
        reader.parameters().len() == 2
            && reader
                .parameters()
                .iter()
                .all(|a| a.mode() == ParameterMode::Borrow && a.ty() == int)
            && reader.parameters()[0].symbol() == Some(root(n, i.reader_params[0])?)
            && reader.parameters()[1].symbol() == Some(root(n, i.reader_params[1])?),
        "Int qualified actual declared reader parameter ABI/Copy types",
        reader,
    )?;
    for index in 0..2 {
        let r = one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(p, i.compared[index])),
            "Int qualified actual compared parameter",
        )?;
        need(
            r.target() == &UnitReferenceTarget::Symbol(root(n, i.reader_params[index])?)
                && t.expression_type(e(i.compared[index])) == Some(int)
                && t.expression_type(e(i.literals[index])) == Some(int)
                && t.expression_type(e(i.compares[index])).is_some_and(|ty| {
                    t.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                }),
            "Int qualified original key/value scalar comparisons",
            r,
        )?;
    }
    let scanner = t
        .call(e(i.scanner_call))
        .ok_or_else(|| failure("Int qualified scanner entry", i.scanner_call))?;
    need(
        scanner.target() == UnitCallTarget::Declaration(decl(i.scan_name)?)
            && scanner.arguments().is_empty(),
        "Int qualified actual scanner selected",
        scanner,
    )?;
    let drop = one(
        o.drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(source)),
        "Int qualified unique current source Drop",
    )?;
    need(
        drop.value_origin() == i.source
            && drop.point()
                == if let Some(site) = &i.site {
                    UnitDropPoint::LoopExit(UnitStatementId::new(unit, site.statement))
                } else {
                    UnitDropPoint::CallReturn(e(i.size.unwrap()))
                },
        "Int qualified current source cleanup identity/endpoint",
        drop,
    )?;
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "Int qualified exact provider/control set",
        o.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let statement = UnitStatementId::new(unit, site.statement);
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("Int qualified actual For plan", site))?;
        let d = plan.descriptor();
        let r = one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(p, site.source)),
            "Int qualified actual For source Name",
        )?;
        need(
            t.map_iteration(statement) == Some(d)
                && d.source() == e(site.source)
                && d.source_type() == map
                && d.key_type() == int
                && d.value_type() == int
                && r.target() == &UnitReferenceTarget::Symbol(source)
                && plan.source_access() == UnitIterationSourceAccess::Owned
                && matches!(plan.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==source),
            "Int qualified original owned source Int storage",
            plan,
        )?;
        let read = t
            .call(e(site.read))
            .ok_or_else(|| failure("Int qualified actual read Call", site.read))?;
        need(
            read.target() == UnitCallTarget::Declaration(decl(i.reader_name)?)
                && read.arguments().len() == 2,
            "Int qualified actual read target",
            read,
        )?;
        for index in 0..2 {
            let binding = root(n, site.bindings[index])?;
            let component = &plan.loans()[index];
            let loan = one(
                o.loans().iter().filter(|l| {
                    l.call() == e(site.read) && l.argument() == e(site.arguments[index])
                }),
                "Int qualified actual component call loan",
            )?;
            need(
                d.bindings()[index] == binding
                    && o.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && component.binding() == binding
                    && component.statement() == statement
                    && component.source() == e(site.source)
                    && Some(component.origin()) == plan.source()
                    && component.kind() == LoanKind::Shared
                    && read.arguments()[index].argument_index() == index
                    && read.arguments()[index].parameter_index() == index
                    && read.arguments()[index].mode() == ParameterMode::Borrow
                    && read.arguments()[index].parameter_type() == int
                    && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                    && loan.kind() == LoanKind::Shared
                    && loan.end_span() == ast::span(p, site.read),
                "Int qualified exact projection/reader delivery/loan bounds",
                (component, loan),
            )?;
        }
        for (kind, point, mut actions) in [
            (
                UnitIterationExitKind::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                vec![
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                UnitIterationExitKind::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![
                    UnitIterationCleanupAction::FinishProvider(statement),
                    UnitIterationCleanupAction::EndSource(statement),
                ],
            ),
        ] {
            actions.extend(
                o.drops()
                    .iter()
                    .filter(|d| d.point() == point)
                    .copied()
                    .map(UnitIterationCleanupAction::Drop),
            );
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "Int qualified actual normal exit",
            )?;
            need(
                plan.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "Int qualified complete ordered provider cleanup",
                exit,
            )?;
        }
    } else {
        let size = t
            .map_size(e(i.size.unwrap()))
            .ok_or_else(|| failure("Int qualified real MapSize", i.size))?;
        need(
            size.map_type() == map && t.expression_type(size.receiver()) == Some(map),
            "Int qualified control uses actual named Map size",
            size,
        )?;
    }
    Ok(Proof {
        drop: drop.value_origin(),
    })
}
