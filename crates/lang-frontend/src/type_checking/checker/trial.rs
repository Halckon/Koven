use crate::type_checking::IntegerOperationDescriptor;
use std::collections::BTreeMap;

use crate::{
    diagnostic::Diagnostic,
    name_resolution::ExternalSymbolId,
    type_checking::{
        AggregateProjectionDescriptor, CallDescriptor, ConstructionDescriptor,
        ContainerAppendDescriptor, ContainerClearDescriptor, ContainerConstructionDescriptor,
        ContainerRemoveAtDescriptor, ContainerSizeDescriptor, DestructuringDescriptor,
        ElementPlaceDescriptor, ExpressionCategory, NonNullUseDescriptor, NullComparisonDescriptor,
        OwnershipPrimitiveDescriptor, ParameterMode, RcOperationDescriptor,
        StringOperationDescriptor, TypeId, TypeTable,
    },
};

use super::{Checker, FlowKey};

/// 一个 callable candidate trial 可以改变的完整类型检查状态。
///
/// 声明索引、签名和名称解析事实在 trial 前已经封闭，因此不属于事务；表达式、局部 symbol、
/// flow、诊断和所有后续阶段可见 descriptor 必须一起快照，避免失败候选泄漏半成品事实。
#[derive(Clone)]
pub(super) struct TrialState {
    associated_constant_uses: BTreeMap<usize, crate::name_resolution::SymbolId>,
    types: TypeTable,
    expression_types: Vec<Option<TypeId>>,
    type_ref_types: Vec<Option<TypeId>>,
    symbol_types: Vec<Option<TypeId>>,
    parameter_modes: Vec<Option<ParameterMode>>,
    non_null_uses: Vec<NonNullUseDescriptor>,
    null_comparisons: Vec<NullComparisonDescriptor>,
    nullable_whens: Vec<crate::type_checking::NullableWhenDescriptor>,
    non_null_assertions: Vec<crate::type_checking::NonNullAssertionDescriptor>,
    external_types: BTreeMap<ExternalSymbolId, TypeId>,
    flow_facts: BTreeMap<FlowKey, TypeId>,
    flow_versions: BTreeMap<FlowKey, u64>,
    destructurings: Vec<DestructuringDescriptor>,
    iterations: Vec<crate::type_checking::SequentialIterationDescriptor>,
    expression_categories: Vec<ExpressionCategory>,
    calls: Vec<CallDescriptor>,
    constructions: Vec<ConstructionDescriptor>,
    aggregate_projections: Vec<AggregateProjectionDescriptor>,
    ownership_primitives: Vec<OwnershipPrimitiveDescriptor>,
    rc_operations: Vec<RcOperationDescriptor>,
    string_operations: Vec<StringOperationDescriptor>,
    integer_operations: Vec<IntegerOperationDescriptor>,
    container_constructions: Vec<ContainerConstructionDescriptor>,
    container_sizes: Vec<ContainerSizeDescriptor>,
    container_appends: Vec<ContainerAppendDescriptor>,
    container_clears: Vec<ContainerClearDescriptor>,
    container_remove_ats: Vec<ContainerRemoveAtDescriptor>,
    element_places: Vec<ElementPlaceDescriptor>,
    diagnostics: Vec<Diagnostic>,
}

impl Checker<'_> {
    pub(super) fn trial_state(&self) -> TrialState {
        TrialState {
            associated_constant_uses: self.associated_constant_uses.clone(),
            types: self.types.clone(),
            expression_types: self.expression_types.clone(),
            type_ref_types: self.type_ref_types.clone(),
            symbol_types: self.symbol_types.clone(),
            parameter_modes: self.parameter_modes.clone(),
            non_null_uses: self.non_null_uses.clone(),
            null_comparisons: self.null_comparisons.clone(),
            nullable_whens: self.nullable_whens.clone(),
            non_null_assertions: self.non_null_assertions.clone(),
            external_types: self.external_types.clone(),
            flow_facts: self.flow_facts.clone(),
            flow_versions: self.flow_versions.clone(),
            destructurings: self.destructurings.clone(),
            iterations: self.iterations.clone(),
            expression_categories: self.expression_categories.clone(),
            calls: self.calls.clone(),
            constructions: self.constructions.clone(),
            aggregate_projections: self.aggregate_projections.clone(),
            ownership_primitives: self.ownership_primitives.clone(),
            rc_operations: self.rc_operations.clone(),
            string_operations: self.string_operations.clone(),
            integer_operations: self.integer_operations.clone(),
            container_constructions: self.container_constructions.clone(),
            container_sizes: self.container_sizes.clone(),
            container_appends: self.container_appends.clone(),
            container_clears: self.container_clears.clone(),
            container_remove_ats: self.container_remove_ats.clone(),
            element_places: self.element_places.clone(),
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub(super) fn restore_trial_state(&mut self, state: TrialState) {
        self.associated_constant_uses = state.associated_constant_uses;
        self.types = state.types;
        self.expression_types = state.expression_types;
        self.type_ref_types = state.type_ref_types;
        self.symbol_types = state.symbol_types;
        self.parameter_modes = state.parameter_modes;
        self.non_null_uses = state.non_null_uses;
        self.null_comparisons = state.null_comparisons;
        self.nullable_whens = state.nullable_whens;
        self.non_null_assertions = state.non_null_assertions;
        self.external_types = state.external_types;
        self.flow_facts = state.flow_facts;
        self.flow_versions = state.flow_versions;
        self.destructurings = state.destructurings;
        self.iterations = state.iterations;
        self.expression_categories = state.expression_categories;
        self.calls = state.calls;
        self.constructions = state.constructions;
        self.aggregate_projections = state.aggregate_projections;
        self.ownership_primitives = state.ownership_primitives;
        self.rc_operations = state.rc_operations;
        self.string_operations = state.string_operations;
        self.integer_operations = state.integer_operations;
        self.container_constructions = state.container_constructions;
        self.container_sizes = state.container_sizes;
        self.container_appends = state.container_appends;
        self.container_clears = state.container_clears;
        self.container_remove_ats = state.container_remove_ats;
        self.element_places = state.element_places;
        self.diagnostics = state.diagnostics;
    }
}
