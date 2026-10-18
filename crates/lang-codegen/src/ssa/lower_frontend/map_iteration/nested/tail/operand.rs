//! The actual four-Borrow ordinary call supplies the Return operand.
use super::*;
use lang_frontend::{
    parser::NameMarker,
    type_checking::{
        CallableResultSource, CallableTarget, ExpressionCategory, MapIterationDescriptor,
    },
};

pub(super) fn validate(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    outer: MapIterationDescriptor,
    inner: MapIterationDescriptor,
    value: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .expressions()
        .get(value)
        .map_err(|_| missing())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if callee.payload() != &Expression::Name
        || arguments.len() != 4
        || !type_arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
    {
        return Err(unsupported());
    }
    let call = typed.call(value).ok_or_else(missing)?;
    let CallableTarget::Source(target) = call.target() else {
        return Err(unsupported());
    };
    let symbol = names.symbols().get(target.index()).ok_or_else(missing)?;
    let signature = unique(
        typed
            .callables()
            .iter()
            .filter(|signature| signature.symbol() == target),
    )
    .ok_or_else(missing)?;
    if symbol.kind() != SymbolKind::Function
        || call.receiver().is_some()
        || call.aborts()
        || call.result_source() != CallableResultSource::Owned
        || call.range_construction().is_some()
        || !call.instance().type_arguments().is_empty()
        || signature.owner().is_some()
        || signature.receiver().is_some()
        || signature.extension_receiver_symbol().is_some()
        || !signature.type_parameters().is_empty()
        || builtin_type(typed, call.return_type()) != Some(BuiltinType::Int)
    {
        return Err(unsupported());
    }
    let declaration = unique(parsed.roots().iter().filter_map(|root| {
        parsed
            .ast()
            .items()
            .get(*root)
            .ok()
            .and_then(|node| match node.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } if *name == symbol.span() => Some(parameters),
                _ => None,
            })
    }))
    .ok_or_else(missing)?;
    let reference = unique(
        names
            .references()
            .iter()
            .filter(|reference| reference.span() == callee.span()),
    )
    .ok_or_else(missing)?;
    if !match reference.target() {
        ReferenceTarget::Symbol(symbol) => *symbol == target,
        ReferenceTarget::OverloadSet(symbols) => symbols.contains(&target),
        _ => false,
    } {
        return Err(missing());
    }
    if call.expression() != value
        || typed.expression_type(value) != Some(call.return_type())
        || typed.expression_category(value) != Some(ExpressionCategory::Temporary)
        || call.arguments().len() != 4
        || signature.parameters().len() != 4
        || signature.parameter_symbols().len() != 4
        || declaration.len() != 4
        || signature.return_type() != call.return_type()
        || signature.result_source() != CallableResultSource::Owned
    {
        return Err(missing());
    }
    let bindings = [
        outer.key_binding(),
        outer.value_binding(),
        inner.key_binding(),
        inner.value_binding(),
    ];
    let types = [
        outer.key_type(),
        outer.value_type(),
        inner.key_type(),
        inner.value_type(),
    ];
    for (index, argument) in arguments.iter().enumerate() {
        let node = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|_| missing())?;
        if node.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let reference = unique(
            names
                .references()
                .iter()
                .filter(|reference| reference.span() == node.span()),
        )
        .ok_or_else(missing)?;
        if reference.target() != &ReferenceTarget::Symbol(bindings[index]) {
            return Err(unsupported());
        }
        let actual = call.arguments()[index];
        let parameter = signature.parameter_symbols()[index].ok_or_else(missing)?;
        let parameter_symbol = names.symbols().get(parameter.index()).ok_or_else(missing)?;
        if actual.argument_index() != index
            || actual.parameter_index() != index
            || actual.mode() != ParameterMode::Borrow
            || actual.crosses_thread()
            || actual.category() != ExpressionCategory::Place
            || signature.parameters()[index].mode != ParameterMode::Borrow
            || signature.parameters()[index].ty != actual.parameter_type()
            || actual.parameter_type() != types[index]
            || typed.expression_type(argument.value) != Some(types[index])
            || typed.expression_category(argument.value) != Some(ExpressionCategory::Place)
            || typed.symbol_type(bindings[index]) != Some(types[index])
            || typed.parameter_mode(parameter) != Some(ParameterMode::Borrow)
            || typed.symbol_type(parameter) != Some(types[index])
            || parameter_symbol.kind() != SymbolKind::ValueParameter
            || declaration[index].name != NameMarker::Present(parameter_symbol.span())
        {
            return Err(missing());
        }
    }
    Ok(())
}
