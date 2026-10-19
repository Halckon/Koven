//! Unit reads only its own source-qualified contracts, delivery and cleanup tables.
use super::*;
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitCallArgumentOwnershipKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget, UnitValueDeliveryKind, UnitValueDeliverySource,
    },
    type_checking::{
        CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};
pub(super) fn symbol(
    n: &ValidatedCompilationUnitNames,
    marker: Span,
) -> Result<UnitSymbolId, Failure> {
    let (u, id) = one(
        n.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == marker && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "D original qualified symbol marker",
    )?;
    one(
        n.names()
            .references()
            .iter()
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) if s.source_unit() == u && s.symbol() == id => {
                    Some(*s)
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter(),
        "D original qualified referenced symbol",
    )
}
pub(super) fn reference(
    p: &ParsedFile,
    n: &ValidatedCompilationUnitNames,
    e: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    one(
        n.names()
            .references()
            .iter()
            .filter(|r| r.span() == ast::span(p, e))
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "D actual qualified Name reference",
    )
}
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_future_observer) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let u = one(n.names().source_units().iter(), "D original source unit")?.source_unit();
    let e = |id| UnitExpressionId::new(u, id);
    let declaration = |marker| {
        one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == u && d.name_span() == marker)
                .map(|d| d.id()),
            "D original qualified declaration",
        )
    };
    let root = symbol(n, i.source)?;
    let ty = t
        .symbol_type(root)
        .ok_or_else(|| failure("D qualified source type", root))?;
    println!(
        "SCOPE D FUTURE FE label={} unit=true root={root:?} type={ty:?} formed={:#?} drops={:#?} loans={:#?} contracts={:#?} deliveries={:#?} plans={:#?} puts={:#?} strings={:#?}",
        c.label,
        o.formed_owned_locals(),
        o.drops(),
        o.loans(),
        o.call_argument_contracts(),
        o.value_deliveries(),
        o.map_iterations(),
        t.map_puts(),
        o.string_effects()
    );
    for d in o.drops() {
        println!(
            "SCOPE D FUTURE DROP label={} unit=true target={:?} point={:?} origin={:?}",
            c.label,
            d.target(),
            d.point(),
            d.value_origin()
        );
    }
    need(
        matches!(t.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor,arguments})if *constructor==if c.code==21{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2&&arguments.iter().all(|a|t.types().get(*a)==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
        "D qualified concrete Map storage",
        ty,
    )?;
    if c.code == 21 {
        need(
            o.formed_owned_locals().contains(&root),
            "D qualified formed MutableMap local",
            root,
        )?;
    } else {
        let scanner = t
            .signatures()
            .declaration(declaration(i.scan_name)?)
            .and_then(|d| d.callable())
            .ok_or_else(|| failure("D original qualified scanner signature", i.scan_name))?;
        println!(
            "SCOPE D FUTURE PARAMETER label={} unit=true root={root:?} body_mode={:?} binding={:?} signature={scanner:?}",
            c.label,
            t.body_parameter_mode(root),
            o.binding_kind(root)
        );
        need(
            matches!(scanner.parameters(), [parameter] if parameter.symbol() == Some(root)
                && parameter.mode() == ParameterMode::Value && parameter.ty() == ty)
                && o.binding_kind(root) == Some(OwnershipBindingKind::Owned),
            "D original qualified own parameter",
            (root, scanner),
        )?;
    }
    for birth in [i.birth, i.entry_birth].into_iter().flatten() {
        let call = t
            .call(e(birth))
            .ok_or_else(|| failure("D qualified factory Call", birth))?;
        need(
            call.target() == UnitCallTarget::Declaration(declaration(i.factory_name)?)
                && call.arguments().is_empty()
                && call.receiver().is_none()
                && call.result_source() == CallableResultSource::Owned
                && t.expression_type(e(birth)) == Some(ty)
                && t.expression_category(e(birth)) == Some(ExpressionCategory::Temporary),
            "D qualified selected owned factory result",
            call,
        )?;
    }
    let call = t
        .call(e(i.scan_call))
        .ok_or_else(|| failure("D qualified scanner Call", i.scan_call))?;
    need(
        call.target() == UnitCallTarget::Declaration(declaration(i.scan_name)?)
            && call.receiver().is_none()
            && call.arguments().len() == usize::from(c.code != 21)
            && call.result_source() == CallableResultSource::Owned
            && if c.code == 19 {
                call.return_type() == ty
            } else {
                t.types().get(call.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            },
        "D qualified actual scanner target",
        call,
    )?;
    if let Some(arg) = i.scan_argument {
        let contract = one(
            o.call_argument_contracts_for(e(i.scan_call))
                .filter(|c| c.argument() == e(arg)),
            "D actual Value contract",
        )?;
        let delivery = one(
            o.value_deliveries()
                .iter()
                .filter(|d| d.call() == e(i.scan_call) && d.argument() == e(arg)),
            "D actual owned temporary delivery",
        )?;
        need(
            matches!(call.arguments(),[a]if a.mode()==ParameterMode::Value&&a.parameter_index()==0&&a.argument_index()==0&&a.parameter_type()==ty)
                && contract.kind() == UnitCallArgumentOwnershipKind::Value
                && contract.parameter_type() == ty
                && delivery.kind() == UnitValueDeliveryKind::Temporary
                && matches!(delivery.source(),UnitValueDeliverySource::Temporary(value)if *value==e(arg))
                && !o.loans().iter().any(|l| l.call() == e(i.scan_call)),
            "D qualified Value argument has actual temporary handoff not Borrow",
            (contract, delivery),
        )?;
    }
    if let Some((ret, value)) = i.returned {
        need(
            reference(p, n, value)? == root
                && t.expression_type(e(value)) == Some(ty)
                && !o
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(root)),
            "D qualified actual Return owns source without callee Drop",
            (e(ret), e(value)),
        )?;
    }
    if let Some((marker, init)) = i.successor {
        let next = symbol(n, marker)?;
        need(
            reference(p, n, init)? == root
                && t.expression_type(e(init)) == Some(ty)
                && o.formed_owned_locals().contains(&next)
                && !o
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(root)),
            "D qualified MoveName successor",
            (next, e(init)),
        )?;
        one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(next)),
            "D qualified successor sole Drop",
        )?;
    }
    if c.code == 21 {
        one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(root)),
            "D qualified updated MutableMap sole named Drop",
        )?;
    }
    let mut observes = i
        .observes
        .iter()
        .enumerate()
        .map(|(index, &(a, b))| {
            let expected = if c.code == 20 && (c.provider || index == 1) {
                symbol(
                    n,
                    i.successor
                        .ok_or_else(|| failure("D qualified successor AST", i))?
                        .0,
                )?
            } else {
                root
            };
            need(
                reference(p, n, b)? == expected,
                "D actual qualified future observe Name identity",
                b,
            )?;
            Ok((a, b, expected))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    if let Some((a, b)) = i.entry_observe {
        let result = symbol(
            n,
            i.result
                .ok_or_else(|| failure("D actual result marker", i))?,
        )?;
        need(
            reference(p, n, b)? == result && o.formed_owned_locals().contains(&result),
            "D qualified actual returned result binding",
            result,
        )?;
        one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(result)),
            "D qualified caller result sole Drop",
        )?;
        observes.push((a, b, result));
    }
    for (call, arg, actualroot) in observes {
        let d = t
            .call(e(call))
            .ok_or_else(|| failure("D qualified observe Call", call))?;
        let loan = one(
            o.loans_ending_at(e(call))
                .filter(|l| l.argument() == e(arg)),
            "D qualified observe loan/end",
        )?;
        let contract = one(
            o.call_argument_contracts_for(e(call))
                .filter(|c| c.argument() == e(arg)),
            "D qualified observe argument contract",
        )?;
        need(
            d.target() == UnitCallTarget::Declaration(declaration(i.observer_name)?)
                && matches!(d.arguments(),[a]if a.mode()==ParameterMode::Borrow&&a.parameter_index()==0&&a.argument_index()==0&&a.parameter_type()==ty)
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && contract.parameter_type() == ty
                && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==actualroot)
                && loan.begin_span() == ast::span(p, arg)
                && loan.end_span() == ast::span(p, call),
            "D qualified actual owner Borrow observation and synchronous End",
            (d, loan, contract),
        )?;
    }
    if let Some(put) = i.put {
        let d = t
            .map_put(e(put))
            .ok_or_else(|| failure("D qualified future put", put))?;
        need(
            reference(p, n, d.receiver().expression())? == root
                && [d.key(), d.value()]
                    .iter()
                    .all(|e| o.string_effect(*e).is_some()),
            "D qualified actual put root and clone ownership effects",
            d,
        )?;
    }
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "D qualified exact acquired plans",
        o.map_iterations(),
    )?;
    if let Some((statement, source)) = i.site {
        let statement = UnitStatementId::new(u, statement);
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("D qualified provider plan", statement))?;
        need(
            t.map_iteration(statement) == Some(plan.descriptor())
                && plan.descriptor().source() == e(source)
                && plan.descriptor().source_type() == ty
                && reference(p, n, source)? == root
                && matches!(plan.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==root)
                && plan.source_access() == UnitIterationSourceAccess::Owned
                && plan.loans().len() == 2
                && !plan.exits().is_empty(),
            "D qualified owned original provider",
            plan,
        )?;
    }
    if c.provider {
        super::provider::unit(p, i, n, t, o)?;
    }
    super::deep::unit(c, p, i, n, o)
}
