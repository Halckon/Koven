//! frontend 产物门禁、函数预声明与文件级 lowering 编排。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::{
        CleanupCaptureValue, CleanupCondition, ClosureCaptureEffect, ClosureCaptureMode, DropPoint,
        OwnershipCheckedFile,
    },
    parser::{ClassifierKind, FunctionBody, FunctionForm, Item, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{BuiltinType, CallableDescriptor, ParameterMode, TypeId, TypedFile},
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
    instances::{FunctionInstanceKey, FunctionTemplate, plan_instances, resolve_concrete_type},
    nominal::NominalTypeMapper,
    present_name, return_values, source_closure, span_key,
};
use crate::ssa::{
    model::{EntityId, EntityType, FunctionId, Origin, Program, TerminatorKind},
    verify::verify_program,
};

const LOWERED_BUILTINS: [BuiltinType; 12] = [
    BuiltinType::Byte,
    BuiltinType::Short,
    BuiltinType::Int,
    BuiltinType::Long,
    BuiltinType::UByte,
    BuiltinType::UShort,
    BuiltinType::UInt,
    BuiltinType::ULong,
    BuiltinType::Boolean,
    BuiltinType::Char,
    BuiltinType::String,
    BuiltinType::Unit,
];

struct FunctionPlan {
    id: FunctionId,
    item_id: ItemId,
    body: FunctionPlanBody,
    parameter_symbols: Vec<SymbolId>,
    return_type: TypeId,
    substitutions: BTreeMap<SymbolId, TypeId>,
    span: Span,
}

struct FunctionDeclaration {
    item_id: ItemId,
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
    if let Some(root) =
        owned.cleanup_steps().iter().find_map(|(_, action)| {
            match action {
            lang_frontend::ownership_checking::IterationCleanupAction::ReleaseClosureInstances {
                root,
                ..
            } | lang_frontend::ownership_checking::IterationCleanupAction::ReleaseRetainedClosureSources { root } => Some(root),
            _ => None,
        }
        })
    {
        return Err(error(
            LoweringErrorKind::UnsupportedNode,
            root.value_origin(),
        ));
    }
    if let Some(selector) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::TestLastCaptureLoan {
                selector,
                ..
            } => Some(*selector),
            _ => None,
        })
    {
        return Err(error(
            LoweringErrorKind::UnsupportedNode,
            owned
                .cleanup_conditions()
                .selector(selector)
                .expect("published last-loan selector")
                .origin(),
        ));
    }
    // The narrow String move-closure bridge reads a current symbol directly. It cannot
    // execute a guarded source choice or read an earlier enclosing environment instance.
    if let Some(origin) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::SaveClosureCapture {
                input,
                ..
            } if input.mode() == ClosureCaptureMode::Owned
                && input.effect() == ClosureCaptureEffect::Move
                && (!matches!(input.value(), CleanupCaptureValue::Owner(_))
                    || !matches!(
                        owned.cleanup_conditions().get(input.condition()),
                        Some(CleanupCondition::Always)
                    )) =>
            {
                Some(input.origin())
            }
            _ => None,
        })
    {
        return Err(error(LoweringErrorKind::UnsupportedNode, origin));
    }
    if let Some(origin) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                ..
            } => Some(
                owned
                    .cleanup_conditions()
                    .owner_snapshot(*owner)
                    .expect("published snapshot identity")
                    .origin(),
            ),
            _ => None,
        })
    {
        return Err(error(LoweringErrorKind::UnsupportedNode, origin));
    }
    // Saved owner choices have no SSA carrier yet; reject before planning any bindings.
    if let Some(fact) = owned.drops().iter().find(|fact| fact.condition().is_some()) {
        return Err(error(
            LoweringErrorKind::UnsupportedNode,
            fact.value_origin(),
        ));
    }
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
    let mut type_mapper = NominalTypeMapper::new(typed)?;
    for builtin in LOWERED_BUILTINS {
        let ty = typed.types().builtin(builtin).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        type_mapper.intern(module, names, typed, ty, file_anchor)?;
    }
    for construction in typed.constructions() {
        let construction_span = parsed
            .ast()
            .expressions()
            .get(construction.expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        type_mapper.intern(
            module,
            names,
            typed,
            construction.result_type(),
            construction_span,
        )?;
        for argument in construction.arguments() {
            type_mapper.intern(
                module,
                names,
                typed,
                argument.parameter_type(),
                construction_span,
            )?;
        }
    }
    for construction in typed.container_constructions() {
        let construction_span = parsed
            .ast()
            .expressions()
            .get(construction.expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        type_mapper.intern(
            module,
            names,
            typed,
            construction.container_type(),
            construction_span,
        )?;
        type_mapper.intern(
            module,
            names,
            typed,
            construction.element_type(),
            construction_span,
        )?;
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
        // Assertion operands can introduce a nullable type used only inside this function body.
        for assertion in owned
            .non_null_assertions()
            .iter()
            .map(|plan| plan.descriptor())
            .filter(|assertion| {
                let operator = assertion.operator_span();
                operator.source_id() == span.source_id()
                    && span.start() <= operator.start()
                    && operator.end() <= span.end()
            })
        {
            let nullable = resolve_concrete_type(
                typed,
                assertion.nullable_type(),
                &instance.substitutions,
                assertion.operator_span(),
            )?;
            type_mapper.intern(module, names, typed, nullable, assertion.operator_span())?;
        }
        // Root intrinsics can demand storage (notably nullable handles) only in the body.
        for primitive in typed.ownership_primitives() {
            let primitive_span = parsed
                .ast()
                .expressions()
                .get(primitive.expression())
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
                .span();
            if primitive_span.source_id() == span.source_id()
                && span.start() <= primitive_span.start()
                && primitive_span.end() <= span.end()
            {
                let concrete = resolve_concrete_type(
                    typed,
                    primitive.value_type(),
                    &instance.substitutions,
                    primitive_span,
                )?;
                type_mapper.intern(module, names, typed, concrete, primitive_span)?;
            }
        }
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
                let concrete =
                    resolve_concrete_type(typed, parameter.ty, &instance.substitutions, span)?;
                let ty = type_mapper.intern(module, names, typed, concrete, span)?;
                match parameter.mode {
                    ParameterMode::Value => Ok(EntityType::Value(ty)),
                    ParameterMode::Borrow => Ok(EntityType::Loan {
                        kind: crate::ssa::model::LoanKind::Shared,
                        target: ty,
                    }),
                    ParameterMode::Inout => Err(error(LoweringErrorKind::UnsupportedNode, span)),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_type =
            resolve_concrete_type(typed, callable.return_type(), &instance.substitutions, span)?;
        let return_types = if builtin_type(typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![type_mapper.intern(module, names, typed, return_type, span)?]
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
            item_id: declaration.item_id,
            body,
            parameter_symbols,
            return_type,
            substitutions: instance.substitutions,
            span,
        });
    }

    let source_closures =
        source_closure::declare(module, &mut type_mapper, parsed, names, typed, owned)?;

    let (type_ids, heap_payloads, enum_payloads) = type_mapper.into_parts();
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
        let mut bindings = BTreeMap::new();
        let mut borrow_bindings = BTreeMap::new();
        for (symbol, entity) in plan.parameter_symbols.into_iter().zip(parameters) {
            match entity {
                EntityId::Value(value) => {
                    bindings.insert(symbol, LoweredValue::Value(value));
                }
                EntityId::Loan(loan) => {
                    borrow_bindings.insert(symbol, loan);
                }
                EntityId::Place(_) => unreachable!("callable parameters cannot be places"),
            }
        }
        let mut lowerer = ExpressionLowerer {
            parsed,
            names,
            typed,
            owned,
            source_text,
            references: &references,
            function_ids: &function_ids,
            source_closures: &source_closures,
            type_ids: &type_ids,
            heap_payloads: &heap_payloads,
            enum_payloads: &enum_payloads,
            substitutions: &plan.substitutions,
            function,
            block: entry,
            bindings,
            borrow_bindings,
            non_null_bindings: BTreeMap::new(),
            pending_call_loans: BTreeMap::new(),
            temporaries: BTreeMap::new(),
            return_type: plan.return_type,
            loops: Vec::new(),
        };
        lowerer.emit_drops(DropPoint::FunctionEntry(plan.item_id))?;
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

    let unit = typed
        .types()
        .builtin(BuiltinType::Unit)
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let empty_substitutions = BTreeMap::new();
    for plan in source_closures.values() {
        let function = module
            .function_mut(plan.thunk)
            .expect("planned thunk exists");
        let entry = function.entry_block().expect("planned thunk entry exists");
        let mut lowerer = ExpressionLowerer {
            parsed,
            names,
            typed,
            owned,
            source_text,
            references: &references,
            function_ids: &function_ids,
            source_closures: &source_closures,
            type_ids: &type_ids,
            heap_payloads: &heap_payloads,
            enum_payloads: &enum_payloads,
            substitutions: &empty_substitutions,
            function,
            block: entry,
            bindings: BTreeMap::new(),
            borrow_bindings: BTreeMap::new(),
            non_null_bindings: BTreeMap::new(),
            pending_call_loans: BTreeMap::new(),
            temporaries: BTreeMap::new(),
            return_type: unit,
            loops: Vec::new(),
        };
        lowerer.bind_capture_views(plan)?;
        let result = lowerer.lower_statement(plan.body)?;
        if !matches!(result, LoweredValue::Diverged) {
            lowerer
                .function
                .set_terminator(
                    lowerer.block,
                    TerminatorKind::Return { values: Vec::new() },
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
    let constants = typed.constants().ok_or(LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: None,
    })?;
    let materializations = owned.constant_materializations().ok_or(LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: None,
    })?;
    if !constants.matches(typed) || !materializations.matches(typed) {
        return Err(LoweringError {
            kind: LoweringErrorKind::MismatchedAnalysis,
            span: None,
        });
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
        // Entry drop facts refer to the function itself, not its modifier wrapper.
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
