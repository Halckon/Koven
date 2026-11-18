//! Admission and source identity for a borrowed String returned from an entry loan.
use super::*;
use std::collections::BTreeSet;

// Every normal return must keep the declared String parameter as its source.
pub(super) fn string_entry_block_return(
    file: &lang_frontend::parser::ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    callable: &UnitCallableSignature,
) -> Option<()> {
    use lang_frontend::parser::{FunctionBody, FunctionForm, Item};

    let contract = callable.borrow_return()?;
    let BorrowReturnOrigin::Parameter(index) = contract.origin() else {
        return None;
    };
    let parameter = callable.parameters().get(index)?;
    if parameter.mode() != ParameterMode::Borrow
        || callable.return_type() != parameter.ty()
        || typed.types().get(parameter.ty()) != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || !callable.type_parameters().is_empty()
    {
        return None;
    }
    let root = parameter.symbol()?;
    let UnitCallableTarget::Declaration(id) = callable.target() else {
        return None;
    };
    let declaration = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.id() == id)?;
    let unit = declaration.source_unit();
    if root.source_unit() != unit {
        return None;
    }
    let node = file.ast().items().get(declaration.root()).ok()?;
    let Item::Function {
        form:
            FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            },
        ..
    } = node.payload()
    else {
        return None;
    };
    let body = file.ast().statements().get(*block).ok()?.span();
    let mut operands = BTreeSet::new();
    let mut needs_entry_route = false;
    for fact in owned
        .borrow_return_origins()
        .iter()
        .filter(|fact| fact.declaration_span() == contract.marker_span())
    {
        let expression = fact.expression();
        if expression.source_unit() != unit
            || !matches!(fact.origin(), UnitLoanTarget::Place(place)
                if place.is_root() && place.root() == root)
            || !file
                .ast()
                .expressions()
                .get(expression.expression())
                .is_ok_and(|node| span_contains(body, node.span()))
        {
            return None;
        }
        let operand =
            crate::ssa::borrow_result_support::ungroup(file, expression.expression(), body).ok()?;
        operands.insert(operand.index());
        if super::branch_declaration::is_symbol(file, names, operand, root) {
            needs_entry_route = true;
        } else {
            let call = UnitExpressionId::new(unit, operand);
            forwards_string_entry(file, names, typed, owned, call, root)?;
            needs_entry_route |= typed.call(call)?.arguments().len() > 1;
        }
    }
    if operands.is_empty() {
        return None;
    }
    // Pure one-argument forwarding trees use the existing receipt gates. A lexical
    // borrow ending at an actual returned operand requires the ordinary entry route.
    (needs_entry_route
        || owned.borrow_results().ends().iter().any(|end| {
            let UnitDropPoint::ControlTransfer(transfer) = end.point() else {
                return false;
            };
            let binding = end.binding();
            if transfer.source_unit() != unit
                || binding.source_unit() != unit
                || typed
                    .symbol_type(binding)
                    .and_then(|ty| typed.types().get(ty))
                    != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            {
                return false;
            }
            let Ok(node) = file.ast().expressions().get(transfer.expression()) else {
                return false;
            };
            let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            else {
                return false;
            };
            crate::ssa::borrow_result_support::ungroup(file, *value, body)
                .is_ok_and(|value| operands.contains(&value.index()))
        }))
    .then_some(())
}

/// The forwarded loan must come from the named entry parameter. Refine can
/// then end other scoped loans without releasing this result's lender.
pub(in crate::ssa) fn forwards_string_entry(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    call: UnitExpressionId,
    root: UnitSymbolId,
) -> Option<BorrowSourceLoan<UnitExpressionId>> {
    let ty = typed.symbol_type(root)?;
    if typed.types().get(ty) != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || call.source_unit() != root.source_unit()
    {
        return None;
    }
    let descriptor = typed.call(call)?;
    let BorrowReturnOrigin::Parameter(index) = descriptor.borrow_return()?.origin() else {
        return None;
    };
    if descriptor.return_type() != ty
        || descriptor.receiver().is_some()
        || descriptor
            .arguments()
            .iter()
            .any(|arg| arg.mode() == ParameterMode::Inout)
        || !matches!(
            descriptor.target(),
            UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
        )
    {
        return None;
    }
    let source = owned
        .borrow_results()
        .forwarded_source_loans()
        .iter()
        .find(|source| source.call() == call)
        .copied()?;
    let mapping = descriptor
        .arguments()
        .iter()
        .find(|arg| arg.parameter_index() == index)?;
    let node = file.ast().expressions().get(call.expression()).ok()?;
    let Expression::Call { arguments, .. } = node.payload() else {
        return None;
    };
    if mapping.mode() != ParameterMode::Borrow
        || mapping.parameter_type() != ty
        || source.argument()
            != UnitExpressionId::new(
                call.source_unit(),
                arguments.get(mapping.argument_index())?.value,
            )
    {
        return None;
    }
    let name = crate::ssa::borrow_result_support::ungroup(
        file,
        source.argument().expression(),
        node.span(),
    )
    .ok()?;
    super::branch_declaration::is_symbol(file, names, name, root).then_some(source)
}
