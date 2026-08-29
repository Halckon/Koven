//! SPEC-0199 compilation-unit frontend 到单一 verified SSA module 的 lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::ValidatedCompilationUnitOwnership,
    parser::{Expression, FunctionBody, FunctionForm, IntegerLiteralKind, Item, LiteralKind},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, ParameterMode, TypeEnvironment, UnitCallTarget, UnitExpressionId, UnitTypeId,
        UnitTypeKind, ValidatedCompilationUnitTypes,
    },
};

use super::{
    LoweringError, LoweringErrorKind,
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
}

struct FunctionPlan {
    id: FunctionId,
    instance: UnitPlannedInstance,
    body: ExpressionId,
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
        let (item, _) = unwrap_modified(parsed, instance.item())?;
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
            } => expression,
            FunctionForm::ImplicitUnitBlock(_)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(_),
                ..
            }
            | FunctionForm::ImplicitUnitAbsent
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
                EntityId::Value(value) => Ok((symbol, value)),
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
            typed,
            function_ids: &function_ids,
            type_ids: &type_ids,
            substitutions: plan.instance.substitutions(),
            references: &references,
            function,
            block,
            bindings,
        };
        let result = lowerer.lower(plan.body)?;
        let values = match (builtin_type(typed, plan.return_type), result) {
            (Some(BuiltinType::Unit), LoweredValue::Unit) => Vec::new(),
            (Some(BuiltinType::Unit), LoweredValue::Value(_))
            | (Some(_), LoweredValue::Unit)
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
    typed: &'a ValidatedCompilationUnitTypes,
    function_ids: &'a BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    type_ids: &'a BTreeMap<UnitTypeId, SsaTypeId>,
    substitutions: &'a BTreeMap<UnitSymbolId, UnitTypeId>,
    references: &'a BTreeMap<(usize, usize), UnitSymbolId>,
    function: &'a mut Function,
    block: BlockId,
    bindings: BTreeMap<UnitSymbolId, ValueId>,
}

impl UnitExpressionLowerer<'_> {
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
        match node.payload() {
            Expression::Literal(literal) => self.lower_literal(*literal, expression, span),
            Expression::Name => self.lower_name(span),
            Expression::Group { expression } => self.lower(*expression),
            Expression::Call { arguments, .. } => self.lower_call(expression, arguments, span),
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

    fn lower_name(&self, span: Span) -> Result<LoweredValue, LoweringError> {
        let symbol = self
            .references
            .get(&span_key(span))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.bindings
            .get(symbol)
            .copied()
            .map(LoweredValue::Value)
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
                LoweredValue::Unit => {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        argument.span,
                    ));
                }
            };
            let slot = ordered
                .get_mut(mapping.parameter_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            if slot.replace(EntityId::Value(value)).is_some() {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
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
        match results.as_slice() {
            [] => Ok(LoweredValue::Unit),
            [result] => Ok(LoweredValue::Value(require_value(*result, span)?)),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
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
) -> Result<(Item, Span), LoweringError> {
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

const fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

const fn lowering_error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}
