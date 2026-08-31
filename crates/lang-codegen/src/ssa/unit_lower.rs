//! SPEC-0199 compilation-unit frontend 到单一 verified SSA module 的 lowering。

mod aggregate;
mod assignment;
mod call;
mod cfg;
mod closure;
mod construction;
mod container;
mod control;
mod enum_lower;
mod loop_control;
mod ownership;
mod rc;
mod receiver;
mod scalar;
mod type_lower;
mod type_plan;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{UnitDropPoint, ValidatedCompilationUnitOwnership},
    parser::{
        Expression, FunctionBody, FunctionForm, IntegerLiteralKind, Item, LiteralKind, NameMarker,
        Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, ParameterMode, TypeEnvironment,
        UnitCallableSignature, UnitCallableTarget, UnitExpressionId, UnitItemId, UnitStatementId,
        UnitTypeId, UnitTypeKind, ValidatedCompilationUnitTypes,
    },
};

use super::{
    LoweringError, LoweringErrorKind,
    lower_frontend::string_literal,
    model::{
        BlockId, EntityId, EntityType, Function, FunctionId, LoanId, LoanKind, Operation, Origin,
        Program, ScalarConstant, SsaTypeId, TerminatorKind, ValueId,
    },
    unit_plan::{
        UnitFunctionInstanceKey, UnitPlannedInstance, plan_unit_instances, resolve_concrete_type,
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
}

#[derive(Clone, Copy)]
struct ReceiverBinding {
    owner: DeclarationId,
    mode: ParameterMode,
    template_ty: UnitTypeId,
    ty: UnitTypeId,
    entity: EntityId,
}

/// 把 compilation unit 当前封闭的 scalar expression-body 子集 lower 为 verified SSA。
pub(crate) fn lower_scalar_unit_with_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    let instances = plan_unit_instances(sources, inputs, names, environment, typed, owned, entry)?;
    let parsed_by_source = parsed_by_source_unit(inputs, names)?;
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new unit module must exist");
    let mut types = type_lower::UnitTypeLowering::new();
    let mut function_ids = BTreeMap::new();
    let mut plans = Vec::new();

    for instance in instances {
        let callable = unit_callable_signature(typed, instance.key().target())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
        let parsed = parsed_by_source
            .get(instance.source_unit().index())
            .copied()
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let (function_item, item, _) = unwrap_modified(parsed, instance.item())?;
        let Item::Function { form, .. } = item else {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                instance.span(),
            ));
        };
        let body = match form {
            FunctionForm::Explicit {
                body: FunctionBody::Expression { expression, .. },
                ..
            } => FunctionPlanBody::Expression(expression),
            FunctionForm::ImplicitUnitBlock(block)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            } => FunctionPlanBody::Block(block),
            FunctionForm::ImplicitUnitAbsent
            | FunctionForm::Explicit {
                body: FunctionBody::Absent,
                ..
            } => {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    instance.span(),
                ));
            }
        };
        let receiver = match (callable.receiver(), instance.owner()) {
            (Some(receiver), Some(owner)) => {
                let concrete = resolve_concrete_type(
                    typed,
                    receiver.ty(),
                    instance.substitutions(),
                    instance.key().static_self(),
                    receiver.declaration_span(),
                )?;
                let ty = types.intern(module, typed, concrete, receiver.declaration_span())?;
                let entity_type = match receiver.mode() {
                    ParameterMode::Value => EntityType::Value(ty),
                    ParameterMode::Borrow => EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    },
                    ParameterMode::Inout => EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: ty,
                    },
                };
                Some(ReceiverPlan {
                    owner,
                    mode: receiver.mode(),
                    template_ty: receiver.ty(),
                    ty: concrete,
                    entity_type,
                })
            }
            (None, None) => None,
            (Some(receiver), None) => {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    receiver.declaration_span(),
                ));
            }
            (None, Some(_)) => {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    instance.span(),
                ));
            }
        };
        let mut parameter_symbols = Vec::with_capacity(callable.parameters().len());
        let mut parameter_types =
            Vec::with_capacity(callable.parameters().len() + usize::from(receiver.is_some()));
        if let Some(receiver) = receiver {
            parameter_types.push(receiver.entity_type);
        }
        for parameter in callable.parameters() {
            let symbol = parameter
                .symbol()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, parameter.span()))?;
            let concrete = resolve_concrete_type(
                typed,
                parameter.ty(),
                instance.substitutions(),
                instance.key().static_self(),
                parameter.span(),
            )?;
            if parameter.mode() == ParameterMode::Borrow
                && builtin_type(typed, concrete) == Some(BuiltinType::Unit)
            {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    parameter.span(),
                ));
            }
            let ty = types.intern(module, typed, concrete, parameter.span())?;
            parameter_symbols.push(symbol);
            parameter_types.push(match parameter.mode() {
                ParameterMode::Value => EntityType::Value(ty),
                ParameterMode::Borrow => EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                ParameterMode::Inout => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        parameter.span(),
                    ));
                }
            });
        }
        let return_type = resolve_concrete_type(
            typed,
            callable.return_type(),
            instance.substitutions(),
            instance.key().static_self(),
            instance.span(),
        )?;
        let return_types = if builtin_type(typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![types.intern(module, typed, return_type, instance.span())?]
        };
        let name = instance_function_name(names, &instance);
        let origin = Origin::Source(instance.span());
        let id = match receiver {
            Some(receiver) => {
                module.add_instance_function(name, receiver.entity_type, return_types, origin)
            }
            None => module.add_function(name, return_types, origin),
        }
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
        module
            .function_mut(id)
            .expect("new unit function must exist")
            .add_block(parameter_types, Origin::Source(instance.span()))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
        function_ids.insert(instance.key().clone(), id);
        plans.push(FunctionPlan {
            id,
            instance,
            function_item,
            body,
            receiver,
            parameter_symbols,
            return_type,
        });
    }

    // 保持既有 signature-first 类型编号；body-only type 只在全部函数签名建立后追加。
    for plan in &plans {
        type_plan::intern_body_scalar_types(
            module,
            parsed_by_source[plan.instance.source_unit().index()],
            &plan.instance,
            typed,
            &mut types,
        )?;
    }
    let callable_plans = closure::declare(
        module,
        &parsed_by_source,
        &plans,
        names,
        typed,
        owned,
        &mut types,
    )?;

    let entry_id = function_ids
        .get(&UnitFunctionInstanceKey::for_entry(entry))
        .copied()
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;

    for plan in plans {
        let parsed = parsed_by_source[plan.instance.source_unit().index()];
        let references = symbol_references(names, plan.instance.source_unit(), Namespace::Value);
        let type_references =
            symbol_references(names, plan.instance.source_unit(), Namespace::Type);
        let function = module
            .function_mut(plan.id)
            .expect("planned unit function must exist");
        let block = function
            .entry_block()
            .expect("planned unit function has an entry block");
        let parameters = function
            .block(block)
            .expect("entry block exists")
            .parameters
            .clone();
        if parameters.len() != plan.parameter_symbols.len() + usize::from(plan.receiver.is_some()) {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                plan.instance.span(),
            ));
        }
        let (current_receiver, parameters) = match (plan.receiver, parameters.split_first()) {
            (Some(receiver), Some((entity, parameters)))
                if receiver.entity_type
                    == function
                        .entity(*entity)
                        .map(|data| data.ty)
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::InvalidModel, plan.instance.span())
                        })? =>
            {
                (
                    Some(ReceiverBinding {
                        owner: receiver.owner,
                        mode: receiver.mode,
                        template_ty: receiver.template_ty,
                        ty: receiver.ty,
                        entity: *entity,
                    }),
                    parameters,
                )
            }
            (None, _) => (None, parameters.as_slice()),
            _ => {
                return Err(lowering_error(
                    LoweringErrorKind::InvalidModel,
                    plan.instance.span(),
                ));
            }
        };
        let mut bindings = BTreeMap::new();
        let mut borrow_bindings = BTreeMap::new();
        for (symbol, entity) in plan
            .parameter_symbols
            .into_iter()
            .zip(parameters.iter().copied())
        {
            match entity {
                EntityId::Value(value) => {
                    bindings.insert(symbol, LoweredValue::Value(value));
                }
                EntityId::Loan(loan) => {
                    borrow_bindings.insert(symbol, loan);
                }
                EntityId::Place(_) => {
                    return Err(lowering_error(
                        LoweringErrorKind::InvalidModel,
                        plan.instance.span(),
                    ));
                }
            }
        }
        let mut lowerer = UnitExpressionLowerer {
            sources,
            parsed,
            source_unit: plan.instance.source_unit(),
            names,
            typed,
            owned,
            function_ids: &function_ids,
            type_ids: types.type_ids(),
            heap_payloads: types.heap_payloads(),
            enum_payloads: types.enum_payloads(),
            field_indices: types.field_indices(),
            substitutions: plan.instance.substitutions(),
            static_self: plan.instance.key().static_self(),
            references: &references,
            type_references: &type_references,
            function,
            block,
            bindings,
            borrow_bindings,
            current_receiver,
            closure_bindings: BTreeMap::new(),
            closure_binding_context: false,
            callable_plans: &callable_plans,
            closure_scope: plan.id,
            temporaries: BTreeMap::new(),
            loops: Vec::new(),
            return_type: plan.return_type,
        };
        lowerer.emit_drops(UnitDropPoint::FunctionEntry(UnitItemId::new(
            plan.instance.source_unit(),
            plan.function_item,
        )))?;
        let (mut result, result_expression) = match plan.body {
            FunctionPlanBody::Expression(expression) => {
                (lowerer.lower(expression)?, Some(expression))
            }
            FunctionPlanBody::Block(statement) => (lowerer.lower_statement(statement)?, None),
        };
        if result == LoweredValue::Diverged {
            continue;
        }
        if let (Some(expression), LoweredValue::Value(value)) = (result_expression, result) {
            let (value, transferred) = lowerer.adapt_owned_value_to_expected(
                expression,
                value,
                plan.return_type,
                plan.instance.span(),
            )?;
            if !transferred {
                lowerer.transfer_owned_expression(expression, value, plan.instance.span())?;
            }
            result = LoweredValue::Value(value);
        }
        if let Some(expression) = result_expression {
            lowerer.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                plan.instance.source_unit(),
                expression,
            )))?;
        }
        let values = match (builtin_type(typed, plan.return_type), result) {
            (Some(BuiltinType::Unit), LoweredValue::Unit) => Vec::new(),
            (Some(BuiltinType::Unit), LoweredValue::Value(_))
            | (_, LoweredValue::Unit | LoweredValue::Diverged) => {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    plan.instance.span(),
                ));
            }
            (_, LoweredValue::Value(value)) => vec![value],
        };
        lowerer
            .function
            .set_terminator(
                lowerer.block,
                TerminatorKind::Return { values },
                Origin::Source(plan.instance.span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, plan.instance.span()))?;
    }

    for plan in callable_plans.values() {
        let parsed = parsed_by_source[plan.source_unit.index()];
        let references = symbol_references(names, plan.source_unit, Namespace::Value);
        let type_references = symbol_references(names, plan.source_unit, Namespace::Type);
        let function = module
            .function_mut(plan.thunk)
            .expect("planned unit closure thunk exists");
        let block = function
            .entry_block()
            .expect("planned unit closure thunk has entry");
        let mut lowerer = UnitExpressionLowerer {
            sources,
            parsed,
            source_unit: plan.source_unit,
            names,
            typed,
            owned,
            function_ids: &function_ids,
            type_ids: types.type_ids(),
            heap_payloads: types.heap_payloads(),
            enum_payloads: types.enum_payloads(),
            field_indices: types.field_indices(),
            substitutions: &plan.substitutions,
            static_self: plan.static_self,
            references: &references,
            type_references: &type_references,
            function,
            block,
            bindings: BTreeMap::new(),
            borrow_bindings: BTreeMap::new(),
            current_receiver: None,
            closure_bindings: BTreeMap::new(),
            closure_binding_context: false,
            callable_plans: &callable_plans,
            closure_scope: plan.scope,
            temporaries: BTreeMap::new(),
            loops: Vec::new(),
            return_type: plan.return_type,
        };
        closure::finish_thunk(&mut lowerer, plan)?;
    }

    verify_program(&program).map_err(|_| LoweringError {
        kind: LoweringErrorKind::InvalidSsa,
        span: None,
    })?;
    Ok((program, entry_id))
}

struct UnitExpressionLowerer<'a> {
    sources: &'a SourceMap,
    parsed: &'a lang_frontend::parser::ParsedFile,
    source_unit: SourceUnitId,
    names: &'a ValidatedCompilationUnitNames,
    typed: &'a ValidatedCompilationUnitTypes,
    owned: &'a ValidatedCompilationUnitOwnership,
    function_ids: &'a BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    type_ids: &'a BTreeMap<UnitTypeId, SsaTypeId>,
    heap_payloads: &'a BTreeMap<SsaTypeId, SsaTypeId>,
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
    current_receiver: Option<ReceiverBinding>,
    closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
    closure_binding_context: bool,
    callable_plans: &'a BTreeMap<closure::CallablePlanKey, closure::CallablePlan>,
    closure_scope: FunctionId,
    temporaries: BTreeMap<UnitExpressionId, ValueId>,
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
                .types()
                .expression_type(unit_expression)
                .map(|ty| self.typed.types().copyability(ty))
                == Some(Copyability::MoveOnly)
            && (self.typed.types().expression_category(unit_expression)
                == Some(ExpressionCategory::Temporary)
                || self.typed.types().construction(unit_expression).is_some())
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
        if let Some(construction) = self.typed.types().construction(unit_expression) {
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
        if self.typed.types().rc_operation(unit_expression).is_some() {
            return self.lower_rc_operation(expression, span);
        }
        if self
            .typed
            .types()
            .container_construction(unit_expression)
            .is_some()
        {
            return self.lower_container_construction(expression, span);
        }
        if self.typed.types().element_place(unit_expression).is_some() {
            return self.lower_container_index(expression, span);
        }
        if self
            .typed
            .types()
            .aggregate_projection(unit_expression)
            .is_some()
        {
            return self.lower_aggregate_projection(expression, span);
        }
        match node.payload() {
            Expression::Literal(literal) => self.lower_literal(*literal, expression, span),
            Expression::String { .. } => self.lower_string_literal(expression, span),
            Expression::Name => self.lower_name(expression, span),
            Expression::This => self.lower_this(expression, span),
            Expression::Group { expression } => self.lower(*expression),
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
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
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
            Statement::Error | Statement::LocalDestructuring { .. } | Statement::For { .. } => {
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
            .types()
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
            .types()
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
            .types()
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
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
            .types()
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
        let Some(UnitTypeKind::Nullable(inner)) = self.typed.types().types().get(expected) else {
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

fn parsed_by_source_unit<'a>(
    inputs: &'a [SourceUnitInput<'a>],
    names: &ValidatedCompilationUnitNames,
) -> Result<Vec<&'a lang_frontend::parser::ParsedFile>, LoweringError> {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .map(|source_unit| {
            inputs
                .iter()
                .copied()
                .find(|input| input.source_id() == source_unit.source_id())
                .map(SourceUnitInput::parsed)
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MismatchedSource,
                    span: None,
                })
        })
        .collect()
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

fn builtin_type(typed: &ValidatedCompilationUnitTypes, ty: UnitTypeId) -> Option<BuiltinType> {
    match typed.types().types().get(ty) {
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
    if matches!(instance.key().target(), UnitCallableTarget::Symbol(_)) {
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
    typed: &ValidatedCompilationUnitTypes,
    target: UnitCallableTarget,
) -> Option<&UnitCallableSignature> {
    match target {
        UnitCallableTarget::Declaration(declaration) => typed
            .types()
            .signatures()
            .declaration(declaration)
            .and_then(|signature| signature.callable()),
        UnitCallableTarget::Symbol(symbol) => typed
            .types()
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
    let digits = match kind {
        IntegerLiteralKind::Unsuffixed => text,
        IntegerLiteralKind::Long | IntegerLiteralKind::Unsigned => &text[..text.len() - 1],
        IntegerLiteralKind::UnsignedLong => &text[..text.len() - 2],
    };
    digits
        .parse()
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidLiteral, span))
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

const fn lowering_error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}
