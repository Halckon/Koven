//! Preserve actual Symbol-selected member/override/delegation provenance.
use super::*;
use crate::ssa::unit_plan;
use lang_frontend::name_resolution::UnitSymbolId;
#[derive(Debug)]
pub(in super::super) struct Receiver {
    pub field: Option<usize>,
    pub begin: Span,
    pub end: Span,
}
fn symbol_span(names: &ValidatedCompilationUnitNames, id: UnitSymbolId) -> Result<Span, Failure> {
    names
        .names()
        .source_units()
        .iter()
        .find(|u| u.source_unit() == id.source_unit())
        .and_then(|u| u.resolution().symbols().get(id.symbol().index()))
        .map(|s| s.span())
        .ok_or_else(|| failure("local argument original qualified member symbol", id))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    call: &UnitCallDescriptor,
) -> Result<Receiver, Failure> {
    let UnitCallTarget::Symbol(selected) = call.target() else {
        return Err(failure(
            "local argument member original Symbol target",
            call,
        ));
    };
    let target = UnitCallableTarget::Symbol(selected);
    let resolved = unit_plan::resolve_unit_call_instance(
        typed,
        owned,
        target,
        call.instance().type_arguments().to_vec(),
        call.receiver().map(|r| r.ty()),
        ast::span(parsed, input.call),
    )
    .map_err(|e| failure("local argument actual static member route", e))?;
    let UnitCallableTarget::Symbol(implementation) = resolved.key().target() else {
        return Err(failure(
            "local argument actual implementation Symbol",
            resolved.key(),
        ));
    };
    need(
        symbol_span(names, implementation)? == input.factory.name
            && resolved.delegation().len() == usize::from(case.delegated)
            && call.receiver().is_some(),
        "local argument exact selected Factory.other implementation",
        (call, resolved.key()),
    )?;
    let field = if case.delegated {
        let [route] = resolved.delegation() else {
            unreachable!()
        };
        let nominal = |ty| match typed.types().get(ty) {
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) if arguments.is_empty() => typed
                .signatures()
                .declaration(*declaration)
                .and_then(|d| d.nominal()),
            _ => None,
        };
        let outer = nominal(route.outer_receiver())
            .ok_or_else(|| failure("local argument actual Host receiver", route))?;
        let inner = nominal(route.delegate_receiver())
            .ok_or_else(|| failure("local argument actual Factory delegate", route))?;
        let (slot, field) = one(
            outer
                .fields()
                .iter()
                .enumerate()
                .filter(|(_, f)| f.symbol() == route.field()),
            "local argument exact delegation field",
        )?;
        need(
            target != resolved.key().target()
                && call
                    .receiver()
                    .is_some_and(|r| r.ty() == route.outer_receiver())
                && sources.slice(symbol_span(names, outer.symbol())?) == Ok("Host")
                && sources.slice(symbol_span(names, inner.symbol())?) == Ok("Factory")
                && field.name() == "delegate"
                && field.ty() == route.delegate_receiver()
                && sources.slice(symbol_span(names, route.field())?) == Ok("delegate"),
            "local argument actual Borrow static delegation projection",
            route,
        )?;
        Some(slot)
    } else {
        need(
            target == resolved.key().target(),
            "local argument unchanged member/override static target",
            target,
        )?;
        None
    };
    let receiver = owned
        .receiver_fact(call.expression())
        .ok_or_else(|| failure("local argument actual receiver loan fact", call))?;
    need(
        receiver.kind() == UnitReceiverOwnershipKind::SharedLoan,
        "local argument member shared receiver contract",
        receiver,
    )?;
    Ok(Receiver {
        field,
        begin: receiver.begin_span(),
        end: receiver.end_span(),
    })
}
