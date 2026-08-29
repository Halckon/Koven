//! SPEC-0199 compilation-unit frontend 到单一 verified SSA module 的 lowering。

mod cfg;
mod control;
mod loop_control;
mod ownership;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{UnitDropPoint, UnitValueDeliveryKind, ValidatedCompilationUnitOwnership},
    parser::{
        Expression, FunctionBody, FunctionForm, IntegerLiteralKind, Item, LiteralKind, NameMarker,
        Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, ParameterMode, TypeEnvironment,
        UnitCallTarget, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeId, UnitTypeKind,
        ValidatedCompilationUnitTypes,
    },
};

use super::{
    LoweringError, LoweringErrorKind,
    lower_frontend::string_literal,
    model::{
        BlockId, EntityId, EntityType, Function, FunctionId, Operation, Origin, Program,
        ScalarConstant, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
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
    parameter_symbols: Vec<UnitSymbolId>,
    return_type: UnitTypeId,
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
    let mut type_ids = BTreeMap::new();
    let mut function_ids = BTreeMap::new();
    let mut plans = Vec::new();

    for instance in instances {
        let declaration = names
            .names()
            .index()
            .declarations()
            .get(instance.key().declaration().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let callable = typed
            .types()
            .signatures()
            .declaration(declaration.id())
            .and_then(|signature| signature.callable())
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
        let mut parameter_symbols = Vec::with_capacity(callable.parameters().len());
        let mut parameter_types = Vec::with_capacity(callable.parameters().len());
        for parameter in callable.parameters() {
            if parameter.mode() != ParameterMode::Value {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    parameter.span(),
                ));
            }
            let symbol = parameter
                .symbol()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, parameter.span()))?;
            let concrete = resolve_concrete_type(
                typed,
                parameter.ty(),
                instance.substitutions(),
                parameter.span(),
            )?;
            let ty = intern_scalar_type(module, typed, &mut type_ids, concrete, parameter.span())?;
            parameter_symbols.push(symbol);
            parameter_types.push(EntityType::Value(ty));
        }
        let return_type = resolve_concrete_type(
            typed,
            callable.return_type(),
            instance.substitutions(),
            instance.span(),
        )?;
        let return_types = if builtin_type(typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![intern_scalar_type(
                module,
                typed,
                &mut type_ids,
                return_type,
                instance.span(),
            )?]
        };
        let id = module
            .add_function(
                instance_function_name(names, &instance),
                return_types,
                Origin::Source(instance.span()),
            )
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
            parameter_symbols,
            return_type,
        });
    }

    let entry_id = function_ids
        .get(&UnitFunctionInstanceKey::for_entry(entry))
        .copied()
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;

    for plan in plans {
        let parsed = parsed_by_source[plan.instance.source_unit().index()];
        let references = value_references(names, plan.instance.source_unit());
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
        if parameters.len() != plan.parameter_symbols.len() {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                plan.instance.span(),
            ));
        }
        let bindings = plan
            .parameter_symbols
            .into_iter()
            .zip(parameters)
            .map(|(symbol, entity)| match entity {
                EntityId::Value(value) => Ok((symbol, LoweredValue::Value(value))),
                EntityId::Place(_) | EntityId::Loan(_) => Err(lowering_error(
                    LoweringErrorKind::InvalidModel,
                    plan.instance.span(),
                )),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut lowerer = UnitExpressionLowerer {
            sources,
            parsed,
            source_unit: plan.instance.source_unit(),
            names,
            typed,
            owned,
            function_ids: &function_ids,
            type_ids: &type_ids,
            substitutions: plan.instance.substitutions(),
            references: &references,
            function,
            block,
            bindings,
            temporaries: BTreeMap::new(),
            loops: Vec::new(),
            return_type: plan.return_type,
        };
        lowerer.emit_drops(UnitDropPoint::FunctionEntry(UnitItemId::new(
            plan.instance.source_unit(),
            plan.function_item,
        )))?;
        let result = match plan.body {
            FunctionPlanBody::Expression(expression) => lowerer.lower(expression)?,
            FunctionPlanBody::Block(statement) => lowerer.lower_statement(statement)?,
        };
        if result == LoweredValue::Diverged {
            continue;
        }
        let values = match (builtin_type(typed, plan.return_type), result) {
            (Some(BuiltinType::Unit), LoweredValue::Unit) => Vec::new(),
            (Some(BuiltinType::Unit), LoweredValue::Value(_))
            | (Some(_), LoweredValue::Unit)
            | (Some(_), LoweredValue::Diverged)
            | (None, _) => {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    plan.instance.span(),
                ));
            }
            (Some(_), LoweredValue::Value(value)) => vec![value],
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
    substitutions: &'a BTreeMap<UnitSymbolId, UnitTypeId>,
    references: &'a BTreeMap<(usize, usize), UnitSymbolId>,
    function: &'a mut Function,
    block: BlockId,
    bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    temporaries: BTreeMap<UnitExpressionId, ValueId>,
    loops: Vec<loop_control::LoopContext>,
    return_type: UnitTypeId,
}

impl UnitExpressionLowerer<'_> {
    fn lower(&mut self, expression: ExpressionId) -> Result<LoweredValue, LoweringError> {
        let result = self.lower_expression(expression)?;
        let unit_expression = UnitExpressionId::new(self.source_unit, expression);
        if let LoweredValue::Value(value) = result
            && self.typed.types().expression_category(unit_expression)
                == Some(ExpressionCategory::Temporary)
            && self
                .typed
                .types()
                .expression_type(unit_expression)
                .map(|ty| self.typed.types().copyability(ty))
                == Some(Copyability::MoveOnly)
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
        match node.payload() {
            Expression::Literal(literal) => self.lower_literal(*literal, expression, span),
            Expression::String { .. } => self.lower_string_literal(expression, span),
            Expression::Name => self.lower_name(span),
            Expression::Group { expression } => self.lower(*expression),
            Expression::Call { arguments, .. } => self.lower_call(expression, arguments, span),
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
        let constant = match literal {
            LiteralKind::Boolean(value) => ScalarConstant::Boolean(value),
            LiteralKind::Integer(kind) => {
                ScalarConstant::Integer(parse_integer_literal(self.sources, kind, span)?)
            }
            LiteralKind::Float(_) | LiteralKind::Char | LiteralKind::Null => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
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

    fn lower_name(&self, span: Span) -> Result<LoweredValue, LoweringError> {
        let symbol = self
            .references
            .get(&span_key(span))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.bindings
            .get(symbol)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))
    }

    fn lower_call(
        &mut self,
        expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let unit_expression = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .types()
            .call(unit_expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let type_arguments = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| resolve_concrete_type(self.typed, *ty, self.substitutions, span))
            .collect::<Result<Vec<_>, _>>()?;
        let callee = self
            .function_ids
            .get(&UnitFunctionInstanceKey::new(target, type_arguments))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut ordered = vec![None; descriptor.arguments().len()];
        for (argument_index, argument) in arguments.iter().enumerate() {
            let mapping = descriptor
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == argument_index)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            if mapping.mode() != ParameterMode::Value {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    argument.span,
                ));
            }
            let value = match self.lower(argument.value)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Unit | LoweredValue::Diverged => {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        argument.span,
                    ));
                }
            };
            let mut deliveries =
                self.owned
                    .ownership()
                    .value_deliveries()
                    .iter()
                    .filter(|delivery| {
                        delivery.call() == unit_expression
                            && delivery.argument()
                                == UnitExpressionId::new(self.source_unit, argument.value)
                    });
            let delivery = deliveries
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
            let argument_type = self
                .typed
                .types()
                .expression_type(argument_id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let expected_delivery = match (
                self.typed.types().expression_category(argument_id),
                self.typed.types().copyability(argument_type),
            ) {
                (Some(ExpressionCategory::Temporary), _) => UnitValueDeliveryKind::Temporary,
                (Some(ExpressionCategory::Place), Copyability::Copyable) => {
                    UnitValueDeliveryKind::Copy
                }
                (Some(ExpressionCategory::Place), Copyability::MoveOnly) => {
                    UnitValueDeliveryKind::Move
                }
                (None, _)
                | (Some(ExpressionCategory::Place), Copyability::Unknown | Copyability::Error) => {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        argument.span,
                    ));
                }
            };
            if deliveries.next().is_some() || delivery.kind() != expected_delivery {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            let slot = ordered
                .get_mut(mapping.parameter_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            if slot.replace(EntityId::Value(value)).is_some() {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            match expected_delivery {
                UnitValueDeliveryKind::Copy => {}
                UnitValueDeliveryKind::Move => {
                    let place = delivery
                        .place()
                        .filter(|place| place.is_root())
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, argument.span)
                        })?;
                    self.take_owned_binding(place.root(), value, argument.span)?;
                }
                UnitValueDeliveryKind::Temporary => {
                    if self.typed.types().copyability(argument_type) == Copyability::MoveOnly {
                        self.take_owned_temporary(value, argument.span)?;
                    }
                }
            }
        }
        let arguments = ordered
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let return_type = resolve_concrete_type(
            self.typed,
            descriptor.return_type(),
            self.substitutions,
            span,
        )?;
        let result_types = if builtin_type(self.typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![EntityType::Value(
                *self
                    .type_ids
                    .get(&return_type)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
            )]
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::DirectCall { callee, arguments },
                result_types,
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.emit_drops(UnitDropPoint::CallReturn(unit_expression))?;
        match results.as_slice() {
            [] => Ok(LoweredValue::Unit),
            [result] => Ok(LoweredValue::Value(require_value(*result, span)?)),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    fn lower_return(
        &mut self,
        expression: ExpressionId,
        value: Option<ExpressionId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let result = match value {
            Some(value) => self.lower(value)?,
            None => LoweredValue::Unit,
        };
        if result == LoweredValue::Diverged {
            return Ok(result);
        }
        if let (Some(value_expression), LoweredValue::Value(value)) = (value, result) {
            self.transfer_owned_expression(value_expression, value, span)?;
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
        let lowered = self.lower(initializer)?;
        if lowered == LoweredValue::Diverged {
            return Ok(lowered);
        }
        if let LoweredValue::Value(value) = lowered {
            self.transfer_owned_expression(initializer, value, span)?;
        }
        let name_span = present_name(name)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.sources.slice(name_span).is_ok_and(|name| name == "_") {
            return Ok(LoweredValue::Unit);
        }
        let symbol = self.declaration_symbol(name_span, SymbolKind::Variable)?;
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
            | (Some(_), LoweredValue::Unit | LoweredValue::Diverged)
            | (None, _) => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            (Some(_), LoweredValue::Value(value)) => Ok(vec![value]),
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
        let ty = resolve_concrete_type(self.typed, ty, self.substitutions, span)?;
        self.type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
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

fn value_references(
    names: &ValidatedCompilationUnitNames,
    source_unit: SourceUnitId,
) -> BTreeMap<(usize, usize), UnitSymbolId> {
    names
        .names()
        .references()
        .iter()
        .filter_map(|reference| {
            if reference.source_unit() != source_unit
                || reference.namespace() != Some(Namespace::Value)
            {
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

fn intern_scalar_type(
    module: &mut super::model::Module,
    typed: &ValidatedCompilationUnitTypes,
    type_ids: &mut BTreeMap<UnitTypeId, SsaTypeId>,
    ty: UnitTypeId,
    span: Span,
) -> Result<SsaTypeId, LoweringError> {
    if let Some(id) = type_ids.get(&ty).copied() {
        return Ok(id);
    }
    let kind = match typed.types().types().get(ty) {
        Some(UnitTypeKind::Builtin(BuiltinType::Boolean)) => SsaTypeKind::Boolean,
        Some(UnitTypeKind::Builtin(BuiltinType::Byte)) => integer_type(8, true),
        Some(UnitTypeKind::Builtin(BuiltinType::UByte)) => integer_type(8, false),
        Some(UnitTypeKind::Builtin(BuiltinType::Short)) => integer_type(16, true),
        Some(UnitTypeKind::Builtin(BuiltinType::UShort)) => integer_type(16, false),
        Some(UnitTypeKind::Builtin(BuiltinType::Int)) => integer_type(32, true),
        Some(UnitTypeKind::Builtin(BuiltinType::UInt)) => integer_type(32, false),
        Some(UnitTypeKind::Builtin(BuiltinType::Long)) => integer_type(64, true),
        Some(UnitTypeKind::Builtin(BuiltinType::ULong)) => integer_type(64, false),
        Some(UnitTypeKind::Builtin(BuiltinType::String)) => {
            let id = module.add_string_owner_type();
            type_ids.insert(ty, id);
            return Ok(id);
        }
        Some(UnitTypeKind::Builtin(BuiltinType::Unit)) => SsaTypeKind::Unit,
        Some(_) => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        None => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    };
    let id = module.intern_type(kind);
    type_ids.insert(ty, id);
    Ok(id)
}

const fn integer_type(bits: u16, signed: bool) -> SsaTypeKind {
    SsaTypeKind::Integer { bits, signed }
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
    let declaration = &names.names().index().declarations()[instance.key().declaration().index()];
    let package = &names.names().index().packages()[declaration.package().index()];
    let mut name = String::from("koven");
    for segment in package.name().segments() {
        name.push('.');
        name.push_str(segment);
    }
    name.push('.');
    name.push_str(declaration.name());
    name.push_str(".d");
    name.push_str(&declaration.id().index().to_string());
    for argument in instance.key().type_arguments() {
        name.push_str(".t");
        name.push_str(&argument.index().to_string());
    }
    name
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
