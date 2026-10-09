//! SPEC-0199 compilation-unit frontend 到单一 verified SSA module 的 lowering。
mod orchestrate;
use orchestrate::lower_unit_from_facts;

mod aggregate;
mod assignment;
mod borrow;
mod borrow_result;
mod call;
mod call_lifetimes;
mod callable_abi;
mod cfg;
mod closure;
pub(crate) mod constant;
mod construction;
mod container;
mod control;
mod deinit;
mod deinit_borrow;
mod enum_lower;
mod field_borrow;
mod field_replace;
mod handoff;
mod integer;
mod loop_control;
mod map;
mod map_require;
mod map_result;
mod map_with;
mod non_null_assertion;
mod nullable_comparison;
mod ownership;
mod ownership_primitive;
mod range;
mod rc;
mod receiver;
mod scalar;
mod string_clone;
mod type_lower;
mod type_plan;

pub(crate) use handoff::lower_owned_unit_with_entry;
#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "保留旧 crate-private lowering 入口路径供直接消费者使用"
    )
)]
pub(crate) use handoff::lower_scalar_unit_with_entry;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{CompilationUnitOwnership, ConstEnabledOwnedUnit, UnitDropPoint},
    parser::{
        Expression, FunctionBody, FunctionForm, IntegerLiteralKind, Item, LiteralKind, NameMarker,
        Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, Copyability, ExpressionCategory, ParameterMode,
        UnitCallableSignature, UnitCallableTarget, UnitExpressionId, UnitItemId, UnitStatementId,
        UnitTypeId, UnitTypeKind,
    },
};

use super::{
    LoweringError, LoweringErrorKind,
    lowering_support::{error as lowering_error, string_literal},
    model::{
        BlockId, EntityId, EntityType, Function, FunctionId, LoanId, LoanKind, Operation, Origin,
        Program, ScalarConstant, SsaTypeId, TerminatorKind, ValueId,
    },
    unit_plan::{
        MAX_UNIT_GENERIC_INSTANCES, UnitFunctionInstanceKey, UnitPlannedInstance,
        UnitRuntimeTypeDemand, plan_unit_instances_from_facts, resolve_concrete_type,
    },
    verify::verify_program,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
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

struct UnitExpressionLowerer<'a> {
    sources: &'a SourceMap,
    parsed: &'a lang_frontend::parser::ParsedFile,
    source_unit: SourceUnitId,
    names: &'a ValidatedCompilationUnitNames,
    typed: &'a CompilationUnitTypes,
    owned: &'a CompilationUnitOwnership,
    constant_owned: Option<&'a ConstEnabledOwnedUnit>,
    function_ids: &'a BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    source_plan: &'a super::unit_plan::UnitCallablePlan,
    callable_abi: &'a callable_abi::CallableAbi,
    source_token: super::lowering_support::callable_instances::SourceToken,
    type_ids: &'a BTreeMap<UnitTypeId, SsaTypeId>,
    heap_payloads: &'a BTreeMap<SsaTypeId, SsaTypeId>,
    map_results: &'a BTreeMap<SsaTypeId, SsaTypeId>,
    ssa_types: &'a [crate::ssa::model::SsaTypeKind],
    enum_payloads: &'a BTreeMap<(SsaTypeId, UnitSymbolId), (usize, SsaTypeId)>,
    field_indices: &'a BTreeMap<(UnitTypeId, UnitSymbolId), usize>,
    substitutions: &'a BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    references: &'a BTreeMap<(usize, usize), UnitSymbolId>,
    type_references: &'a BTreeMap<(usize, usize), UnitSymbolId>,
    function: &'a mut Function,
    block: BlockId,
    bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    borrow_bindings: BTreeMap<UnitSymbolId, LoanId>,
    result_source_loans: BTreeMap<UnitSymbolId, Vec<LoanId>>,
    temporary_range_slots: BTreeMap<UnitExpressionId, Vec<usize>>,
    short_range_ends: BTreeMap<LoanId, UnitExpressionId>,
    current_receiver: Option<ReceiverBinding>,
    consumed_receiver: Option<ConsumedReceiver>,
    closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
    closure_binding_context: bool,
    capture_loans: BTreeMap<(UnitExpressionId, usize), LoanId>,
    thunk_expression: Option<UnitExpressionId>,
    callable_plans: &'a BTreeMap<closure::CallablePlanKey, closure::CallablePlan>,
    closure_scope: FunctionId,
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
        if self.typed.range_size(unit_expression).is_some() {
            return self.lower_range_size(expression, span);
        }
        if let Some(value) = self.lower_constant(unit_expression, span)? {
            return Ok(value);
        }
        if let Some(construction) = self.typed.construction(unit_expression) {
            return match construction.target() {
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicRc => {
                    self.lower_rc_construction(expression, span)
                }
                lang_frontend::type_checking::UnitConstructionTarget::Nominal(_)
                | lang_frontend::type_checking::UnitConstructionTarget::IntrinsicBox => {
                    self.lower_aggregate_construction(expression, span)
                }
                lang_frontend::type_checking::UnitConstructionTarget::EnumCase(_) => {
                    self.lower_enum_construction(expression, span)
                }
            };
        }
        if self.typed.integer_operation(unit_expression).is_some() {
            return self.lower_integer_operation(expression, span);
        }
        if self.typed.string_operation(unit_expression).is_some() {
            return self.lower_string_clone(expression, span);
        }
        if self.typed.rc_operation(unit_expression).is_some() {
            return self.lower_rc_operation(expression, span);
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
            Expression::NonNullAssert { .. } => self.lower_non_null_assertion(expression, span),
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
            } => self.lower_boolean_when(expression, *subject, entries, span),
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
        if self.function.carrier_return.is_some() {
            return self.lower_range_return(
                value.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
                span,
            );
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
        } else {
            self.end_pending_abi_call_slots(0, span)?;
        }
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.end_thunk_capture_views(span)?;
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

    fn require_expression_value(
        &mut self,
        expression: ExpressionId,
    ) -> Result<ValueId, LoweringError> {
        match self.lower(expression)? {
            LoweredValue::Value(value) => Ok(value),
            LoweredValue::Unit | LoweredValue::Diverged => {
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
        let result = self.lower_statement_inner(statement)?;
        if result != LoweredValue::Diverged {
            self.emit_drops(UnitDropPoint::AfterStatement(UnitStatementId::new(
                self.source_unit,
                statement,
            )))?;
        }
        Ok(result)
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
                for element in elements {
                    if self.lower_statement(*element)? == LoweredValue::Diverged {
                        return Ok(LoweredValue::Diverged);
                    }
                }
                Ok(LoweredValue::Unit)
            }
            Statement::LocalVariable { declaration } => {
                self.lower_local_variable(*declaration, span)
            }
            Statement::Expression { expression } => self.lower(*expression),
            Statement::While {
                condition, body, ..
            } => self.lower_while(statement, *condition, *body, span),
            Statement::Loop { body, .. } => self.lower_loop(statement, *body, span),
            Statement::For { source, body, .. } => self.lower_for(statement, *source, *body, span),
            Statement::LocalDestructuring { initializer, .. } => {
                self.lower_destructuring(statement, *initializer, span)
            }
            Statement::Error => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
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
        if let Some(closure) = closure {
            self.closure_bindings.insert(symbol, closure);
        }
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

    fn expression_ssa_type(
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
            return self.callable_abi.expression_type(
                self.source_plan,
                self.typed,
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
        if self.map_results.contains_key(&nullable)
            && self.typed.copyability(actual) == Copyability::Copyable
        {
            return Ok((self.wrap_map_result(value, nullable, span)?, false));
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
) -> String {
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
    let mut name = String::from("koven");
    for segment in package.name().segments() {
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
    name
}

fn unit_callable_signature(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
) -> Option<&UnitCallableSignature> {
    match target {
        UnitCallableTarget::Declaration(declaration) => typed
            .signatures()
            .declaration(declaration)
            .and_then(|signature| signature.callable()),
        UnitCallableTarget::Symbol(symbol) => typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .flat_map(|nominal| nominal.members().iter().chain(nominal.companion_members()))
            .find(|callable| callable.target() == UnitCallableTarget::Symbol(symbol)),
    }
}

fn parse_integer_literal(
    sources: &SourceMap,
    kind: IntegerLiteralKind,
    span: Span,
) -> Result<i128, LoweringError> {
    let text = sources
        .slice(span)
        .map_err(|_| lowering_error(LoweringErrorKind::MismatchedSource, span))?;
    lang_frontend::type_checking::integer_literal_magnitude(text, kind)
        .and_then(|value| i128::try_from(value).ok())
        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidLiteral, span))
}

fn require_value(entity: EntityId, span: Span) -> Result<ValueId, LoweringError> {
    match entity {
        EntityId::Value(value) => Ok(value),
        EntityId::Place(_) | EntityId::Loan(_) => {
            Err(lowering_error(LoweringErrorKind::InvalidModel, span))
        }
    }
}

const fn present_name(marker: NameMarker) -> Option<Span> {
    match marker {
        NameMarker::Present(span) => Some(span),
        NameMarker::Missing(_) | NameMarker::Error(_) => None,
    }
}

const fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

#[cfg(test)]
#[path = "unit_lower_deinit_tests.rs"]
mod deinit_tests;
