//! SPEC-0199 compilation-unit 可达 callable 与具体实例的确定性计划。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::ItemId,
    name_resolution::{
        DeclarationId, SourceUnitId, SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames,
        index_compilation_unit,
    },
    ownership_checking::ValidatedCompilationUnitOwnership,
    parser::{Item, NameMarker, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        IntrinsicTypeConstructor, NominalKind, TypeEnvironment, UnitCallTarget,
        UnitCallableSignature, UnitCallableTarget, UnitNominalSignature, UnitTypeId, UnitTypeKind,
        ValidatedCompilationUnitTypes,
    },
};

use super::{LoweringError, LoweringErrorKind};

/// 防止 unit-wide 泛型实例图被合法但病态的源码无界扩张。
const MAX_UNIT_GENERIC_INSTANCES: usize = 1024;

/// 一个 unit-wide 具体函数实例的规范 identity。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UnitFunctionInstanceKey {
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    static_self: Option<UnitTypeId>,
}

/// 一个已解析的 Borrow-only delegation receiver 投影。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnitDelegatedCallRoute {
    outer_receiver: UnitTypeId,
    field: UnitSymbolId,
    delegate_receiver: UnitTypeId,
}

impl UnitDelegatedCallRoute {
    pub(crate) const fn outer_receiver(self) -> UnitTypeId {
        self.outer_receiver
    }

    pub(crate) const fn field(self) -> UnitSymbolId {
        self.field
    }

    pub(crate) const fn delegate_receiver(self) -> UnitTypeId {
        self.delegate_receiver
    }
}

/// planner 与 expression lowerer 共用的静态 call route。
pub(crate) struct ResolvedUnitCallInstance {
    key: UnitFunctionInstanceKey,
    delegation: Vec<UnitDelegatedCallRoute>,
}

impl ResolvedUnitCallInstance {
    pub(crate) const fn key(&self) -> &UnitFunctionInstanceKey {
        &self.key
    }

    pub(crate) fn delegation(&self) -> &[UnitDelegatedCallRoute] {
        &self.delegation
    }
}

impl UnitFunctionInstanceKey {
    pub(crate) fn new(declaration: DeclarationId, type_arguments: Vec<UnitTypeId>) -> Self {
        Self::for_target(UnitCallableTarget::Declaration(declaration), type_arguments)
    }

    pub(crate) fn for_target(target: UnitCallableTarget, type_arguments: Vec<UnitTypeId>) -> Self {
        Self::for_specialized_target(target, type_arguments, None)
    }

    pub(crate) fn for_specialized_target(
        target: UnitCallableTarget,
        type_arguments: Vec<UnitTypeId>,
        static_self: Option<UnitTypeId>,
    ) -> Self {
        Self {
            target,
            type_arguments,
            static_self,
        }
    }

    pub(crate) fn for_entry(declaration: DeclarationId) -> Self {
        Self::new(declaration, Vec::new())
    }

    pub(crate) const fn target(&self) -> UnitCallableTarget {
        self.target
    }

    pub(crate) fn type_arguments(&self) -> &[UnitTypeId] {
        &self.type_arguments
    }

    pub(crate) const fn static_self(&self) -> Option<UnitTypeId> {
        self.static_self
    }

    pub(crate) fn is_specialized(&self) -> bool {
        !self.type_arguments.is_empty() || self.static_self.is_some()
    }
}

/// lower 单个 body 所需的 source-local locator 与类型替换。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnitPlannedInstance {
    key: UnitFunctionInstanceKey,
    source_unit: SourceUnitId,
    item: ItemId,
    substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
    owner: Option<DeclarationId>,
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

    pub(crate) const fn owner(&self) -> Option<DeclarationId> {
        self.owner
    }

    pub(crate) const fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Debug)]
struct UnitFunctionTemplate {
    target: UnitCallableTarget,
    source_unit: SourceUnitId,
    item: ItemId,
    type_parameters: Vec<UnitSymbolId>,
    owner: Option<DeclarationId>,
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
    let template_by_target = templates
        .iter()
        .enumerate()
        .map(|(index, template)| (template.target, index))
        .collect::<BTreeMap<_, _>>();
    if template_by_target.len() != templates.len() {
        return Err(LoweringError {
            kind: LoweringErrorKind::InvalidModel,
            span: None,
        });
    }
    let entry_template = template_by_target
        .get(&UnitCallableTarget::Declaration(entry))
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
        let template_index =
            template_by_target
                .get(&key.target())
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
        let requires_static_self = callable_static_self_receiver(typed, key.target())?;
        if requires_static_self != key.static_self().is_some() {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                template.span,
            ));
        }
        if key.is_specialized() && generic_instance_count >= MAX_UNIT_GENERIC_INSTANCES {
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
            let target = match call.target() {
                UnitCallTarget::Declaration(declaration) => {
                    UnitCallableTarget::Declaration(declaration)
                }
                UnitCallTarget::Symbol(symbol) => UnitCallableTarget::Symbol(symbol),
                UnitCallTarget::External(_)
                | UnitCallTarget::FunctionValue
                | UnitCallTarget::StructuralComponent(_) => continue,
            };
            let arguments = call
                .instance()
                .type_arguments()
                .iter()
                .map(|ty| {
                    resolve_concrete_type(typed, *ty, &substitutions, key.static_self(), *span)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let receiver = call
                .receiver()
                .map(|receiver| {
                    resolve_concrete_type(
                        typed,
                        receiver.ty(),
                        &substitutions,
                        key.static_self(),
                        *span,
                    )
                })
                .transpose()?;
            let target_key =
                resolve_unit_call_instance(typed, owned, target, arguments, receiver, *span)?;
            let target_template_index = template_by_target
                .get(&target_key.key().target())
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, *span))?;
            if target_key.key().type_arguments().len()
                != templates[target_template_index].type_parameters.len()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, *span));
            }
            pending.insert(target_key.key);
        }

        if key.is_specialized() {
            generic_instance_count += 1;
        }
        planned.insert(
            key.clone(),
            UnitPlannedInstance {
                key,
                source_unit: template.source_unit,
                item: template.item,
                substitutions,
                owner: template.owner,
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
        if let Some(callable) = signature.callable() {
            if callable.target() != UnitCallableTarget::Declaration(declaration.id()) {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    declaration.name_span(),
                ));
            }
            let parsed = parsed_by_source
                .get(declaration.source_unit().index())
                .copied()
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
                target: UnitCallableTarget::Declaration(declaration.id()),
                source_unit: declaration.source_unit(),
                item: declaration.root(),
                type_parameters: callable.type_parameters().to_vec(),
                owner: None,
                span,
            });
        }
        let Some(nominal) = signature.nominal() else {
            continue;
        };
        let parsed = parsed_by_source
            .get(declaration.source_unit().index())
            .copied()
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        for member in nominal.members() {
            let UnitCallableTarget::Symbol(symbol) = member.target() else {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    member.name_span(),
                ));
            };
            let item = callable_item(parsed, member)?;
            let span = parsed
                .ast()
                .items()
                .get(item)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, member.name_span()))?
                .span();
            let mut type_parameters = nominal.type_parameters().to_vec();
            type_parameters.extend_from_slice(member.type_parameters());
            templates.push(UnitFunctionTemplate {
                target: UnitCallableTarget::Symbol(symbol),
                source_unit: symbol.source_unit(),
                item,
                type_parameters,
                owner: Some(declaration.id()),
                span,
            });
        }
    }
    Ok(templates)
}

fn callable_item(
    parsed: &ParsedFile,
    callable: &UnitCallableSignature,
) -> Result<ItemId, LoweringError> {
    parsed
        .ast()
        .items()
        .iter()
        .find_map(|(item, node)| match node.payload() {
            Item::Function {
                name: NameMarker::Present(span),
                ..
            } if *span == callable.name_span() => Some(item),
            _ => None,
        })
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, callable.name_span()))
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
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::StaticSelf(_)) => {
            static_self.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nullable(inner)) => {
            let inner = resolve_concrete_type(typed, *inner, substitutions, static_self, span)?;
            typed
                .types()
                .types()
                .find(&UnitTypeKind::Nullable(inner))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) => {
            let arguments = arguments
                .iter()
                .map(|argument| {
                    resolve_direct_type_argument(typed, *argument, substitutions, static_self, span)
                })
                .collect::<Result<Vec<_>, _>>()?;
            typed
                .types()
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: *declaration,
                    arguments,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

fn resolve_direct_type_argument(
    typed: &ValidatedCompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::StaticSelf(_)) => {
            static_self.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

pub(crate) fn callable_static_self_receiver(
    typed: &ValidatedCompilationUnitTypes,
    target: UnitCallableTarget,
) -> Result<bool, LoweringError> {
    let callable = unit_callable_signature(typed, target).ok_or(LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: None,
    })?;
    let Some(receiver) = callable.receiver() else {
        return Ok(false);
    };
    Ok(matches!(
        typed.types().types().get(receiver.ty()),
        Some(UnitTypeKind::StaticSelf(_))
    ))
}

/// 把 typed call target 与 concrete receiver 解析为 planner/lowerer 共用的实例 identity。
pub(crate) fn resolve_unit_call_instance(
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
) -> Result<ResolvedUnitCallInstance, LoweringError> {
    let direct = || {
        resolve_direct_unit_call_instance(typed, target, type_arguments.clone(), receiver, span)
            .map(|key| ResolvedUnitCallInstance {
                key,
                delegation: Vec::new(),
            })
    };
    let Some(receiver) = receiver else {
        return direct();
    };
    let mut current_receiver = receiver;
    let mut current_target = target;
    let mut current_type_arguments = type_arguments.clone();
    let mut delegation = Vec::new();
    let mut visited = BTreeSet::new();
    loop {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = typed.types().types().get(current_receiver)
        else {
            return if delegation.is_empty() {
                direct()
            } else {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            };
        };
        let nominal = typed
            .types()
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let routes = typed
            .types()
            .signatures()
            .delegations()
            .iter()
            .filter(|plan| plan.owner() == *declaration)
            .flat_map(|plan| {
                plan.forwarders()
                    .iter()
                    .filter(move |forwarder| forwarder.requirement() == current_target)
                    .map(move |forwarder| (plan, forwarder))
            })
            .collect::<Vec<_>>();
        let (route, forwarder) = match routes.as_slice() {
            [] if delegation.is_empty() => return direct(),
            [] => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            [(route, forwarder)] => (*route, *forwarder),
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if !visited.insert((*declaration, current_target)) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let ownership_routes = owned
            .ownership()
            .delegations()
            .iter()
            .filter(|plan| {
                plan.owner() == *declaration
                    && plan.target() == route.target()
                    && plan.forwarders().contains(&current_target)
            })
            .collect::<Vec<_>>();
        if !matches!(ownership_routes.as_slice(), [_]) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if arguments.len() != nominal.type_parameters().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let concrete_fields =
            resolve_nominal_runtime_field_types(typed, current_receiver, nominal, arguments)?;
        let field_index = nominal
            .fields()
            .iter()
            .enumerate()
            .find(|(_, field)| field.symbol() == route.target())
            .map(|(index, _)| index)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let delegate_receiver = concrete_fields
            .get(field_index)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(UnitTypeKind::Nominal {
            declaration: delegate,
            arguments: delegate_arguments,
        }) = typed.types().types().get(delegate_receiver)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let delegate_nominal = typed
            .types()
            .signatures()
            .declaration(*delegate)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if delegate_arguments.len() != delegate_nominal.type_parameters().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        resolve_nominal_runtime_field_types(
            typed,
            delegate_receiver,
            delegate_nominal,
            delegate_arguments,
        )?;
        delegation.push(UnitDelegatedCallRoute {
            outer_receiver: current_receiver,
            field: route.target(),
            delegate_receiver,
        });
        if let Some(next_hop) = forwarder.next_hop() {
            current_type_arguments = remap_delegation_next_hop_arguments(
                typed,
                current_target,
                next_hop.requirement(),
                forwarder.receiver_type(),
                next_hop.receiver_type(),
                nominal,
                arguments,
                &current_type_arguments,
                span,
            )?;
            current_target = next_hop.requirement();
            current_receiver = delegate_receiver;
            continue;
        }
        let Some(implementation) = forwarder.implementation() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };

        // Frontend 已选定 effective implementation；这里只校验 recipe 并重映射泛型槽位，
        // 不重新执行 member selection。
        let requirement_callable = unit_callable_signature(typed, current_target)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let implementation_callable = unit_callable_signature(typed, implementation.target())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (requirement_owner, requirement_owner_arguments) =
            instantiate_delegated_dispatch_owner_arguments(
                typed,
                forwarder.receiver_type(),
                nominal,
                arguments,
                span,
            )?;
        let (implementation_owner, implementation_owner_arguments) =
            instantiate_delegated_dispatch_owner_arguments(
                typed,
                implementation.receiver_type(),
                nominal,
                arguments,
                span,
            )?;
        let (declared_requirement_owner, declared_requirement_owner_arity) =
            unit_callable_owner(typed, current_target)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (declared_implementation_owner, declared_implementation_owner_arity) =
            unit_callable_owner(typed, implementation.target())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !implementation_callable.has_body()
            || implementation_callable.type_parameters().len()
                != requirement_callable.type_parameters().len()
            || declared_requirement_owner != requirement_owner
            || declared_implementation_owner != implementation_owner
            || declared_requirement_owner_arity != requirement_owner_arguments.len()
            || declared_implementation_owner_arity != implementation_owner_arguments.len()
            || current_type_arguments.len()
                != requirement_owner_arguments.len() + requirement_callable.type_parameters().len()
            || current_type_arguments[..requirement_owner_arguments.len()]
                != requirement_owner_arguments
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let callable_argument_start = requirement_owner_arguments.len();
        let mut implementation_arguments = implementation_owner_arguments;
        implementation_arguments
            .extend_from_slice(&current_type_arguments[callable_argument_start..]);
        let key = resolve_direct_unit_call_instance(
            typed,
            implementation.target(),
            implementation_arguments,
            Some(delegate_receiver),
            span,
        )?;
        return Ok(ResolvedUnitCallInstance { key, delegation });
    }
}

#[allow(clippy::too_many_arguments)]
fn remap_delegation_next_hop_arguments(
    typed: &ValidatedCompilationUnitTypes,
    current_target: UnitCallableTarget,
    next_target: UnitCallableTarget,
    current_owner_template: UnitTypeId,
    next_owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    type_arguments: &[UnitTypeId],
    span: Span,
) -> Result<Vec<UnitTypeId>, LoweringError> {
    let current_callable = unit_callable_signature(typed, current_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let next_callable = unit_callable_signature(typed, next_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (current_owner, current_owner_arguments) = instantiate_dispatch_owner_arguments(
        typed,
        current_owner_template,
        concrete_owner,
        concrete_arguments,
        span,
    )?;
    let (next_owner, next_owner_arguments) = instantiate_dispatch_owner_arguments(
        typed,
        next_owner_template,
        concrete_owner,
        concrete_arguments,
        span,
    )?;
    let (declared_current_owner, current_owner_arity) = unit_callable_owner(typed, current_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (declared_next_owner, next_owner_arity) = unit_callable_owner(typed, next_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if current_callable.type_parameters().len() != next_callable.type_parameters().len()
        || declared_current_owner != current_owner
        || declared_next_owner != next_owner
        || current_owner_arity != current_owner_arguments.len()
        || next_owner_arity != next_owner_arguments.len()
        || type_arguments.len()
            != current_owner_arguments.len() + current_callable.type_parameters().len()
        || type_arguments[..current_owner_arguments.len()] != current_owner_arguments
    {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let callable_argument_start = current_owner_arguments.len();
    let mut remapped = next_owner_arguments;
    remapped.extend_from_slice(&type_arguments[callable_argument_start..]);
    Ok(remapped)
}

fn resolve_direct_unit_call_instance(
    typed: &ValidatedCompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitFunctionInstanceKey, LoweringError> {
    let callable = unit_callable_signature(typed, target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if callable.has_body() {
        let static_self = if callable_static_self_receiver(typed, target)? {
            Some(receiver.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?)
        } else {
            None
        };
        return Ok(UnitFunctionInstanceKey::for_specialized_target(
            target,
            type_arguments,
            static_self,
        ));
    }

    let receiver = receiver.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments: owner_arguments,
    }) = typed.types().types().get(receiver)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    let nominal = typed
        .types()
        .signatures()
        .declaration(*declaration)
        .and_then(|signature| signature.nominal())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let dispatch = nominal
        .static_dispatch_overrides()
        .iter()
        .find(|dispatch| dispatch.requirement() == target)
        .copied()
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let implementation = dispatch.implementation();
    let implementation_callable = unit_callable_signature(typed, implementation)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (requirement_declaration, requirement_owner_arity) = unit_callable_owner(typed, target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (implementation_declaration, implementation_owner_arity) =
        unit_callable_owner(typed, implementation)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (requirement_owner, requirement_owner_arguments) = instantiate_dispatch_owner_arguments(
        typed,
        dispatch.requirement_owner(),
        nominal,
        owner_arguments,
        span,
    )?;
    let (implementation_owner, implementation_owner_arguments) =
        if implementation_declaration == *declaration {
            instantiate_dispatch_owner_arguments(
                typed,
                dispatch.implementation_owner(),
                nominal,
                owner_arguments,
                span,
            )?
        } else {
            instantiate_inherited_dispatch_owner_arguments(
                typed,
                dispatch.implementation_owner(),
                nominal,
                owner_arguments,
                span,
            )?
        };
    if !implementation_callable.has_body()
        || implementation_callable.type_parameters().len() != callable.type_parameters().len()
        || requirement_declaration != requirement_owner
        || implementation_declaration != implementation_owner
        || requirement_owner_arity != requirement_owner_arguments.len()
        || implementation_owner_arity != implementation_owner_arguments.len()
        || type_arguments.len()
            != requirement_owner_arguments.len() + callable.type_parameters().len()
        || type_arguments[..requirement_owner_arguments.len()] != requirement_owner_arguments
    {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let callable_argument_start = requirement_owner_arguments.len();
    let mut implementation_arguments = implementation_owner_arguments;
    implementation_arguments.extend_from_slice(&type_arguments[callable_argument_start..]);
    let static_self = callable_static_self_receiver(typed, implementation)?.then_some(receiver);
    Ok(UnitFunctionInstanceKey::for_specialized_target(
        implementation,
        implementation_arguments,
        static_self,
    ))
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
        UnitCallableTarget::Symbol(_) => typed
            .types()
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .flat_map(|nominal| nominal.members().iter().chain(nominal.companion_members()))
            .find(|callable| callable.target() == target),
    }
}

fn unit_callable_owner(
    typed: &ValidatedCompilationUnitTypes,
    target: UnitCallableTarget,
) -> Option<(DeclarationId, usize)> {
    match target {
        UnitCallableTarget::Declaration(_) => None,
        UnitCallableTarget::Symbol(_) => typed
            .types()
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .find(|nominal| {
                nominal
                    .members()
                    .iter()
                    .chain(nominal.companion_members())
                    .any(|callable| callable.target() == target)
            })
            .map(|nominal| (nominal.declaration(), nominal.type_parameters().len())),
    }
}

fn instantiate_dispatch_owner_arguments(
    typed: &ValidatedCompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().types().get(owner_template)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    if concrete_owner.type_parameters().len() != concrete_arguments.len() {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let substitutions = concrete_owner
        .type_parameters()
        .iter()
        .copied()
        .zip(concrete_arguments.iter().copied())
        .collect::<BTreeMap<_, _>>();
    let arguments = arguments
        .iter()
        .map(|argument| resolve_direct_type_argument(typed, *argument, &substitutions, None, span))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((*declaration, arguments))
}

/// 只为 frontend 已选定的 inherited effective implementation 实例化有限 `List` owner recipe。
fn instantiate_inherited_dispatch_owner_arguments(
    typed: &ValidatedCompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().types().get(owner_template)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    if concrete_owner.type_parameters().len() != concrete_arguments.len() {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let substitutions = concrete_owner
        .type_parameters()
        .iter()
        .copied()
        .zip(concrete_arguments.iter().copied())
        .collect::<BTreeMap<_, _>>();
    let arguments = arguments
        .iter()
        .map(|argument| {
            resolve_inherited_dispatch_owner_argument(
                typed,
                *argument,
                &substitutions,
                span,
                &mut BTreeSet::new(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((*declaration, arguments))
}

pub(super) fn resolve_inherited_dispatch_owner_argument(
    typed: &ValidatedCompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
    visiting: &mut BTreeSet<UnitTypeId>,
) -> Result<UnitTypeId, LoweringError> {
    if !visiting.insert(ty) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    match typed.types().types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        }) => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let argument = resolve_inherited_dispatch_owner_argument(
                typed,
                *argument,
                substitutions,
                span,
                visiting,
            )?;
            typed
                .types()
                .types()
                .find(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

fn instantiate_delegated_dispatch_owner_arguments(
    typed: &ValidatedCompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().types().get(owner_template)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    if concrete_owner.type_parameters().len() != concrete_arguments.len() {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let substitutions = concrete_owner
        .type_parameters()
        .iter()
        .copied()
        .zip(concrete_arguments.iter().copied())
        .collect::<BTreeMap<_, _>>();
    let arguments = arguments
        .iter()
        .map(|argument| {
            resolve_delegated_dispatch_owner_argument(
                typed,
                *argument,
                &substitutions,
                span,
                &mut BTreeSet::new(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((*declaration, arguments))
}

/// 只在 frontend 已发布的 dispatch owner template 内递归替换现行 runtime recipe。
pub(super) fn resolve_delegated_dispatch_owner_argument(
    typed: &ValidatedCompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
    visiting: &mut BTreeSet<UnitTypeId>,
) -> Result<UnitTypeId, LoweringError> {
    if !visiting.insert(ty) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    match typed.types().types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        }) => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let argument = resolve_delegated_dispatch_owner_argument(
                typed,
                *argument,
                substitutions,
                span,
                visiting,
            )?;
            typed
                .types()
                .types()
                .find(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) if typed
            .types()
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| {
                nominal.kind() == NominalKind::Class && nominal.type_parameters().len() == 1
            }) =>
        {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let argument = resolve_delegated_dispatch_owner_argument(
                typed,
                *argument,
                substitutions,
                span,
                visiting,
            )?;
            typed
                .types()
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: *declaration,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
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

/// 把 nominal runtime fields 解析为当前 concrete owner instance 的存储类型。
///
/// 优先消费 SPEC-0219 按 exact owner 发布的 descriptor。字段替换中新 intern、未进入 frontend
/// owner 候选快照的 nested owner 只允许回退到 closed/direct type-parameter 旧规则。
pub(crate) fn resolve_nominal_runtime_field_types(
    typed: &ValidatedCompilationUnitTypes,
    owner_type: UnitTypeId,
    nominal: &UnitNominalSignature,
    arguments: &[UnitTypeId],
) -> Result<Vec<UnitTypeId>, LoweringError> {
    let owner_matches = matches!(
        typed.types().types().get(owner_type),
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments: owner_arguments,
        }) if *declaration == nominal.declaration() && owner_arguments == arguments
    );
    if !owner_matches {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        });
    }
    if nominal.type_parameters().len() != arguments.len() {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        });
    }
    if let Some(layout) = typed.types().runtime_field_layout(owner_type) {
        if layout.declaration() != nominal.declaration()
            || layout.arguments() != arguments
            || layout.fields().len() != nominal.fields().len()
        {
            return Err(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            });
        }
        return layout
            .fields()
            .iter()
            .zip(nominal.fields())
            .map(|(actual, template)| {
                if actual.symbol() != template.symbol()
                    || actual.template_type() != template.ty()
                    || actual.span() != template.span()
                    || typed.types().types().get(actual.concrete_type()).is_none()
                {
                    Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        template.span(),
                    ))
                } else if !supported_nested_runtime_field_recipe(typed, nominal, template.ty()) {
                    Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        template.span(),
                    ))
                } else {
                    Ok(actual.concrete_type())
                }
            })
            .collect();
    }
    nominal
        .fields()
        .iter()
        .map(|field| match typed.types().types().get(field.ty()) {
            Some(UnitTypeKind::TypeParameter(parameter)) => nominal
                .type_parameters()
                .iter()
                .position(|candidate| candidate == parameter)
                .and_then(|index| arguments.get(index).copied())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, field.span())),
            Some(kind) if contains_type_parameter(typed, kind) => Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                field.span(),
            )),
            Some(_) => Ok(field.ty()),
            None => Err(lowering_error(LoweringErrorKind::MissingFact, field.span())),
        })
        .collect()
}

/// SPEC-0191 当前只递归接受由 `List`、单参数 ordinary class 与 direct owner parameter 组成的
/// 有限 recipe；nullable/function/其他 intrinsic 与非 class nominal 继续保持门禁。
fn supported_nested_runtime_field_recipe(
    typed: &ValidatedCompilationUnitTypes,
    owner: &UnitNominalSignature,
    template: UnitTypeId,
) -> bool {
    if let Some(UnitTypeKind::Nullable(inner)) = typed.types().types().get(template) {
        return direct_owner_type_parameter(typed, owner, *inner);
    }
    supported_nested_runtime_field_recipe_with(typed, owner, template, &mut BTreeSet::new())
}

fn supported_nested_runtime_field_recipe_with(
    typed: &ValidatedCompilationUnitTypes,
    owner: &UnitNominalSignature,
    template: UnitTypeId,
    visiting: &mut BTreeSet<UnitTypeId>,
) -> bool {
    if !visiting.insert(template) {
        return false;
    }
    let Some(kind) = typed.types().types().get(template) else {
        return false;
    };
    if !contains_type_parameter(typed, kind) {
        return true;
    }
    match kind {
        UnitTypeKind::TypeParameter(parameter) => owner.type_parameters().contains(parameter),
        UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        } => matches!(
            arguments.as_slice(),
            [argument]
                if supported_nested_runtime_field_recipe_with(
                    typed,
                    owner,
                    *argument,
                    visiting,
                )
        ),
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            matches!(
                arguments.as_slice(),
                [argument]
                    if supported_nested_runtime_field_recipe_with(
                        typed,
                        owner,
                        *argument,
                        visiting,
                    )
            ) && typed
                .types()
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nested| {
                    nested.kind() == NominalKind::Class && nested.type_parameters().len() == 1
                })
        }
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Nullable(_)
        | UnitTypeKind::Function { .. }
        | UnitTypeKind::Intrinsic { .. }
        | UnitTypeKind::EnumCase { .. }
        | UnitTypeKind::StaticSelf(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => false,
    }
}

fn direct_owner_type_parameter(
    typed: &ValidatedCompilationUnitTypes,
    owner: &UnitNominalSignature,
    ty: UnitTypeId,
) -> bool {
    matches!(
        typed.types().types().get(ty),
        Some(UnitTypeKind::TypeParameter(parameter))
            if owner.type_parameters().contains(parameter)
    )
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
