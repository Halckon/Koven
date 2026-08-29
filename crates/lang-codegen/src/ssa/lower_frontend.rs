//! 已完成 frontend 产物到 typed SSA 的标量 lowering。

mod aggregate;
mod container;
mod control;
mod drops;
mod instances;
mod loop_control;
mod nominal;
pub(super) mod orchestrate;
mod source_closure;
pub(in crate::ssa) mod string_literal;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{EnumCaseId, NameResolution, SymbolId, SymbolKind},
    ownership_checking::{
        ConstructionDeliveryKind, ConstructionRootKind, DropPoint, LoanTarget,
        OwnershipCheckedFile, RcOwnershipEffectKind,
    },
    parser::{
        AssignmentOperator, BinaryOperator as AstBinaryOperator, Expression, IntegerLiteralKind,
        Item, LiteralKind, NameMarker, ParsedFile, PrefixOperator, Statement,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableTarget, ConstructionTarget, Copyability, NominalKind, ParameterMode,
        RcOperationKind, TypeId, TypeKind, TypedFile,
    },
};

use super::model::{
    BlockId, CheckedArithmeticOperator, ComparisonOperator, Edge, EntityId, EntityType, Function,
    FunctionId, LoanId, LoanKind, ModelError, Operation, Origin, PlaceId, ScalarConstant,
    SsaTypeId, TerminatorKind, ValueId,
};
use instances::FunctionInstanceKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoweringErrorKind {
    MismatchedSource,
    MismatchedAnalysis,
    FrontendDiagnostics,
    BlockingDeferred,
    InstanceLimitExceeded,
    UnsupportedNode,
    MissingFact,
    InvalidLiteral,
    InvalidModel,
    InvalidSsa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoweringError {
    pub(crate) kind: LoweringErrorKind,
    pub(crate) span: Option<Span>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
}

fn builtin_type(typed: &TypedFile, ty: TypeId) -> Option<BuiltinType> {
    match typed.types().get(ty) {
        Some(TypeKind::Builtin(builtin)) => Some(*builtin),
        _ => None,
    }
}

fn return_values(
    typed: &TypedFile,
    return_type: TypeId,
    result: LoweredValue,
    span: Span,
) -> Result<Vec<ValueId>, LoweringError> {
    match (builtin_type(typed, return_type), result) {
        (Some(BuiltinType::Unit), LoweredValue::Unit) => Ok(Vec::new()),
        (Some(BuiltinType::Unit), LoweredValue::Value(_))
        | (Some(_), LoweredValue::Unit)
        | (None, LoweredValue::Unit)
        | (_, LoweredValue::Diverged) => Err(error(LoweringErrorKind::MissingFact, span)),
        (Some(_), LoweredValue::Value(value)) | (None, LoweredValue::Value(value)) => {
            Ok(vec![value])
        }
    }
}

struct ExpressionLowerer<'a> {
    parsed: &'a ParsedFile,
    names: &'a NameResolution,
    typed: &'a TypedFile,
    owned: &'a OwnershipCheckedFile,
    source_text: &'a str,
    references: &'a BTreeMap<(usize, usize), SymbolId>,
    function_ids: &'a BTreeMap<FunctionInstanceKey, FunctionId>,
    source_closures: &'a BTreeMap<usize, source_closure::ClosurePlan>,
    type_ids: &'a BTreeMap<TypeId, SsaTypeId>,
    heap_payloads: &'a BTreeMap<SsaTypeId, SsaTypeId>,
    enum_payloads: &'a BTreeMap<(SsaTypeId, EnumCaseId), (usize, SsaTypeId)>,
    substitutions: &'a BTreeMap<SymbolId, TypeId>,
    function: &'a mut Function,
    block: BlockId,
    bindings: BTreeMap<SymbolId, LoweredValue>,
    borrow_bindings: BTreeMap<SymbolId, LoanId>,
    non_null_bindings: BTreeMap<SymbolId, LoanId>,
    temporaries: BTreeMap<usize, ValueId>,
    return_type: TypeId,
    loops: Vec<loop_control::LoopContext>,
}

impl ExpressionLowerer<'_> {
    fn lower(&mut self, expression: ExpressionId) -> Result<LoweredValue, LoweringError> {
        let result = self.lower_expression(expression)?;
        if let LoweredValue::Value(value) = result
            && self.typed.expression_category(expression)
                == Some(lang_frontend::type_checking::ExpressionCategory::Temporary)
            && self
                .typed
                .expression_type(expression)
                .and_then(|ty| self.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
        {
            self.temporaries.insert(expression.index(), value);
        }
        if !matches!(result, LoweredValue::Diverged) {
            self.emit_drops(DropPoint::AfterExpression(expression))?;
        }
        Ok(result)
    }

    fn lower_expression(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        if self.typed.container_construction(expression).is_some() {
            return self.lower_container_construction(expression);
        }
        if self.typed.construction(expression).is_some() {
            return self.lower_construction(expression);
        }
        if self.typed.rc_operation(expression).is_some() {
            return self.lower_rc_operation(expression);
        }
        if self.typed.aggregate_projection(expression).is_some() {
            return self.lower_aggregate_projection(expression);
        }
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
        match node.payload().clone() {
            Expression::Literal(literal) => self.lower_literal(literal, expression, span),
            Expression::String { .. } => self.lower_string_literal(expression, span),
            Expression::Lambda { .. } => self.lower_source_closure(expression, span),
            Expression::Name => self.lower_name(expression, span),
            Expression::Group { expression } => self.lower(expression),
            Expression::Prefix {
                operator, operand, ..
            } => self.lower_prefix(operator, operand, expression, span),
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => self.lower_binary(left, operator, right, expression, span),
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => self.lower_assignment(target, operator, value, span),
            Expression::Call {
                callee, arguments, ..
            } => self.lower_call(expression, callee, &arguments, span),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.lower_if(expression, condition, then_branch, else_branch, span),
            Expression::When {
                subject, entries, ..
            } => self.lower_when(expression, subject, &entries, span),
            Expression::Return { value, .. } => self.lower_return(expression, value, span),
            Expression::Break { .. } => self.lower_break(span),
            Expression::Continue { .. } => self.lower_continue(span),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn lower_construction(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.construction(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
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
            .construction_plan(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.target() != descriptor.target()
            || (plan.terminating_operand().is_none()
                && plan.deliveries().len() != descriptor.arguments().len())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_construction_root(descriptor.result_type(), descriptor.target(), plan, span)?;

        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut fields = vec![None; arguments.len()];
        for (evaluation_index, argument) in arguments.into_iter().enumerate() {
            if plan.terminating_operand() == Some(argument.argument()) {
                return match self.lower(argument.argument())? {
                    LoweredValue::Diverged => Ok(LoweredValue::Diverged),
                    LoweredValue::Unit | LoweredValue::Value(_) => {
                        Err(error(LoweringErrorKind::MissingFact, span))
                    }
                };
            }
            let effect = plan
                .deliveries()
                .get(evaluation_index)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if argument.evaluation_index() != evaluation_index
                || effect.argument() != argument.argument()
                || effect.parameter_index() != argument.parameter_index()
                || effect.parameter_symbol() != argument.parameter_symbol()
                || effect.evaluation_index() != evaluation_index
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let LoweredValue::Value(mut field) = self.lower(argument.argument())? else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if effect.kind() == ConstructionDeliveryKind::Copy {
                let ty = self.expression_ssa_type(argument.argument(), span)?;
                let (_, results) = self.append(
                    Operation::Copy { source: field },
                    vec![EntityType::Value(ty)],
                    span,
                )?;
                field = value(results[0]);
            }
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(field).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        if plan.terminating_operand().is_some() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let fields = fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let result = match descriptor.target() {
            ConstructionTarget::Nominal(target) => {
                let nominal = self
                    .typed
                    .nominals()
                    .iter()
                    .find(|nominal| nominal.id() == target)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let aggregate = match nominal.kind() {
                    NominalKind::ValueClass => result_type,
                    NominalKind::Class => self
                        .heap_payloads
                        .get(&result_type)
                        .copied()
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?,
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                };
                let (_, aggregate_results) = self.append(
                    Operation::AggregateConstruct { aggregate, fields },
                    vec![EntityType::Value(aggregate)],
                    span,
                )?;
                let payload = value(aggregate_results[0]);
                if nominal.kind() == NominalKind::Class {
                    let (_, owner_results) = self.append(
                        Operation::HeapAllocate {
                            owner: result_type,
                            payload,
                        },
                        vec![EntityType::Value(result_type)],
                        span,
                    )?;
                    value(owner_results[0])
                } else {
                    payload
                }
            }
            ConstructionTarget::IntrinsicBox => {
                let [payload] = fields.as_slice() else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let (_, results) = self.append(
                    Operation::HeapAllocate {
                        owner: result_type,
                        payload: *payload,
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                value(results[0])
            }
            ConstructionTarget::IntrinsicRc => {
                let [payload] = fields.as_slice() else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let (_, results) = self.append(
                    Operation::SharedAllocate {
                        owner: result_type,
                        payload: *payload,
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                value(results[0])
            }
            ConstructionTarget::EnumCase(_) => {
                let ConstructionTarget::EnumCase(case) = descriptor.target() else {
                    unreachable!();
                };
                let (variant, payload_type) = self
                    .enum_payloads
                    .get(&(result_type, case))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let (_, payload_results) = self.append(
                    Operation::AggregateConstruct {
                        aggregate: payload_type,
                        fields,
                    },
                    vec![EntityType::Value(payload_type)],
                    span,
                )?;
                let (_, results) = self.append(
                    Operation::TaggedConstruct {
                        tagged: result_type,
                        variant,
                        payload: value(payload_results[0]),
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                value(results[0])
            }
        };
        Ok(LoweredValue::Value(result))
    }

    fn lower_rc_operation(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.rc_operation(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let effect = self.owned.rc_effect(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
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
        let expected_effect = match descriptor.kind() {
            RcOperationKind::Share => RcOwnershipEffectKind::Retain,
            RcOperationKind::Value => RcOwnershipEffectKind::BorrowPayload,
        };
        if effect.receiver() != descriptor.receiver()
            || effect.payload_type() != descriptor.payload_type()
            || effect.kind() != expected_effect
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let owner = self.shared_owner_operand(descriptor.receiver(), span)?;
        match descriptor.kind() {
            RcOperationKind::Share => {
                let result_type = self.expression_ssa_type(expression, span)?;
                let (_, results) = self.append(
                    Operation::SharedRetain { owner },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                Ok(LoweredValue::Value(value(results[0])))
            }
            RcOperationKind::Value => {
                if self.typed.copyability(descriptor.payload_type()) != Some(Copyability::Copyable)
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let result_type = self.expression_ssa_type(expression, span)?;
                let (_, places) = self.append(
                    Operation::SharedPayloadPlace { owner },
                    vec![EntityType::Place(result_type)],
                    span,
                )?;
                let (_, results) = self.append(
                    Operation::Read {
                        source: super::model::PlaceAccess::Place(place(places[0])),
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                Ok(LoweredValue::Value(value(results[0])))
            }
        }
    }

    fn shared_owner_operand(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        if let Some(non_null) = self.typed.non_null_use(receiver)
            && let Some(view) = self.non_null_bindings.get(&non_null.symbol()).copied()
        {
            return Ok(EntityId::Loan(view));
        }
        self.require_value(receiver)
            .map(EntityId::Value)
            .map_err(|mut error| {
                error.span.get_or_insert(span);
                error
            })
    }

    fn validate_construction_root(
        &self,
        result_type: TypeId,
        target: ConstructionTarget,
        plan: &lang_frontend::ownership_checking::ConstructionOwnershipPlan,
        span: Span,
    ) -> Result<(), LoweringError> {
        if plan.terminating_operand().is_some() {
            return if plan.root_obligation().is_none() {
                Ok(())
            } else {
                Err(error(LoweringErrorKind::MissingFact, span))
            };
        }
        let expected_kind = match target {
            ConstructionTarget::IntrinsicBox => Some(ConstructionRootKind::HeapOwner),
            ConstructionTarget::IntrinsicRc => Some(ConstructionRootKind::SharedOwner),
            ConstructionTarget::EnumCase(_) => Some(ConstructionRootKind::Inline),
            ConstructionTarget::Nominal(target) => self
                .typed
                .nominals()
                .iter()
                .find(|nominal| nominal.id() == target)
                .and_then(|nominal| match nominal.kind() {
                    NominalKind::ValueClass => Some(ConstructionRootKind::Inline),
                    NominalKind::Class => Some(ConstructionRootKind::HeapOwner),
                    NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => None,
                }),
        };
        match (self.typed.copyability(result_type), plan.root_obligation()) {
            (Some(Copyability::Copyable), None) => Ok(()),
            (Some(Copyability::MoveOnly), Some(obligation))
                if obligation.construction() == plan.construction()
                    && obligation.result_type() == result_type
                    && Some(obligation.kind()) == expected_kind =>
            {
                Ok(())
            }
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }

    fn lower_statement(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
        let result = self.lower_statement_inner(statement)?;
        if !matches!(result, LoweredValue::Diverged) {
            self.emit_drops(DropPoint::AfterStatement(statement))?;
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
        match node.payload().clone() {
            Statement::Block { elements } | Statement::LambdaBody { elements } => {
                for element in elements {
                    if matches!(self.lower_statement(element)?, LoweredValue::Diverged) {
                        return Ok(LoweredValue::Diverged);
                    }
                }
                Ok(LoweredValue::Unit)
            }
            Statement::LocalVariable { declaration } => {
                self.lower_local_variable(declaration, span)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.lower_destructuring(statement, initializer, span)
            }
            Statement::Expression { expression } => self.lower(expression),
            Statement::While {
                condition, body, ..
            } => self.lower_while(condition, body, span),
            Statement::Loop { body, .. } => self.lower_loop(body, span),
            Statement::For { .. } => Err(error(LoweringErrorKind::UnsupportedNode, span)),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn lower_local_variable(
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
        if let Some(TypeKind::Nullable(inner)) = self.typed.types().get(declared)
            && self.typed.expression_type(initializer) == Some(*inner)
        {
            let nullable = self
                .type_ids
                .get(&self.resolve_type(declared, span)?)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let LoweredValue::Value(owner) = lowered else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let (_, results) = self.append(
                Operation::NullableWrap { nullable, owner },
                vec![EntityType::Value(nullable)],
                span,
            )?;
            lowered = LoweredValue::Value(value(results[0]));
        }
        self.bindings.insert(symbol, lowered);
        Ok(LoweredValue::Unit)
    }

    fn lower_literal(
        &mut self,
        literal: LiteralKind,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if literal == LiteralKind::Null {
            let ty = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(
                Operation::NullableNull { nullable: ty },
                vec![EntityType::Value(ty)],
                span,
            )?;
            return Ok(LoweredValue::Value(value(results[0])));
        }
        let constant = match literal {
            LiteralKind::Boolean(value) => ScalarConstant::Boolean(value),
            LiteralKind::Integer(kind) => {
                ScalarConstant::Integer(self.parse_integer_literal(kind, span)?)
            }
            LiteralKind::Float(_) | LiteralKind::Char => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            LiteralKind::Null => unreachable!("handled above"),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::Constant(constant),
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_string_literal(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let bytes = string_literal::decode_plain(self.parsed, self.source_text, expression)?
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let string = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::StringLiteral { string, bytes },
            vec![EntityType::Value(string)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_name(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let symbol = self
            .references
            .get(&span_key(span))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.non_null_use(expression).is_some()
            && self.non_null_bindings.contains_key(symbol)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some(value) = self.bindings.get(symbol).copied() {
            return Ok(value);
        }
        let loan = self
            .borrow_bindings
            .get(symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let ty = self
            .typed
            .symbol_type(*symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(ty) != Some(Copyability::Copyable) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let ty = self.resolve_type(ty, span)?;
        let ssa_type = self
            .type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, results) = self.append(
            Operation::Read {
                source: super::model::PlaceAccess::Loan(loan),
            },
            vec![EntityType::Value(ssa_type)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_prefix(
        &mut self,
        operator: PrefixOperator,
        operand: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if operator == PrefixOperator::Minus {
            let operand_node =
                self.parsed
                    .ast()
                    .expressions()
                    .get(operand)
                    .map_err(|_| LoweringError {
                        kind: LoweringErrorKind::MissingFact,
                        span: None,
                    })?;
            if let Expression::Literal(LiteralKind::Integer(kind)) = operand_node.payload() {
                let constant = self
                    .parse_integer_literal(*kind, operand_node.span())?
                    .checked_neg()
                    .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?;
                let ty = self.expression_ssa_type(expression, span)?;
                let (_, results) = self.append(
                    Operation::Constant(ScalarConstant::Integer(constant)),
                    vec![EntityType::Value(ty)],
                    span,
                )?;
                return Ok(LoweredValue::Value(value(results[0])));
            }
        }
        let operand = self.require_value(operand)?;
        match operator {
            PrefixOperator::Plus => Ok(LoweredValue::Value(operand)),
            PrefixOperator::Not => {
                let ty = self.expression_ssa_type(expression, span)?;
                let (_, results) = self.append(
                    Operation::BooleanNot { operand },
                    vec![EntityType::Value(ty)],
                    span,
                )?;
                Ok(LoweredValue::Value(value(results[0])))
            }
            PrefixOperator::Minus => {
                let ty = self.expression_ssa_type(expression, span)?;
                let (_, zero) = self.append(
                    Operation::Constant(ScalarConstant::Integer(0)),
                    vec![EntityType::Value(ty)],
                    span,
                )?;
                self.checked(
                    CheckedArithmeticOperator::Subtract,
                    value(zero[0]),
                    operand,
                    ty,
                    span,
                )
            }
        }
    }

    fn lower_binary(
        &mut self,
        left: ExpressionId,
        operator: AstBinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if matches!(
            operator,
            AstBinaryOperator::LogicalAnd | AstBinaryOperator::LogicalOr
        ) {
            return self.lower_short_circuit(left, operator, right, expression, span);
        }
        if self.is_string_expression(left) && self.is_string_expression(right) {
            return self.lower_string_binary(left, operator, right, expression, span);
        }
        let left = self.require_value(left)?;
        let right = self.require_value(right)?;
        if let Some(operator) = checked_operator(operator) {
            let ty = self.expression_ssa_type(expression, span)?;
            return self.checked(operator, left, right, ty, span);
        }
        let operator = comparison_operator(operator)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::Compare {
                operator,
                left,
                right,
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_string_binary(
        &mut self,
        left: ExpressionId,
        operator: AstBinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let left = self.lower_string_view(left)?;
        let right = self.lower_string_view(right)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let operation = match operator {
            AstBinaryOperator::Add => Operation::StringConcat { left, right },
            AstBinaryOperator::Equal | AstBinaryOperator::NotEqual => {
                Operation::StringEqual { left, right }
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        self.emit_drops(DropPoint::AfterBinaryOperands(expression))?;
        let result = value(results[0]);
        if operator != AstBinaryOperator::NotEqual {
            return Ok(LoweredValue::Value(result));
        }
        let (_, results) = self.append(
            Operation::BooleanNot { operand: result },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_string_view(&mut self, expression: ExpressionId) -> Result<EntityId, LoweringError> {
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
        match node.payload().clone() {
            Expression::Group { expression } => self.lower_string_view(expression),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(span))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if let Some(LoweredValue::Value(value)) = self.bindings.get(symbol).copied() {
                    return Ok(EntityId::Value(value));
                }
                self.borrow_bindings
                    .get(symbol)
                    .copied()
                    .map(EntityId::Loan)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
            }
            _ => self.require_value(expression).map(EntityId::Value),
        }
    }

    fn is_string_expression(&self, expression: ExpressionId) -> bool {
        self.typed
            .expression_type(expression)
            .and_then(|ty| self.typed.types().get(ty))
            == Some(&TypeKind::Builtin(BuiltinType::String))
    }

    fn lower_assignment(
        &mut self,
        target: ExpressionId,
        operator: AssignmentOperator,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let target_node =
            self.parsed
                .ast()
                .expressions()
                .get(target)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
        if !matches!(target_node.payload(), Expression::Name) {
            return Err(error(
                LoweringErrorKind::UnsupportedNode,
                target_node.span(),
            ));
        }
        let symbol = *self
            .references
            .get(&span_key(target_node.span()))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, target_node.span()))?;
        let assigned = match operator {
            AssignmentOperator::Assign => self.lower(expression)?,
            AssignmentOperator::AddAssign
            | AssignmentOperator::SubtractAssign
            | AssignmentOperator::MultiplyAssign
            | AssignmentOperator::DivideAssign
            | AssignmentOperator::RemainderAssign => {
                let left = match self.bindings.get(&symbol).copied() {
                    Some(LoweredValue::Value(value)) => value,
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                        return Err(error(LoweringErrorKind::MissingFact, target_node.span()));
                    }
                };
                let right = self.require_value(expression)?;
                let ty = self.expression_ssa_type(target, target_node.span())?;
                self.checked(assignment_operator(operator), left, right, ty, span)?
            }
        };
        if matches!(assigned, LoweredValue::Diverged) {
            return Ok(assigned);
        }
        self.bindings.insert(symbol, assigned);
        Ok(LoweredValue::Unit)
    }

    fn lower_return(
        &mut self,
        return_expression: ExpressionId,
        expression: Option<ExpressionId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let result = match expression {
            Some(expression) => self.lower(expression)?,
            None => LoweredValue::Unit,
        };
        if matches!(result, LoweredValue::Diverged) {
            return Ok(result);
        }
        self.emit_drops(DropPoint::ControlTransfer(return_expression))?;
        let values = return_values(self.typed, self.return_type, result, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Return { values },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Diverged)
    }

    fn lower_call(
        &mut self,
        expression: ExpressionId,
        callee_expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = match self.typed.call(expression) {
            Some(descriptor) => descriptor,
            // 尚未封闭的 constructor 等 call family 使用 Error/Deferred 类型占位，但不会伪造
            // CallDescriptor；这属于可预期的 source 边界，而非 typed 产物缺失内部事实。
            None if self
                .typed
                .expression_type(expression)
                .and_then(|ty| self.typed.types().get(ty))
                .is_some_and(|kind| matches!(kind, TypeKind::Deferred(_) | TypeKind::Error)) =>
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            None => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if descriptor.aborts() {
            let [argument] = arguments else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            self.lower_borrow_argument(expression, argument.value, argument.span)?;
            self.function
                .set_terminator(self.block, TerminatorKind::Abort, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            return Ok(LoweredValue::Diverged);
        }
        if descriptor.prints_line() {
            let [argument] = arguments else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let (loan, ends_after_call) =
                self.lower_borrow_argument(expression, argument.value, argument.span)?;
            self.append(Operation::PrintString { value: loan }, Vec::new(), span)?;
            if ends_after_call {
                self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            }
            self.emit_drops(DropPoint::CallReturn(expression))?;
            return Ok(LoweredValue::Unit);
        }
        if descriptor.target() == CallableTarget::FunctionValue {
            if !arguments.is_empty()
                || !descriptor.arguments().is_empty()
                || builtin_type(
                    self.typed,
                    self.resolve_type(descriptor.return_type(), span)?,
                ) != Some(BuiltinType::Unit)
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let callable = match self.lower_expression(callee_expression)? {
                LoweredValue::Value(value) => value,
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            self.append(
                Operation::CallableInvoke {
                    callable,
                    arguments: Vec::new(),
                },
                Vec::new(),
                span,
            )?;
            self.emit_drops(DropPoint::AfterExpression(callee_expression))?;
            self.emit_drops(DropPoint::CallReturn(expression))?;
            return Ok(LoweredValue::Unit);
        }
        let CallableTarget::Source(symbol) = descriptor.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let type_arguments = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| self.resolve_type(*ty, span))
            .collect::<Result<Vec<_>, _>>()?;
        let instance = FunctionInstanceKey::new(symbol, type_arguments);
        let callee = *self
            .function_ids
            .get(&instance)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let mut ordered = vec![None; descriptor.arguments().len()];
        let mut call_loans = BTreeMap::new();
        for (argument_index, argument) in arguments.iter().enumerate() {
            let mapping = descriptor
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == argument_index)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
            let index = mapping.parameter_index();
            if ordered.get(index).is_none_or(|slot| slot.is_some()) {
                return Err(error(LoweringErrorKind::MissingFact, argument.span));
            }
            let operand = match mapping.mode() {
                ParameterMode::Value => EntityId::Value(self.require_value(argument.value)?),
                ParameterMode::Borrow => {
                    let (loan, ends_after_call) =
                        self.lower_borrow_argument(expression, argument.value, argument.span)?;
                    call_loans.insert(argument.value.index(), ends_after_call.then_some(loan));
                    EntityId::Loan(loan)
                }
                ParameterMode::Inout => {
                    return Err(error(LoweringErrorKind::UnsupportedNode, argument.span));
                }
            };
            ordered[index] = Some(operand);
        }
        let arguments = ordered
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let return_type = self.resolve_type(descriptor.return_type(), span)?;
        let result_types = if builtin_type(self.typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![EntityType::Value(
                self.expression_ssa_type(expression, span)?,
            )]
        };
        let (_, results) = self.append(
            Operation::DirectCall { callee, arguments },
            result_types,
            span,
        )?;
        let ending_loans = self.owned.loans_ending_at(expression).collect::<Vec<_>>();
        for fact in &ending_loans {
            let loan = call_loans
                .remove(&fact.argument().index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.end_span()))?;
            if let Some(loan) = loan {
                self.append(Operation::BorrowEnd { loan }, Vec::new(), fact.end_span())?;
            }
        }
        if !call_loans.is_empty() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Borrow argument lowering may form a place directly (for example `Rc.value`) without
        // recursively lowering the argument expression. Emit its expression-local ASAP drops
        // only after every call loan has ended, so two arguments may safely view the same owner.
        for fact in ending_loans {
            self.emit_drops(DropPoint::AfterExpression(fact.argument()))?;
        }
        self.emit_drops(DropPoint::CallReturn(expression))?;
        match results.as_slice() {
            [] => Ok(LoweredValue::Unit),
            [entity] => Ok(LoweredValue::Value(value(*entity))),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    fn lower_borrow_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        span: Span,
    ) -> Result<(LoanId, bool), LoweringError> {
        if let Some(non_null) = self.typed.non_null_use(argument)
            && let Some(view) = self.non_null_bindings.get(&non_null.symbol()).copied()
        {
            return Ok((view, false));
        }
        let fact = self
            .owned
            .loan_begin(argument)
            .filter(|fact| fact.call() == call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.kind() != lang_frontend::ownership_checking::LoanKind::Shared {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if let LoanTarget::Place(target) = fact.target()
            && target.is_root()
            && let Some(loan) = self.borrow_bindings.get(&target.root()).copied()
        {
            return Ok((loan, false));
        }
        let place = self.lower_borrow_place(argument, fact.target(), span)?;
        let target = self.expression_ssa_type(argument, span)?;
        let (_, results) = self.append(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            fact.begin_span(),
        )?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, fact.begin_span()));
        };
        Ok((loan, true))
    }

    fn lower_borrow_place(
        &mut self,
        argument: ExpressionId,
        target: &LoanTarget,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        if let LoanTarget::Place(target) = target
            && !target.is_root()
            && let Some(place) = self.lower_borrowed_container_element(argument, target, span)?
        {
            return Ok(place);
        }
        if let Some(operation) = self.typed.rc_operation(argument)
            && operation.kind() == RcOperationKind::Value
        {
            let owner = self.shared_owner_operand(operation.receiver(), span)?;
            let payload = self.expression_ssa_type(argument, span)?;
            let (_, results) = self.append(
                Operation::SharedPayloadPlace { owner },
                vec![EntityType::Place(payload)],
                span,
            )?;
            return Ok(place(results[0]));
        }
        let owner = match target {
            LoanTarget::Place(place) if place.is_root() => self
                .bindings
                .get(&place.root())
                .and_then(|value| match value {
                    LoweredValue::Value(value) => Some(*value),
                    LoweredValue::Unit | LoweredValue::Diverged => None,
                })
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?,
            LoanTarget::Temporary(temporary) => self.require_value(*temporary)?,
            LoanTarget::Place(_) => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let target = self.expression_ssa_type(argument, span)?;
        let (_, results) = self.append(
            Operation::RootPlace { owner },
            vec![EntityType::Place(target)],
            span,
        )?;
        Ok(place(results[0]))
    }

    fn checked(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let boolean = self
            .type_ids
            .iter()
            .find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, results) = self.append(
            Operation::CheckedArithmetic {
                operator,
                left,
                right,
            },
            vec![EntityType::Value(ty), EntityType::Value(boolean)],
            span,
        )?;
        let failure = self
            .function
            .add_block(Vec::new(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let success = self
            .function
            .add_block(Vec::new(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: value(results[1]),
                    when_true: Edge {
                        target: failure,
                        arguments: Vec::new(),
                    },
                    when_false: Edge {
                        target: success,
                        arguments: Vec::new(),
                    },
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(failure, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = success;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn require_value(&mut self, expression: ExpressionId) -> Result<ValueId, LoweringError> {
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
                Err(error(LoweringErrorKind::UnsupportedNode, span))
            }
        }
    }

    fn declaration_symbol(&self, span: Span, kind: SymbolKind) -> Result<SymbolId, LoweringError> {
        self.names
            .symbols()
            .iter()
            .find(|symbol| symbol.span() == span && symbol.kind() == kind)
            .map(|symbol| symbol.id())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn source_slice(&self, span: Span) -> Result<&str, LoweringError> {
        self.source_text
            .get(span.start()..span.end())
            .ok_or_else(|| error(LoweringErrorKind::MismatchedSource, span))
    }

    fn parse_integer_literal(
        &self,
        kind: IntegerLiteralKind,
        span: Span,
    ) -> Result<i128, LoweringError> {
        let text = self.source_slice(span)?;
        let digits = match kind {
            IntegerLiteralKind::Unsuffixed => text,
            IntegerLiteralKind::Long | IntegerLiteralKind::Unsigned => &text[..text.len() - 1],
            IntegerLiteralKind::UnsignedLong => &text[..text.len() - 2],
        };
        digits
            .parse()
            .map_err(|_| error(LoweringErrorKind::InvalidLiteral, span))
    }

    fn expression_ssa_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ty = self.resolve_type(ty, span)?;
        self.type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
    }

    fn resolve_type(&self, ty: TypeId, span: Span) -> Result<TypeId, LoweringError> {
        instances::resolve_concrete_type(self.typed, ty, self.substitutions, span)
    }

    fn append(
        &mut self,
        operation: Operation,
        results: Vec<EntityType>,
        span: Span,
    ) -> Result<(super::model::InstructionId, Vec<EntityId>), LoweringError> {
        self.function
            .append_instruction(self.block, operation, results, Origin::Source(span))
            .map_err(|_: ModelError| error(LoweringErrorKind::InvalidModel, span))
    }
}

fn checked_operator(operator: AstBinaryOperator) -> Option<CheckedArithmeticOperator> {
    match operator {
        AstBinaryOperator::Add => Some(CheckedArithmeticOperator::Add),
        AstBinaryOperator::Subtract => Some(CheckedArithmeticOperator::Subtract),
        AstBinaryOperator::Multiply => Some(CheckedArithmeticOperator::Multiply),
        AstBinaryOperator::Divide => Some(CheckedArithmeticOperator::Divide),
        AstBinaryOperator::Remainder => Some(CheckedArithmeticOperator::Remainder),
        _ => None,
    }
}

fn assignment_operator(operator: AssignmentOperator) -> CheckedArithmeticOperator {
    match operator {
        AssignmentOperator::AddAssign => CheckedArithmeticOperator::Add,
        AssignmentOperator::SubtractAssign => CheckedArithmeticOperator::Subtract,
        AssignmentOperator::MultiplyAssign => CheckedArithmeticOperator::Multiply,
        AssignmentOperator::DivideAssign => CheckedArithmeticOperator::Divide,
        AssignmentOperator::RemainderAssign => CheckedArithmeticOperator::Remainder,
        AssignmentOperator::Assign => unreachable!("plain assignment has no arithmetic operator"),
    }
}

fn comparison_operator(operator: AstBinaryOperator) -> Option<ComparisonOperator> {
    match operator {
        AstBinaryOperator::Equal => Some(ComparisonOperator::Equal),
        AstBinaryOperator::NotEqual => Some(ComparisonOperator::NotEqual),
        AstBinaryOperator::Less => Some(ComparisonOperator::LessThan),
        AstBinaryOperator::LessEqual => Some(ComparisonOperator::LessThanOrEqual),
        AstBinaryOperator::Greater => Some(ComparisonOperator::GreaterThan),
        AstBinaryOperator::GreaterEqual => Some(ComparisonOperator::GreaterThanOrEqual),
        _ => None,
    }
}

fn present_name(marker: NameMarker) -> Option<Span> {
    match marker {
        NameMarker::Present(span) => Some(span),
        NameMarker::Missing(_) | NameMarker::Error(_) => None,
    }
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        unreachable!("scalar lowering only requests value results");
    };
    value
}

fn place(entity: EntityId) -> super::model::PlaceId {
    let EntityId::Place(place) = entity else {
        unreachable!("frontend lowering requested a place result");
    };
    place
}

fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

fn error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}
