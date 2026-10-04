//! SPEC-0199 compilation-unit 可达 callable 与具体实例的确定性计划。

mod call_routes;
mod concrete_types;
mod deinit;
mod recipe_preflight;
mod recipe_validation;
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
        CompilationUnitTypes, NominalKind, TypeEnvironment, UnitCallTarget, UnitCallableSignature,
        UnitCallableTarget, UnitNominalSignature, UnitTypeId, UnitTypeKind,
        ValidatedCompilationUnitTypes,
    },
};

use super::{
    LoweringError, LoweringErrorKind, lowering_support::error as lowering_error,
    unit_source_query::parsed_by_source_unit,
};
pub(crate) use call_routes::{callable_static_self_receiver, resolve_unit_call_instance};
#[cfg(test)]
pub(super) use call_routes::{
    resolve_delegated_dispatch_owner_argument, resolve_inherited_dispatch_owner_argument,
};
use concrete_types::contains_type_parameter;
pub(crate) use concrete_types::resolve_concrete_type;
use recipe_preflight::{
    collect_frontier_unit_recipe_failures, pending_recipe_frontier, preflight_unit_call_recipes,
    stable_recipe_failure,
};
use recipe_validation::recipe_root_facts_for_instance;
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
    let parsed_by_source = parsed_by_source_unit(unit.inputs(), unit.names())?;
    plan_unit_instances_from_facts(
        &parsed_by_source,
        unit.names(),
        unit.types(),
        unit.ownership(),
        entry,
        max_generic_instances,
    )
}

/// 仅供已通过入口身份校验的 lowering driver 复用；不发布新的 frontend capability。
pub(super) fn plan_unit_instances_from_facts(
    parsed_by_source: &[&ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    entry: DeclarationId,
    max_generic_instances: usize,
) -> Result<UnitInstancePlan, LoweringError> {
    let templates = collect_templates(names, typed, parsed_by_source)?;
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

    let calls_by_template = index_calls(typed, parsed_by_source, &templates)?;
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
        parsed_by_source,
        &instances,
        &mut runtime_type_demands,
    )?;
    Ok(UnitInstancePlan {
        instances,
        runtime_type_demands,
    })
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

fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}
