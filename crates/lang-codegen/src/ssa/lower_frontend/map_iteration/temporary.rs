//! Temporary source exits retain the real Expression owner without parameter closure phis.
mod cleanup;
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{
        ClosureCaptureSource, IterationSource, MapIterationComponent, OwnershipBindingKind,
    },
    type_checking::{IntrinsicTypeConstructor, MapIterationKind},
};

pub(super) mod guard {
    //! Original lexical Resource sentinel proof, shared by temporary direct exits.
    use super::*;
    use lang_frontend::{
        name_resolution::ReferenceTarget,
        parser::{NameMarker, VariableKind},
        type_checking::{ConstructionTarget, NominalKind},
    };
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in super::super) struct Local {
        pub(in super::super) symbol: SymbolId,
        pub(in super::super) initializer: ExpressionId,
        pub(in super::super) initializer_span: Span,
        pub(in super::super) origin: Span,
        pub(in super::super) ty: TypeId,
        pub(in super::super) deinit: lang_frontend::ast::ItemId,
    }
    pub(in super::super) fn local(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        statement: StatementId,
        span: Span,
    ) -> Result<Local, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let node = parsed
            .ast()
            .statements()
            .get(statement)
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
            kind: VariableKind::Val,
            name: NameMarker::Present(origin),
            initializer,
            ..
        } = item.payload()
        else {
            return Err(unsupported());
        };
        let symbol =
            unique(names.symbols().iter().filter(|symbol| {
                symbol.span() == *origin && symbol.kind() == SymbolKind::Variable
            }))
            .ok_or_else(missing)?;
        let ty = typed.symbol_type(symbol.id()).ok_or_else(missing)?;
        let TypeKind::Nominal { nominal, arguments } = typed.types().get(ty).ok_or_else(missing)?
        else {
            return Err(unsupported());
        };
        let nominal = unique(typed.nominals().iter().filter(|item| item.id() == *nominal))
            .ok_or_else(missing)?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
            || !nominal.fields().is_empty()
            || !nominal.variants().is_empty()
            || !nominal.interfaces().is_empty()
        {
            return Err(unsupported());
        }
        match typed.is_resource_type(ty) {
            Some(true) => {}
            Some(false) => return Err(unsupported()),
            None => return Err(missing()),
        }
        let deinit = nominal.deinit().ok_or_else(missing)?;
        let init = parsed
            .ast()
            .expressions()
            .get(*initializer)
            .map_err(|_| missing())?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = init.payload()
        else {
            return Err(unsupported());
        };
        if !arguments.is_empty() || !type_arguments.is_empty() {
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
        let reference = unique(
            names
                .references()
                .iter()
                .filter(|r| r.span() == callee.span()),
        )
        .ok_or_else(missing)?;
        if reference.target() != &ReferenceTarget::Symbol(nominal.id().symbol()) {
            return Err(missing());
        }
        let construction = typed.construction(*initializer).ok_or_else(missing)?;
        if construction.expression() != *initializer
            || construction.target() != ConstructionTarget::Nominal(nominal.id())
            || construction.result_type() != ty
            || typed.expression_type(*initializer) != Some(ty)
            || !construction.instance().type_arguments().is_empty()
            || !construction.arguments().is_empty()
            || symbol.is_synthetic()
            || !owned.formed_owned_locals().contains(&symbol.id())
            || deinit.owner() != nominal.id()
            || deinit.receiver_type() != ty
            || deinit.receiver_mode() != ParameterMode::Borrow
        {
            return Err(missing());
        }
        let deinit_node = parsed
            .ast()
            .items()
            .get(deinit.item())
            .map_err(|_| missing())?;
        if !matches!(deinit_node.payload(), Item::Deinit { body, .. } if *body == deinit.body())
            || !matches!(
                parsed
                    .ast()
                    .statements()
                    .get(deinit.body())
                    .map_err(|_| missing())?
                    .payload(),
                Statement::Block { .. }
            )
        {
            return Err(missing());
        }
        let nominal_symbol = names
            .symbols()
            .get(nominal.id().symbol().index())
            .ok_or_else(missing)?;
        let declaration =
            unique(
                parsed
                    .ast()
                    .items()
                    .iter()
                    .filter_map(|(_, item)| match item.payload() {
                        Item::Classifier(declaration)
                            if declaration.name == NameMarker::Present(nominal_symbol.span()) =>
                        {
                            Some(declaration)
                        }
                        _ => None,
                    }),
            )
            .ok_or_else(missing)?;
        if !declaration
            .body
            .as_ref()
            .is_some_and(|body| body.members.contains(&deinit.item()))
        {
            return Err(missing());
        }
        Ok(Local {
            symbol: symbol.id(),
            initializer: *initializer,
            initializer_span: init.span(),
            origin: *origin,
            ty,
            deinit: deinit.item(),
        })
    }
}

pub(super) fn qualify<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    plan: &'a MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<qualify::Qualified<'a>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let source =
        crate::ssa::map_source::transparent_expression(parsed, descriptor.source(), |id| {
            typed.expression_type(id).zip(typed.expression_category(id))
        })?;
    let node = parsed
        .ast()
        .expressions()
        .get(source)
        .map_err(|_| missing())?;
    let origin = node.span();
    // Named and temporary providers consume the same selected ordinary Call
    // formation; only the source lifetime/cleanup proof differs below.
    local::producer(
        parsed,
        names,
        typed,
        owned,
        source,
        descriptor.source_type(),
        span,
    )?;
    if typed.copyability(descriptor.source_type()) != Some(Copyability::MoveOnly)
        || plan.source_access() != MapIterationSourceAccess::Temporary
        || plan.source_kind() != &IterationSource::BorrowedSource(LoanTarget::Temporary(source))
    {
        return Err(missing());
    }
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(descriptor.source_type()),
        Some(TypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor
                && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
        || descriptor.key_binding() == descriptor.value_binding()
    {
        return Err(missing());
    }
    // Storage planning admits the represented key/value layout. Temporary lifetime
    // proof depends on this Call owner and its cleanup, not a String/String syntax gate.
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
            || loan.origin() != &LoanTarget::Temporary(source)
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(symbol)
                != Some([descriptor.key_type(), descriptor.value_type()][index])
        {
            return Err(missing());
        }
    }
    if qualify::has_enclosing_control(parsed, span) {
        return Err(unsupported());
    }
    let source_identity = SourceIdentity::Temporary(source);
    if let Some(shape) = conditional::shape(
        parsed,
        names,
        typed,
        *descriptor,
        body,
        source_identity,
        span,
    )? {
        let owner = conditional::qualify(parsed, names, typed, owned, plan, shape, span)?;
        return Ok(qualify::Qualified {
            plan,
            source: source_identity,
            body,
            origin,
            owner,
            body_exit: IterationExitKind::Fallthrough,
            conditional: Some(shape),
            nesting: nested::Nesting { parent: None },
            local: None,
        });
    }
    let body_exit = exit_shape::body_exit(parsed, names, typed, *descriptor, body, span)?;
    let guard = if body_exit == IterationExitKind::Fallthrough {
        None
    } else {
        Some(tail_guard(
            parsed, names, typed, owned, body, body_exit, span,
        )?)
    };
    let proof = cleanup::prove(parsed, owned, plan, body, body_exit, guard, span)?;
    Ok(qualify::Qualified {
        plan,
        source: SourceIdentity::Temporary(source),
        body,
        origin,
        owner: Some(proof),
        body_exit,
        conditional: None,
        nesting: nested::Nesting { parent: None },
        local: None,
    })
}

fn tail_guard(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    body: StatementId,
    _kind: IterationExitKind,
    span: Span,
) -> Result<super::guard::Local, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let body_node = parsed.ast().statements().get(body).map_err(|_| missing())?;
    let Statement::Block { elements } = body_node.payload() else {
        return Err(unsupported());
    };
    let Some((_, prefix)) = elements.split_last() else {
        return Err(unsupported());
    };
    let Some((&first, rest)) = prefix.split_first() else {
        return Err(unsupported());
    };
    let guard = super::guard::local(parsed, names, typed, owned, first, span)?;
    for statement in rest {
        let node = parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| missing())?;
        if !matches!(node.payload(), Statement::Expression { .. }) {
            return Err(unsupported());
        }
    }
    // Formation is historical; only the exact transfer Drop below proves current cleanup.
    // This local is a lexical sentinel, with no aliases, calls, moves or captures.
    if names
        .references()
        .iter()
        .any(|reference| reference.target() == &ReferenceTarget::Symbol(guard.symbol))
        || owned
            .captures()
            .iter()
            .any(|capture| capture.source() == ClosureCaptureSource::Symbol(guard.symbol))
    {
        return Err(unsupported());
    }
    for (expression, node) in parsed.ast().expressions().iter() {
        if contains(body_node.span(), node.span())
            && expression != guard.initializer
            && matches!(node.payload(), Expression::Call { .. })
        {
            let ty = typed.expression_type(expression).ok_or_else(missing)?;
            if typed.is_resource_type(ty).ok_or_else(missing)? {
                return Err(unsupported());
            }
        }
    }
    Ok(guard)
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

pub(in crate::ssa::lower_frontend) mod qualify {
    //! Pure frontend qualification, before instance planning or SSA type allocation.
    use super::super::*;
    use lang_frontend::{
        name_resolution::ReferenceTarget,
        ownership_checking::{IterationSource, MapIterationComponent, OwnershipBindingKind},
        parser::StringPart,
        type_checking::{IntrinsicTypeConstructor, MapIterationDescriptor, MapIterationKind},
    };

    pub(in crate::ssa::lower_frontend::map_iteration) struct Qualified<'a> {
        pub(in crate::ssa::lower_frontend::map_iteration) plan: &'a MapIterationOwnershipPlan,
        pub(in crate::ssa::lower_frontend::map_iteration) source: SourceIdentity,
        pub(in crate::ssa::lower_frontend::map_iteration) body: StatementId,
        pub(in crate::ssa::lower_frontend::map_iteration) origin: Span,
        pub(in crate::ssa::lower_frontend::map_iteration) owner: Option<source_owner::Proof>,
        pub(in crate::ssa::lower_frontend::map_iteration) body_exit: IterationExitKind,
        pub(in crate::ssa::lower_frontend::map_iteration) conditional: Option<conditional::Shape>,
        pub(in crate::ssa::lower_frontend::map_iteration) nesting: nested::Nesting,
        pub(in crate::ssa::lower_frontend::map_iteration) local: Option<local::Proof>,
    }

    pub(in crate::ssa::lower_frontend) fn validate(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
    ) -> Result<(), LoweringError> {
        // validate_inputs has checked the complete acquisition templates and plans.
        // Capability preflight concerns actual acquisitions; a dead tail is never
        // visited by ordinary lowering, and a reached For still requires its plan.
        let mut descriptors = owned
            .map_iterations()
            .iter()
            .map(|plan| plan.descriptor())
            .collect::<Vec<_>>();
        descriptors.sort_by_key(|descriptor| descriptor.statement().index());
        for descriptor in descriptors {
            qualify(parsed, names, typed, owned, *descriptor)?;
        }
        Ok(())
    }

    pub(in crate::ssa::lower_frontend::map_iteration) fn qualify<'a>(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &'a OwnershipCheckedFile,
        descriptor: MapIterationDescriptor,
    ) -> Result<Qualified<'a>, LoweringError> {
        let nesting = nested::nesting(parsed, typed, owned, descriptor.statement())?;
        let mut qualified = qualify_one(parsed, names, typed, owned, descriptor)?;
        qualified.nesting = nesting;
        Ok(qualified)
    }

    fn qualify_one<'a>(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &'a OwnershipCheckedFile,
        descriptor: MapIterationDescriptor,
    ) -> Result<Qualified<'a>, LoweringError> {
        let node = parsed
            .ast()
            .statements()
            .get(descriptor.statement())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = node.span();
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let Statement::For { source, body, .. } = node.payload() else {
            return Err(missing());
        };
        if *source != descriptor.source() {
            return Err(missing());
        }
        // A reached provider cannot use absence of an acquisition plan as capability.
        let plan = owned
            .map_iteration(descriptor.statement())
            .ok_or_else(missing)?;
        if plan.descriptor() != &descriptor
            || typed.map_iteration(descriptor.statement()) != Some(plan.descriptor())
        {
            return Err(missing());
        }
        let canonical = crate::ssa::map_source::transparent_expression(parsed, *source, |id| {
            typed.expression_type(id).zip(typed.expression_category(id))
        })?;
        let source_node = parsed
            .ast()
            .expressions()
            .get(canonical)
            .map_err(|_| missing())?;
        if plan.source_access() == MapIterationSourceAccess::Temporary {
            return temporary::qualify(parsed, names, typed, owned, plan, *body, span);
        }
        if source_node.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let source = plan.source().ok_or_else(missing)?;
        let LoanTarget::Place(place) = source else {
            return Err(unsupported());
        };
        if !place.is_root() {
            return Err(unsupported());
        }
        if plan.source_kind() != &IterationSource::BorrowedSource(source.clone()) {
            return Err(missing());
        }
        let symbol = place.root();
        let references = names
            .references()
            .iter()
            .filter(|reference| reference.span() == source_node.span())
            .collect::<Vec<_>>();
        let binding = match references.as_slice() {
            [reference] => match reference.target() {
                ReferenceTarget::Symbol(binding) => *binding,
                _ => return Err(missing()),
            },
            _ => return Err(missing()),
        };
        let borrowed = if binding == symbol {
            None
        } else {
            Some(borrowed::prove(
                (parsed, names, typed, owned),
                plan,
                binding,
                span,
            )?)
        };
        let data = names.symbols().get(symbol.index()).ok_or_else(missing)?;
        if data.kind() == SymbolKind::Variable {
            return match local::qualify(parsed, names, typed, owned, plan, *body, span) {
                Err(failure) if failure.kind == LoweringErrorKind::UnsupportedNode => {
                    formed::qualify(parsed, names, typed, owned, plan, *body, span)
                }
                result => result,
            };
        }
        if data.kind() != SymbolKind::ValueParameter {
            return Err(unsupported());
        }
        let mode = typed.parameter_mode(symbol).ok_or_else(missing)?;
        let binding = owned.binding_kind(symbol).ok_or_else(missing)?;
        let access = plan.source_access();
        let expected_access = match mode {
            ParameterMode::Value => (OwnershipBindingKind::Owned, MapIterationSourceAccess::Owned),
            ParameterMode::Borrow => (
                OwnershipBindingKind::Shared,
                MapIterationSourceAccess::Shared,
            ),
            ParameterMode::Inout => (
                OwnershipBindingKind::Exclusive,
                MapIterationSourceAccess::Exclusive,
            ),
        };
        if (binding, access) != expected_access {
            return Err(missing());
        }
        if !matches!(mode, ParameterMode::Value | ParameterMode::Borrow) {
            return Err(unsupported());
        }
        let constructor = match descriptor.kind() {
            MapIterationKind::Map => IntrinsicTypeConstructor::Map,
            MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
        };
        if typed.expression_type(descriptor.source()) != Some(descriptor.source_type())
            || typed.symbol_type(symbol) != Some(descriptor.source_type())
            || !matches!(typed.types().get(descriptor.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
                if *actual == constructor && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
            || descriptor.key_binding() == descriptor.value_binding()
            || descriptor.bindings().contains(&symbol)
        {
            return Err(missing());
        }
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = plan.bindings()[index];
            let loan = &plan.loans()[index];
            let symbol = descriptor.bindings()[index];
            let ty = [descriptor.key_type(), descriptor.value_type()][index];
            if binding.symbol() != symbol
                || binding.kind() != OwnershipBindingKind::Shared
                || loan.binding() != symbol
                || loan.component() != component
                || loan.statement() != descriptor.statement()
                || loan.source() != descriptor.source()
                || loan.origin() != source
                || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
                || typed.symbol_type(symbol) != Some(ty)
            {
                return Err(missing());
            }
        }
        if has_enclosing_control(parsed, span) {
            return Err(unsupported());
        }
        let identity = borrowed
            .map(SourceIdentity::BorrowedBinding)
            .unwrap_or(SourceIdentity::Parameter(symbol));
        let conditional =
            conditional::shape(parsed, names, typed, descriptor, *body, identity, span)?;
        let body_exit = if conditional.is_some() {
            IterationExitKind::Fallthrough
        } else {
            exit_shape::body_exit(parsed, names, typed, descriptor, *body, span)?
        };
        if let Some(shape) = conditional {
            let owner = conditional::qualify(parsed, names, typed, owned, plan, shape, span)?;
            return Ok(Qualified {
                plan,
                source: identity,
                body: *body,
                origin: data.span(),
                owner,
                body_exit,
                conditional,
                nesting: nested::Nesting { parent: None },
                local: None,
            });
        }
        exit_shape::cleanup(owned, plan, *body, body_exit, span)?;
        let exhaustion = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
        )
        .ok_or_else(missing)?;
        let owner = if access == MapIterationSourceAccess::Owned {
            Some(
                if typed.is_resource_type(descriptor.source_type()) == Some(true) {
                    resource::qualify(parsed, names, typed, owned, plan, (symbol, None), body_exit)?
                } else {
                    source_owner::qualify(parsed, names, owned, plan, symbol, *body, body_exit)?
                },
            )
        } else {
            if exhaustion.actions().len() != 2 || !plan.closure_phis().is_empty() {
                return Err(unsupported());
            }
            None
        };
        Ok(Qualified {
            plan,
            source: identity,
            body: *body,
            origin: data.span(),
            owner,
            body_exit,
            conditional: None,
            nesting: nested::Nesting { parent: None },
            local: None,
        })
    }

    pub(in crate::ssa::lower_frontend::map_iteration) fn has_enclosing_control(
        parsed: &ParsedFile,
        span: Span,
    ) -> bool {
        let contains = |outer: Span| {
            outer != span
                && outer.source_id() == span.source_id()
                && outer.start() <= span.start()
                && outer.end() >= span.end()
        };
        // Lambda execution has a separate callable/storage boundary. Ordinary
        // If/When/loop transport owns the enclosing CFG and the current source view.
        parsed.ast().expressions().iter().any(|(_, node)| {
            contains(node.span()) && matches!(node.payload(), Expression::Lambda { .. })
        })
    }

    pub(in crate::ssa::lower_frontend::map_iteration) fn straight_statement(
        parsed: &ParsedFile,
        typed: &TypedFile,
        statement: StatementId,
    ) -> Result<bool, LoweringError> {
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        Ok(match node.payload() {
            Statement::Block { elements } => {
                for element in elements {
                    if !straight_statement(parsed, typed, *element)? {
                        return Ok(false);
                    }
                }
                true
            }
            Statement::Expression { expression } => {
                straight_expression(parsed, typed, *expression)?
            }
            Statement::LocalVariable { declaration } => {
                let item = parsed
                    .ast()
                    .items()
                    .get(*declaration)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, node.span()))?;
                match item.payload() {
                    Item::Variable { initializer, .. } => {
                        straight_expression(parsed, typed, *initializer)?
                    }
                    _ => false,
                }
            }
            _ => false,
        })
    }

    fn straight_expression(
        parsed: &ParsedFile,
        typed: &TypedFile,
        expression: ExpressionId,
    ) -> Result<bool, LoweringError> {
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        Ok(match node.payload() {
            Expression::Name | Expression::Literal(_) => true,
            Expression::String { parts } => {
                parts.iter().all(|part| matches!(part, StringPart::Text(_)))
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let result = typed
                    .expression_type(expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
                if builtin_type(typed, result) == Some(BuiltinType::Nothing) {
                    return Ok(false);
                }
                let callee = parsed
                    .ast()
                    .expressions()
                    .get(*callee)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, node.span()))?;
                if callee.payload() != &Expression::Name {
                    return Ok(false);
                }
                for argument in arguments {
                    if !straight_expression(parsed, typed, argument.value)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => false,
        })
    }
}
