//! 专用常量入口只消费同一轮物化事实；不重访声明 initializer。
use super::*;
use crate::ssa::constant_materialization;
use lang_frontend::{
    name_resolution::DeclarationId,
    ownership_checking::{
        ConstOwnedCompilationUnitView, OwnedCompilationUnitView, OwnedCompilationUnitViewError,
        UnitValueDeliverySource,
    },
    source::Span,
    type_checking::UnitExpressionId,
};
#[cfg(test)]
use lang_frontend::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{
        ConstEnabledOwnedUnit, ValidatedCompilationUnitOwnership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    source::SourceMap,
    type_checking::{ConstEnabledTypedUnit, TypeEnvironment, ValidatedCompilationUnitTypes},
};

impl From<OwnedCompilationUnitViewError> for LoweringError {
    fn from(error: OwnedCompilationUnitViewError) -> Self {
        Self {
            kind: match error {
                OwnedCompilationUnitViewError::MismatchedSource => {
                    LoweringErrorKind::MismatchedSource
                }
                OwnedCompilationUnitViewError::MismatchedAnalysis => {
                    LoweringErrorKind::MismatchedAnalysis
                }
            },
            span: None,
        }
    }
}

/// 保留既有 scalar unit adapter；一次 factory 校验后进入封闭交接。
#[cfg(test)]
pub(crate) fn lower_scalar_unit_with_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    let unit = owned_compilation_unit_view(sources, inputs, names, environment, typed, owned)?;
    lower_owned_unit_with_entry(&unit, entry)
}

/// 已封闭的普通 unit 不再重建来源 index。
pub(crate) fn lower_owned_unit_with_entry(
    unit: &OwnedCompilationUnitView<'_, '_>,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    lower_unit_from_facts(
        unit.sources(),
        unit.inputs(),
        unit.names(),
        unit.types(),
        unit.ownership(),
        None,
        entry,
    )
}

/// 私有交付切片：先验证完整来源，再进入共享 driver；不提供基础 capability 转换。
#[cfg(test)]
pub(crate) fn lower_constant_unit_with_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ConstEnabledTypedUnit,
    owned: &ConstEnabledOwnedUnit,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    let unit =
        const_owned_compilation_unit_view(sources, inputs, names, environment, typed, owned)?;
    lower_const_owned_unit_with_entry(&unit, entry)
}

/// 已封闭 const unit 不再重建 index；保留完整专用物化与短路 capability。
pub(crate) fn lower_const_owned_unit_with_entry(
    unit: &ConstOwnedCompilationUnitView<'_, '_>,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    lower_unit_from_facts(
        unit.sources(),
        unit.inputs(),
        unit.names(),
        unit.types(),
        unit.ownership(),
        Some(unit.constant_ownership()),
        entry,
    )
}

impl UnitExpressionLowerer<'_> {
    /// Validate the real constant materialization owner without changing the argument identity.
    pub(super) fn validate_temporary_delivery_origin(
        &self,
        argument: UnitExpressionId,
        source: &UnitValueDeliverySource,
        span: Span,
    ) -> Result<(), LoweringError> {
        let origin = self
            .constant_materialization_origin(argument, span)?
            .unwrap_or(argument);
        if source != &UnitValueDeliverySource::Temporary(origin) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    /// 与 frontend 一致，只有实际常量物化 use 才穿透透明 Group；不改普通实参来源。
    pub(super) fn constant_materialization_origin(
        &self,
        mut expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<UnitExpressionId>, LoweringError> {
        let Some(owned) = self.constant_owned else {
            return Ok(None);
        };
        loop {
            if owned.materialization_at(expression).is_some() {
                return Ok(Some(expression));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if let Expression::Group { expression: inner } = node.payload() {
                expression = UnitExpressionId::new(expression.source_unit(), *inner);
            } else {
                return Ok(None);
            }
        }
    }

    pub(super) fn lower_constant(
        &mut self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(facts) = self.typed.constants() else {
            return Ok(None);
        };
        let Ok(index) = facts
            .uses()
            .binary_search_by_key(&expression, |use_| use_.expression())
        else {
            return Ok(None);
        };
        let descriptor = &facts.uses()[index];
        let plan = self
            .constant_owned
            .and_then(|owned| owned.materialization_at(expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != descriptor {
            return Err(lowering_error(LoweringErrorKind::MismatchedAnalysis, span));
        }
        let ty = self.expression_ssa_type(expression.expression(), span)?;
        let operation = constant_materialization::operation(plan.kind(), descriptor.value(), ty)
            .map_err(|kind| lowering_error(kind, span))?;
        self.append_scalar(operation, ty, span).map(Some)
    }
}

pub(in crate::ssa::unit_lower) mod branch {
    use super::super::control::*;
    use super::super::*;
    use crate::ssa::statement_core::{self, StatementLowering};
    use lang_frontend::parser::Statement;

    /// Retain shared statement ordering while materializing a Nothing? tail in its
    /// already checked control-result representation, including nested null controls.
    struct NullTail<'a, 'b> {
        lowerer: &'a mut UnitExpressionLowerer<'b>,
        tail: ExpressionId,
        string_result: (UnitTypeId, SsaTypeId),
    }

    impl StatementLowering for UnitExpressionLowerer<'_> {
        fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
            self.lower_statement_inner(statement)
        }

        fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
            self.emit_drops(UnitDropPoint::AfterStatement(
                lang_frontend::type_checking::UnitStatementId::new(self.source_unit, statement),
            ))
        }
    }

    impl StatementLowering for NullTail<'_, '_> {
        fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
            if matches!(self.lowerer.parsed.ast().statements().get(statement)
                .map(|node| node.payload()), Ok(Statement::Expression { expression }) if *expression == self.tail)
            {
                return self
                    .lowerer
                    .lower_joined_null(self.tail, self.string_result);
            }
            self.lowerer.lower_statement_inner(statement)
        }

        fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
            StatementLowering::emit_after_statement(self.lowerer, statement)
        }
    }

    impl UnitExpressionLowerer<'_> {
        #[allow(clippy::too_many_arguments)]
        pub(in crate::ssa::unit_lower) fn lower_if_branch(
            &mut self,
            block: BlockId,
            statement: StatementId,
            receiver: Option<ReceiverBinding>,
            consumed_receiver: Option<ConsumedReceiver>,
            bindings: BTreeMap<UnitSymbolId, LoweredValue>,
            borrow_bindings: BTreeMap<UnitSymbolId, crate::ssa::model::LoanId>,
            nullable_proofs: NullableProofs,
            pending_operands: Vec<EntityId>,
            pending_call_frames: Vec<call_lifetimes::PendingCallFrame>,
            temporaries: BTreeMap<UnitExpressionId, ValueId>,
            drop_point: UnitDropPoint,
            result_plan: ControlResult,
        ) -> Result<Option<BranchExit>, LoweringError> {
            self.block = block;
            self.current_receiver = receiver;
            self.consumed_receiver = consumed_receiver;
            self.bindings = bindings;
            self.borrow_bindings = borrow_bindings;
            self.nullable_proofs = nullable_proofs;
            self.pending_operands = pending_operands;
            self.pending_call_frames = pending_call_frames;
            self.temporaries = temporaries;
            self.lower_current_if_branch(statement, drop_point, result_plan)
        }

        pub(in crate::ssa::unit_lower) fn lower_current_if_branch(
            &mut self,
            statement: StatementId,
            drop_point: UnitDropPoint,
            result_plan: ControlResult,
        ) -> Result<Option<BranchExit>, LoweringError> {
            if !matches!(drop_point, UnitDropPoint::BranchExit { control, .. } if control == result_plan.control)
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    self.statement_span(statement)?,
                ));
            }
            let result_required = result_plan.required;
            let entry_symbols = self.bindings.keys().copied().collect();
            let entry_temporaries = self.temporaries.keys().copied().collect::<Vec<_>>();
            // A branch can diverge through a statement without a syntactic tail value.
            let result_expression = if result_required {
                self.control_tail_expression(statement)?
            } else {
                None
            };
            let string_result = result_plan.string;
            let null_tail = result_expression
                .zip(string_result)
                .and_then(|(tail, string_result)| {
                    let ty = self
                        .typed
                        .expression_type(UnitExpressionId::new(self.source_unit, tail))?;
                    (matches!(self.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
                            if builtin_type(self.typed, *inner) == Some(BuiltinType::Nothing)))
                    .then_some((tail, string_result))
                });
            let mut result = if result_required {
                self.lower_tail_value_body_with_null(statement, null_tail)?
            } else {
                self.lower_statement(statement)?
            };
            if result == LoweredValue::Diverged {
                return Ok(None);
            }
            if result_required && let LoweredValue::Value(value) = result {
                let Some(expression) = result_expression else {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        self.statement_span(statement)?,
                    ));
                };
                if let Some((expected, _)) = string_result.filter(|_| null_tail.is_none()) {
                    let (value, transferred) = self.adapt_owned_value_to_expected(
                        expression,
                        value,
                        expected,
                        self.statement_span(statement)?,
                    )?;
                    if !transferred {
                        self.transfer_owned_expression(
                            expression,
                            value,
                            self.statement_span(statement)?,
                        )?;
                    }
                    result = LoweredValue::Value(value);
                } else if null_tail.is_none() {
                    self.transfer_owned_expression(
                        expression,
                        value,
                        self.statement_span(statement)?,
                    )?;
                }
            }
            let expected_result = if result_required {
                matches!(result, LoweredValue::Value(_))
            } else {
                result == LoweredValue::Unit
            };
            if !expected_result || self.temporaries.keys().copied().ne(entry_temporaries) {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    self.statement_span(statement)?,
                ));
            }
            self.emit_drops(drop_point)?;
            self.discard_non_entry_bindings(&entry_symbols, self.statement_span(statement)?)?;
            Ok(Some(BranchExit {
                block: self.block,
                result,
                receiver: self.current_receiver,
                consumed_receiver: self.consumed_receiver,
                bindings: self.bindings.clone(),
                borrow_bindings: self.borrow_bindings.clone(),
                nullable_proofs: self.nullable_proofs.clone(),
                pending_operands: self.pending_operands.clone(),
                pending_call_frames: self.pending_call_frames.clone(),
                temporaries: self.temporaries.clone(),
            }))
        }

        fn control_tail_expression(
            &self,
            statement: StatementId,
        ) -> Result<Option<ExpressionId>, LoweringError> {
            let node = self
                .parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            let tail = match node.payload() {
                Statement::Expression { expression } => return Ok(Some(*expression)),
                Statement::ControlBody { elements } => elements.last(),
                _ => None,
            };
            let Some(tail) = tail else {
                return Ok(None);
            };
            let tail = self
                .parsed
                .ast()
                .statements()
                .get(*tail)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
            Ok(match tail.payload() {
                Statement::Expression { expression } => Some(*expression),
                _ => None,
            })
        }

        fn lower_tail_value_body_with_null(
            &mut self,
            statement: StatementId,
            null_tail: Option<(ExpressionId, (UnitTypeId, SsaTypeId))>,
        ) -> Result<LoweredValue, LoweringError> {
            let node = self
                .parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            let elements = match node.payload() {
                lang_frontend::parser::Statement::ControlBody { elements }
                | lang_frontend::parser::Statement::LambdaBody { elements } => elements,
                _ => {
                    return if let Some((tail, string_result)) = null_tail {
                        statement_core::lower_statement(
                            &mut NullTail {
                                lowerer: self,
                                tail,
                                string_result,
                            },
                            statement,
                        )
                    } else {
                        self.lower_statement(statement)
                    };
                }
            };
            let elements = elements.clone();
            if let Some((tail, string_result)) = null_tail {
                statement_core::lower_tail_sequence(
                    &mut NullTail {
                        lowerer: self,
                        tail,
                        string_result,
                    },
                    statement,
                    &elements,
                )
            } else {
                statement_core::lower_tail_sequence(self, statement, &elements)
            }
        }

        /// Preserve nested control evaluation and Group cleanup in the published joined type.
        fn lower_joined_null(
            &mut self,
            expression: ExpressionId,
            string_result: (UnitTypeId, SsaTypeId),
        ) -> Result<LoweredValue, LoweringError> {
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
            let operands = self.nullable_when_group_chain(
                UnitExpressionId::new(self.source_unit, expression),
                span,
            )?;
            for &id in &operands {
                self.guard_nullable_when_expression(id, span)?;
                let ty = self
                    .typed
                    .expression_type(id)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if !matches!(self.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
                    if builtin_type(self.typed, *inner) == Some(BuiltinType::Nothing))
                    || self.typed.copyability(ty) != Copyability::Copyable
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            let literal = operands
                .last()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(literal.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let terminal = literal.expression();
            let terminal_span = node.span();
            let payload = node.payload().clone();
            let result = match payload {
                Expression::Literal(LiteralKind::Null) => self.append_scalar(
                    Operation::NullableNull {
                        nullable: string_result.1,
                    },
                    string_result.1,
                    terminal_span,
                )?,
                Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    let plan = self.control_result(terminal, Some(string_result), terminal_span)?;
                    self.lower_conditional(
                        terminal,
                        condition,
                        then_branch,
                        else_branch,
                        0,
                        1,
                        terminal_span,
                        plan,
                    )?
                }
                Expression::When {
                    subject, entries, ..
                } => self.lower_boolean_when(
                    terminal,
                    subject,
                    &entries,
                    terminal_span,
                    Some(string_result),
                )?,
                _ => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        terminal_span,
                    ));
                }
            };
            if result == LoweredValue::Diverged {
                return Ok(result);
            }
            for id in operands.into_iter().rev() {
                self.emit_drops(UnitDropPoint::AfterExpression(id))?;
            }
            Ok(result)
        }

        pub(in crate::ssa::unit_lower) fn statement_span(
            &self,
            statement: StatementId,
        ) -> Result<Span, LoweringError> {
            self.parsed
                .ast()
                .statements()
                .get(statement)
                .map(|statement| statement.span())
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssa::{
        model::SsaTypeKind,
        unit_lower_test_support::{declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::check_compilation_unit_constant_ownership,
        type_checking::{check_compilation_unit_types, standard_environments},
    };

    #[test]
    fn namespace_constants_need_no_storage_initializer_or_closure_capture() {
        for (ty, literal) in [("Int", "42"), ("String", "\"中文\"")] {
            let constant = format!("const val VALUE: {ty} = {literal}");
            for (prefix, root) in [
                ("", constant.clone()),
                ("Config.", format!("object Config {{ {constant} }}")),
                (
                    "Config.",
                    format!("class Config {{ companion {{ {constant} }} }}"),
                ),
                (
                    "Config.",
                    format!("value class Config(val item: Int) {{ companion {{ {constant} }} }}"),
                ),
                (
                    "Config.",
                    format!("interface Config {{ companion {{ {constant} }} }}"),
                ),
                (
                    "Config.",
                    format!("enum class Config {{ One; companion {{ {constant} }} }}"),
                ),
            ] {
                let mut sources = SourceMap::new();
                let (provider_source, provider) =
                    parsed(&mut sources, "p/provider.ko", &format!("package p\n{root}"));
                let (consumer_source, consumer) = parsed(
                    &mut sources,
                    "q/consumer.ko",
                    &format!(
                        "package q\nfun entry(): {ty} {{ val action: () -> {ty} = {{ -> p.{prefix}VALUE }}\nreturn action() }}"
                    ),
                );
                let inputs = [
                    SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
                    SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
                ];
                let (name_environment, environment) = standard_environments();
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names =
                    resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
                        .unwrap()
                        .validate()
                        .unwrap();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
                    .unwrap()
                    .validate_constants()
                    .unwrap();
                let owned = check_compilation_unit_constant_ownership(
                    &sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                let entry = declaration(&names, "q", "entry");
                let (program, _) = lower_constant_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                )
                .unwrap();
                let module = &program.modules[0];
                assert_eq!(
                    module.functions.len(),
                    2,
                    "only entry and capture-free lambda: {root}"
                );
                assert!(
                    !module
                        .types
                        .iter()
                        .any(|ty| matches!(ty, SsaTypeKind::ConcreteClosure { .. }))
                );
                // An allowlist excludes namespace allocation, loads/stores and initializer calls.
                for instruction in module
                    .functions
                    .iter()
                    .flat_map(|function| &function.instructions)
                {
                    assert!(
                        matches!(
                            instruction.operation,
                            Operation::Constant(_)
                                | Operation::StringLiteral { .. }
                                | Operation::FunctionAddress { .. }
                                | Operation::CallableInvoke { .. }
                                | Operation::Drop { .. }
                        ),
                        "{root}: {:?}",
                        instruction.operation
                    );
                }
                let ir = crate::llvm::render_verified_program(&program).unwrap();
                let globals = ir
                    .lines()
                    .filter(|line| line.starts_with('@'))
                    .collect::<Vec<_>>();
                assert_eq!(globals.len(), usize::from(ty == "String"), "{ir}");
                assert!(
                    globals
                        .iter()
                        .all(|line| line.contains("private constant [6 x i8]")),
                    "only literal byte data is allowed: {ir}"
                );
                assert!(!ir.contains("@llvm.global_ctors") && !ir.contains("@llvm.global_dtors"));

                // Exercise the private driver's missing-fact guard without exposing a forgeable
                // public capability or adding a test-only mutation API to the frontend.
                let error = lower_unit_from_facts(
                    &sources,
                    &inputs,
                    &names,
                    typed.types(),
                    owned.ownership(),
                    None,
                    entry,
                )
                .err()
                .unwrap();
                assert_eq!(error.kind, LoweringErrorKind::MissingFact);
                assert_eq!(
                    sources.slice(error.span.unwrap()).unwrap(),
                    format!("p.{prefix}VALUE")
                );
            }
        }
    }
}
