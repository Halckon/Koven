//! Bound ordinary local declarations, producer calls and sequential size guards.
use super::*;
use lang_frontend::{
    name_resolution::{Namespace, ReferenceTarget},
    ownership_checking::{IterationSource, MapIterationComponent, OwnershipBindingKind},
    parser::{BinaryOperator, NameMarker, VariableKind},
    type_checking::{
        CallableResultSource, CallableTarget, ExpressionCategory, IntrinsicTypeConstructor,
        MapIterationKind,
    },
};

pub(super) struct Site<'a> {
    pub(super) plan: &'a MapIterationOwnershipPlan,
    pub(super) root: SymbolId,
    pub(super) initializer: ExpressionId,
    pub(super) consumer: ExpressionId,
    pub(super) body: StatementId,
    pub(super) origin: Span,
    pub(super) span: Span,
    pub(super) guard: Option<Guard>,
}

#[derive(Clone, Copy)]
pub(super) struct Guard {
    pub(super) control: ExpressionId,
    pub(super) condition: ExpressionId,
    pub(super) success: StatementId,
    pub(super) failure: StatementId,
    pub(super) size: ExpressionId,
}

pub(super) fn collect<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    current: &MapIterationOwnershipPlan,
    span: Span,
) -> Result<Vec<Site<'a>>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    if qualify::has_enclosing_control(parsed, span) {
        return Err(unsupported());
    }
    let elements =
        unique(
            parsed
                .ast()
                .statements()
                .iter()
                .filter_map(|(_, node)| match node.payload() {
                    Statement::Block { elements }
                        if elements.contains(&current.descriptor().statement()) =>
                    {
                        Some(elements)
                    }
                    _ => None,
                }),
        )
        .ok_or_else(missing)?;
    let mut sites = Vec::new();
    let mut covered = Vec::new();
    for (index, statement) in elements.iter().enumerate() {
        let node = parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| missing())?;
        let Statement::For { source, body, .. } = node.payload() else {
            continue;
        };
        let descriptor = typed.map_iteration(*statement).ok_or_else(unsupported)?;
        // Descriptors without an acquired provider keep the pre-existing U boundary.
        let plan = owned.map_iteration(*statement).ok_or_else(unsupported)?;
        if plan.descriptor() != descriptor || descriptor.source() != *source {
            return Err(missing());
        }
        if sites.len() == 2 {
            return Err(unsupported());
        }
        let prior = index
            .checked_sub(1)
            .and_then(|index| elements.get(index))
            .ok_or_else(unsupported)?;
        let (root, initializer, origin) =
            declaration(parsed, names, typed, owned, plan, *prior, node.span())?;
        let consumer =
            consumer::prove(parsed, names, typed, owned, descriptor, *body, node.span())?;
        let guard = elements
            .get(index + 1)
            .copied()
            .map(|next| {
                size_guard(
                    parsed,
                    names,
                    typed,
                    root,
                    descriptor.source_type(),
                    next,
                    node.span(),
                )
            })
            .transpose()?
            .flatten();
        let later_references = names
            .references()
            .iter()
            .filter(|reference| {
                reference.target() == &ReferenceTarget::Symbol(root)
                    && reference.span().source_id() == node.span().source_id()
                    && reference.span().start() >= node.span().end()
            })
            .collect::<Vec<_>>();
        let allowed_receiver = guard
            .map(|guard| {
                let size = typed.map_size(guard.size).ok_or_else(missing)?;
                parsed
                    .ast()
                    .expressions()
                    .get(size.receiver())
                    .map(|node| node.span())
                    .map_err(|_| missing())
            })
            .transpose()?;
        if later_references.len() != usize::from(allowed_receiver.is_some())
            || later_references
                .iter()
                .any(|reference| Some(reference.span()) != allowed_receiver)
        {
            // A well-formed additional use means a different lifetime/CFG slice.
            // A missing Drop in this exact no-later-use shape remains M below.
            return Err(unsupported());
        }
        covered.extend([*prior, *statement]);
        if guard.is_some() {
            covered.push(elements[index + 1]);
        }
        sites.push(Site {
            plan,
            root,
            initializer,
            consumer,
            body: *body,
            origin,
            span: node.span(),
            guard,
        });
    }
    if sites.is_empty()
        || sites
            .iter()
            .enumerate()
            .any(|(i, site)| sites[..i].iter().any(|other| other.root == site.root))
    {
        return Err(missing());
    }
    // No hidden join/reassignment/extra loop between formation, provider and use.
    // The one recognized size guard is independently justified below and at SSA.
    for statement in elements {
        if !covered.contains(statement) && !qualify::straight_statement(parsed, typed, *statement)?
        {
            return Err(unsupported());
        }
    }
    Ok(sites)
}

fn declaration(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    declaration: StatementId,
    span: Span,
) -> Result<(SymbolId, ExpressionId, Span), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let source = plan.source().ok_or_else(missing)?;
    let LoanTarget::Place(place) = source else {
        return Err(unsupported());
    };
    if !place.is_root() {
        return Err(unsupported());
    }
    let root = place.root();
    let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
    if symbol.kind() != SymbolKind::Variable {
        return Err(unsupported());
    }
    let node = parsed
        .ast()
        .statements()
        .get(declaration)
        .map_err(|_| missing())?;
    let Statement::LocalVariable { declaration } = node.payload() else {
        return Err(unsupported());
    };
    let item = parsed
        .ast()
        .items()
        .get(*declaration)
        .map_err(|_| missing())?;
    let Item::Variable {
        kind,
        name: NameMarker::Present(marker),
        initializer,
        ..
    } = item.payload()
    else {
        return Err(unsupported());
    };
    if !matches!(kind, VariableKind::Val | VariableKind::Var) {
        return Err(unsupported());
    }
    if symbol.is_synthetic()
        || *marker != symbol.span()
        || !owned.formed_owned_locals().contains(&root)
        || reference(
            parsed,
            names,
            crate::ssa::map_source::transparent_expression(parsed, descriptor.source(), |id| {
                typed.expression_type(id).zip(typed.expression_category(id))
            })?,
            span,
        )? != root
        || plan.source_kind() != &IterationSource::BorrowedSource(source.clone())
        || plan.source_access() != MapIterationSourceAccess::Owned
        || typed.expression_category(descriptor.source()) != Some(ExpressionCategory::Place)
        || typed.expression_type(descriptor.source()) != Some(descriptor.source_type())
        || typed.symbol_type(root) != Some(descriptor.source_type())
        || typed.expression_type(*initializer) != Some(descriptor.source_type())
        || typed.copyability(descriptor.source_type()) != Some(Copyability::MoveOnly)
        || descriptor.key_binding() == descriptor.value_binding()
        || descriptor.bindings().contains(&root)
    {
        return Err(missing());
    }
    match typed.is_resource_type(descriptor.source_type()) {
        Some(false) => {}
        Some(true) => return Err(unsupported()),
        None => return Err(missing()),
    }
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(descriptor.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
        if *actual == constructor && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
    {
        return Err(missing());
    }
    if [descriptor.key_type(), descriptor.value_type()]
        .iter()
        .any(|ty| builtin_type(typed, *ty) != Some(BuiltinType::String))
    {
        return Err(unsupported());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        let symbol = descriptor.bindings()[index];
        if binding.symbol() != symbol
            || binding.kind() != OwnershipBindingKind::Shared
            || owned.binding_kind(symbol) != Some(OwnershipBindingKind::Shared)
            || loan.binding() != symbol
            || loan.component() != component
            || loan.statement() != descriptor.statement()
            || loan.source() != descriptor.source()
            || loan.origin() != source
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(symbol)
                != Some([descriptor.key_type(), descriptor.value_type()][index])
        {
            return Err(missing());
        }
    }
    producer(
        parsed,
        names,
        typed,
        owned,
        *initializer,
        descriptor.source_type(),
        span,
    )?;
    Ok((root, *initializer, *marker))
}

fn producer(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    initializer: ExpressionId,
    storage: TypeId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .expressions()
        .get(initializer)
        .map_err(|_| missing())?;
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if callee.payload() != &Expression::Name || !arguments.is_empty() {
        return Err(unsupported());
    }
    let call = typed.call(initializer).ok_or_else(missing)?;
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
    if signature.result_source() != call.result_source() {
        return Err(missing());
    }
    if symbol.kind() != SymbolKind::Function
        || !parsed.roots().iter().any(|item| parsed.ast().items().get(*item).is_ok_and(|node| {
            matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == symbol.span())
        }))
        || call.receiver().is_some() || call.aborts() || call.result_source() != CallableResultSource::Owned
        || !call.instance().type_arguments().is_empty() || signature.owner().is_some()
        || signature.receiver().is_some() || !signature.type_parameters().is_empty() || !signature.parameters().is_empty()
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
        || call.expression() != initializer
        || !call.arguments().is_empty()
        || call.return_type() != storage
        || signature.return_type() != storage
        || typed.expression_category(initializer) != Some(ExpressionCategory::Temporary)
        || owned.loans().iter().any(|loan| loan.call() == initializer)
        || owned
            .loan_ends()
            .iter()
            .any(|end| end.call() == initializer)
    {
        return Err(missing());
    }
    Ok(())
}

fn size_guard(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    root: SymbolId,
    storage: TypeId,
    statement: StatementId,
    span: Span,
) -> Result<Option<Guard>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| missing())?;
    let Statement::Expression {
        expression: control,
    } = node.payload()
    else {
        return Ok(None);
    };
    let node = parsed
        .ast()
        .expressions()
        .get(*control)
        .map_err(|_| missing())?;
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(failure),
        ..
    } = node.payload()
    else {
        return Ok(None);
    };
    let condition_node = parsed
        .ast()
        .expressions()
        .get(*condition)
        .map_err(|_| missing())?;
    let Expression::Binary {
        left: size,
        operator: BinaryOperator::Equal,
        right,
        ..
    } = condition_node.payload()
    else {
        return Err(unsupported());
    };
    let Some(descriptor) = typed.map_size(*size) else {
        return Err(unsupported());
    };
    let right_node = parsed
        .ast()
        .expressions()
        .get(*right)
        .map_err(|_| missing())?;
    if !matches!(right_node.payload(), Expression::Literal(_)) {
        return Err(unsupported());
    }
    if descriptor.expression() != *size
        || descriptor.map_type() != storage
        || reference(parsed, names, descriptor.receiver(), span)? != root
        || typed.expression_type(descriptor.receiver()) != Some(storage)
        || typed
            .expression_type(*size)
            .and_then(|ty| builtin_type(typed, ty))
            != Some(BuiltinType::Int)
        || typed
            .expression_type(*right)
            .and_then(|ty| builtin_type(typed, ty))
            != Some(BuiltinType::Int)
        || typed
            .expression_type(*condition)
            .and_then(|ty| builtin_type(typed, ty))
            != Some(BuiltinType::Boolean)
    {
        return Err(missing());
    }
    let success_node = parsed
        .ast()
        .statements()
        .get(*then_branch)
        .map_err(|_| missing())?;
    let failure_node = parsed
        .ast()
        .statements()
        .get(*failure)
        .map_err(|_| missing())?;
    let Statement::ControlBody { elements: success } = success_node.payload() else {
        return Err(unsupported());
    };
    let Statement::ControlBody {
        elements: failure_elements,
    } = failure_node.payload()
    else {
        return Err(unsupported());
    };
    for statement in success {
        if !qualify::straight_statement(parsed, typed, *statement)? {
            return Err(unsupported());
        }
    }
    let [abort] = failure_elements.as_slice() else {
        return Err(unsupported());
    };
    let abort = parsed
        .ast()
        .statements()
        .get(*abort)
        .map_err(|_| missing())?;
    let Statement::Expression { expression } = abort.payload() else {
        return Err(unsupported());
    };
    let call = typed.call(*expression).ok_or_else(unsupported)?;
    if !call.aborts() || builtin_type(typed, call.return_type()) != Some(BuiltinType::Nothing) {
        return Err(unsupported());
    }
    Ok(Some(Guard {
        control: *control,
        condition: *condition,
        success: *then_branch,
        failure: *failure,
        size: *size,
    }))
}

fn reference(
    parsed: &ParsedFile,
    names: &NameResolution,
    expression: ExpressionId,
    span: Span,
) -> Result<SymbolId, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|_| missing())?;
    if node.payload() != &Expression::Name {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    }
    let reference = unique(names.references().iter().filter(|reference| {
        reference.span() == node.span() && reference.namespace() == Namespace::Value
    }))
    .ok_or_else(missing)?;
    match reference.target() {
        ReferenceTarget::Symbol(symbol) => Ok(*symbol),
        _ => Err(missing()),
    }
}
