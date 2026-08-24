//! 已完成 frontend 产物到 typed SSA 的标量 lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::OwnershipCheckedFile,
    parser::{
        AssignmentOperator, BinaryOperator as AstBinaryOperator, Expression, FunctionBody,
        FunctionForm, IntegerLiteralKind, Item, LiteralKind, NameMarker, ParsedFile,
        PrefixOperator, Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableDescriptor, CallableTarget, ParameterMode, TypeId, TypeKind, TypedFile,
    },
};

use super::{
    model::{
        BlockId, CheckedArithmeticOperator, ComparisonOperator, Edge, EntityId, EntityType,
        Function, FunctionId, ModelError, Module, Operation, Origin, Program, ScalarConstant,
        SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    verify::verify_program,
};

const LOWERED_BUILTINS: [BuiltinType; 10] = [
    BuiltinType::Byte,
    BuiltinType::Short,
    BuiltinType::Int,
    BuiltinType::Long,
    BuiltinType::UByte,
    BuiltinType::UShort,
    BuiltinType::UInt,
    BuiltinType::ULong,
    BuiltinType::Boolean,
    BuiltinType::Unit,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoweringErrorKind {
    MismatchedSource,
    MismatchedAnalysis,
    FrontendDiagnostics,
    BlockingDeferred,
    UnsupportedNode,
    MissingFact,
    InvalidLiteral,
    InvalidModel,
    InvalidSsa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LoweringError {
    pub(super) kind: LoweringErrorKind,
    pub(super) span: Option<Span>,
}

struct FunctionPlan {
    id: FunctionId,
    body: FunctionPlanBody,
    parameter_symbols: Vec<SymbolId>,
    return_type: TypeId,
    span: Span,
}

#[derive(Clone, Copy)]
enum FunctionPlanBody {
    Expression(ExpressionId),
    Block(StatementId),
}

#[derive(Clone, Copy)]
enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
}

pub(super) fn lower_scalar_file(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Program, LoweringError> {
    validate_inputs(sources, parsed, names, typed, owned)?;
    let file_anchor = sources
        .span(parsed.source_id(), 0, 0)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MismatchedSource,
            span: None,
        })?;

    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must exist");
    let mut type_ids = BTreeMap::new();
    for builtin in LOWERED_BUILTINS {
        let ty = typed.types().builtin(builtin).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        intern_scalar_type(module, typed, &mut type_ids, ty, file_anchor)?;
    }
    let declarations = collect_functions(parsed, names, typed)?;
    let mut function_ids = BTreeMap::new();
    let mut plans = Vec::new();

    for (item, symbol, callable, span) in declarations {
        let Item::Function {
            type_parameters,
            form,
            ..
        } = item
        else {
            unreachable!("collector returns only functions");
        };
        if !type_parameters.is_empty() || !callable.type_parameters().is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
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
            } => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let parameter_symbols = callable
            .parameter_symbols()
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let parameter_types = callable
            .parameters()
            .iter()
            .map(|parameter| {
                if parameter.mode == ParameterMode::Inout {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let ty = intern_scalar_type(module, typed, &mut type_ids, parameter.ty, span)?;
                Ok(EntityType::Value(ty))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_types = match builtin_type(typed, callable.return_type()) {
            Some(BuiltinType::Unit) => Vec::new(),
            Some(_) => vec![intern_scalar_type(
                module,
                typed,
                &mut type_ids,
                callable.return_type(),
                span,
            )?],
            None => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let id = module
            .add_function(
                callable_symbol_name(names, symbol)?,
                return_types,
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        module
            .function_mut(id)
            .expect("new function must exist")
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        function_ids.insert(symbol, id);
        plans.push(FunctionPlan {
            id,
            body,
            parameter_symbols,
            return_type: callable.return_type(),
            span,
        });
    }

    let source_text = sources
        .source_text(parsed.source_id())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MismatchedSource,
            span: None,
        })?;
    let references = names
        .references()
        .iter()
        .filter_map(|reference| match reference.target() {
            ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

    for plan in plans {
        let function = module
            .function_mut(plan.id)
            .expect("planned function must exist");
        let entry = function
            .entry_block()
            .expect("signature created entry block");
        let parameters = function
            .block(entry)
            .expect("entry block must exist")
            .parameters
            .clone();
        let bindings = plan
            .parameter_symbols
            .into_iter()
            .zip(parameters)
            .map(|(symbol, entity)| {
                let EntityId::Value(value) = entity else {
                    unreachable!("scalar parameters are values");
                };
                (symbol, LoweredValue::Value(value))
            })
            .collect();
        let mut lowerer = ExpressionLowerer {
            parsed,
            names,
            typed,
            source_text,
            references: &references,
            function_ids: &function_ids,
            type_ids: &type_ids,
            function,
            block: entry,
            bindings,
            return_type: plan.return_type,
        };
        let result = match plan.body {
            FunctionPlanBody::Expression(expression) => lowerer.lower(expression)?,
            FunctionPlanBody::Block(block) => lowerer.lower_statement(block)?,
        };
        if !matches!(result, LoweredValue::Diverged) {
            let values = return_values(typed, plan.return_type, result, plan.span)?;
            lowerer
                .function
                .set_terminator(
                    lowerer.block,
                    TerminatorKind::Return { values },
                    Origin::Source(plan.span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, plan.span))?;
        }
    }

    verify_program(&program).map_err(|_| LoweringError {
        kind: LoweringErrorKind::InvalidSsa,
        span: None,
    })?;
    Ok(program)
}

fn validate_inputs(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), LoweringError> {
    let source = parsed.source_id();
    if names.source_id() != source || typed.source_id() != source || owned.source_id() != source {
        return Err(LoweringError {
            kind: LoweringErrorKind::MismatchedSource,
            span: None,
        });
    }
    sources.source_text(source).map_err(|_| LoweringError {
        kind: LoweringErrorKind::MismatchedSource,
        span: None,
    })?;
    if !typed.is_compatible_with_names(names) || !owned.is_compatible_with(names, typed) {
        return Err(LoweringError {
            kind: LoweringErrorKind::MismatchedAnalysis,
            span: None,
        });
    }
    if !parsed.diagnostics().is_empty()
        || !names.diagnostics().is_empty()
        || !typed.diagnostics().is_empty()
        || !owned.diagnostics().is_empty()
    {
        return Err(LoweringError {
            kind: LoweringErrorKind::FrontendDiagnostics,
            span: None,
        });
    }
    if let Some(deferred) = owned.deferred().first() {
        return Err(error(
            LoweringErrorKind::BlockingDeferred,
            parsed
                .ast()
                .expressions()
                .get(deferred.expression())
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?
                .span(),
        ));
    }
    Ok(())
}

fn collect_functions(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<Vec<(Item, SymbolId, CallableDescriptor, Span)>, LoweringError> {
    let mut functions = Vec::new();
    for root in parsed.roots() {
        let node = parsed.ast().items().get(*root).map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let (item, span) = unwrap_modified(parsed, *root)?;
        let Item::Function { name, .. } = &item else {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
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
        functions.push((item, symbol, callable, span));
    }
    Ok(functions)
}

fn unwrap_modified(parsed: &ParsedFile, mut item: ItemId) -> Result<(Item, Span), LoweringError> {
    loop {
        let node = parsed.ast().items().get(item).map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        match node.payload() {
            Item::Modified { declaration, .. } => item = *declaration,
            payload => return Ok((payload.clone(), node.span())),
        }
    }
}

fn callable_symbol_name(names: &NameResolution, symbol: SymbolId) -> Result<String, LoweringError> {
    names
        .symbols()
        .get(symbol.index())
        .map(|symbol| symbol.name().to_owned())
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })
}

fn intern_scalar_type(
    module: &mut Module,
    typed: &TypedFile,
    type_ids: &mut BTreeMap<TypeId, SsaTypeId>,
    ty: TypeId,
    span: Span,
) -> Result<SsaTypeId, LoweringError> {
    if let Some(mapped) = type_ids.get(&ty).copied() {
        return Ok(mapped);
    }
    let kind = match builtin_type(typed, ty) {
        Some(BuiltinType::Boolean) => SsaTypeKind::Boolean,
        Some(BuiltinType::Byte) => SsaTypeKind::Integer {
            bits: 8,
            signed: true,
        },
        Some(BuiltinType::UByte) => SsaTypeKind::Integer {
            bits: 8,
            signed: false,
        },
        Some(BuiltinType::Short) => SsaTypeKind::Integer {
            bits: 16,
            signed: true,
        },
        Some(BuiltinType::UShort) => SsaTypeKind::Integer {
            bits: 16,
            signed: false,
        },
        Some(BuiltinType::Int) => SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        Some(BuiltinType::UInt) => SsaTypeKind::Integer {
            bits: 32,
            signed: false,
        },
        Some(BuiltinType::Long) => SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        },
        Some(BuiltinType::ULong) => SsaTypeKind::Integer {
            bits: 64,
            signed: false,
        },
        Some(BuiltinType::Unit) => SsaTypeKind::Unit,
        Some(_) | None => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
    };
    let mapped = module.intern_type(kind);
    type_ids.insert(ty, mapped);
    Ok(mapped)
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
        | (None, _)
        | (_, LoweredValue::Diverged) => Err(error(LoweringErrorKind::MissingFact, span)),
        (Some(_), LoweredValue::Value(value)) => Ok(vec![value]),
    }
}

struct ExpressionLowerer<'a> {
    parsed: &'a ParsedFile,
    names: &'a NameResolution,
    typed: &'a TypedFile,
    source_text: &'a str,
    references: &'a BTreeMap<(usize, usize), SymbolId>,
    function_ids: &'a BTreeMap<SymbolId, FunctionId>,
    type_ids: &'a BTreeMap<TypeId, SsaTypeId>,
    function: &'a mut Function,
    block: BlockId,
    bindings: BTreeMap<SymbolId, LoweredValue>,
    return_type: TypeId,
}

impl ExpressionLowerer<'_> {
    fn lower(&mut self, expression: ExpressionId) -> Result<LoweredValue, LoweringError> {
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
            Expression::Name => self.lower_name(span),
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
            Expression::Call { arguments, .. } => self.lower_call(expression, &arguments, span),
            Expression::Return { value, .. } => self.lower_return(value, span),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn lower_statement(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
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
            Statement::Block { elements } => {
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
            Statement::Expression { expression } => self.lower(expression),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn lower_local_variable(
        &mut self,
        declaration: ItemId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (item, _) = unwrap_modified(self.parsed, declaration)?;
        let Item::Variable {
            name, initializer, ..
        } = item
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let value = self.lower(initializer)?;
        if matches!(value, LoweredValue::Diverged) {
            return Ok(value);
        }
        let name_span =
            present_name(name).ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.source_slice(name_span)? == "_" {
            return Ok(LoweredValue::Unit);
        }
        let symbol = self.declaration_symbol(name_span, SymbolKind::Variable)?;
        self.bindings.insert(symbol, value);
        Ok(LoweredValue::Unit)
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
                let text = self
                    .source_text
                    .get(span.start()..span.end())
                    .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?;
                let digits = match kind {
                    IntegerLiteralKind::Unsuffixed => text,
                    IntegerLiteralKind::Long | IntegerLiteralKind::Unsigned => {
                        &text[..text.len() - 1]
                    }
                    IntegerLiteralKind::UnsignedLong => &text[..text.len() - 2],
                };
                ScalarConstant::Integer(
                    digits
                        .parse()
                        .map_err(|_| error(LoweringErrorKind::InvalidLiteral, span))?,
                )
            }
            LiteralKind::Float(_) | LiteralKind::Char | LiteralKind::Null => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::Constant(constant),
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn lower_name(&self, span: Span) -> Result<LoweredValue, LoweringError> {
        let symbol = self
            .references
            .get(&span_key(span))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.bindings
            .get(symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
    }

    fn lower_prefix(
        &mut self,
        operator: PrefixOperator,
        operand: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
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
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .call(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let CallableTarget::Source(symbol) = descriptor.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !descriptor.instance().type_arguments().is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let callee = *self
            .function_ids
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let mut ordered = vec![None; descriptor.arguments().len()];
        for (argument_index, argument) in arguments.iter().enumerate() {
            let value = self.require_value(argument.value)?;
            let index = descriptor
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == argument_index)
                .map(|mapping| mapping.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
            if ordered.get(index).is_none_or(|slot| slot.is_some()) {
                return Err(error(LoweringErrorKind::MissingFact, argument.span));
            }
            ordered[index] = Some(value);
        }
        let arguments = ordered
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_types = match builtin_type(self.typed, descriptor.return_type()) {
            Some(BuiltinType::Unit) => Vec::new(),
            Some(_) => vec![EntityType::Value(
                self.expression_ssa_type(expression, span)?,
            )],
            None => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let (_, results) = self.append(
            Operation::DirectCall { callee, arguments },
            result_types,
            span,
        )?;
        match results.as_slice() {
            [] => Ok(LoweredValue::Unit),
            [entity] => Ok(LoweredValue::Value(value(*entity))),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
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

    fn expression_ssa_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
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

fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

fn error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}
