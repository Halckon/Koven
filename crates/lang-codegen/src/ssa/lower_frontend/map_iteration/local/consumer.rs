//! One ordinary call reads both components through their actual shared loans.
use super::*;
use lang_frontend::{
    name_resolution::{Namespace, ReferenceTarget},
    ownership_checking::{LoanEndPoint, LoanKind},
    parser::NameMarker,
    type_checking::{
        CallableResultSource, CallableTarget, ExpressionCategory, MapIterationDescriptor,
    },
};

pub(super) fn prove(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    descriptor: &MapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<ExpressionId, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let body = parsed.ast().statements().get(body).map_err(|_| missing())?;
    let Statement::Block { elements } = body.payload() else {
        return Err(unsupported());
    };
    let [statement] = elements.as_slice() else {
        return Err(unsupported());
    };
    let statement = parsed
        .ast()
        .statements()
        .get(*statement)
        .map_err(|_| missing())?;
    let Statement::Expression { expression } = statement.payload() else {
        return Err(unsupported());
    };
    let node = parsed
        .ast()
        .expressions()
        .get(*expression)
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
    if arguments.len() != 2
        || !type_arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
    {
        return Err(unsupported());
    }
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if callee.payload() != &Expression::Name {
        return Err(unsupported());
    }
    let call = typed.call(*expression).ok_or_else(missing)?;
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
        || !parsed.roots().iter().any(|item| parsed.ast().items().get(*item).is_ok_and(|node| {
            matches!(node.payload(), Item::Function { name: NameMarker::Present(marker), .. } if *marker == symbol.span())
        }))
        || call.receiver().is_some() || call.aborts() || call.prints_line()
        || !call.instance().type_arguments().is_empty() || signature.owner().is_some()
        || signature.receiver().is_some() || !signature.type_parameters().is_empty()
        || builtin_type(typed, call.return_type()) != Some(BuiltinType::Unit)
    {
        return Err(unsupported());
    }
    let reference = unique(names.references().iter().filter(|reference| {
        reference.span() == callee.span() && reference.namespace() == Namespace::Value
    }))
    .ok_or_else(missing)?;
    let selected = match reference.target() {
        ReferenceTarget::Symbol(symbol) => *symbol == target,
        ReferenceTarget::OverloadSet(symbols) => symbols.contains(&target),
        _ => false,
    };
    if !selected
        || call.expression() != *expression
        || call.result_source() != CallableResultSource::Owned
        || signature.result_source() != call.result_source()
        || signature.return_type() != call.return_type()
        || typed.expression_type(*expression) != Some(call.return_type())
        || call.arguments().len() != 2
        || signature.parameters().len() != 2
    {
        return Err(missing());
    }
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == *expression)
        .collect::<Vec<_>>();
    let ends = owned
        .loan_ends()
        .iter()
        .filter(|end| end.call() == *expression)
        .collect::<Vec<_>>();
    if loans.len() != 2 || ends.len() != 2 {
        return Err(missing());
    }
    for (index, argument) in arguments.iter().enumerate() {
        let symbol = descriptor.bindings()[index];
        let ty = [descriptor.key_type(), descriptor.value_type()][index];
        let argument_node = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|_| missing())?;
        if argument_node.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let reference = unique(names.references().iter().filter(|reference| {
            reference.span() == argument_node.span() && reference.namespace() == Namespace::Value
        }))
        .ok_or_else(missing)?;
        let parameter = &signature.parameters()[index];
        let mapping = call.arguments()[index];
        let loan = unique(
            loans
                .iter()
                .copied()
                .filter(|loan| loan.argument() == argument.value),
        )
        .ok_or_else(missing)?;
        let end = unique(
            ends.iter()
                .copied()
                .filter(|end| end.argument() == argument.value),
        )
        .ok_or_else(missing)?;
        if reference.target() != &ReferenceTarget::Symbol(symbol)
            || parameter.mode != ParameterMode::Borrow
            || parameter.ty != ty
            || mapping.argument_index() != index
            || mapping.parameter_index() != index
            || mapping.mode() != ParameterMode::Borrow
            || mapping.parameter_type() != ty
            || mapping.crosses_thread()
            || mapping.category() != ExpressionCategory::Place
            || typed.expression_type(argument.value) != Some(ty)
            || typed.expression_category(argument.value) != Some(ExpressionCategory::Place)
            || loan.kind() != LoanKind::Shared
            || loan.begin_span() != argument_node.span()
            || loan.end_span() != node.span()
            || loan.is_receiver_reservation()
            || loan.activation_point().is_some()
            || !matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == symbol)
            || end.point() != LoanEndPoint::CallReturn(*expression)
        {
            return Err(missing());
        }
    }
    Ok(*expression)
}
