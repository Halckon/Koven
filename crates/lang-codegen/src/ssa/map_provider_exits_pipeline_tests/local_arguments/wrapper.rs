//! A generic forwarding Call returns the exact owner built by the existing ordinary implementation.
use super::ast;
use crate::ssa::map_provider_exits_pipeline_tests::temporary_arguments::graph::factory;
use crate::ssa::map_provider_exits_pipeline_tests::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result, value},
    query_inputs::one,
};
use crate::ssa::model::*;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, ReferenceTarget, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, LoanEndPoint, LoanKind as FeLoanKind, LoanTarget,
        OwnershipCheckedFile, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::*,
};
struct Input {
    wrapper: ast::Function,
    implementation: ast::Function,
    call: ExpressionId,
    argument: ExpressionId,
    returned: Span,
}
fn input(sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
    let wrapper = ast::function(sources, parsed, "produce", Some("Int"))?;
    let implementation = ast::function(sources, parsed, "nonGenericImpl", Some("Int"))?;
    let call = one(
        parsed.ast().expressions().iter().filter_map(|(id, n)| {
            let Expression::Call {
                callee, arguments, ..
            } = n.payload()
            else {
                return None;
            };
            let [argument] = arguments.as_slice() else {
                return None;
            };
            (ast::inside(n.span(), wrapper.span)
                && sources.slice(ast::span(parsed, *callee)) == Ok("nonGenericImpl"))
            .then_some((id, argument.value))
        }),
        "generic wrapper actual ordinary implementation Call",
    )?;
    need(
        sources.slice(ast::span(parsed, call.1)) == Ok("input")
            && matches!(
                parsed.ast().expressions().get(call.1).unwrap().payload(),
                Expression::Name
            ),
        "generic wrapper forwards original input Name",
        call,
    )?;
    let returned = one(
        parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Expression::Return {
                    value: Some(value), ..
                } if ast::inside(n.span(), wrapper.span) && *value == call.0 => Some(n.span()),
                _ => None,
            }),
        "generic wrapper exact Return of implementation Call",
    )?;
    Ok(Input {
        wrapper,
        implementation,
        call: call.0,
        argument: call.1,
        returned,
    })
}
pub(in super::super) fn single(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let i = input(sources, parsed)?;
    let symbol = |span| {
        one(
            names
                .symbols()
                .iter()
                .filter(|s| s.span() == span && !s.is_synthetic()),
            "generic wrapper actual source symbol",
        )
        .map(|s| s.id())
    };
    let parameter = symbol(i.wrapper.parameter.unwrap())?;
    let call = typed
        .call(i.call)
        .ok_or_else(|| failure("generic wrapper typed inner Call", i.call))?;
    let signature = one(
        typed
            .callables()
            .iter()
            .filter(|c| c.symbol() == symbol(i.implementation.name).unwrap()),
        "generic wrapper implementation signature",
    )?;
    let reference = one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == ast::span(parsed, i.argument)),
        "generic wrapper actual input reference",
    )?;
    need(
        call.target() == CallableTarget::Source(symbol(i.implementation.name)?)
            && call.receiver().is_none()
            && call.instance().type_arguments().is_empty()
            && call.result_source() == CallableResultSource::Owned
            && call.return_type() == signature.return_type()
            && matches!(call.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.category()==ExpressionCategory::Place
            &&a.parameter_type()==signature.parameters()[0].ty)
            && reference.target() == &ReferenceTarget::Symbol(parameter),
        "generic wrapper actual selected non-generic Call forwards exact Borrow parameter",
        call,
    )?;
    let loan = one(
        owned
            .loans()
            .iter()
            .filter(|l| l.call() == i.call && l.argument() == i.argument),
        "generic wrapper input loan",
    )?;
    let end = one(
        owned
            .loan_ends()
            .iter()
            .filter(|e| e.call() == i.call && e.argument() == i.argument),
        "generic wrapper input loan endpoint",
    )?;
    need(
        loan.kind() == FeLoanKind::Shared
            && matches!(loan.target(),LoanTarget::Place(p)if p.is_root()&&p.root()==parameter)
            && loan.begin_span() == ast::span(parsed, i.argument)
            && loan.end_span() == ast::span(parsed, i.call)
            && end.point() == LoanEndPoint::CallReturn(i.call),
        "generic wrapper exact original parameter loan/CallReturn",
        (loan, end),
    )
}
pub(in super::super) fn unit(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let i = input(sources, parsed)?;
    let resolution = one(
        names.names().source_units().iter(),
        "generic wrapper original unit",
    )?;
    let unit = resolution.source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let declaration = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.source_unit() == unit && d.name_span() == i.implementation.name),
        "generic wrapper actual implementation declaration",
    )?;
    let wrapper = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|s| s.name_span() == i.wrapper.name),
        "generic wrapper actual source signature",
    )?;
    let parameter = wrapper.parameters()[0]
        .symbol()
        .ok_or_else(|| failure("generic wrapper original parameter symbol", wrapper))?;
    let call = typed
        .call(e(i.call))
        .ok_or_else(|| failure("generic wrapper qualified inner Call", i.call))?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == ast::span(parsed, i.argument)),
        "generic wrapper qualified actual input Name",
    )?;
    let signature = typed
        .signatures()
        .declaration(declaration.id())
        .and_then(|d| d.callable())
        .ok_or_else(|| failure("generic wrapper actual ordinary signature", declaration))?;
    need(
        call.target() == UnitCallTarget::Declaration(declaration.id())
            && call.receiver().is_none()
            && call.instance().type_arguments().is_empty()
            && call.result_source() == CallableResultSource::Owned
            && call.return_type() == signature.return_type()
            && matches!(call.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.category()==ExpressionCategory::Place&&a.parameter_type()==signature.parameters()[0].ty())
            && reference.target() == &UnitReferenceTarget::Symbol(parameter),
        "generic wrapper qualified actual ordinary target and original Borrow forwarding",
        call,
    )?;
    let contract = one(
        owned.call_argument_contracts_for(e(i.call)),
        "generic wrapper actual input ownership contract",
    )?;
    let loan = one(
        owned
            .loans()
            .iter()
            .filter(|l| l.call() == e(i.call) && l.argument() == e(i.argument)),
        "generic wrapper original qualified loan",
    )?;
    need(
        contract.argument() == e(i.argument)
            && contract.parameter_index() == 0
            && contract.parameter_span() == Some(signature.parameters()[0].span())
            && contract.category() == ExpressionCategory::Place
            && contract.kind()
                == lang_frontend::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
            && loan.kind() == FeLoanKind::Shared
            && matches!(loan.target(),UnitLoanTarget::Place(p)if p.is_root()&&p.root()==parameter)
            && loan.begin_span() == ast::span(parsed, i.argument)
            && loan.end_span() == ast::span(parsed, i.call),
        "generic wrapper exact qualified ownership loan",
        (contract, loan),
    )
}
pub(in super::super) fn ssa(
    mutable: bool,
    sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    wrapper: &Function,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let i = input(sources, parsed)?;
    let implementation = function(module, i.implementation.span)?;
    let g = Graph::new(wrapper)?;
    need(
        g.blocks.len() == 1 && g.edges.is_empty(),
        "generic wrapper has one actual forwarding block",
        wrapper.id,
    )?;
    let params = &g.block(wrapper.entry_block().unwrap())?.parameters;
    let parameter = one(params.iter().copied(), "generic wrapper sole Borrow formal")?;
    need(
        matches!(
            wrapper.entity(parameter).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ) && implementation
            .block(implementation.entry_block().unwrap())
            .unwrap()
            .parameters
            .len()
            == 1
            && wrapper.entity(parameter).unwrap().ty
                == implementation
                    .entity(
                        implementation
                            .block(implementation.entry_block().unwrap())
                            .unwrap()
                            .parameters[0],
                    )
                    .unwrap()
                    .ty
            && wrapper.return_types.as_slice() == [map]
            && implementation.return_types.as_slice() == [map],
        "generic wrapper and ordinary impl exact borrowed-Int/owned-Map ABI",
        (wrapper.id, implementation.id),
    )?;
    let call=one(g.instructions.iter().copied().filter(|n|matches!(n.operation,Operation::DirectCall{callee,receiver:None,..}if callee==implementation.id)&&n.origin==Origin::Source(ast::span(parsed,i.call))),"generic wrapper selects actual implementation once")?;
    let Operation::DirectCall { arguments, .. } = &call.operation else {
        unreachable!()
    };
    let actual = one(
        arguments.iter().copied(),
        "generic wrapper sole forwarded ABI argument",
    )?;
    let mut lineage = std::collections::BTreeSet::from([parameter]);
    let mut uses = Uses::default();
    uses.protect(parameter);
    let mut called = false;
    let mut ended = std::collections::BTreeSet::new();
    for op in &g.instructions {
        match op.operation {
            Operation::SharedReborrow { source } => {
                let parent = EntityId::Loan(source);
                let child = result(op)?;
                need(
                    !called
                        && lineage.contains(&parent)
                        && !ended.contains(&parent)
                        && op.origin == Origin::Source(ast::span(parsed, i.argument))
                        && wrapper.entity(parent).unwrap().ty == wrapper.entity(child).unwrap().ty,
                    "generic wrapper exact live source-parameter reborrow",
                    op,
                )?;
                uses.permit(parent, op);
                uses.protect(child);
                lineage.insert(child);
            }
            Operation::DirectCall { .. } => {
                need(
                    op.id == call.id
                        && !called
                        && lineage.contains(&actual)
                        && !ended.contains(&actual),
                    "generic wrapper only exact selected forwarding Call",
                    op,
                )?;
                uses.permit(actual, op);
                called = true;
                uses.protect(result(op)?);
            }
            Operation::BorrowEnd { loan } => {
                let entity = EntityId::Loan(loan);
                need(
                    called
                        && lineage.contains(&entity)
                        && ended.insert(entity)
                        && op.origin
                            == Origin::Source(if entity == parameter {
                                i.returned
                            } else {
                                ast::span(parsed, i.call)
                            }),
                    "generic wrapper synchronous child or final ABI loan End",
                    op,
                )?;
                uses.permit(entity, op);
            }
            _ => {
                return Err(failure(
                    "generic wrapper unexpected forwarding body operation",
                    op,
                ));
            }
        }
    }
    let term = g.term(call.block)?;
    need(
        called
            && term.origin == Origin::Source(i.returned)
            && matches!(&term.kind,TerminatorKind::Return{values}if values.as_slice()==[value(call)?])
            && lineage.iter().all(|e| *e == parameter || ended.contains(e)),
        "generic wrapper Return delivers the same implementation owner once",
        term,
    )?;
    uses.check(&g, Some((call.block, value(call)?)))?;
    let case = super::temporary_arguments::sources()
        .into_iter()
        .find(|c| c.mutable == mutable)
        .unwrap();
    factory::check(&case, sources, parsed, module, implementation, map)
}
