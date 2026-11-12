//! Validated String clone / field-loan lowering and Copyable Map result comparison/unwrap helpers.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::DropPoint,
    parser::BinaryOperator,
    source::Span,
    type_checking::{ParameterMode, StringOperationKind},
};

use super::{
    ComparisonOperator, EntityId, EntityType, ExpressionLowerer, LoanKind, LoweredValue,
    LoweringError, LoweringErrorKind, Operation, ScalarConstant, SsaTypeId, ValueId, error, value,
};

impl ExpressionLowerer<'_> {
    pub(super) fn map_result_expression_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<SsaTypeId>, LoweringError> {
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ty = self.resolve_type(ty, span)?;
        Ok(self
            .type_ids
            .get(&ty)
            .copied()
            .filter(|ty| self.map_results.contains_key(ty)))
    }

    pub(super) fn is_map_result_value(&self, source: ValueId) -> bool {
        self.function
            .entity(EntityId::Value(source))
            .is_some_and(|data| self.map_results.contains_key(&data.ty.semantic_type()))
    }

    pub(super) fn unwrap_map_result(
        &mut self,
        result: ValueId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let ty = self
            .function
            .entity(EntityId::Value(result))
            .and_then(|data| self.map_results.get(&data.ty.semantic_type()).copied())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, output) = self.append(
            Operation::MapResultUnwrap { result },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(output[0])))
    }

    pub(super) fn wrap_map_result(
        &mut self,
        payload: ValueId,
        result: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let present = self
            .typed
            .types()
            .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
            .and_then(|ty| self.type_ids.get(&ty).copied())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, flag) = self.append(
            Operation::Constant(ScalarConstant::Boolean(true)),
            vec![EntityType::Value(present)],
            span,
        )?;
        let (_, wrapped) = self.append(
            Operation::AggregateConstruct {
                aggregate: result,
                fields: vec![value(flag[0]), payload],
            },
            vec![EntityType::Value(result)],
            span,
        )?;
        Ok(value(wrapped[0]))
    }

    pub(super) fn is_null_literal(&self, expression: ExpressionId) -> bool {
        crate::ssa::lowering_support::is_null_literal(self.parsed, expression)
    }

    pub(super) fn lower_map_result_binary(
        &mut self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
            return Ok(None);
        }
        let left_type = self.map_result_expression_type(left, span)?;
        let right_type = self.map_result_expression_type(right, span)?;
        if left_type.is_none() && right_type.is_none() {
            return Ok(None);
        }
        let boolean = self.expression_ssa_type(expression, span)?;
        let nullable = if self.is_null_literal(right) && left_type.is_some() {
            Some(left)
        } else if self.is_null_literal(left) && right_type.is_some() {
            Some(right)
        } else {
            None
        };
        if let Some(nullable) = nullable {
            let aggregate = self.require_value(nullable)?;
            let (_, present) = self.append(
                Operation::AggregateProject {
                    aggregate,
                    field: 0,
                },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            let present = value(present[0]);
            if operator == BinaryOperator::NotEqual {
                return Ok(Some(LoweredValue::Value(present)));
            }
            let (_, absent) = self.append(
                Operation::BooleanNot { operand: present },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            return Ok(Some(LoweredValue::Value(value(absent[0]))));
        }
        let mut left = self.require_value(left)?;
        let mut right = self.require_value(right)?;
        if let Some(ty) = left_type
            && right_type.is_none()
        {
            right = self.wrap_map_result(right, ty, span)?;
        }
        if let Some(ty) = right_type
            && left_type.is_none()
        {
            left = self.wrap_map_result(left, ty, span)?;
        }
        let comparison = if operator == BinaryOperator::Equal {
            ComparisonOperator::Equal
        } else {
            ComparisonOperator::NotEqual
        };
        let (_, result) = self.append(
            Operation::Compare {
                operator: comparison,
                left,
                right,
            },
            vec![EntityType::Value(boolean)],
            span,
        )?;
        Ok(Some(LoweredValue::Value(value(result[0]))))
    }

    pub(super) fn lower_string_clone(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .string_operation(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let effect = self
            .owned
            .string_effect(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.kind() != StringOperationKind::Clone
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || descriptor.result_mode() != ParameterMode::Value
            || effect.receiver() != descriptor.receiver()
            || effect.receiver_type() != descriptor.receiver_type()
            || effect.result_type() != descriptor.result_type()
            || effect.kind() != descriptor.kind()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver();
        let receiver_span = self.expression_span(receiver)?;
        let (source, created) = if self.clone_field_projection(receiver)?.is_some() {
            let mut created = Vec::new();
            let source = self.clone_field_loan(receiver, &mut created)?;
            (source, created)
        } else {
            let (source, created) =
                self.lower_borrow_argument(expression, receiver, receiver_span)?;
            (source, if created { vec![source] } else { Vec::new() })
        };
        let result_type = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::StringClone { source },
            vec![EntityType::Value(result_type)],
            span,
        )?;
        for loan in created.into_iter().rev() {
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.emit_drops(DropPoint::AfterExpression(receiver))?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn clone_field_projection(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<lang_frontend::type_checking::AggregateProjectionDescriptor>, LoweringError>
    {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
            return self.clone_field_projection(*expression);
        }
        Ok(self.typed.aggregate_projection(expression))
    }

    pub(super) fn clone_field_loan(
        &mut self,
        expression: ExpressionId,
        created: &mut Vec<super::LoanId>,
    ) -> Result<super::LoanId, LoweringError> {
        use lang_frontend::type_checking::{AggregateProjectionReceiver, NominalKind, TypeKind};
        let span = self.expression_span(expression)?;
        let target = self.expression_ssa_type(expression, span)?;
        if let Some((loan, loans)) = self.readonly_view(expression)? {
            created.extend(loans);
            return Ok(loan);
        }

        if let Some(projection) = self.clone_field_projection(expression)? {
            let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let receiver_type = self
                .typed
                .expression_type(receiver)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let receiver_type = self.resolve_type(receiver_type, span)?;
            let Some(TypeKind::Nominal { nominal, .. }) = self.typed.types().get(receiver_type)
            else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let nominal = self
                .typed
                .nominals()
                .iter()
                .find(|value| value.id() == *nominal)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let field = nominal
                .fields()
                .iter()
                .position(|field| *field == projection.field())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let kind = nominal.kind();
            let base = self.clone_field_loan(receiver, created)?;
            let operation = match kind {
                NominalKind::Class => Operation::SharedHeapFieldLoan { base, field },
                NominalKind::ValueClass => Operation::SharedFieldLoan { base, field },
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let (_, results) = self.append(
                operation,
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                span,
            )?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            created.push(loan);
            return Ok(loan);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
            return self.clone_field_loan(*expression, created);
        }
        if matches!(node.payload(), lang_frontend::parser::Expression::Name)
            && let Some(symbol) = self.references.get(&super::span_key(node.span()))
            && let Some(loan) = self.borrow_bindings.get(symbol).copied()
        {
            return Ok(loan);
        }
        if self.references.get(&super::span_key(node.span())).is_some_and(|symbol| {
            self.owned.loans().iter().any(|active| {
                self.pending_call_loans.contains_key(&(active.call().index(), active.argument().index()))
                    && active.kind() == lang_frontend::ownership_checking::LoanKind::Exclusive
                    && matches!(active.target(), lang_frontend::ownership_checking::LoanTarget::Place(path)
                        if !path.is_root() && path.root() == *symbol)
            })
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.require_value(expression)?;
        let (_, root) = self.append(
            Operation::RootPlace { owner },
            vec![EntityType::Place(target)],
            span,
        )?;
        let (_, result) = self.append(
            Operation::BorrowBegin {
                place: super::place(root[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            span,
        )?;
        let EntityId::Loan(loan) = result[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        created.push(loan);
        Ok(loan)
    }
}

pub(in crate::ssa::lower_frontend) mod declarations {
    //! Source function declaration collection, statement dispatch, constant materialization, and local binding delivery.
    use super::super::{
        AssignmentOperator, Copyability, EntityId, EntityType, ExpressionLowerer, LoweredValue,
        LoweringError, LoweringErrorKind, Operation, TypeId, TypeKind, ValueId, error,
        orchestrate::{self, unwrap_modified},
        present_name, readonly_receiver, value,
    };
    use crate::ssa::{constant_materialization, statement_core::StatementLowering};
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
        pub(in crate::ssa::lower_frontend) fn adapt_owned_value_to_expected(
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
        pub(in crate::ssa::lower_frontend) fn adapt_pending_source_value_argument(
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

        pub(in crate::ssa::lower_frontend) fn lower_local_variable(
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

        pub(in crate::ssa::lower_frontend) fn deliver_assignment_rhs(
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

        pub(in crate::ssa::lower_frontend) fn lower_constant(
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
            let operation =
                constant_materialization::operation(plan.kind(), descriptor.value(), ty)
                    .map_err(|kind| error(kind, span))?;
            let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
            Ok(Some(LoweredValue::Value(value(results[0]))))
        }
    }

    pub(in crate::ssa::lower_frontend) struct FunctionDeclaration {
        pub(in crate::ssa::lower_frontend) item_id: ItemId,
        pub(in crate::ssa::lower_frontend) item: Item,
        pub(in crate::ssa::lower_frontend) symbol: SymbolId,
        pub(in crate::ssa::lower_frontend) callable: CallableDescriptor,
        pub(in crate::ssa::lower_frontend) span: Span,
    }

    pub(in crate::ssa::lower_frontend) fn collect_functions(
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
            if readonly_receiver::admitted(typed, callable).is_none() {
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
}
