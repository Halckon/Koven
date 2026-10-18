//! The four ordered Borrow arguments belong to one actual source declaration and call.
use super::*;
use lang_frontend::parser::ParameterModeMarker;

pub(super) fn validate(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    pair: [&UnitMapIterationDescriptor; 2],
    operand: UnitExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let source = pair[0].statement().source_unit();
    let node = parsed
        .ast()
        .expressions()
        .get(operand.expression())
        .map_err(|_| bad())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let callee = parsed.ast().expressions().get(*callee).map_err(|_| bad())?;
    if callee.payload() != &Expression::Name
        || arguments.len() != 4
        || !type_arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
    {
        return Err(unsupported());
    }
    let call = typed.call(operand).ok_or_else(bad)?;
    let UnitCallTarget::Declaration(target) = call.target() else {
        return Err(unsupported());
    };
    let declaration = typed.signatures().declaration(target).ok_or_else(bad)?;
    let signature = declaration.callable().ok_or_else(bad)?;
    if call.receiver().is_some()
        || call.aborts()
        || call.prints_line()
        || call.result_source() != CallableResultSource::Owned
        || !call.instance().type_arguments().is_empty()
        || signature.receiver().is_some()
        || signature.extension_receiver_symbol().is_some()
        || !signature.has_body()
        || !signature.type_parameters().is_empty()
        || builtin_type(typed, call.return_type()) != Some(BuiltinType::Int)
    {
        return Err(unsupported());
    }
    let declarations = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| declaration.id() == target)
        .collect::<Vec<_>>();
    let [source_declaration] = declarations.as_slice() else {
        return Err(bad());
    };
    if source_declaration.source_unit() != source {
        return Err(unsupported());
    }
    let (_, item, _) = unwrap_modified(parsed, source_declaration.root())?;
    let Item::Function {
        name, parameters, ..
    } = item
    else {
        return Err(bad());
    };
    let target_symbol = names.names().declaration_symbol(target).ok_or_else(bad)?;
    let local = names
        .names()
        .source_units()
        .get(source.index())
        .ok_or_else(bad)?
        .resolution();
    let symbol = local
        .symbols()
        .get(target_symbol.symbol().index())
        .ok_or_else(bad)?;
    if declaration.symbol() != target_symbol
        || target_symbol.source_unit() != source
        || symbol.kind() != SymbolKind::Function
        || name != NameMarker::Present(symbol.span())
        || source_declaration.name_span() != symbol.span()
        || signature.name_span() != symbol.span()
    {
        return Err(bad());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == source
                && reference.namespace() == Some(Namespace::Value)
                && reference.span() == callee.span()
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(bad());
    };
    let selected = match reference.target() {
        UnitReferenceTarget::Declaration(actual) => *actual == target,
        UnitReferenceTarget::OverloadSet(targets) => targets.contains(&target),
        UnitReferenceTarget::Symbol(actual) => *actual == target_symbol,
        _ => false,
    };
    if !selected
        || operand.source_unit() != source
        || call.expression() != operand
        || typed.expression_type(operand) != Some(call.return_type())
        || typed.expression_category(operand) != Some(ExpressionCategory::Temporary)
        || signature.target() != UnitCallableTarget::Declaration(target)
        || signature.return_type() != call.return_type()
        || signature.result_source() != CallableResultSource::Owned
        || signature.parameters().len() != 4
        || parameters.len() != 4
        || call.arguments().len() != 4
    {
        return Err(bad());
    }
    let bindings = [
        pair[0].key_binding(),
        pair[0].value_binding(),
        pair[1].key_binding(),
        pair[1].value_binding(),
    ];
    let types = [
        pair[0].key_type(),
        pair[0].value_type(),
        pair[1].key_type(),
        pair[1].value_type(),
    ];
    for (index, argument) in arguments.iter().enumerate() {
        let node = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|_| bad())?;
        if node.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let references = names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == source
                    && reference.namespace() == Some(Namespace::Value)
                    && reference.span() == node.span()
            })
            .collect::<Vec<_>>();
        let [reference] = references.as_slice() else {
            return Err(bad());
        };
        if reference.target() != &UnitReferenceTarget::Symbol(bindings[index]) {
            return Err(unsupported());
        }
        if !matches!(
            parameters[index].mode_marker,
            None | Some(ParameterModeMarker::Borrow(_))
        ) {
            return Err(unsupported());
        }
        let parameter = &signature.parameters()[index];
        let parameter_symbol = parameter.symbol().ok_or_else(bad)?;
        let local_symbol = local
            .symbols()
            .get(parameter_symbol.symbol().index())
            .ok_or_else(bad)?;
        let mapping = call.arguments()[index];
        let expression = UnitExpressionId::new(source, argument.value);
        if parameter_symbol.source_unit() != source
            || local_symbol.kind() != SymbolKind::ValueParameter
            || parameters[index].name != NameMarker::Present(local_symbol.span())
            || parameter.span() != parameters[index].span
            || parameter.mode() != ParameterMode::Borrow
            || mapping.mode() != ParameterMode::Borrow
            || mapping.argument_index() != index
            || mapping.parameter_index() != index
            || mapping.crosses_thread()
            || mapping.category() != ExpressionCategory::Place
            || parameter.ty() != types[index]
            || mapping.parameter_type() != types[index]
            || builtin_type(typed, types[index]) != Some(BuiltinType::String)
            || typed.expression_type(expression) != Some(types[index])
            || typed.expression_category(expression) != Some(ExpressionCategory::Place)
            || typed.symbol_type(bindings[index]) != Some(types[index])
            || typed.symbol_type(parameter_symbol) != Some(types[index])
            || owned.binding_kind(parameter_symbol) != Some(OwnershipBindingKind::Shared)
        {
            return Err(bad());
        }
    }
    Ok(())
}
