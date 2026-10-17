//! Source identity for the first actual consumer in one non-null If body.
use super::*;
use crate::ssa::borrow_result_support::ungroup;
use lang_frontend::name_resolution::ScopeKind;
use lang_frontend::parser::VariableKind;
use lang_frontend::type_checking::ExpressionCategory;

impl ExpressionLowerer<'_> {
    pub(super) fn nullable_consume_source(
        &self,
        descriptor: MapConsumeDescriptor,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<NullableConsumeSource, LoweringError> {
        let (root, storage) = descriptor
            .source()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let proof = self
            .non_null_bindings
            .get(&root)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let (control, _) = proof
            .if_origin()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let source = self.nullable_consume_local_source(root, control, span)?;
        let declared = self.resolve_type(storage, span)?;
        let narrowed = self.resolve_type(descriptor.receiver_type(), span)?;
        let nullable = *self
            .type_ids
            .get(&declared)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(self.typed.types().get(declared), Some(TypeKind::Nullable(target)) if *target == narrowed)
            || self.typed.copyability(declared) != Some(Copyability::MoveOnly)
            || self.type_ids.get(&narrowed) != Some(&inner)
            || self.nullable_maps.get(&nullable) != Some(&inner)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_call(descriptor, span)?;
        let mut operand = descriptor.receiver();
        let mut operands = Vec::new();
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let fact = self
                .typed
                .non_null_use(operand)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != operand
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != descriptor.receiver_type()
                || self.typed.expression_type(operand) != Some(descriptor.receiver_type())
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            operands.push(operand);
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                    break;
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }
        operands.reverse();
        let comparison = self.nullable_consume_comparison(control, span)?;
        let condition = self
            .typed
            .null_comparison(comparison)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let source = NullableConsumeSource {
            source,
            control,
            call: descriptor.expression(),
            root,
            storage,
            nullable,
            inner,
            non_null_when_true: condition.non_null_when_true(),
            operands,
        };
        self.validate_nullable_consume_control(&source, span)?;
        Ok(source)
    }

    fn validate_nullable_consume_call(
        &self,
        descriptor: MapConsumeDescriptor,
        span: Span,
    ) -> Result<(), LoweringError> {
        let call = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.expression())
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = call.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let member = self
            .parsed
            .ast()
            .expressions()
            .get(*callee)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !arguments.is_empty()
            || !type_arguments.is_empty()
            || !matches!(member.payload(), Expression::Member { receiver, safe: false, .. }
                if *receiver == descriptor.receiver())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn nullable_consume_comparison(
        &self,
        control: ExpressionId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If { condition, .. } = node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        ungroup(self.parsed, *condition, span)
    }

    pub(super) fn validate_nullable_consume_control(
        &self,
        source: &NullableConsumeSource,
        span: Span,
    ) -> Result<(), LoweringError> {
        let comparison = self.nullable_consume_comparison(source.control, span)?;
        let descriptor = self
            .typed
            .null_comparison(comparison)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != comparison
            || descriptor.symbol() != source.root
            || descriptor.nullable_type() != source.storage
            || descriptor.non_null_when_true() != source.non_null_when_true
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_condition(source, comparison, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(source.control)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If {
            then_branch,
            else_branch,
            ..
        } = node.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let body = if source.non_null_when_true {
            Some(*then_branch)
        } else {
            *else_branch
        }
        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let body = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Statement::ControlBody { elements } = body.payload() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let first = elements
            .first()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let first = self
            .parsed
            .ast()
            .statements()
            .get(*first)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let first_expression = match first.payload() {
            Statement::Expression { expression } => *expression,
            Statement::LocalVariable { declaration } => {
                let (item, _) = orchestrate::unwrap_modified(self.parsed, *declaration)?;
                let Item::Variable {
                    initializer,
                    kind: VariableKind::Val | VariableKind::Var,
                    ..
                } = item
                else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                };
                initializer
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        if ungroup(self.parsed, first_expression, span)? != source.call {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn validate_nullable_consume_condition(
        &self,
        source: &NullableConsumeSource,
        comparison: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(comparison)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let polarity = match operator {
            AstBinaryOperator::NotEqual => true,
            AstBinaryOperator::Equal => false,
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let is_null = |operand| -> Result<bool, LoweringError> {
            let operand = ungroup(self.parsed, operand, span)?;
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            Ok(matches!(
                node.payload(),
                Expression::Literal(LiteralKind::Null)
            ))
        };
        let mut subject = match (is_null(left)?, is_null(right)?) {
            (true, false) => right,
            (false, true) => left,
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if polarity != source.non_null_when_true {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(subject)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(subject) != Some(source.storage)
                || self.typed.expression_category(subject) != Some(ExpressionCategory::Place)
                || self.typed.non_null_use(subject).is_some()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => subject = *expression,
                Expression::Name
                    if self.references.get(&span_key(node.span())) == Some(&source.root) =>
                {
                    return Ok(());
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }
    }

    fn nullable_consume_local_source(
        &self,
        root: SymbolId,
        control: ExpressionId,
        span: Span,
    ) -> Result<SourceToken, LoweringError> {
        let source = self
            .source_token
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let key = self
            .instance_plan
            .source(source)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let callable = self
            .names
            .symbols()
            .get(key.symbol().index())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let local = self
            .names
            .symbols()
            .get(root.index())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.function_ids.get(key) != Some(&self.function.id)
            || callable.id() != key.symbol()
            || callable.kind() != SymbolKind::Function
            || local.id() != root
            || local.kind() != SymbolKind::Variable
            || local.is_synthetic()
            || local.name() == "_"
            || self.typed.parameter_mode(root).is_some()
            || !self.owned.formed_owned_locals().contains(&root)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut functions = self.parsed.ast().items().iter().filter_map(|(_, node)| {
            matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                if *name == callable.span())
            .then_some(node.span())
        });
        let function_span = functions
            .next()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let control_span = self.expression_span(control)?;
        if functions.next().is_some()
            || self.function.origin != Origin::Source(function_span)
            || control_span.source_id() != self.parsed.source_id()
            || function_span.source_id() != control_span.source_id()
            || control_span.start() < function_span.start()
            || control_span.end() > function_span.end()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_local(root, function_span, span)?;
        Ok(source)
    }

    fn validate_nullable_consume_local(
        &self,
        root: SymbolId,
        function_span: Span,
        span: Span,
    ) -> Result<(), LoweringError> {
        let local = &self.names.symbols()[root.index()];
        let mut declarations = 0;
        for (_, node) in self.parsed.ast().statements().iter() {
            let Statement::LocalVariable { declaration } = node.payload() else {
                continue;
            };
            let (item, _) = orchestrate::unwrap_modified(self.parsed, *declaration)?;
            if matches!(item, Item::Variable {
                name: NameMarker::Present(name), kind: VariableKind::Val | VariableKind::Var, ..
            } if name == local.span())
            {
                declarations += 1;
            }
        }
        if declarations != 1 || local.span().source_id() != self.parsed.source_id() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut id = local.scope();
        for _ in 0..self.names.scopes().len() {
            let scope = self
                .names
                .scopes()
                .get(id.index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            match scope.kind() {
                ScopeKind::Function if scope.span() == Some(function_span) => return Ok(()),
                ScopeKind::Block | ScopeKind::ControlBody | ScopeKind::Loop => {
                    id = scope
                        .parent()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        Err(error(LoweringErrorKind::MissingFact, span))
    }
}
