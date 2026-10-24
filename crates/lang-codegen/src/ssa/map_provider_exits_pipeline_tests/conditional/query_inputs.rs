//! 同次公开 FE 身份桥；AST anchors、原始 facts 与 returned SSA 分别保持独立。
//! 原 collector 继续硬核全部 type/loan/delivery 合同；本查询仅增加身份连接与精确条件/出口。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Input},
    query_conditions,
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, SymbolId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, OwnershipBindingKind, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::{Item, NameMarker, ValueParameter},
    source::Span,
    type_checking::{
        CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitCallableSignature,
        UnitCallableTarget, UnitConstructionTarget, UnitExpressionId, UnitStatementId,
    },
};

#[derive(Debug)]
pub(super) struct Inputs {
    pub parameter: Span,
    pub parameter_slot: usize,
    pub source_drop: Option<Span>,
}

fn parameters<'a>(f: &'a Input, a: &Anchors, name: &str) -> Result<&'a [ValueParameter], Failure> {
    let anchor = one(
        a.functions.iter().filter(|v| v.name == name),
        "conditional FE AST function",
    )?;
    let node = f
        .parsed
        .ast()
        .items()
        .get(anchor.item)
        .map_err(|e| failure("conditional FE AST item", e))?;
    let Item::Function {
        name: NameMarker::Present(actual),
        parameters,
        ..
    } = node.payload()
    else {
        return Err(failure("conditional FE actual function declaration", node));
    };
    need(
        *actual == anchor.name_span && node.span() == anchor.span,
        "conditional FE function anchor identity",
        (anchor, node),
    )?;
    Ok(parameters)
}

fn symbol(names: &NameResolution, span: Span) -> Result<SymbolId, Failure> {
    Ok(one(
        names
            .symbols()
            .iter()
            .filter(|v| v.span() == span && !v.is_synthetic()),
        "conditional FE unique declaration symbol",
    )?
    .id())
}

fn unit_reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let (_, span) = f.expression(id)?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|v| v.span() == span),
        "conditional FE unit reference",
    )?;
    match reference.target() {
        UnitReferenceTarget::Symbol(id) => Ok(*id),
        _ => Err(failure(
            "conditional FE unit source-local target",
            reference,
        )),
    }
}

fn unit_callable<'t>(
    f: &Input,
    a: &Anchors,
    name: &str,
    names: &ValidatedCompilationUnitNames,
    typed: &'t CompilationUnitTypes,
) -> Result<&'t UnitCallableSignature, Failure> {
    let source = one(
        names.names().source_units().iter(),
        "conditional FE unit source",
    )?;
    let anchor = one(
        a.functions.iter().filter(|v| v.name == name),
        "conditional FE unit function anchor",
    )?;
    let declaration = one(
        names.names().index().declarations().iter().filter(|v| {
            v.source_unit() == source.source_unit()
                && v.root() == anchor.item
                && v.name_span() == anchor.name_span
                && v.kind() == SymbolKind::Function
        }),
        "conditional FE unit function declaration",
    )?;
    let callable = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|v| v.callable())
            .filter(|v| v.target() == UnitCallableTarget::Declaration(declaration.id())),
        "conditional FE unit actual declaration signature",
    )?;
    let ast_parameters = parameters(f, a, name)?;
    need(
        callable.name_span() == anchor.name_span
            && callable.receiver().is_none()
            && callable.parameters().len() == ast_parameters.len(),
        "conditional FE unit actual signature shape",
        (declaration, callable),
    )?;
    for (index, parameter) in ast_parameters.iter().enumerate() {
        let NameMarker::Present(span) = parameter.name else {
            return Err(failure(
                "conditional FE unit real parameter name",
                parameter,
            ));
        };
        let id = callable.parameters()[index]
            .symbol()
            .ok_or_else(|| failure("conditional FE unit parameter symbol", callable))?;
        need(
            id.source_unit() == source.source_unit()
                && id.symbol() == symbol(source.resolution(), span)?,
            "conditional FE unit parameter declaration and slot",
            (index, parameter, callable),
        )?;
    }
    Ok(callable)
}

pub(super) fn unit(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let source_names = one(
        names.names().source_units().iter(),
        "conditional FE single actual source unit",
    )?;
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let statement = UnitStatementId::new(unit, a.statement);
    let d = typed
        .map_iteration(statement)
        .ok_or_else(|| failure("conditional FE unit Map descriptor", statement))?;
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("conditional FE unit Map plan", statement))?;
    let source = unit_reference(f, names, a.source)?;
    let parameter = one(
        source_names
            .resolution()
            .symbols()
            .iter()
            .filter(|v| v.id() == source.symbol()),
        "conditional FE unit source parameter",
    )?;
    let scan = unit_callable(f, a, "scan", names, typed)?;
    let parameter_slot = one(
        scan.parameters()
            .iter()
            .enumerate()
            .filter_map(|(i, v)| (v.symbol() == Some(source)).then_some(i)),
        "conditional FE unit parameter slot",
    )?;
    let mode = if f.map {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    let capability = if f.map {
        OwnershipBindingKind::Owned
    } else {
        OwnershipBindingKind::Shared
    };
    need(
        source_names.resolution().source_id() == f.parsed.source_id()
            && source.source_unit() == unit
            && d.statement() == statement
            && d.source() == expression(a.source)
            && p.descriptor() == d
            && typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && parameter.kind() == SymbolKind::ValueParameter
            && !parameter.is_synthetic()
            && matches!(scan.parameters(), [parameter] if parameter.symbol() == Some(source)
            && parameter.mode() == mode && parameter.ty() == d.source_type())
            && typed.expression_type(expression(a.source)) == Some(d.source_type())
            && owned.binding_kind(source) == Some(capability)
            && matches!(p.source(), Some(UnitLoanTarget::Place(path)) if path.is_root() && path.root() == source)
            && matches!(p.source_kind(), IterationSource::BorrowedSource(_))
            && p.source_access()
                == if f.map {
                    UnitIterationSourceAccess::Owned
                } else {
                    UnitIterationSourceAccess::Shared
                },
        "conditional FE unit actual parameter to Map plan",
        (parameter, scan, d, p),
    )?;
    for (index, binding) in d.bindings().into_iter().enumerate() {
        need(
            binding.source_unit() == unit
                && symbol(source_names.resolution(), a.bindings[index])? == binding.symbol()
                && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[index].symbol() == binding
                && p.loans()[index].binding() == binding
                && p.loans()[index].source() == expression(a.source),
            "conditional FE unit component identity",
            (index, binding, p),
        )?;
    }
    need(
        a.branches.len() == 3,
        "conditional FE unit three actual branches",
        &a.branches,
    )?;
    for branch in &a.branches {
        need(
            unit_reference(f, names, branch.key)? == d.key_binding(),
            "conditional FE unit three If reference the actual key",
            branch,
        )?;
    }
    let local_symbol = symbol(source_names.resolution(), a.local)?;
    let local = one(
        owned
            .formed_owned_locals()
            .iter()
            .copied()
            .filter(|id| id.source_unit() == unit && id.symbol() == local_symbol),
        "conditional FE unit Guard local",
    )?;
    let construction = typed
        .construction(expression(a.guard))
        .ok_or_else(|| failure("conditional FE unit Guard constructor", a.guard))?;
    let guard = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|v| v.source_unit() == unit && v.name_span() == a.guard_name),
        "conditional FE unit Guard declaration",
    )?;
    need(
        construction.expression() == expression(a.guard)
            && construction.arguments().is_empty()
            && typed.expression_type(expression(a.guard)) == Some(construction.result_type())
            && typed.symbol_type(local) == Some(construction.result_type())
            && typed.is_resource_type(construction.result_type()) == Some(true)
            && matches!(construction.target(), UnitConstructionTarget::Nominal(id) if id == guard.id()),
        "conditional FE unit actual Guard construction and local",
        (local, construction),
    )?;
    let read = unit_callable(f, a, "read", names, typed)?;
    let operand = unit_callable(f, a, "operand", names, typed)?;
    need(
        read.parameters().len() == 2 && operand.parameters().len() == 2,
        "conditional FE unit exact two consumer parameters before indexed queries",
        (read, operand),
    )?;
    need(
        a.component_calls.len() == 3,
        "conditional FE unit component call set",
        &a.component_calls,
    )?;
    for (name, id, arguments) in &a.component_calls {
        let expected = if *name == "operand" { operand } else { read };
        let call = typed
            .call(expression(*id))
            .ok_or_else(|| failure("conditional FE unit component call", id))?;
        need(
            matches!(call.target(), UnitCallTarget::Declaration(id)
            if expected.target() == UnitCallableTarget::Declaration(id))
                && call.receiver().is_none()
                && call.arguments().len() == 2,
            "conditional FE unit component callee identity",
            call,
        )?;
        for (index, argument) in call.arguments().iter().enumerate() {
            need(
                argument.argument_index() == index
                    && argument.parameter_index() == index
                    && argument.mode() == ParameterMode::Borrow
                    && argument.parameter_type() == expected.parameters()[index].ty()
                    && unit_reference(f, names, arguments[index])? == d.bindings()[index],
                "conditional FE unit component argument identity",
                (index, call),
            )?;
        }
    }
    let forwarding = typed
        .call(expression(a.operand_read.0))
        .ok_or_else(|| failure("conditional FE unit operand read", a.operand_read))?;
    need(
        matches!(forwarding.target(), UnitCallTarget::Declaration(id)
        if read.target() == UnitCallableTarget::Declaration(id))
            && forwarding.receiver().is_none()
            && forwarding.arguments().len() == 2,
        "conditional FE unit operand actual read target",
        forwarding,
    )?;
    for (index, argument) in forwarding.arguments().iter().enumerate() {
        need(
            argument.argument_index() == index
                && argument.parameter_index() == index
                && argument.mode() == ParameterMode::Borrow
                && Some(unit_reference(f, names, a.operand_read.1[index])?)
                    == operand.parameters()[index].symbol(),
            "conditional FE unit operand own parameters",
            (index, argument),
        )?;
    }
    unit_callable(f, a, "entry", names, typed)?;
    need(
        a.entry_calls.len() == 5,
        "conditional FE unit actual entry calls",
        &a.entry_calls,
    )?;
    for entry in &a.entry_calls {
        let call_id = expression(entry.call);
        let call = typed
            .call(call_id)
            .ok_or_else(|| failure("conditional FE unit entry scan call", entry))?;
        let argument = unit_reference(f, names, entry.argument)?;
        let result = unit_reference(f, names, entry.result_reference)?;
        need(
            matches!(call.target(), UnitCallTarget::Declaration(id)
            if scan.target() == UnitCallableTarget::Declaration(id))
                && call.receiver().is_none()
                && matches!(call.arguments(), [arg] if arg.argument_index() == 0 && arg.parameter_index() == parameter_slot
                && arg.mode() == mode && arg.parameter_type() == d.source_type())
                && argument.source_unit() == unit
                && argument.symbol() == symbol(source_names.resolution(), entry.source_binding)?
                && result.source_unit() == unit
                && result.symbol() == symbol(source_names.resolution(), entry.result_binding)?
                && typed.expression_type(expression(entry.source_initializer))
                    == Some(d.source_type()),
            "conditional FE unit entry source and scan slot",
            (entry, call),
        )?;
    }
    query_conditions::unit(f, a, owned, source, local, parameter.span())?;
    Ok(Inputs {
        parameter: parameter.span(),
        parameter_slot,
        source_drop: f.map.then_some(parameter.span()),
    })
}
