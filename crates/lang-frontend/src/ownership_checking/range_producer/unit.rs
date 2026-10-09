//! 将 unit 已选择 target 和 source-qualified 名称投影到同一返回依赖证明。
use super::*;
use crate::{
    name_resolution::{SourceUnitInput, UnitReferenceTarget, ValidatedCompilationUnitNames},
    type_checking::{CompilationUnitTypes, UnitExpressionId},
};

pub(in crate::ownership_checking) fn collect_unit(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<Vec<Span>, OwnershipCheckingError> {
    let mut collected = Vec::new();
    for source in names.names().index().source_units() {
        let Some(input) = inputs
            .iter()
            .find(|input| input.source_id() == source.source_id())
        else {
            return Err(OwnershipCheckingError::InvalidUnitSource {
                source_unit: source.id().index(),
            });
        };
        let parsed = input.parsed();
        collected.extend(candidates(
            sources,
            parsed,
            |expression| {
                let node = parsed.ast().expressions().get(expression).ok()?;
                if !matches!(node.payload(), Expression::Name | Expression::This) {
                    return None;
                }
                let reference = names
                    .names()
                    .references()
                    .iter()
                    .find(|r| r.span() == node.span())?;
                let UnitReferenceTarget::Symbol(symbol) = reference.target() else {
                    return None;
                };
                let local = names
                    .names()
                    .source_units()
                    .iter()
                    .find(|unit| unit.source_unit() == symbol.source_unit())?;
                local
                    .resolution()
                    .symbols()
                    .get(symbol.symbol().index())
                    .map(|symbol| symbol.span())
            },
            |name| {
                let callable = typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|d| d.callable())
                    .find(|c| c.name_span() == name)?;
                callable.range_extension()?;
                let symbol = callable.extension_receiver_symbol()?;
                names.names().source_units()[symbol.source_unit().index()]
                    .resolution()
                    .symbols()
                    .get(symbol.symbol().index())
                    .map(|s| s.span())
            },
            |expression| {
                let call = typed.calls().iter().find(|call| {
                    call.expression() == UnitExpressionId::new(source.id(), expression)
                })?;
                if let Some(range) = call.range_construction() {
                    return Some((range.source().expression(), None));
                }
                let CallableResultSource::Carrier(contract) = call.result_source() else {
                    return None;
                };
                let argument = match contract.origin() {
                    BorrowReturnOrigin::Parameter(parameter) => source_argument(
                        parsed,
                        expression,
                        parameter,
                        call.arguments()
                            .iter()
                            .map(|a| (a.parameter_index(), a.argument_index())),
                    )?,
                    BorrowReturnOrigin::Receiver => match call.receiver()?.origin() {
                        crate::type_checking::UnitCallReceiverOrigin::Expression(id)
                            if id.source_unit() == source.id() =>
                        {
                            id.expression()
                        }
                        _ => return None,
                    },
                };
                let signature =
                    super::super::compilation_unit::contracts::source_callable_signature(
                        typed,
                        call.target(),
                    )?;
                Some((argument, Some(signature.name_span())))
            },
        )?);
    }
    Ok(finish(collected))
}
