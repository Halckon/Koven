//! frontend 产物门禁、函数预声明与文件级 lowering 编排。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::OwnershipCheckedFile,
    parser::{FunctionBody, FunctionForm, Item, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{BuiltinType, CallableDescriptor, ParameterMode, TypeId, TypedFile},
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
    instances::{FunctionInstanceKey, FunctionTemplate, plan_instances, resolve_concrete_type},
    present_name, return_values, span_key,
};
use crate::ssa::{
    model::{
        EntityId, EntityType, FunctionId, Module, Origin, Program, SsaTypeId, SsaTypeKind,
        TerminatorKind,
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

struct FunctionPlan {
    id: FunctionId,
    body: FunctionPlanBody,
    parameter_symbols: Vec<SymbolId>,
    return_type: TypeId,
    substitutions: BTreeMap<SymbolId, TypeId>,
    span: Span,
}

struct FunctionDeclaration {
    item: Item,
    symbol: SymbolId,
    callable: CallableDescriptor,
    span: Span,
}

#[derive(Clone, Copy)]
enum FunctionPlanBody {
    Expression(ExpressionId),
    Block(StatementId),
}

pub(in crate::ssa) fn lower_scalar_file(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Program, LoweringError> {
    Ok(lower_scalar_file_product(sources, parsed, names, typed, owned)?.program)
}

pub(crate) fn lower_scalar_file_with_entry(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    entry: SymbolId,
) -> Result<(Program, FunctionId), LoweringError> {
    let lowered = lower_scalar_file_product(sources, parsed, names, typed, owned)?;
    let entry = lowered
        .function_ids
        .get(&FunctionInstanceKey::new(entry, Vec::new()))
        .copied()
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    Ok((lowered.program, entry))
}

struct LoweredFile {
    program: Program,
    function_ids: BTreeMap<FunctionInstanceKey, FunctionId>,
}

fn lower_scalar_file_product(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<LoweredFile, LoweringError> {
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
    let templates = declarations
        .iter()
        .map(|declaration| FunctionTemplate {
            symbol: declaration.symbol,
            type_parameters: declaration.callable.type_parameters().to_vec(),
            span: declaration.span,
        })
        .collect::<Vec<_>>();
    let instances = plan_instances(parsed, typed, &templates)?;
    let mut function_ids = BTreeMap::new();
    let mut plans = Vec::new();

    for instance in instances {
        let declaration = declarations
            .get(instance.template_index)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let item = &declaration.item;
        let callable = &declaration.callable;
        let span = declaration.span;
        let Item::Function { form, .. } = item else {
            unreachable!("collector returns only functions");
        };
        let body = match form {
            FunctionForm::Explicit {
                body: FunctionBody::Expression { expression, .. },
                ..
            } => FunctionPlanBody::Expression(*expression),
            FunctionForm::ImplicitUnitBlock(block)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            } => FunctionPlanBody::Block(*block),
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
                let concrete =
                    resolve_concrete_type(typed, parameter.ty, &instance.substitutions, span)?;
                let ty = intern_scalar_type(module, typed, &mut type_ids, concrete, span)?;
                Ok(EntityType::Value(ty))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_type =
            resolve_concrete_type(typed, callable.return_type(), &instance.substitutions, span)?;
        let return_types = match builtin_type(typed, return_type) {
            Some(BuiltinType::Unit) => Vec::new(),
            Some(_) => vec![intern_scalar_type(
                module,
                typed,
                &mut type_ids,
                return_type,
                span,
            )?],
            None => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let id = module
            .add_function(
                instance_function_name(names, typed, &instance.key, span)?,
                return_types,
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        module
            .function_mut(id)
            .expect("new function must exist")
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        function_ids.insert(instance.key.clone(), id);
        plans.push(FunctionPlan {
            id,
            body,
            parameter_symbols,
            return_type,
            substitutions: instance.substitutions,
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
            substitutions: &plan.substitutions,
            function,
            block: entry,
            bindings,
            return_type: plan.return_type,
            loops: Vec::new(),
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
    Ok(LoweredFile {
        program,
        function_ids,
    })
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
) -> Result<Vec<FunctionDeclaration>, LoweringError> {
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
        functions.push(FunctionDeclaration {
            item,
            symbol,
            callable,
            span,
        });
    }
    Ok(functions)
}

pub(super) fn unwrap_modified(
    parsed: &ParsedFile,
    mut item: ItemId,
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

fn instance_function_name(
    names: &NameResolution,
    typed: &TypedFile,
    instance: &FunctionInstanceKey,
    span: Span,
) -> Result<String, LoweringError> {
    let mut name = callable_symbol_name(names, instance.symbol())?;
    if instance.type_arguments().is_empty() {
        return Ok(name);
    }
    name.push('<');
    for (index, ty) in instance.type_arguments().iter().copied().enumerate() {
        if index != 0 {
            name.push(',');
        }
        let builtin = builtin_type(typed, ty)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if !LOWERED_BUILTINS.contains(&builtin) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        name.push_str(builtin.name());
    }
    name.push('>');
    Ok(name)
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
