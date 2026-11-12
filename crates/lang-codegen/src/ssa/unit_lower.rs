//! SPEC-0199 compilation-unit frontend 到单一 verified SSA module 的 lowering。
mod orchestrate;
use orchestrate::lower_unit_from_facts;

mod aggregate;
mod borrow;
pub(in crate::ssa) use super::provider::borrow_result;
mod call;
mod call_lifetimes;
use call_lifetimes::callable_abi;
mod cfg;
mod closure;
pub(crate) mod constant;
mod construction;
mod container;
mod control;
mod loop_control;
mod map;
use construction::nullable_proof;
mod nullable_when;
mod ownership;
mod receiver;
use control::scalar;
pub(in crate::ssa) mod type_lower;
mod type_plan;

#[cfg(test)]
pub(crate) use borrow_result::validate_bindings;
pub(crate) use constant::lower_owned_unit_with_entry;
#[cfg(test)]
pub(crate) use constant::lower_scalar_unit_with_entry;

use nullable_proof::NullableProofs;
use std::collections::BTreeMap;

pub(in crate::ssa) use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{CompilationUnitOwnership, ConstEnabledOwnedUnit, UnitDropPoint},
    parser::{
        Expression, FunctionBody, FunctionForm, IntegerLiteralKind, Item, LiteralKind, ParsedFile,
        Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, Copyability, ExpressionCategory, ParameterMode,
        UnitCallableTarget, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeId,
        UnitTypeKind,
    },
};

pub(in crate::ssa) use super::{
    LoweringError, LoweringErrorKind,
    lowering_support::{
        error as lowering_error, parse_integer_literal_text, present_name, span_key, string_literal,
    },
    model::{
        BlockId, EntityId, EntityType, Function, FunctionId, LoanId, LoanKind, Operation, Origin,
        Program, ScalarConstant, SsaTypeId, TerminatorKind, ValueId,
    },
    unit_plan::{
        MAX_UNIT_GENERIC_INSTANCES, UnitFunctionInstanceKey, UnitPlannedInstance,
        UnitRuntimeTypeDemand, plan_unit_instances_from_facts, resolve_concrete_type,
        unit_callable_signature,
    },
    verify::verify_program,
};

pub(in crate::ssa) use super::expression_core::LoweredValue;

/// Same-source span containment formerly hosted by the retired map-iteration module.
pub(in crate::ssa) fn span_contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

pub(in crate::ssa) fn strictly_contains(outer: Span, inner: Span) -> bool {
    span_contains(outer, inner) && (outer.start() < inner.start() || inner.end() < outer.end())
}

/// Nearest enclosing lambda; identity filtering, not capability.
pub(super) fn enclosing_lambda(parsed: &ParsedFile, span: Span) -> Option<ExpressionId> {
    parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| {
            span_contains(node.span(), span) && matches!(node.payload(), Expression::Lambda { .. })
        })
        .min_by_key(|(_, node)| node.span().end() - node.span().start())
        .map(|(id, _)| id)
}

enum FunctionPlanBody {
    Expression(ExpressionId),
    Block(StatementId),
}

struct FunctionPlan {
    id: FunctionId,
    instance: UnitPlannedInstance,
    function_item: ItemId,
    body: FunctionPlanBody,
    receiver: Option<ReceiverPlan>,
    parameter_symbols: Vec<UnitSymbolId>,
    /// The first Inout parameter's declaration; only the refinement layer lowers it.
    inout_parameter: Option<Span>,
    return_type: UnitTypeId,
}

#[derive(Clone, Copy)]
struct ReceiverPlan {
    owner: DeclarationId,
    mode: ParameterMode,
    template_ty: UnitTypeId,
    ty: UnitTypeId,
    entity_type: EntityType,
    origin: Span,
}

#[derive(Clone, Copy)]
struct ReceiverBinding {
    owner: DeclarationId,
    mode: ParameterMode,
    template_ty: UnitTypeId,
    ty: UnitTypeId,
    entity: EntityId,
    origin: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConsumedReceiver {
    owner: DeclarationId,
    mode: ParameterMode,
    template_ty: UnitTypeId,
    ty: UnitTypeId,
    origin: Span,
}

impl From<ReceiverBinding> for ConsumedReceiver {
    fn from(receiver: ReceiverBinding) -> Self {
        Self {
            owner: receiver.owner,
            mode: receiver.mode,
            template_ty: receiver.template_ty,
            ty: receiver.ty,
            origin: receiver.origin,
        }
    }
}

pub(in crate::ssa) struct UnitExpressionLowerer<'a> {
    migrated_family_compatibility: bool,
    sources: &'a SourceMap,
    pub(in crate::ssa) parsed: &'a lang_frontend::parser::ParsedFile,
    pub(in crate::ssa) source_unit: SourceUnitId,
    pub(in crate::ssa) names: &'a ValidatedCompilationUnitNames,
    pub(in crate::ssa) typed: &'a CompilationUnitTypes,
    pub(in crate::ssa) owned: &'a CompilationUnitOwnership,
    constant_owned: Option<&'a ConstEnabledOwnedUnit>,
    function_ids: &'a BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    source_plan: &'a super::unit_plan::UnitCallablePlan,
    callable_abi: &'a callable_abi::CallableAbi,
    source_token: super::lowering_support::callable_instances::SourceToken,
    type_ids: &'a BTreeMap<UnitTypeId, SsaTypeId>,
    heap_payloads: &'a BTreeMap<SsaTypeId, SsaTypeId>,
    map_results: &'a BTreeMap<SsaTypeId, SsaTypeId>,
    nullable_maps: &'a BTreeMap<SsaTypeId, SsaTypeId>,
    ssa_types: &'a [crate::ssa::model::SsaTypeKind],
    field_indices: &'a BTreeMap<(UnitTypeId, UnitSymbolId), usize>,
    substitutions: &'a BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    references: &'a BTreeMap<(usize, usize), UnitSymbolId>,
    pub(in crate::ssa) function: &'a mut Function,
    pub(in crate::ssa) block: BlockId,
    bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    pub(in crate::ssa) borrow_bindings: BTreeMap<UnitSymbolId, LoanId>,
    entry_boolean_roots: std::collections::BTreeSet<UnitSymbolId>,
    nullable_proofs: NullableProofs,
    current_receiver: Option<ReceiverBinding>,
    consumed_receiver: Option<ConsumedReceiver>,
    closure_binding_context: bool,
    callable_plans: &'a BTreeMap<closure::CallablePlanKey, closure::CallablePlan>,
    temporaries: BTreeMap<UnitExpressionId, ValueId>,
    // Evaluated operands remain live until their call or construction consumes them.
    pending_operands: Vec<EntityId>,
    pending_call_frames: Vec<call_lifetimes::PendingCallFrame>,
    loops: Vec<loop_control::LoopContext>,
    return_type: UnitTypeId,
}

impl UnitExpressionLowerer<'_> {
    fn lower(&mut self, expression: ExpressionId) -> Result<LoweredValue, LoweringError> {
        let result = self.lower_expression(expression)?;
        let unit_expression = UnitExpressionId::new(self.source_unit, expression);
        if let LoweredValue::Value(value) = result
            && self
                .typed
                .expression_type(unit_expression)
                .map(|ty| self.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
            && (self.typed.expression_category(unit_expression)
                == Some(ExpressionCategory::Temporary)
                || self.typed.construction(unit_expression).is_some())
        {
            self.temporaries.insert(unit_expression, value);
        }
        if result != LoweredValue::Diverged {
            self.emit_drops(UnitDropPoint::AfterExpression(unit_expression))?;
        }
        Ok(result)
    }

    fn lower_expression(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = node.span();
        let unit_expression = UnitExpressionId::new(self.source_unit, expression);
        self.guard_nullable_when_expression(unit_expression, span)?;
        // Reject unsupported If carriers/transfers before specialized dispatch can
        // create a call frame, a temporary owner, or any SSA instruction.
        if matches!(
            node.payload(),
            Expression::Call { .. }
                | Expression::Lambda { .. }
                | Expression::String { .. }
                | Expression::Index { .. }
                | Expression::NonNullAssert { .. }
                | Expression::CallableReference { .. }
                | Expression::Propagate { .. }
                | Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. }
        ) {
            let root_borrow_call = match node.payload() {
                Expression::Call { arguments, .. } => {
                    self.can_call_with_nullable_root_proof(unit_expression, arguments, span)?
                }
                _ => false,
            };
            if !root_borrow_call {
                self.reject_live_if_proof(span)?;
            }
        }
        if self.typed.range_size(unit_expression).is_some()
            || self.typed.string_operation(unit_expression).is_some()
            || self.typed.rc_operation(unit_expression).is_some()
            || self
                .typed
                .integer_range_operation(unit_expression)
                .is_some()
            || self.typed.integer_operation(unit_expression).is_some()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some(value) = self.lower_constant(unit_expression, span)? {
            return Ok(value);
        }
        if let Some(construction) = self.typed.construction(unit_expression) {
            return match construction.target() {
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicRc => {
                    self.lower_rc_construction(expression, span)
                }
                lang_frontend::type_checking::UnitConstructionTarget::Nominal(_) => {
                    self.lower_aggregate_construction(expression, span)
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicBox
                    if !self.typed.nullable_whens().is_empty() =>
                {
                    self.lower_aggregate_construction(expression, span)
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicBox
                | lang_frontend::type_checking::UnitConstructionTarget::EnumCase(_) => {
                    Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
                }
            };
        }
        if let Some(value) = self.lower_container_expression(expression, span)? {
            return Ok(value);
        }
        if let Some(value) = self.lower_map_expression(expression, span)? {
            return Ok(value);
        }
        if self.typed.aggregate_projection(unit_expression).is_some() {
            return self.lower_aggregate_projection(expression, span);
        }
        match node.payload() {
            Expression::Literal(literal) => self.lower_literal(*literal, expression, span),
            Expression::String { .. } => self.lower_string_literal(expression, span),
            Expression::Name => self.lower_name(expression, span),
            Expression::This => self.lower_this(expression, span),
            Expression::Group { expression } => self.lower(*expression),
            Expression::NonNullAssert { .. } | Expression::TypeTest { .. } => {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            }
            Expression::Call {
                callee, arguments, ..
            } => self.lower_call(expression, *callee, arguments, span),
            Expression::Lambda { .. } => self.lower_callable_literal(expression, span),
            Expression::Prefix {
                operator, operand, ..
            } => self.lower_prefix(*operator, *operand, expression, span),
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => self.lower_scalar_binary(*left, *operator, *right, expression, span),
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => self.lower_assignment(expression, *target, *operator, *value, span),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.lower_if(expression, *condition, *then_branch, *else_branch, span),
            Expression::When {
                subject, entries, ..
            } => self.lower_boolean_when(expression, *subject, entries, span, None),
            Expression::Return { value, .. } => self.lower_return(expression, *value, span),
            Expression::Break { .. } => self.lower_break(expression, span),
            Expression::Continue { .. } => self.lower_continue(expression, span),
            _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn lower_literal(
        &mut self,
        literal: LiteralKind,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if literal == LiteralKind::Null {
            let ty = self.expression_ssa_type(expression, span)?;
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::NullableNull { nullable: ty },
                    vec![EntityType::Value(ty)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            return Ok(LoweredValue::Value(require_value(results[0], span)?));
        }
        let constant = match literal {
            LiteralKind::Boolean(value) => ScalarConstant::Boolean(value),
            LiteralKind::Integer(kind) => {
                ScalarConstant::Integer(parse_integer_literal(self.sources, kind, span)?)
            }
            LiteralKind::Float(_) | LiteralKind::Char => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            LiteralKind::Null => unreachable!("handled above"),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Constant(constant),
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    fn lower_string_literal(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let source_text = self
            .sources
            .source_text(self.parsed.source_id())
            .map_err(|_| lowering_error(LoweringErrorKind::MismatchedSource, span))?;
        let bytes = string_literal::decode_plain(self.parsed, source_text, expression)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let string = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::StringLiteral { string, bytes },
                vec![EntityType::Value(string)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    fn lower_return(
        &mut self,
        expression: ExpressionId,
        value: Option<ExpressionId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        if self.function.carrier_return.is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some(value) = value {
            // A nullable-When extraction is atomic at the whole `return` boundary.
            self.guard_nullable_when_expression(
                UnitExpressionId::new(self.source_unit, value),
                span,
            )?;
        }
        let mut result = match value {
            Some(value) => self.lower(value)?,
            None => LoweredValue::Unit,
        };
        if result == LoweredValue::Diverged {
            return Ok(result);
        }
        if let (Some(value_expression), LoweredValue::Value(lowered)) = (value, result) {
            let (lowered, transferred) = self.adapt_owned_value_to_expected(
                value_expression,
                lowered,
                self.return_type,
                span,
            )?;
            if !transferred {
                self.transfer_owned_expression(value_expression, lowered, span)?;
            }
            result = LoweredValue::Value(lowered);
        }
        if self
            .owned
            .iteration_cleanup_at(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                self.source_unit,
                expression,
            )))
            .is_none()
        {
            self.end_pending_call_loans(0, span)?;
        }
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.retire_nullable_when_transfers(0, span)?;
        let values = self.return_values(result, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Return { values },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Diverged)
    }

    fn lower_optional_expression_value(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<ValueId>, LoweringError> {
        match self.lower(expression)? {
            LoweredValue::Value(value) => Ok(Some(value)),
            LoweredValue::Diverged => Ok(None),
            LoweredValue::Unit => {
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
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            }
        }
    }

    fn lower_statement(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
        super::statement_core::lower_statement(self, statement)
    }

    fn lower_statement_inner(
        &mut self,
        statement: StatementId,
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
        let span = node.span();
        match node.payload() {
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => {
                super::statement_core::lower_sequence(self, elements.iter().copied())
            }
            Statement::LocalVariable { declaration } => {
                self.lower_local_variable(*declaration, span)
            }
            Statement::Expression { expression } => self.lower(*expression),
            Statement::While {
                condition, body, ..
            } => self.lower_while(statement, *condition, *body, span),
            Statement::Loop { body, .. } => self.lower_loop(statement, *body, span),
            Statement::For { source, body, .. } => {
                if self
                    .typed
                    .map_iteration(UnitStatementId::new(self.source_unit, statement))
                    .is_some()
                {
                    Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
                } else {
                    self.lower_for(statement, *source, *body, span)
                }
            }
            Statement::LocalDestructuring { .. } | Statement::Error => {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            }
        }
    }

    fn lower_local_variable(
        &mut self,
        declaration: ItemId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (_, item, _) = unwrap_modified(self.parsed, declaration)?;
        let Item::Variable {
            name, initializer, ..
        } = item
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let closure = self.closure_origin(initializer)?;
        let name_span = present_name(name)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Some(fact) =
            self.owned.borrow_results().bindings().iter().find(|fact| {
                fact.initializer() == UnitExpressionId::new(self.source_unit, initializer)
            })
        {
            return self.lower_result_binding(fact.binding(), initializer, span);
        }
        let discards_binding = self.sources.slice(name_span).is_ok_and(|name| name == "_");
        if closure.is_some() && discards_binding {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let previous_context = self.closure_binding_context;
        self.closure_binding_context = closure.is_some();
        let lowered = self.lower(initializer);
        self.closure_binding_context = previous_context;
        let lowered = lowered?;
        if lowered == LoweredValue::Diverged {
            return Ok(lowered);
        }
        if discards_binding {
            if let LoweredValue::Value(value) = lowered {
                self.transfer_owned_expression(initializer, value, span)?;
            }
            return Ok(LoweredValue::Unit);
        }
        let symbol = self.declaration_symbol(name_span, SymbolKind::Variable)?;
        let declared = self
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let lowered = match lowered {
            LoweredValue::Value(value) => {
                let (value, transferred) =
                    self.adapt_owned_value_to_expected(initializer, value, declared, span)?;
                if !transferred {
                    self.transfer_owned_expression(initializer, value, span)?;
                }
                LoweredValue::Value(value)
            }
            LoweredValue::Unit => LoweredValue::Unit,
            LoweredValue::Diverged => unreachable!("divergence returned above"),
        };
        self.bindings.insert(symbol, lowered);
        Ok(LoweredValue::Unit)
    }

    fn declaration_symbol(
        &self,
        span: Span,
        kind: SymbolKind,
    ) -> Result<UnitSymbolId, LoweringError> {
        let local = self
            .names
            .names()
            .source_units()
            .get(self.source_unit.index())
            .and_then(|source| {
                source
                    .resolution()
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.span() == span && symbol.kind() == kind)
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.typed
            .body_symbol_types()
            .keys()
            .find(|symbol| {
                symbol.source_unit() == self.source_unit && symbol.symbol() == local.id()
            })
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    fn return_values(
        &self,
        result: LoweredValue,
        span: Span,
    ) -> Result<Vec<ValueId>, LoweringError> {
        match (builtin_type(self.typed, self.return_type), result) {
            (Some(BuiltinType::Unit), LoweredValue::Unit) => Ok(Vec::new()),
            (Some(BuiltinType::Unit), LoweredValue::Value(_))
            | (_, LoweredValue::Unit | LoweredValue::Diverged) => {
                Err(lowering_error(LoweringErrorKind::MissingFact, span))
            }
            (_, LoweredValue::Value(value)) => Ok(vec![value]),
        }
    }

    pub(in crate::ssa) fn expression_ssa_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let ty = self
            .typed
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
        if matches!(
            self.typed.types().get(ty),
            Some(UnitTypeKind::Function { .. })
        ) {
            if let Some(view) = self.view_parameter_type(expression) {
                return Ok(view);
            }
            return self.callable_abi.expression_type(
                self.source_plan,
                self.owned,
                self.source_token,
                UnitExpressionId::new(self.source_unit, expression),
                span,
            );
        }
        self.type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    /// ADR-0034: a Borrow function parameter's callable view target type.
    fn view_parameter_type(&self, expression: ExpressionId) -> Option<SsaTypeId> {
        let fact = self
            .owned
            .callable_origin(UnitExpressionId::new(self.source_unit, expression))?;
        let lang_frontend::ownership_checking::UnitCallableOrigin::Parameter(symbol) =
            fact.origin()
        else {
            return None;
        };
        let loan = self.borrow_bindings.get(&symbol)?;
        match self.function.entity(EntityId::Loan(*loan))?.ty {
            EntityType::Loan { target, .. } => Some(target),
            _ => None,
        }
    }

    /// 把 frontend 已批准的 `inner -> inner?` 隐式适配映射为消费式 nullable wrap。
    ///
    /// 返回值中的布尔量表示原表达式 owner 已被本方法消费；调用方仍负责 exact-type
    /// delivery 的 move/copy fact，避免适配 helper 重算 Phase 3 所有权语义。
    pub(super) fn adapt_owned_value_to_expected(
        &mut self,
        expression: ExpressionId,
        value: ValueId,
        expected: UnitTypeId,
        span: Span,
    ) -> Result<(ValueId, bool), LoweringError> {
        let actual = self
            .typed
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let actual = resolve_concrete_type(
            self.typed,
            actual,
            self.substitutions,
            self.static_self,
            span,
        )?;
        let expected = resolve_concrete_type(
            self.typed,
            expected,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if actual == expected {
            return Ok((value, false));
        }
        let Some(UnitTypeKind::Nullable(inner)) = self.typed.types().get(expected) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if *inner != actual {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let nullable = self
            .type_ids
            .get(&expected)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.function.entity(EntityId::Value(value)).map(|e| e.ty)
            == Some(EntityType::Value(nullable))
        {
            return Ok((value, false));
        }
        if self.map_results.contains_key(&nullable) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.transfer_owned_expression(expression, value, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::NullableWrap {
                    nullable,
                    owner: value,
                },
                vec![EntityType::Value(nullable)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok((require_value(results[0], span)?, true))
    }
}

fn symbol_references(
    names: &ValidatedCompilationUnitNames,
    source_unit: SourceUnitId,
    namespace: Namespace,
) -> BTreeMap<(usize, usize), UnitSymbolId> {
    names
        .names()
        .references()
        .iter()
        .filter_map(|reference| {
            if reference.source_unit() != source_unit || reference.namespace() != Some(namespace) {
                return None;
            }
            match reference.target() {
                UnitReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
                _ => None,
            }
        })
        .collect()
}

fn unwrap_modified(
    parsed: &lang_frontend::parser::ParsedFile,
    mut item: lang_frontend::ast::ItemId,
) -> Result<(ItemId, Item, Span), LoweringError> {
    loop {
        let node = parsed.ast().items().get(item).map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        match node.payload() {
            Item::Modified { declaration, .. } => item = *declaration,
            payload => return Ok((item, payload.clone(), node.span())),
        }
    }
}

fn builtin_type(typed: &CompilationUnitTypes, ty: UnitTypeId) -> Option<BuiltinType> {
    match typed.types().get(ty) {
        Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
        _ => None,
    }
}

fn instance_function_name(
    names: &ValidatedCompilationUnitNames,
    instance: &UnitPlannedInstance,
) -> Result<String, LoweringError> {
    let (declaration, callable_name, target_index) = match instance.key().target() {
        UnitCallableTarget::Declaration(declaration) => {
            let declaration = &names.names().index().declarations()[declaration.index()];
            (declaration, declaration.name(), declaration.id().index())
        }
        UnitCallableTarget::Symbol(symbol) => {
            let owner = instance
                .owner()
                .expect("planned member callable must retain its nominal owner");
            let declaration = &names.names().index().declarations()[owner.index()];
            let symbol_name = names.names().source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()[symbol.symbol().index()]
            .name();
            (declaration, symbol_name, symbol.symbol().index())
        }
    };
    let package = &names.names().index().packages()[declaration.package().index()];
    let package_name = package
        .name()
        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
    let mut name = String::from("koven");
    for segment in package_name.segments() {
        name.push('.');
        name.push_str(segment);
    }
    name.push('.');
    name.push_str(declaration.name());
    if instance.key().deinit_owner().is_some() {
        name.push_str(".deinit.d");
    } else if matches!(instance.key().target(), UnitCallableTarget::Symbol(_)) {
        name.push('.');
        name.push_str(callable_name);
        name.push_str(".s");
    } else {
        name.push_str(".d");
    }
    name.push_str(&target_index.to_string());
    for argument in instance.key().type_arguments() {
        name.push_str(".t");
        name.push_str(&argument.index().to_string());
    }
    if let Some(static_self) = instance.key().static_self() {
        name.push_str(".r");
        name.push_str(&static_self.index().to_string());
    }
    Ok(name)
}

fn parse_integer_literal(
    sources: &SourceMap,
    kind: IntegerLiteralKind,
    span: Span,
) -> Result<i128, LoweringError> {
    let text = sources
        .slice(span)
        .map_err(|_| lowering_error(LoweringErrorKind::MismatchedSource, span))?;
    parse_integer_literal_text(text, kind, span)
}

fn require_value(entity: EntityId, span: Span) -> Result<ValueId, LoweringError> {
    match entity {
        EntityId::Value(value) => Ok(value),
        EntityId::Place(_) | EntityId::Loan(_) => {
            Err(lowering_error(LoweringErrorKind::InvalidModel, span))
        }
    }
}

#[cfg(test)]
#[path = "unit_lower_deinit_tests.rs"]
mod deinit_tests;
