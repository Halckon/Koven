//! Source function declaration collection, statement dispatch, constant materialization, and local binding delivery.
use super::{
    AssignmentOperator, Copyability, EntityId, EntityType, ExpressionLowerer, LoweredValue,
    LoweringError, LoweringErrorKind, Operation, TypeId, TypeKind, ValueId, error,
    orchestrate::{self, unwrap_modified},
    present_name, value,
};
use crate::ssa::{
    constant_materialization,
    statement_core::StatementLowering,
};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, SymbolId, SymbolKind},
    ownership_checking::DropPoint,
    parser::{ClassifierKind, Item, ParsedFile},
    source::Span,
    type_checking::{CallableDescriptor, TypedFile},
};

impl StatementLowering for ExpressionLowerer<'_> {
    fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
        self.lower_statement_inner(statement)
    }

    fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        self.emit_drops(DropPoint::AfterStatement(statement))
    }
}

impl ExpressionLowerer<'_> {
    /// Consume the original owner before changing its SSA identity for approved T -> T?.
    pub(super) fn adapt_owned_value_to_expected(
        &mut self,
        expression: ExpressionId,
        owner: ValueId,
        expected: TypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let actual = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let actual = self.resolve_type(actual, span)?;
        let expected = self.resolve_type(expected, span)?;
        if actual == expected {
            return Ok(owner);
        }
        if !matches!(self.typed.types().get(expected), Some(TypeKind::Nullable(inner)) if *inner == actual)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let nullable = self
            .type_ids
            .get(&expected)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.map_results.contains_key(&nullable)
            && self.typed.copyability(actual) == Some(Copyability::Copyable)
        {
            return self.wrap_map_result(owner, nullable, span);
        }
        self.forget_delivered_owners(&[owner]);
        let (_, results) = self.append(
            Operation::NullableWrap { nullable, owner },
            vec![EntityType::Value(nullable)],
            span,
        )?;
        Ok(value(results[0]))
    }

    /// Adapt only the already-mapped owned source-call boundary, preserving its
    /// existing argument slot so later operands carry the wrapped owner.
    pub(super) fn adapt_pending_source_value_argument(
        &mut self,
        expression: ExpressionId,
        owner: ValueId,
        expected: TypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        use crate::ssa::model::SsaTypeKind;
        let qualified = (|| {
            let actual = self
                .resolve_type(self.typed.expression_type(expression)?, span)
                .ok()?;
            let expected = self.resolve_type(expected, span).ok()?;
            if self.typed.copyability(actual) != Some(Copyability::MoveOnly)
                || !matches!(self.typed.types().get(expected), Some(TypeKind::Nullable(inner)) if *inner == actual)
                || self.temporaries.get(&expression.index()).copied() != Some(owner)
            {
                return None;
            }
            let actual_ssa = *self.type_ids.get(&actual)?;
            let nullable = *self.type_ids.get(&expected)?;
            if !matches!(
                self.ssa_types.get(actual_ssa.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            ) || !matches!(self.ssa_types.get(nullable.index()), Some(SsaTypeKind::NullableHandle { inner }) if *inner == actual_ssa)
                || self
                    .function
                    .entity(EntityId::Value(owner))
                    .map(|entity| entity.ty)
                    != Some(EntityType::Value(actual_ssa))
            {
                return None;
            }
            Some(expected)
        })();
        let Some(expected) = qualified else {
            return Ok(owner);
        };
        let wrapped = self.adapt_owned_value_to_expected(expression, owner, expected, span)?;
        self.temporaries.insert(expression.index(), wrapped);
        Ok(wrapped)
    }

    pub(super) fn lower_local_variable(
        &mut self,
        declaration: ItemId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (item, _) = orchestrate::unwrap_modified(self.parsed, declaration)?;
        let Item::Variable {
            name, initializer, ..
        } = item
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if let Some(fact) = self
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.initializer() == initializer)
        {
            return self.lower_result_binding(fact.binding(), initializer, span);
        }
        let mut lowered = self.lower(initializer)?;
        if matches!(lowered, LoweredValue::Diverged) {
            return Ok(lowered);
        }
        let name_span =
            present_name(name).ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.source_slice(name_span)? == "_" {
            return Ok(LoweredValue::Unit);
        }
        let symbol = self.declaration_symbol(name_span, SymbolKind::Variable)?;
        let declared = self
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if let LoweredValue::Value(owner) = lowered {
            let concrete = self.resolve_type(declared, span)?;
            if self.typed.copyability(concrete) == Some(Copyability::MoveOnly) {
                self.forget_delivered_owners(&[owner]);
            } else {
                self.temporaries.retain(|_, temporary| *temporary != owner);
            }
        }
        if let LoweredValue::Value(owner) = lowered {
            lowered = LoweredValue::Value(self.adapt_owned_value_to_expected(
                initializer,
                owner,
                declared,
                span,
            )?);
        }
        self.bindings.insert(symbol, lowered);
        Ok(LoweredValue::Unit)
    }

    pub(super) fn deliver_assignment_rhs(
        &mut self,
        operator: AssignmentOperator,
        expression: ExpressionId,
        assigned: LoweredValue,
        span: Span,
    ) {
        if operator == AssignmentOperator::Assign
            && let LoweredValue::Value(owner) = assigned
            && let Some(template) = self.typed.expression_type(expression)
            && let Ok(concrete) = self.resolve_type(template, span)
            && self.typed.copyability(concrete) == Some(Copyability::MoveOnly)
        {
            self.forget_delivered_owners(&[owner]);
        }
    }

    pub(super) fn lower_constant(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(descriptor) = self
            .typed
            .constants()
            .and_then(|facts| facts.use_at(expression))
        else {
            return Ok(None);
        };
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let plan = self
            .owned
            .constant_materializations()
            .and_then(|facts| facts.plan_at(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let planned = plan.descriptor();
        if planned.expression() != descriptor.expression()
            || planned.target() != descriptor.target()
            || planned.ty() != descriptor.ty()
            || planned.value() != descriptor.value()
        {
            return Err(error(LoweringErrorKind::MismatchedAnalysis, span));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        let operation = constant_materialization::operation(plan.kind(), descriptor.value(), ty)
            .map_err(|kind| error(kind, span))?;
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        Ok(Some(LoweredValue::Value(value(results[0]))))
    }
}

pub(super) struct FunctionDeclaration {
    pub(super) item_id: ItemId,
    pub(super) item: Item,
    pub(super) symbol: SymbolId,
    pub(super) callable: CallableDescriptor,
    pub(super) span: Span,
}

pub(super) fn collect_functions(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<Vec<FunctionDeclaration>, LoweringError> {
    let mut functions = Vec::new();
    for root in parsed.roots() {
        let node = parsed.ast().items().get(*root).map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let (item, span) = unwrap_modified(parsed, *root)?;
        let mut item_id = *root;
        while let Item::Modified { declaration, .. } = parsed
            .ast()
            .items()
            .get(item_id)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            item_id = *declaration;
        }
        let name = match &item {
            Item::Function { name, .. } => name,
            Item::Constant { .. } => continue,
            Item::Classifier(declaration)
                if matches!(
                    declaration.kind,
                    ClassifierKind::Object { .. }
                        | ClassifierKind::Class { .. }
                        | ClassifierKind::ValueClass { .. }
                        | ClassifierKind::Interface { .. }
                        | ClassifierKind::EnumClass { .. }
                ) =>
            {
                continue;
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, node.span())),
        };
        let name_span =
            present_name(*name).ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.span() == name_span && symbol.kind() == SymbolKind::Function)
            .map(|symbol| symbol.id())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))?;
        let callable = typed
            .callables()
            .iter()
            .find(|callable| callable.symbol() == symbol && callable.owner().is_none())
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))?;
        functions.push(FunctionDeclaration {
            item_id,
            item,
            symbol,
            callable,
            span,
        });
    }
    for callable in typed.callables() {
        if super::readonly_receiver::admitted(typed, callable).is_none() {
            continue;
        }
        let symbol = names
            .symbols()
            .get(callable.symbol().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let (item_id, node) = parsed
            .ast()
            .items()
            .iter()
            .find(|(_, node)| {
                matches!(node.payload(), Item::Function { name, .. }
                if present_name(*name) == Some(symbol.span()))
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, symbol.span()))?;
        functions.push(FunctionDeclaration {
            item_id,
            item: node.payload().clone(),
            symbol: callable.symbol(),
            callable: callable.clone(),
            span: node.span(),
        });
    }
    functions.sort_by_key(|declaration| declaration.span.start());
    Ok(functions)
}
