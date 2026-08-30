//! SPEC-0199 compilation-unit 可达 callable 与具体实例的确定性计划。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::ItemId,
    name_resolution::{
        DeclarationId, SourceUnitId, SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames,
        index_compilation_unit,
    },
    ownership_checking::ValidatedCompilationUnitOwnership,
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        TypeEnvironment, UnitCallTarget, UnitCallableTarget, UnitTypeId, UnitTypeKind,
        ValidatedCompilationUnitTypes,
    },
};

use super::{LoweringError, LoweringErrorKind};

/// 防止 unit-wide 泛型实例图被合法但病态的源码无界扩张。
const MAX_UNIT_GENERIC_INSTANCES: usize = 1024;

/// 一个 unit-wide 具体函数实例的规范 identity。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UnitFunctionInstanceKey {
    declaration: DeclarationId,
    type_arguments: Vec<UnitTypeId>,
}

impl UnitFunctionInstanceKey {
    pub(crate) fn new(declaration: DeclarationId, type_arguments: Vec<UnitTypeId>) -> Self {
        Self {
            declaration,
            type_arguments,
        }
    }

    pub(crate) fn for_entry(declaration: DeclarationId) -> Self {
        Self::new(declaration, Vec::new())
    }

    pub(crate) const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    pub(crate) fn type_arguments(&self) -> &[UnitTypeId] {
        &self.type_arguments
    }
}

/// lower 单个 body 所需的 source-local locator 与类型替换。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnitPlannedInstance {
    key: UnitFunctionInstanceKey,
    source_unit: SourceUnitId,
    item: ItemId,
    substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
}

impl UnitPlannedInstance {
    pub(crate) const fn key(&self) -> &UnitFunctionInstanceKey {
        &self.key
    }

    pub(crate) const fn source_unit(&self) -> SourceUnitId {
        self.source_unit
    }

    pub(crate) const fn item(&self) -> ItemId {
        self.item
    }

    pub(crate) const fn substitutions(&self) -> &BTreeMap<UnitSymbolId, UnitTypeId> {
        &self.substitutions
    }

    pub(crate) const fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Debug)]
struct UnitFunctionTemplate {
    declaration: DeclarationId,
    source_unit: SourceUnitId,
    item: ItemId,
    type_parameters: Vec<UnitSymbolId>,
    span: Span,
}

/// 从显式 entry 建立仅含可达函数的 unit-wide 具体实例计划。
pub(crate) fn plan_unit_instances(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
) -> Result<Vec<UnitPlannedInstance>, LoweringError> {
    validate_unit_inputs(sources, inputs, names, environment, typed, owned)?;
    let parsed_by_source = parsed_by_source_unit(inputs, names)?;
    let templates = collect_templates(names, typed, &parsed_by_source)?;
    let template_by_declaration = templates
        .iter()
        .enumerate()
        .map(|(index, template)| (template.declaration, index))
        .collect::<BTreeMap<_, _>>();
    let entry_template = template_by_declaration
        .get(&entry)
        .copied()
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: names
                .names()
                .index()
                .declarations()
                .get(entry.index())
                .map(|declaration| declaration.name_span()),
        })?;
    if !templates[entry_template].type_parameters.is_empty() {
        return Err(lowering_error(
            LoweringErrorKind::UnsupportedNode,
            templates[entry_template].span,
        ));
    }

    let calls_by_template = index_calls(typed, &parsed_by_source, &templates)?;
    let mut pending = BTreeSet::from([UnitFunctionInstanceKey::new(entry, Vec::new())]);
    let mut planned = BTreeMap::new();
    let mut generic_instance_count = 0;

    while let Some(key) = pending.pop_first() {
        if planned.contains_key(&key) {
            continue;
        }
        let template_index = template_by_declaration
            .get(&key.declaration())
            .copied()
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let template = &templates[template_index];
        if template.type_parameters.len() != key.type_arguments().len() {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                template.span,
            ));
        }
        if !key.type_arguments().is_empty() && generic_instance_count >= MAX_UNIT_GENERIC_INSTANCES
        {
            return Err(lowering_error(
                LoweringErrorKind::InstanceLimitExceeded,
                template.span,
            ));
        }
        let substitutions = template
            .type_parameters
            .iter()
            .copied()
            .zip(key.type_arguments().iter().copied())
            .collect::<BTreeMap<_, _>>();

        for (call_index, span) in &calls_by_template[template_index] {
            let call = &typed.types().calls()[*call_index];
            let UnitCallTarget::Declaration(target) = call.target() else {
                continue;
            };
            let target_template_index = template_by_declaration
                .get(&target)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, *span))?;
            let arguments = call
                .instance()
                .type_arguments()
                .iter()
                .map(|ty| resolve_concrete_type(typed, *ty, &substitutions, *span))
                .collect::<Result<Vec<_>, _>>()?;
            if arguments.len() != templates[target_template_index].type_parameters.len() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, *span));
            }
            pending.insert(UnitFunctionInstanceKey::new(target, arguments));
        }

        if !key.type_arguments().is_empty() {
            generic_instance_count += 1;
        }
        planned.insert(
            key.clone(),
            UnitPlannedInstance {
                key,
                source_unit: template.source_unit,
                item: template.item,
                substitutions,
                span: template.span,
            },
        );
    }

    Ok(planned.into_values().collect())
}

/// 核对 codegen 消费的 source inputs 与 validated unit analysis identity chain。
pub(crate) fn validate_unit_inputs(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
) -> Result<(), LoweringError> {
    let rebuilt = index_compilation_unit(sources, inputs).map_err(|_| LoweringError {
        kind: LoweringErrorKind::MismatchedSource,
        span: None,
    })?;
    if &rebuilt != names.names().index()
        || !typed
            .types()
            .is_compatible_with(sources, inputs, names, environment)
        || !owned.ownership().is_compatible_with(typed)
    {
        return Err(LoweringError {
            kind: LoweringErrorKind::MismatchedAnalysis,
            span: None,
        });
    }
    Ok(())
}

fn parsed_by_source_unit<'a>(
    inputs: &'a [SourceUnitInput<'a>],
    names: &ValidatedCompilationUnitNames,
) -> Result<Vec<&'a ParsedFile>, LoweringError> {
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

fn collect_templates(
    names: &ValidatedCompilationUnitNames,
    typed: &ValidatedCompilationUnitTypes,
    parsed_by_source: &[&ParsedFile],
) -> Result<Vec<UnitFunctionTemplate>, LoweringError> {
    let signatures = typed.types().signatures();
    let mut templates = Vec::new();
    for declaration in names.names().index().declarations() {
        let signature = signatures.declaration(declaration.id()).ok_or_else(|| {
            lowering_error(LoweringErrorKind::MissingFact, declaration.name_span())
        })?;
        let Some(callable) = signature.callable() else {
            continue;
        };
        if callable.target() != UnitCallableTarget::Declaration(declaration.id()) {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                declaration.name_span(),
            ));
        }
        let parsed = parsed_by_source
            .get(declaration.source_unit().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = parsed
            .ast()
            .items()
            .get(declaration.root())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: Some(declaration.name_span()),
            })?
            .span();
        templates.push(UnitFunctionTemplate {
            declaration: declaration.id(),
            source_unit: declaration.source_unit(),
            item: declaration.root(),
            type_parameters: callable.type_parameters().to_vec(),
            span,
        });
    }
    Ok(templates)
}

fn index_calls(
    typed: &ValidatedCompilationUnitTypes,
    parsed_by_source: &[&ParsedFile],
    templates: &[UnitFunctionTemplate],
) -> Result<Vec<Vec<(usize, Span)>>, LoweringError> {
    let mut calls = vec![Vec::new(); templates.len()];
    for (call_index, call) in typed.types().calls().iter().enumerate() {
        let expression = call.expression();
        let span = parsed_by_source
            .get(expression.source_unit().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let mut owners = templates.iter().enumerate().filter(|(_, template)| {
            template.source_unit == expression.source_unit() && span_contains(template.span, span)
        });
        if let Some((owner, _)) = owners.next() {
            if owners.next().is_some() {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
            calls[owner].push((call_index, span));
        }
    }
    Ok(calls)
}

pub(crate) fn resolve_concrete_type(
    typed: &ValidatedCompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

fn contains_type_parameter(typed: &ValidatedCompilationUnitTypes, kind: &UnitTypeKind) -> bool {
    let contains = |ty| {
        typed
            .types()
            .types()
            .get(ty)
            .is_some_and(|kind| contains_type_parameter(typed, kind))
    };
    match kind {
        UnitTypeKind::TypeParameter(_) => true,
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => contains(*inner),
        UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        } => parameters.iter().any(|parameter| contains(parameter.ty())) || contains(*return_type),
        UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. } => {
            arguments.iter().copied().any(contains)
        }
        UnitTypeKind::EnumCase { root, .. } => contains(*root),
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => false,
    }
}

fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

const fn lowering_error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}
