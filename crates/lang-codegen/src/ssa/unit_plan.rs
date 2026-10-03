//! SPEC-0199 compilation-unit 可达 callable 与具体实例的确定性计划。

mod deinit;
mod runtime_layout;

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Deref,
};

use lang_frontend::{
    ast::ItemId,
    name_resolution::{
        DeclarationId, SourceUnitId, SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, ValidatedCompilationUnitOwnership, owned_compilation_unit_view,
    },
    parser::{Item, NameMarker, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, IntrinsicTypeConstructor, NominalKind, TypeEnvironment,
        UnitCallTarget, UnitCallableSignature, UnitCallableTarget, UnitNominalSignature,
        UnitTypeId, UnitTypeKind, ValidatedCompilationUnitTypes,
    },
};

use super::{LoweringError, LoweringErrorKind, lowering_support::error as lowering_error};
use runtime_layout::classify_runtime_type_demands;
pub(crate) use runtime_layout::resolve_nominal_runtime_field_types;

/// 防止 unit-wide 泛型实例图被合法但病态的源码无界扩张。
pub(super) const MAX_UNIT_GENERIC_INSTANCES: usize = 1024;

/// 一个 unit-wide 具体函数实例的规范 identity。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UnitFunctionInstanceKey {
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    static_self: Option<UnitTypeId>,
    /// Hidden deinit uses its nominal declaration as identity, never a source callable.
    deinit: bool,
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
    dependent_owner_types: BTreeSet<UnitTypeId>,
}

impl ResolvedUnitCallInstance {
    pub(crate) const fn key(&self) -> &UnitFunctionInstanceKey {
        &self.key
    }

    pub(crate) fn delegation(&self) -> &[UnitDelegatedCallRoute] {
        &self.delegation
    }
}

/// dependent inherited owner concrete type 在当前 unit plan 中的最强运行时需求。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UnitRuntimeTypeDemand {
    InstanceKeyOnly,
    RuntimeLayoutRequired,
}

/// 可达 callable 实例与 dependent owner 的确定性 strongest-demand 计划。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UnitInstancePlan {
    instances: Vec<UnitPlannedInstance>,
    runtime_type_demands: BTreeMap<UnitTypeId, UnitRuntimeTypeDemand>,
}

#[derive(Clone, Copy, Debug)]
struct UnitRecipeFailure {
    root: UnitSymbolId,
    error: LoweringError,
}

type UnitRecipeRootFacts = BTreeMap<UnitSymbolId, BTreeSet<DeclarationId>>;

impl UnitInstancePlan {
    pub(crate) fn runtime_type_demand(&self, ty: UnitTypeId) -> Option<UnitRuntimeTypeDemand> {
        self.runtime_type_demands.get(&ty).copied()
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        Vec<UnitPlannedInstance>,
        BTreeMap<UnitTypeId, UnitRuntimeTypeDemand>,
    ) {
        (self.instances, self.runtime_type_demands)
    }
}

impl Deref for UnitInstancePlan {
    type Target = [UnitPlannedInstance];

    fn deref(&self) -> &Self::Target {
        &self.instances
    }
}

impl IntoIterator for UnitInstancePlan {
    type Item = UnitPlannedInstance;
    type IntoIter = std::vec::IntoIter<UnitPlannedInstance>;

    fn into_iter(self) -> Self::IntoIter {
        self.instances.into_iter()
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
            deinit: false,
        }
    }

    fn for_deinit(owner: DeclarationId) -> Self {
        Self {
            target: UnitCallableTarget::Declaration(owner),
            type_arguments: Vec::new(),
            static_self: None,
            deinit: true,
        }
    }

    pub(crate) const fn deinit_owner(&self) -> Option<DeclarationId> {
        match (self.deinit, self.target) {
            (true, UnitCallableTarget::Declaration(owner)) => Some(owner),
            _ => None,
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
    deinit: bool,
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
) -> Result<UnitInstancePlan, LoweringError> {
    plan_unit_instances_with_limit(
        sources,
        inputs,
        names,
        environment,
        typed,
        owned,
        entry,
        MAX_UNIT_GENERIC_INSTANCES,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_unit_instances_with_limit(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
    max_generic_instances: usize,
) -> Result<UnitInstancePlan, LoweringError> {
    let unit = owned_compilation_unit_view(sources, inputs, names, environment, typed, owned)?;
    plan_unit_instances_from_facts(
        unit.inputs(),
        unit.names(),
        unit.types(),
        unit.ownership(),
        entry,
        max_generic_instances,
    )
}

/// 仅供已通过入口身份校验的 lowering driver 复用；不发布新的 frontend capability。
pub(super) fn plan_unit_instances_from_facts(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    entry: DeclarationId,
    max_generic_instances: usize,
) -> Result<UnitInstancePlan, LoweringError> {
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
    if templates[entry_template].deinit || !templates[entry_template].type_parameters.is_empty() {
        return Err(lowering_error(
            LoweringErrorKind::UnsupportedNode,
            templates[entry_template].span,
        ));
    }

    let calls_by_template = index_calls(typed, &parsed_by_source, &templates)?;
    let mut pending = BTreeSet::from([UnitFunctionInstanceKey::new(entry, Vec::new())]);
    let mut planned = BTreeMap::new();
    let mut runtime_type_demands = BTreeMap::new();
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
        if key.deinit != template.deinit {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                template.span,
            ));
        }
        let requires_static_self =
            !key.deinit && callable_static_self_receiver(typed, key.target())?;
        if requires_static_self != key.static_self().is_some() {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                template.span,
            ));
        }
        if key.is_specialized() && generic_instance_count >= max_generic_instances {
            let recipe_frontier = pending_recipe_frontier(&key, &pending, &planned);
            let mut recipe_failures = collect_frontier_unit_recipe_failures(
                typed,
                owned,
                &templates,
                &template_by_target,
                &calls_by_template,
                &recipe_frontier,
            )?;
            if !recipe_failures.is_empty() {
                return Err(stable_recipe_failure(&mut recipe_failures));
            }
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
        let instance_recipe_facts =
            recipe_root_facts_for_instance(typed, &template.type_parameters, key.type_arguments())?;

        for (call_index, span) in &calls_by_template[template_index] {
            let call = &typed.calls()[*call_index];
            let target = match call.target() {
                UnitCallTarget::Declaration(declaration) => {
                    UnitCallableTarget::Declaration(declaration)
                }
                UnitCallTarget::Symbol(symbol) => UnitCallableTarget::Symbol(symbol),
                UnitCallTarget::External(_)
                | UnitCallTarget::FunctionValue
                | UnitCallTarget::StructuralComponent(_) => continue,
            };
            let mut call_recipe_failures = Vec::new();
            preflight_unit_call_recipes(
                typed,
                owned,
                *call_index,
                *span,
                (
                    key.static_self(),
                    Some(&substitutions),
                    &instance_recipe_facts,
                ),
                &mut call_recipe_failures,
            )?;
            if !call_recipe_failures.is_empty() {
                let recipe_frontier = pending_recipe_frontier(&key, &pending, &planned);
                let mut recipe_failures = collect_frontier_unit_recipe_failures(
                    typed,
                    owned,
                    &templates,
                    &template_by_target,
                    &calls_by_template,
                    &recipe_frontier,
                )?;
                recipe_failures.extend(call_recipe_failures);
                return Err(stable_recipe_failure(&mut recipe_failures));
            }
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
            for dependent in target_key.dependent_owner_types {
                runtime_type_demands
                    .entry(dependent)
                    .or_insert(UnitRuntimeTypeDemand::InstanceKeyOnly);
            }
            pending.insert(target_key.key);
        }

        deinit::plan_instance_deinits(
            typed,
            names,
            parsed_by_source[template.source_unit.index()],
            template,
            &key,
            &substitutions,
            &mut pending,
        )?;

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

    let instances = planned.into_values().collect::<Vec<_>>();
    classify_runtime_type_demands(
        typed,
        &parsed_by_source,
        &instances,
        &mut runtime_type_demands,
    )?;
    Ok(UnitInstancePlan {
        instances,
        runtime_type_demands,
    })
}

fn dependent_inherited_owner_types(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: &[UnitTypeId],
    receiver: Option<UnitTypeId>,
    span: Span,
) -> Result<BTreeSet<UnitTypeId>, LoweringError> {
    let Some(receiver_declaration) = receiver.and_then(|receiver| {
        let UnitTypeKind::Nominal { declaration, .. } = typed.types().get(receiver)? else {
            return None;
        };
        Some(*declaration)
    }) else {
        return Ok(BTreeSet::new());
    };
    let Some((target_owner, owner_arity)) = unit_callable_owner(typed, target) else {
        return Ok(BTreeSet::new());
    };
    if target_owner == receiver_declaration || type_arguments.len() < owner_arity {
        return Ok(BTreeSet::new());
    }
    let mut roots = type_arguments
        .iter()
        .take(owner_arity)
        .filter_map(|&ty| {
            let UnitTypeKind::Nominal { declaration, .. } = typed.types().get(ty)? else {
                return None;
            };
            let nominal = typed.signatures().declaration(*declaration)?.nominal()?;
            Some((nominal.symbol(), *declaration, ty))
        })
        .collect::<Vec<_>>();
    // UnitSymbolId 携带 compilation index 规范化的 SourceUnitId 与源码内 symbol 顺序；
    // 以 concrete type 破同 declaration 的平局，避免 owner slot 顺序选择 witness。
    roots.sort_unstable();

    let mut dependent = BTreeSet::new();
    for (_, declaration, ty) in roots {
        let nominal = typed
            .signatures()
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal.fields().iter().any(|field| {
            typed
                .types()
                .get(field.ty())
                .is_some_and(|kind| contains_type_parameter(typed, kind))
        }) {
            validate_dependent_inherited_nominal_recipe(
                typed,
                nominal,
                span,
                &mut BTreeSet::new(),
            )?;
            dependent.insert(ty);
        }
    }
    Ok(dependent)
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
    typed: &CompilationUnitTypes,
    parsed_by_source: &[&ParsedFile],
) -> Result<Vec<UnitFunctionTemplate>, LoweringError> {
    let signatures = typed.signatures();
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
                deinit: false,
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
        if let Some(template) = deinit::template(nominal, parsed)? {
            templates.push(template);
        }
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
                deinit: false,
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
    typed: &CompilationUnitTypes,
    parsed_by_source: &[&ParsedFile],
    templates: &[UnitFunctionTemplate],
) -> Result<Vec<Vec<(usize, Span)>>, LoweringError> {
    let mut calls = vec![Vec::new(); templates.len()];
    for (call_index, call) in typed.calls().iter().enumerate() {
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

/// 在当前 concrete key 命中实例上限时，只沿可达 callable template 图收集 recipe failure。
///
/// 图节点只包含有限的 callable identity 与 frontend 已发布的 `StaticSelf` type identity，
/// 不创建变化后的 type arguments，因此不会被 generic instance 上限或 pending key 顺序截断。
/// 普通 lowering 错误仍由正式 planner 的 concrete frontier 报告。
fn pending_recipe_frontier(
    current: &UnitFunctionInstanceKey,
    pending: &BTreeSet<UnitFunctionInstanceKey>,
    planned: &BTreeMap<UnitFunctionInstanceKey, UnitPlannedInstance>,
) -> Vec<UnitFunctionInstanceKey> {
    std::iter::once(current.clone())
        .chain(
            pending
                .iter()
                .filter(|candidate| !planned.contains_key(*candidate))
                .cloned(),
        )
        .collect()
}

fn preflight_unit_call_recipes(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    call_index: usize,
    span: Span,
    context: (
        Option<UnitTypeId>,
        Option<&BTreeMap<UnitSymbolId, UnitTypeId>>,
        &UnitRecipeRootFacts,
    ),
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<Option<ResolvedUnitCallInstance>, LoweringError> {
    let (static_self, substitutions, facts) = context;
    let call = &typed.calls()[call_index];
    let target = match call.target() {
        UnitCallTarget::Declaration(declaration) => UnitCallableTarget::Declaration(declaration),
        UnitCallTarget::Symbol(symbol) => UnitCallableTarget::Symbol(symbol),
        UnitCallTarget::External(_)
        | UnitCallTarget::FunctionValue
        | UnitCallTarget::StructuralComponent(_) => return Ok(None),
    };
    let specialized_type_arguments = call
        .instance()
        .type_arguments()
        .iter()
        .map(|&ty| specialize_preflight_type(typed, ty, static_self, substitutions))
        .collect::<Option<Vec<_>>>();
    let specialized_receiver = match call.receiver() {
        Some(receiver) => {
            specialize_preflight_type(typed, receiver.ty(), static_self, substitutions).map(Some)
        }
        None => Some(None),
    };
    if let (Some(type_arguments), Some(receiver)) =
        (specialized_type_arguments.as_ref(), specialized_receiver)
    {
        let failures_before = failures.len();
        let has_unresolved_type = type_arguments.iter().copied().chain(receiver).any(|ty| {
            typed
                .types()
                .get(ty)
                .is_none_or(|kind| contains_type_parameter(typed, kind))
        });
        let resolved = resolve_unit_call_instance_with_recipe_failures(
            typed,
            owned,
            target,
            type_arguments.clone(),
            receiver,
            span,
            facts,
            failures,
        );
        match resolved {
            Ok(resolved) => {
                let effective_receiver = resolved
                    .key()
                    .static_self()
                    .or_else(|| {
                        resolved
                            .delegation()
                            .last()
                            .map(|route| route.delegate_receiver())
                    })
                    .or(receiver);
                if preflight_direct_inherited_owner_recipes_with_facts(
                    typed,
                    resolved.key().target(),
                    resolved.key().type_arguments(),
                    effective_receiver,
                    facts,
                    failures,
                )
                .is_err()
                {
                    return Ok(None);
                }
                if failures.len() > failures_before {
                    return Ok(None);
                }
                return Ok(Some(resolved));
            }
            Err(_) if failures.len() > failures_before || !has_unresolved_type => {
                return Ok(None);
            }
            Err(_) => {}
        }
    }

    let preflight_type_arguments = specialized_type_arguments
        .as_deref()
        .unwrap_or_else(|| call.instance().type_arguments());
    let preflight_receiver = specialized_receiver
        .flatten()
        .or_else(|| call.receiver().map(|receiver| receiver.ty()));
    let delegation_preflight = preflight_delegation_endpoint_owner_recipes(
        typed,
        target,
        preflight_receiver,
        facts,
        failures,
        span,
    );
    let Ok(has_delegation_route) = delegation_preflight else {
        return Ok(None);
    };
    if !has_delegation_route
        && preflight_direct_inherited_owner_recipes_with_facts(
            typed,
            target,
            preflight_type_arguments,
            preflight_receiver,
            facts,
            failures,
        )
        .is_err()
    {
        return Ok(None);
    }
    Ok(None)
}

/// 只消费 frontend 已选定的 delegation endpoint；local override 不需要 inherited owner recipe。
fn preflight_delegation_endpoint_owner_recipes(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    receiver: Option<UnitTypeId>,
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
    span: Span,
) -> Result<bool, LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration: receiver_declaration,
        arguments: receiver_arguments,
    }) = receiver.and_then(|receiver| typed.types().get(receiver))
    else {
        return Ok(false);
    };
    let receiver_nominal = typed
        .signatures()
        .declaration(*receiver_declaration)
        .and_then(|signature| signature.nominal())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let mut current_owner = *receiver_declaration;
    let mut current_target = target;
    let mut current_facts = extend_recipe_root_facts(
        typed,
        receiver_nominal.type_parameters(),
        receiver_arguments,
        facts,
    )?;
    let mut visited = BTreeSet::new();
    let mut found_route = false;

    loop {
        if !visited.insert((current_owner, current_target)) {
            return Ok(found_route);
        }
        let routes = typed
            .signatures()
            .delegations()
            .iter()
            .filter(|plan| plan.owner() == current_owner)
            .flat_map(|plan| {
                plan.forwarders()
                    .iter()
                    .filter(move |forwarder| forwarder.requirement() == current_target)
                    .map(move |forwarder| (plan, forwarder))
            })
            .collect::<Vec<_>>();
        let [(route, forwarder)] = routes.as_slice() else {
            return Ok(found_route || !routes.is_empty());
        };
        found_route = true;
        let Some((delegate_declaration, delegate_arguments)) = typed
            .signatures()
            .declaration(current_owner)
            .and_then(|signature| signature.nominal())
            .and_then(|nominal| {
                nominal
                    .fields()
                    .iter()
                    .find(|field| field.symbol() == route.target())
            })
            .and_then(|field| match typed.types().get(field.ty()) {
                Some(UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                }) => Some((*declaration, arguments.as_slice())),
                _ => None,
            })
        else {
            return Ok(true);
        };
        let delegate_nominal = typed
            .signatures()
            .declaration(delegate_declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let delegate_facts = extend_recipe_root_facts(
            typed,
            delegate_nominal.type_parameters(),
            delegate_arguments,
            &current_facts,
        )?;
        if let Some(next_hop) = forwarder.next_hop() {
            current_owner = delegate_declaration;
            current_target = next_hop.requirement();
            current_facts = delegate_facts;
            continue;
        }
        let Some(implementation) = forwarder.implementation() else {
            return Ok(true);
        };
        let Some((implementation_owner, _)) = unit_callable_owner(typed, implementation.target())
        else {
            return Ok(true);
        };
        if implementation_owner != delegate_declaration {
            preflight_owner_template_recipes(
                typed,
                implementation.receiver_type(),
                &delegate_facts,
                failures,
                span,
            )?;
        }
        return Ok(true);
    }
}

fn collect_frontier_unit_recipe_failures(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    templates: &[UnitFunctionTemplate],
    template_by_target: &BTreeMap<UnitCallableTarget, usize>,
    calls_by_template: &[Vec<(usize, Span)>],
    frontier: &[UnitFunctionInstanceKey],
) -> Result<Vec<UnitRecipeFailure>, LoweringError> {
    let mut failures = Vec::new();
    for candidate in frontier {
        let Some(&template_index) = template_by_target.get(&candidate.target()) else {
            continue;
        };
        let template = &templates[template_index];
        if template.type_parameters.len() != candidate.type_arguments().len() {
            continue;
        }
        let facts = recipe_root_facts_for_instance(
            typed,
            &template.type_parameters,
            candidate.type_arguments(),
        )?;
        let substitutions = template
            .type_parameters
            .iter()
            .copied()
            .zip(candidate.type_arguments().iter().copied())
            .collect::<BTreeMap<_, _>>();
        for (call_index, span) in &calls_by_template[template_index] {
            preflight_unit_call_recipes(
                typed,
                owned,
                *call_index,
                *span,
                (candidate.static_self(), Some(&substitutions), &facts),
                &mut failures,
            )?;
        }
        failures.extend(collect_reachable_unit_recipe_failures(
            typed,
            owned,
            templates,
            template_by_target,
            calls_by_template,
            (template_index, candidate.static_self(), facts),
        )?);
    }
    Ok(failures)
}

fn collect_reachable_unit_recipe_failures(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    templates: &[UnitFunctionTemplate],
    template_by_target: &BTreeMap<UnitCallableTarget, usize>,
    calls_by_template: &[Vec<(usize, Span)>],
    entry: (usize, Option<UnitTypeId>, UnitRecipeRootFacts),
) -> Result<Vec<UnitRecipeFailure>, LoweringError> {
    let (entry_template, entry_static_self, entry_facts) = entry;
    let entry_context = (entry_template, entry_static_self);
    let mut pending = BTreeSet::from([entry_context]);
    let mut facts_by_context = BTreeMap::from([(entry_context, entry_facts)]);
    let mut failures = Vec::new();

    while let Some((template_index, static_self)) = pending.pop_first() {
        let facts = facts_by_context
            .get(&(template_index, static_self))
            .cloned()
            .unwrap_or_default();
        for (call_index, span) in &calls_by_template[template_index] {
            let Some(resolved) = preflight_unit_call_recipes(
                typed,
                owned,
                *call_index,
                *span,
                (static_self, None, &facts),
                &mut failures,
            )?
            else {
                continue;
            };

            let Some(&target_template) = template_by_target.get(&resolved.key().target()) else {
                continue;
            };
            if templates[target_template].type_parameters.len()
                != resolved.key().type_arguments().len()
            {
                continue;
            }
            let next_roots = templates[target_template]
                .type_parameters
                .iter()
                .zip(resolved.key().type_arguments())
                .map(|(&parameter, &argument)| {
                    recipe_root_declarations_in_types(typed, &[argument], &facts)
                        .map(|roots| (parameter, roots))
                })
                .collect::<Result<Vec<_>, _>>();
            let Ok(next_roots) = next_roots else {
                continue;
            };
            let mut next_facts = facts.clone();
            for (parameter, roots) in next_roots {
                next_facts.entry(parameter).or_default().extend(roots);
            }
            let context = (target_template, resolved.key().static_self());
            let is_new = !facts_by_context.contains_key(&context);
            let stored = facts_by_context.entry(context).or_default();
            let mut changed = false;
            for (parameter, roots) in next_facts {
                let current = stored.entry(parameter).or_default();
                let previous = current.len();
                current.extend(roots);
                changed |= current.len() != previous;
            }
            if is_new || changed {
                pending.insert(context);
            }
        }
    }

    Ok(failures)
}

fn stable_recipe_failure(failures: &mut [UnitRecipeFailure]) -> LoweringError {
    failures.sort_unstable_by_key(|failure| failure.root);
    failures[0].error
}

fn recipe_root_facts_for_instance(
    typed: &CompilationUnitTypes,
    parameters: &[UnitSymbolId],
    arguments: &[UnitTypeId],
) -> Result<UnitRecipeRootFacts, LoweringError> {
    let empty = UnitRecipeRootFacts::new();
    extend_recipe_root_facts(typed, parameters, arguments, &empty)
}

fn extend_recipe_root_facts(
    typed: &CompilationUnitTypes,
    parameters: &[UnitSymbolId],
    arguments: &[UnitTypeId],
    facts: &UnitRecipeRootFacts,
) -> Result<UnitRecipeRootFacts, LoweringError> {
    if parameters.len() != arguments.len() {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        });
    }
    let additions = parameters
        .iter()
        .copied()
        .zip(arguments.iter().copied())
        .map(|(parameter, argument)| {
            recipe_root_declarations_in_types(typed, &[argument], facts)
                .map(|roots| (parameter, roots))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut extended = facts.clone();
    for (parameter, roots) in additions {
        extended.entry(parameter).or_default().extend(roots);
    }
    Ok(extended)
}

/// 只查找 frontend 已发布的 canonical specialization，不创建新的 concrete type。
fn specialize_preflight_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    static_self: Option<UnitTypeId>,
    substitutions: Option<&BTreeMap<UnitSymbolId, UnitTypeId>>,
) -> Option<UnitTypeId> {
    match typed.types().get(ty)? {
        UnitTypeKind::TypeParameter(parameter) => substitutions
            .and_then(|substitutions| substitutions.get(parameter).copied())
            .or(Some(ty)),
        UnitTypeKind::StaticSelf(_) => static_self,
        UnitTypeKind::Nullable(inner) => {
            let inner = specialize_preflight_type(typed, *inner, static_self, substitutions)?;
            typed.types().find(&UnitTypeKind::Nullable(inner))
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|&argument| {
                    specialize_preflight_type(typed, argument, static_self, substitutions)
                })
                .collect::<Option<Vec<_>>>()?;
            typed.types().find(&UnitTypeKind::Nominal {
                declaration: *declaration,
                arguments,
            })
        }
        UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|&argument| {
                    specialize_preflight_type(typed, argument, static_self, substitutions)
                })
                .collect::<Option<Vec<_>>>()?;
            typed.types().find(&UnitTypeKind::Intrinsic {
                constructor: *constructor,
                arguments,
            })
        }
        _ => Some(ty),
    }
}

pub(crate) fn resolve_concrete_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().get(ty) {
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
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().get(ty) {
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
    typed: &CompilationUnitTypes,
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
        typed.types().get(receiver.ty()),
        Some(UnitTypeKind::StaticSelf(_))
    ))
}

/// 把 typed call target 与 concrete receiver 解析为 planner/lowerer 共用的实例 identity。
pub(crate) fn resolve_unit_call_instance(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
) -> Result<ResolvedUnitCallInstance, LoweringError> {
    resolve_unit_call_instance_with_recipe_failures(
        typed,
        owned,
        target,
        type_arguments,
        receiver,
        span,
        &UnitRecipeRootFacts::new(),
        &mut Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_unit_call_instance_with_recipe_failures(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
    recipe_facts: &UnitRecipeRootFacts,
    recipe_failures: &mut Vec<UnitRecipeFailure>,
) -> Result<ResolvedUnitCallInstance, LoweringError> {
    macro_rules! direct {
        () => {
            resolve_direct_unit_call_instance(
                typed,
                target,
                type_arguments.clone(),
                receiver,
                span,
                recipe_failures,
            )
            .map(|key| ResolvedUnitCallInstance {
                key: key.0,
                delegation: Vec::new(),
                dependent_owner_types: key.1,
            })
        };
    }
    let Some(receiver) = receiver else {
        return direct!();
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
        }) = typed.types().get(current_receiver)
        else {
            return if delegation.is_empty() {
                direct!()
            } else {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            };
        };
        let nominal = typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let routes = typed
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
            [] if delegation.is_empty() => return direct!(),
            [] => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            [(route, forwarder)] => (*route, *forwarder),
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if !visited.insert((*declaration, current_target)) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let ownership_routes = owned
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
        }) = typed.types().get(delegate_receiver)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let delegate_nominal = typed
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
        let (declared_implementation_owner, declared_implementation_owner_arity) =
            unit_callable_owner(typed, implementation.target())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if declared_implementation_owner != *delegate {
            let failures_before = recipe_failures.len();
            preflight_owner_template_recipes(
                typed,
                implementation.receiver_type(),
                recipe_facts,
                recipe_failures,
                span,
            )?;
            if recipe_failures.len() > failures_before {
                return Err(recipe_failures[failures_before].error);
            }
        }
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
        let (key, dependent_owner_types) = resolve_direct_unit_call_instance(
            typed,
            implementation.target(),
            implementation_arguments,
            Some(delegate_receiver),
            span,
            recipe_failures,
        )?;
        return Ok(ResolvedUnitCallInstance {
            key,
            delegation,
            dependent_owner_types,
        });
    }
}

fn preflight_owner_template_recipes(
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
    span: Span,
) -> Result<(), LoweringError> {
    let Some(UnitTypeKind::Nominal { arguments, .. }) = typed.types().get(owner_template) else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    preflight_inherited_owner_recipes_with_facts(typed, arguments, facts, failures)
}

#[allow(clippy::too_many_arguments)]
fn remap_delegation_next_hop_arguments(
    typed: &CompilationUnitTypes,
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
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
    recipe_failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(UnitFunctionInstanceKey, BTreeSet<UnitTypeId>), LoweringError> {
    let callable = unit_callable_signature(typed, target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if callable.has_body() {
        let failures_before = recipe_failures.len();
        preflight_direct_inherited_owner_recipes(
            typed,
            target,
            &type_arguments,
            receiver,
            recipe_failures,
        )?;
        if recipe_failures.len() > failures_before {
            return Err(recipe_failures[failures_before].error);
        }
        let dependent_owner_types =
            dependent_inherited_owner_types(typed, target, &type_arguments, receiver, span)?;
        let static_self = if callable_static_self_receiver(typed, target)? {
            Some(receiver.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?)
        } else {
            None
        };
        return Ok((
            UnitFunctionInstanceKey::for_specialized_target(target, type_arguments, static_self),
            dependent_owner_types,
        ));
    }

    let receiver = receiver.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments: owner_arguments,
    }) = typed.types().get(receiver)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    let nominal = typed
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
    let mut dependent_owner_types = BTreeSet::new();
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
                &mut dependent_owner_types,
                recipe_failures,
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
    Ok((
        UnitFunctionInstanceKey::for_specialized_target(
            implementation,
            implementation_arguments,
            static_self,
        ),
        dependent_owner_types,
    ))
}

fn preflight_direct_inherited_owner_recipes(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: &[UnitTypeId],
    receiver: Option<UnitTypeId>,
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    preflight_direct_inherited_owner_recipes_with_facts(
        typed,
        target,
        type_arguments,
        receiver,
        &UnitRecipeRootFacts::new(),
        failures,
    )
}

fn preflight_direct_inherited_owner_recipes_with_facts(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: &[UnitTypeId],
    receiver: Option<UnitTypeId>,
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration: receiver_declaration,
        ..
    }) = receiver.and_then(|receiver| typed.types().get(receiver))
    else {
        return Ok(());
    };
    let Some((target_owner, owner_arity)) = unit_callable_owner(typed, target) else {
        return Ok(());
    };
    if target_owner == *receiver_declaration || type_arguments.len() < owner_arity {
        return Ok(());
    }
    preflight_inherited_owner_recipes_with_facts(
        typed,
        &type_arguments[..owner_arity],
        facts,
        failures,
    )
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
        UnitCallableTarget::Symbol(_) => typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .flat_map(|nominal| nominal.members().iter().chain(nominal.companion_members()))
            .find(|callable| callable.target() == target),
    }
}

fn unit_callable_owner(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
) -> Option<(DeclarationId, usize)> {
    match target {
        UnitCallableTarget::Declaration(_) => None,
        UnitCallableTarget::Symbol(_) => typed
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
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(owner_template)
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

/// 只为 frontend 已选定的 inherited effective implementation 实例化有限 owner recipe。
fn instantiate_inherited_dispatch_owner_arguments(
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
    dependent_owner_types: &mut BTreeSet<UnitTypeId>,
    recipe_failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(owner_template)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    if concrete_owner.type_parameters().len() != concrete_arguments.len() {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let failures_before = recipe_failures.len();
    preflight_inherited_owner_recipes(typed, arguments, recipe_failures)?;
    if recipe_failures.len() > failures_before {
        return Err(recipe_failures[failures_before].error);
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
                dependent_owner_types,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((*declaration, arguments))
}

/// 在按 owner slot 实例化前，以稳定 source identity 选择 dependent recipe 的失败 witness。
fn preflight_inherited_owner_recipes(
    typed: &CompilationUnitTypes,
    arguments: &[UnitTypeId],
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    preflight_inherited_owner_recipes_with_facts(
        typed,
        arguments,
        &UnitRecipeRootFacts::new(),
        failures,
    )
}

fn recipe_root_declarations_in_types(
    typed: &CompilationUnitTypes,
    arguments: &[UnitTypeId],
    facts: &UnitRecipeRootFacts,
) -> Result<BTreeSet<DeclarationId>, LoweringError> {
    fn collect(
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        facts: &UnitRecipeRootFacts,
        roots: &mut BTreeSet<DeclarationId>,
    ) -> Result<(), LoweringError> {
        match typed.types().get(ty) {
            Some(UnitTypeKind::TypeParameter(parameter)) => {
                if let Some(substituted) = facts.get(parameter) {
                    roots.extend(substituted);
                }
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                let nominal = typed
                    .signatures()
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .ok_or(LoweringError {
                        kind: LoweringErrorKind::MissingFact,
                        span: None,
                    })?;
                if nominal.kind() == NominalKind::Class && nominal.type_parameters().len() == 1 {
                    roots.insert(nominal.declaration());
                    if let [argument] = arguments.as_slice() {
                        collect(typed, *argument, facts, roots)?;
                    }
                }
            }
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            }) => {
                for &argument in arguments {
                    collect(typed, argument, facts, roots)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    let mut roots = BTreeSet::new();
    for &argument in arguments {
        collect(typed, argument, facts, &mut roots)?;
    }
    Ok(roots)
}

fn preflight_inherited_owner_recipes_with_facts(
    typed: &CompilationUnitTypes,
    arguments: &[UnitTypeId],
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    let declarations = recipe_root_declarations_in_types(typed, arguments, facts)?;
    let mut roots = declarations
        .into_iter()
        .map(|declaration| {
            typed
                .signatures()
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    roots.sort_unstable_by_key(|nominal| nominal.symbol());
    for nominal in roots {
        if let RecipeTraversal::Cycle(witness) =
            parameter_growing_recipe_witness(typed, nominal, &mut BTreeSet::new())?
        {
            failures.push(UnitRecipeFailure {
                root: nominal.symbol(),
                error: lowering_error(LoweringErrorKind::UnsupportedNode, witness),
            });
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecipeTraversal {
    Clean,
    Cycle(Span),
    Unsupported,
}

/// 只识别 ADR-0024 的 declaration back-edge；unsupported constructor 仍交给正式 resolver。
fn parameter_growing_recipe_witness(
    typed: &CompilationUnitTypes,
    nominal: &UnitNominalSignature,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<RecipeTraversal, LoweringError> {
    if nominal.kind() != NominalKind::Class || nominal.type_parameters().len() != 1 {
        return Ok(RecipeTraversal::Unsupported);
    }
    if !visiting.insert(nominal.declaration()) {
        return Ok(nominal
            .fields()
            .first()
            .map_or(RecipeTraversal::Clean, |field| {
                RecipeTraversal::Cycle(field.span())
            }));
    }

    fn in_type(
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        owner_parameter: UnitSymbolId,
        span: Span,
        visiting: &mut BTreeSet<DeclarationId>,
    ) -> Result<RecipeTraversal, LoweringError> {
        let kind = typed
            .types()
            .get(ty)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !contains_type_parameter(typed, kind) {
            return parameter_growing_closed_recipe_witness(typed, ty, span, visiting);
        }
        match kind {
            UnitTypeKind::TypeParameter(parameter) if *parameter == owner_parameter => {
                Ok(RecipeTraversal::Clean)
            }
            UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            } => {
                let [argument] = arguments.as_slice() else {
                    return Ok(RecipeTraversal::Unsupported);
                };
                in_type(typed, *argument, owner_parameter, span, visiting)
            }
            UnitTypeKind::Nominal {
                declaration,
                arguments,
            } => {
                let [argument] = arguments.as_slice() else {
                    return Ok(RecipeTraversal::Unsupported);
                };
                if visiting.contains(declaration) {
                    return Ok(RecipeTraversal::Cycle(span));
                }
                let nested = typed
                    .signatures()
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let result = parameter_growing_recipe_witness(typed, nested, visiting)?;
                if result != RecipeTraversal::Clean {
                    return Ok(result);
                }
                if matches!(
                    typed.types().get(*argument),
                    Some(UnitTypeKind::TypeParameter(parameter)) if *parameter == owner_parameter
                ) {
                    Ok(RecipeTraversal::Clean)
                } else {
                    Ok(RecipeTraversal::Unsupported)
                }
            }
            _ => Ok(RecipeTraversal::Unsupported),
        }
    }

    let owner_parameter = nominal.type_parameters()[0];
    let mut result = RecipeTraversal::Clean;
    for field in nominal.fields() {
        result = in_type(typed, field.ty(), owner_parameter, field.span(), visiting)?;
        if result != RecipeTraversal::Clean {
            break;
        }
    }
    visiting.remove(&nominal.declaration());
    Ok(result)
}

fn parameter_growing_closed_recipe_witness(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<RecipeTraversal, LoweringError> {
    let kind = typed
        .types()
        .get(ty)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match kind {
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            if !visiting.insert(*declaration) {
                return Ok(RecipeTraversal::Cycle(span));
            }
            let nominal = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let concrete_fields = if nominal.kind() != NominalKind::EnumClass
                && typed.runtime_field_layout(ty).is_some()
            {
                match resolve_nominal_runtime_field_types(typed, ty, nominal, arguments) {
                    Ok(fields) => fields
                        .into_iter()
                        .zip(nominal.fields())
                        .map(|(concrete, field)| (concrete, field.span()))
                        .collect::<Vec<_>>(),
                    Err(error) if error.kind == LoweringErrorKind::UnsupportedNode => {
                        visiting.remove(declaration);
                        return Ok(RecipeTraversal::Unsupported);
                    }
                    Err(error) => {
                        visiting.remove(declaration);
                        return Err(error);
                    }
                }
            } else {
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments.iter().copied())
                    .collect::<BTreeMap<_, _>>();
                let mut concrete = Vec::new();
                for (template, field_span) in nominal
                    .fields()
                    .iter()
                    .map(|field| (field.ty(), field.span()))
                    .chain(nominal.enum_cases().iter().flat_map(|case| {
                        case.payloads()
                            .iter()
                            .map(|payload| (payload.ty(), payload.span()))
                    }))
                {
                    match resolve_closed_recipe_type(typed, template, &substitutions, field_span) {
                        Ok(ty) => concrete.push((ty, field_span)),
                        Err(error) if error.kind == LoweringErrorKind::UnsupportedNode => {
                            visiting.remove(declaration);
                            return Ok(RecipeTraversal::Unsupported);
                        }
                        Err(error) => {
                            visiting.remove(declaration);
                            return Err(error);
                        }
                    }
                }
                concrete
            };
            let mut result = RecipeTraversal::Clean;
            for (concrete, field_span) in concrete_fields {
                result =
                    parameter_growing_closed_recipe_witness(typed, concrete, field_span, visiting)?;
                if result != RecipeTraversal::Clean {
                    break;
                }
            }
            visiting.remove(declaration);
            Ok(result)
        }
        UnitTypeKind::Intrinsic { arguments, .. } => {
            for &argument in arguments {
                let result =
                    parameter_growing_closed_recipe_witness(typed, argument, span, visiting)?;
                if result != RecipeTraversal::Clean {
                    return Ok(result);
                }
            }
            Ok(RecipeTraversal::Clean)
        }
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => {
            parameter_growing_closed_recipe_witness(typed, *inner, span, visiting)
        }
        UnitTypeKind::EnumCase { root, .. } => {
            parameter_growing_closed_recipe_witness(typed, *root, span, visiting)
        }
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Function { .. }
        | UnitTypeKind::TypeParameter(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => Ok(RecipeTraversal::Clean),
    }
}

pub(super) fn resolve_inherited_dispatch_owner_argument(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
    visiting: &mut BTreeSet<UnitTypeId>,
    dependent_owner_types: &mut BTreeSet<UnitTypeId>,
) -> Result<UnitTypeId, LoweringError> {
    if !visiting.insert(ty) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    match typed.types().get(ty) {
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
                dependent_owner_types,
            )?;
            typed
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
        }) => {
            let nominal = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if nominal.kind() != NominalKind::Class
                || nominal.type_parameters().len() != 1
                || arguments.len() != 1
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            validate_dependent_inherited_nominal_recipe(
                typed,
                nominal,
                span,
                &mut BTreeSet::new(),
            )?;
            let argument = resolve_inherited_dispatch_owner_argument(
                typed,
                arguments[0],
                substitutions,
                span,
                visiting,
                dependent_owner_types,
            )?;
            let concrete = typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: *declaration,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if nominal.fields().iter().any(|field| {
                typed
                    .types()
                    .get(field.ty())
                    .is_some_and(|kind| contains_type_parameter(typed, kind))
            }) {
                dependent_owner_types.insert(concrete);
            }
            Ok(concrete)
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

/// dependent inherited owner 只开放有限、非增长的单参数 ordinary-class field graph。
fn validate_dependent_inherited_nominal_recipe(
    typed: &CompilationUnitTypes,
    nominal: &UnitNominalSignature,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<(), LoweringError> {
    if nominal.kind() != NominalKind::Class || nominal.type_parameters().len() != 1 {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    if !visiting.insert(nominal.declaration()) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    let parameter = nominal.type_parameters()[0];
    for field in nominal.fields() {
        validate_dependent_inherited_field_recipe(
            typed,
            field.ty(),
            parameter,
            field.span(),
            visiting,
        )?;
    }
    visiting.remove(&nominal.declaration());
    Ok(())
}

fn validate_dependent_inherited_field_recipe(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    owner_parameter: UnitSymbolId,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<(), LoweringError> {
    let kind = typed
        .types()
        .get(ty)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if !contains_type_parameter(typed, kind) {
        return validate_closed_nominal_recipe_cycles(typed, ty, span, visiting);
    }
    match kind {
        UnitTypeKind::TypeParameter(parameter) if *parameter == owner_parameter => Ok(()),
        UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        } => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            validate_dependent_inherited_field_recipe(
                typed,
                *argument,
                owner_parameter,
                span,
                visiting,
            )
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            let nested = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            // ADR-0024：先沿有限 declaration/template graph 找第一条回边；不能因
            // concrete argument 形状复杂而在更早的边上产生不稳定 witness。
            validate_dependent_inherited_nominal_recipe(typed, nested, span, visiting)?;
            if !matches!(
                typed.types().get(*argument),
                Some(UnitTypeKind::TypeParameter(parameter)) if *parameter == owner_parameter
            ) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(())
        }
        _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

/// 闭合实际参数仍可能把同一 nominal declaration 带回 field graph；这类 SCC 没有
/// SPEC-0219 的有限 descriptor，不能因为字段不再含 owner parameter 而静默放行。
fn validate_closed_nominal_recipe_cycles(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<(), LoweringError> {
    let kind = typed
        .types()
        .get(ty)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match kind {
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            if !visiting.insert(*declaration) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let nominal = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if nominal.kind() != NominalKind::EnumClass && typed.runtime_field_layout(ty).is_some()
            {
                for (concrete, field) in
                    resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)?
                        .into_iter()
                        .zip(nominal.fields())
                {
                    validate_closed_nominal_recipe_cycles(typed, concrete, field.span(), visiting)?;
                }
                visiting.remove(declaration);
                return Ok(());
            }
            let substitutions = nominal
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments.iter().copied())
                .collect::<BTreeMap<_, _>>();
            for (template, field_span) in nominal
                .fields()
                .iter()
                .map(|field| (field.ty(), field.span()))
                .chain(nominal.enum_cases().iter().flat_map(|case| {
                    case.payloads()
                        .iter()
                        .map(|payload| (payload.ty(), payload.span()))
                }))
            {
                let concrete =
                    resolve_closed_recipe_type(typed, template, &substitutions, field_span)?;
                validate_closed_nominal_recipe_cycles(typed, concrete, field_span, visiting)?;
            }
            visiting.remove(declaration);
            Ok(())
        }
        UnitTypeKind::Intrinsic { arguments, .. } => {
            for &argument in arguments {
                validate_closed_nominal_recipe_cycles(typed, argument, span, visiting)?;
            }
            Ok(())
        }
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => {
            validate_closed_nominal_recipe_cycles(typed, *inner, span, visiting)
        }
        UnitTypeKind::EnumCase { root, .. } => {
            validate_closed_nominal_recipe_cycles(typed, *root, span, visiting)
        }
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Function { .. }
        | UnitTypeKind::TypeParameter(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => Ok(()),
    }
}

/// closed recipe 的 SCC 检查需要完整替换容器内参数，但不因此扩张通用 callable
/// specialization 支持面。
fn resolve_closed_recipe_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    let resolve_arguments = |arguments: &[UnitTypeId]| {
        arguments
            .iter()
            .map(|argument| resolve_closed_recipe_type(typed, *argument, substitutions, span))
            .collect::<Result<Vec<_>, _>>()
    };
    let concrete =
        match typed.types().get(ty) {
            Some(UnitTypeKind::TypeParameter(parameter)) => {
                return substitutions
                    .get(parameter)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span));
            }
            Some(UnitTypeKind::Nullable(inner)) => UnitTypeKind::Nullable(
                resolve_closed_recipe_type(typed, *inner, substitutions, span)?,
            ),
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => UnitTypeKind::Nominal {
                declaration: *declaration,
                arguments: resolve_arguments(arguments)?,
            },
            Some(UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            }) => UnitTypeKind::Intrinsic {
                constructor: *constructor,
                arguments: resolve_arguments(arguments)?,
            },
            Some(UnitTypeKind::EnumCase { case, root }) => UnitTypeKind::EnumCase {
                case: *case,
                root: resolve_closed_recipe_type(typed, *root, substitutions, span)?,
            },
            Some(UnitTypeKind::Function { .. }) => return Ok(ty),
            Some(kind) if contains_type_parameter(typed, kind) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Some(_) => return Ok(ty),
            None => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
    Ok(typed
        .types()
        .find(&concrete)
        // SCC 检查只依赖 declaration graph；frontend 未 intern 中间 closed
        // constructor 时保留模板 identity，不能把有限 DAG 误报成 missing fact。
        .unwrap_or(ty))
}

fn instantiate_delegated_dispatch_owner_arguments(
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(owner_template)
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
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
    visiting: &mut BTreeSet<UnitTypeId>,
) -> Result<UnitTypeId, LoweringError> {
    if !visiting.insert(ty) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    match typed.types().get(ty) {
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

fn contains_type_parameter(typed: &CompilationUnitTypes, kind: &UnitTypeKind) -> bool {
    let contains = |ty| {
        typed
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
