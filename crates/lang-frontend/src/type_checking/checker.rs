use crate::type_checking::IntegerOperationDescriptor;
mod argument_mapping;
mod callable;
mod constants;
mod construction;
mod container;
mod copyability;
mod delegation;
mod destructuring;
mod expression;
mod flow;
mod integer;
mod item;
mod iteration;
mod layout;
mod literal;
mod map;
mod members;
mod nominal;
mod ownership_primitives;
mod projection;
mod rc;
mod string;
mod trial;
mod type_inference;
mod type_ref;
mod when;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    name_resolution::{
        EnumCase, EnumCaseId, ExternalSymbolId, NameResolution, Namespace, ReferenceTarget,
        ScopeId, ScopeKind, SymbolId, SymbolKind,
    },
    parser::{ClassifierKind, Item, NameMarker, ParameterModeMarker, ParsedFile, SyntaxAst},
    source::{SourceMap, Span},
};

use super::ExpressionUse;
use super::{
    AggregateProjectionDescriptor, AggregateProjectionKind, AggregateProjectionReceiver,
    BuiltinType, CallDescriptor, CallReceiverDescriptor, CallReceiverOrigin, CallableDescriptor,
    CallableReceiverDescriptor, Capability, ConstructionDescriptor, ContainerAppendDescriptor,
    ContainerClearDescriptor, ContainerConstructionDescriptor, ContainerInsertAtDescriptor,
    ContainerRemoveAtDescriptor, ContainerRemoveFirstDescriptor, ContainerRemoveLastDescriptor,
    ContainerSizeDescriptor, Copyability, DeferredReason, DelegationForwarderDescriptor,
    DelegationPlan, DestructuringDescriptor, ElementPlaceDescriptor, EnumCaseDescriptor,
    EnvironmentFunction, EnvironmentType, ExpressionCategory, ExternalTypeBinding,
    FunctionParameterType, IntrinsicTypeConstructor, NominalDescriptor, NominalId, NominalKind,
    NonNullUseDescriptor, NullComparisonDescriptor, OwnershipPrimitiveDescriptor,
    ParameterBindingDescriptor, ParameterMode, RcOperationDescriptor, SequentialContainerKind,
    StringOperationDescriptor, TypeCheckingError, TypeEnvironment, TypeId, TypeKind,
    TypeParameterBound, TypeParameterDescriptor, TypeTable, TypedFile, TypedFileParts,
    collect_expression_uses,
};
use argument_mapping::{MappedParameter, MappingError, parameter_mode_span};
use flow::FlowKey;

#[derive(Clone, Copy)]
struct ExprCheck {
    ty: TypeId,
    falls_through: bool,
}

#[derive(Clone, Copy)]
struct StatementCheck {
    ty: TypeId,
    falls_through: bool,
}

#[derive(Clone, Copy)]
struct CallableContext {
    return_type: TypeId,
    annotation_span: Option<Span>,
    loop_base: usize,
}

/// 把声明侧源码 marker 规范化为唯一的 typed 参数契约。
const fn source_parameter_mode(marker: Option<ParameterModeMarker>) -> ParameterMode {
    match marker {
        None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
        Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
        Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
    }
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    environment: &TypeEnvironment,
) -> Result<TypedFile, TypeCheckingError> {
    sources.source_text(parsed.source_id())?;
    Checker::new(sources, parsed, names, environment)?.run()
}

struct Checker<'a> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    environment: &'a TypeEnvironment,
    name_analysis_owner: Arc<()>,
    types: TypeTable,
    expression_types: Vec<Option<TypeId>>,
    expression_uses: Vec<ExpressionUse>,
    type_ref_types: Vec<Option<TypeId>>,
    symbol_types: Vec<Option<TypeId>>,
    parameter_modes: Vec<Option<ParameterMode>>,
    non_null_uses: Vec<NonNullUseDescriptor>,
    null_comparisons: Vec<NullComparisonDescriptor>,
    nullable_whens: Vec<crate::type_checking::NullableWhenDescriptor>,
    non_null_assertions: Vec<crate::type_checking::NonNullAssertionDescriptor>,
    constant_dependencies: BTreeMap<SymbolId, Vec<SymbolId>>,
    constant_values: BTreeMap<SymbolId, super::constant_value::ConstValue>,
    constant_items: BTreeMap<SymbolId, ItemId>,
    constant_expressions: BTreeMap<SymbolId, Vec<ExpressionId>>,
    pending_constant_errors: BTreeMap<SymbolId, (Span, bool)>,
    rechecking_constants: bool,
    constants_checked: bool,
    constant_inputs_valid: bool,
    input_error_spans: Vec<Span>,
    associated_constant_uses: BTreeMap<usize, SymbolId>,
    associated_constants: BTreeMap<SymbolId, constants::AssociatedNamespace>,
    references: BTreeMap<(usize, usize, u8), ReferenceTarget>,
    symbols_by_span: BTreeMap<(usize, usize), SymbolId>,
    symbol_kinds: Vec<SymbolKind>,
    symbol_spans: Vec<Span>,
    component_type_spans: Vec<Option<Span>>,
    symbol_scopes: Vec<ScopeId>,
    scope_parents: Vec<Option<ScopeId>>,
    scope_kinds: Vec<ScopeKind>,
    mutable_symbols: BTreeSet<SymbolId>,
    captured_mutable_symbols: BTreeSet<SymbolId>,
    flow_facts: BTreeMap<FlowKey, TypeId>,
    flow_versions: BTreeMap<FlowKey, u64>,
    source_references: Vec<(ScopeId, ReferenceTarget)>,
    nominal_by_symbol: BTreeMap<SymbolId, NominalId>,
    nominal_by_scope: BTreeMap<ScopeId, NominalId>,
    classifier_scope_by_span: BTreeMap<(usize, usize), ScopeId>,
    nominals: Vec<NominalDescriptor>,
    type_parameters: Vec<TypeParameterDescriptor>,
    type_parameter_by_symbol: BTreeMap<SymbolId, usize>,
    delegations: Vec<DelegationPlan>,
    invalid_delegations: Vec<(NominalId, TypeId)>,
    typed_callables: Vec<CallableDescriptor>,
    enum_cases: Vec<EnumCaseDescriptor>,
    enum_case_by_id: BTreeMap<EnumCaseId, usize>,
    enum_case_by_type_symbol: BTreeMap<SymbolId, EnumCaseId>,
    enum_case_by_value_symbol: BTreeMap<SymbolId, EnumCaseId>,
    enum_case_by_payload_symbol: BTreeMap<SymbolId, EnumCaseId>,
    source_enum_cases: Vec<EnumCase>,
    interface_edge_spans: BTreeMap<(NominalId, NominalId), Span>,
    invalid_inline_nominals: BTreeSet<NominalId>,
    external_types: BTreeMap<ExternalSymbolId, TypeId>,
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
    container_remove_lasts: Vec<ContainerRemoveLastDescriptor>,
    container_remove_firsts: Vec<ContainerRemoveFirstDescriptor>,
    container_insert_ats: Vec<ContainerInsertAtDescriptor>,
    element_places: Vec<ElementPlaceDescriptor>,
    pub(super) map_descriptors: super::map_descriptor::MapDescriptors,
    callables: Vec<CallableContext>,
    loop_depth: usize,
    candidate_local_expected: bool,
    classifiers: Vec<TypeId>,
    current_receiver_mode: Option<ParameterMode>,
    diagnostics: Vec<Diagnostic>,
    builtin_arguments_code: DiagnosticCode,
    cannot_infer_code: DiagnosticCode,
    mismatch_code: DiagnosticCode,
    operands_code: DiagnosticCode,
    return_outside_code: DiagnosticCode,
    return_shape_code: DiagnosticCode,
    missing_return_code: DiagnosticCode,
    branch_type_code: DiagnosticCode,
    numeric_range_code: DiagnosticCode,
    type_argument_arity_code: DiagnosticCode,
    invalid_type_bound_code: DiagnosticCode,
    interface_runtime_value_code: DiagnosticCode,
    invalid_supertype_code: DiagnosticCode,
    interface_cycle_code: DiagnosticCode,
    type_argument_bound_code: DiagnosticCode,
    duplicate_callable_shape_code: DiagnosticCode,
    concrete_member_body_code: DiagnosticCode,
    interface_member_mismatch_code: DiagnosticCode,
    invalid_override_code: DiagnosticCode,
    missing_interface_member_code: DiagnosticCode,
    default_member_conflict_code: DiagnosticCode,
    invalid_delegation_target_code: DiagnosticCode,
    delegate_interface_mismatch_code: DiagnosticCode,
    delegation_member_conflict_code: DiagnosticCode,
    non_borrow_delegation_receiver_code: DiagnosticCode,
    enum_case_type_position_code: DiagnosticCode,
    invalid_type_test_code: DiagnosticCode,
    invalid_when_condition_code: DiagnosticCode,
    duplicate_when_else_code: DiagnosticCode,
    non_final_when_else_code: DiagnosticCode,
    duplicate_when_coverage_code: DiagnosticCode,
    non_exhaustive_when_code: DiagnosticCode,
    when_branch_type_code: DiagnosticCode,
    invalid_enum_payload_access_code: DiagnosticCode,
    copyable_type_argument_bound_code: DiagnosticCode,
    infinite_inline_layout_code: DiagnosticCode,
    invalid_box_argument_code: DiagnosticCode,
    destructuring_arity_code: DiagnosticCode,
    non_callable_target_code: DiagnosticCode,
    invalid_named_argument_code: DiagnosticCode,
    call_argument_arity_code: DiagnosticCode,
    call_argument_mode_code: DiagnosticCode,
    no_matching_overload_code: DiagnosticCode,
    ambiguous_call_code: DiagnosticCode,
    generic_call_inference_code: DiagnosticCode,
    invalid_construction_target_code: DiagnosticCode,
    construction_inference_code: DiagnosticCode,
    transferable_type_argument_bound_code: DiagnosticCode,
    hashable_type_argument_bound_code: DiagnosticCode,
    invalid_iteration_source_code: DiagnosticCode,
    invalid_iteration_pattern_code: DiagnosticCode,
    invalid_container_element_code: DiagnosticCode,
    cannot_infer_container_element_code: DiagnosticCode,
    invalid_container_construction_code: DiagnosticCode,
    invalid_container_index_code: DiagnosticCode,
    immutable_container_place_code: DiagnosticCode,
    invalid_container_member_code: DiagnosticCode,
    jump_outside_loop_code: DiagnosticCode,
    invalid_constant_type_code: DiagnosticCode,
    invalid_constant_expression_code: DiagnosticCode,
    invalid_constant_context_code: DiagnosticCode,
    constant_cycle_code: DiagnosticCode,
    constant_evaluation_code: DiagnosticCode,
    invisible_constant_code: DiagnosticCode,
    unresolved_constant_code: DiagnosticCode,
}

impl<'a> Checker<'a> {
    fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        names: &'a NameResolution,
        environment: &'a TypeEnvironment,
    ) -> Result<Self, TypeCheckingError> {
        let catalog = codes::catalog()?;
        let mut references = BTreeMap::new();
        for reference in names.references() {
            references.insert(
                (
                    reference.span().start(),
                    reference.span().end(),
                    namespace_key(reference.namespace()),
                ),
                reference.target().clone(),
            );
        }
        let symbols_by_span = names
            .symbols()
            .iter()
            .map(|symbol| ((symbol.span().start(), symbol.span().end()), symbol.id()))
            .collect();
        let symbol_kinds = names.symbols().iter().map(|symbol| symbol.kind()).collect();
        let symbol_spans = names.symbols().iter().map(|symbol| symbol.span()).collect();
        let symbol_scopes = names
            .symbols()
            .iter()
            .map(|symbol| symbol.scope())
            .collect();
        let classifier_scope_by_span = names
            .scopes()
            .iter()
            .filter_map(|scope| {
                if scope.kind() == ScopeKind::Classifier {
                    scope
                        .span()
                        .map(|span| ((span.start(), span.end()), scope.id()))
                } else {
                    None
                }
            })
            .collect();
        Ok(Self {
            sources,
            parsed,
            environment,
            name_analysis_owner: names.analysis_owner().clone(),
            types: TypeTable::new(),
            expression_types: vec![None; parsed.ast().expressions().len()],
            expression_uses: collect_expression_uses(parsed),
            type_ref_types: vec![None; parsed.ast().type_refs().len()],
            symbol_types: vec![None; names.symbols().len()],
            parameter_modes: vec![None; names.symbols().len()],
            non_null_uses: Vec::new(),
            null_comparisons: Vec::new(),
            nullable_whens: Vec::new(),
            non_null_assertions: Vec::new(),
            constant_dependencies: BTreeMap::new(),
            constant_values: BTreeMap::new(),
            constant_items: BTreeMap::new(),
            constant_expressions: BTreeMap::new(),
            pending_constant_errors: BTreeMap::new(),
            rechecking_constants: false,
            constants_checked: false,
            input_error_spans: parsed
                .diagnostics()
                .iter()
                .chain(names.diagnostics())
                .map(Diagnostic::primary_span)
                .collect(),
            constant_inputs_valid: parsed.diagnostics().is_empty()
                && names.diagnostics().is_empty(),
            associated_constant_uses: BTreeMap::new(),
            associated_constants: BTreeMap::new(),
            references,
            symbols_by_span,
            symbol_kinds,
            symbol_spans,
            component_type_spans: vec![None; names.symbols().len()],
            symbol_scopes,
            scope_parents: names.scopes().iter().map(|scope| scope.parent()).collect(),
            scope_kinds: names.scopes().iter().map(|scope| scope.kind()).collect(),
            mutable_symbols: BTreeSet::new(),
            captured_mutable_symbols: BTreeSet::new(),
            flow_facts: BTreeMap::new(),
            flow_versions: BTreeMap::new(),
            source_references: names
                .references()
                .iter()
                .map(|reference| (reference.scope(), reference.target().clone()))
                .collect(),
            nominal_by_symbol: BTreeMap::new(),
            nominal_by_scope: BTreeMap::new(),
            classifier_scope_by_span,
            nominals: Vec::new(),
            type_parameters: Vec::new(),
            type_parameter_by_symbol: BTreeMap::new(),
            delegations: Vec::new(),
            invalid_delegations: Vec::new(),
            typed_callables: Vec::new(),
            enum_cases: Vec::new(),
            enum_case_by_id: BTreeMap::new(),
            enum_case_by_type_symbol: BTreeMap::new(),
            enum_case_by_value_symbol: BTreeMap::new(),
            enum_case_by_payload_symbol: BTreeMap::new(),
            source_enum_cases: names.enum_cases().to_vec(),
            interface_edge_spans: BTreeMap::new(),
            invalid_inline_nominals: BTreeSet::new(),
            external_types: BTreeMap::new(),
            destructurings: Vec::new(),
            iterations: Vec::new(),
            expression_categories: vec![
                ExpressionCategory::Temporary;
                parsed.ast().expressions().len()
            ],
            calls: Vec::new(),
            constructions: Vec::new(),
            aggregate_projections: Vec::new(),
            ownership_primitives: Vec::new(),
            rc_operations: Vec::new(),
            string_operations: Vec::new(),
            integer_operations: Vec::new(),
            container_constructions: Vec::new(),
            container_sizes: Vec::new(),
            container_appends: Vec::new(),
            container_clears: Vec::new(),
            container_remove_ats: Vec::new(),
            container_remove_lasts: Vec::new(),
            container_remove_firsts: Vec::new(),
            container_insert_ats: Vec::new(),
            element_places: Vec::new(),
            map_descriptors: super::map_descriptor::MapDescriptors::default(),
            callables: Vec::new(),
            loop_depth: 0,
            candidate_local_expected: false,
            classifiers: Vec::new(),
            current_receiver_mode: None,
            diagnostics: Vec::new(),
            builtin_arguments_code: catalog.resolve(codes::BUILTIN_TYPE_ARGUMENTS)?,
            cannot_infer_code: catalog.resolve(codes::CANNOT_INFER_TYPE)?,
            mismatch_code: catalog.resolve(codes::TYPE_MISMATCH)?,
            operands_code: catalog.resolve(codes::INVALID_OPERAND_TYPES)?,
            return_outside_code: catalog.resolve(codes::RETURN_OUTSIDE_CALLABLE)?,
            return_shape_code: catalog.resolve(codes::RETURN_SHAPE_MISMATCH)?,
            missing_return_code: catalog.resolve(codes::MISSING_RETURN)?,
            branch_type_code: catalog.resolve(codes::NO_COMMON_BRANCH_TYPE)?,
            numeric_range_code: catalog.resolve(codes::NUMERIC_LITERAL_OUT_OF_RANGE)?,
            type_argument_arity_code: catalog.resolve(codes::TYPE_ARGUMENT_ARITY)?,
            invalid_type_bound_code: catalog.resolve(codes::INVALID_TYPE_BOUND)?,
            interface_runtime_value_code: catalog.resolve(codes::INTERFACE_RUNTIME_VALUE)?,
            invalid_supertype_code: catalog.resolve(codes::INVALID_SUPERTYPE)?,
            interface_cycle_code: catalog.resolve(codes::INTERFACE_CYCLE)?,
            type_argument_bound_code: catalog.resolve(codes::TYPE_ARGUMENT_BOUND)?,
            duplicate_callable_shape_code: catalog.resolve(codes::DUPLICATE_CALLABLE_SHAPE)?,
            concrete_member_body_code: catalog.resolve(codes::CONCRETE_MEMBER_BODY)?,
            interface_member_mismatch_code: catalog.resolve(codes::INTERFACE_MEMBER_MISMATCH)?,
            invalid_override_code: catalog.resolve(codes::INVALID_OVERRIDE)?,
            missing_interface_member_code: catalog.resolve(codes::MISSING_INTERFACE_MEMBER)?,
            default_member_conflict_code: catalog.resolve(codes::DEFAULT_MEMBER_CONFLICT)?,
            invalid_delegation_target_code: catalog.resolve(codes::INVALID_DELEGATION_TARGET)?,
            delegate_interface_mismatch_code: catalog
                .resolve(codes::DELEGATE_INTERFACE_MISMATCH)?,
            delegation_member_conflict_code: catalog.resolve(codes::DELEGATION_MEMBER_CONFLICT)?,
            non_borrow_delegation_receiver_code: catalog
                .resolve(codes::NON_BORROW_DELEGATION_RECEIVER)?,
            enum_case_type_position_code: catalog.resolve(codes::ENUM_CASE_TYPE_POSITION)?,
            invalid_type_test_code: catalog.resolve(codes::INVALID_TYPE_TEST)?,
            invalid_when_condition_code: catalog.resolve(codes::INVALID_WHEN_CONDITION)?,
            duplicate_when_else_code: catalog.resolve(codes::DUPLICATE_WHEN_ELSE)?,
            non_final_when_else_code: catalog.resolve(codes::NON_FINAL_WHEN_ELSE)?,
            duplicate_when_coverage_code: catalog.resolve(codes::DUPLICATE_WHEN_COVERAGE)?,
            non_exhaustive_when_code: catalog.resolve(codes::NON_EXHAUSTIVE_WHEN)?,
            when_branch_type_code: catalog.resolve(codes::WHEN_BRANCH_TYPE)?,
            invalid_enum_payload_access_code: catalog
                .resolve(codes::INVALID_ENUM_PAYLOAD_ACCESS)?,
            copyable_type_argument_bound_code: catalog
                .resolve(codes::COPYABLE_TYPE_ARGUMENT_BOUND)?,
            infinite_inline_layout_code: catalog.resolve(codes::INFINITE_INLINE_LAYOUT)?,
            invalid_box_argument_code: catalog.resolve(codes::INVALID_BOX_ARGUMENT)?,
            destructuring_arity_code: catalog.resolve(codes::DESTRUCTURING_ARITY)?,
            non_callable_target_code: catalog.resolve(codes::NON_CALLABLE_TARGET)?,
            invalid_named_argument_code: catalog.resolve(codes::INVALID_NAMED_ARGUMENT)?,
            call_argument_arity_code: catalog.resolve(codes::CALL_ARGUMENT_ARITY)?,
            call_argument_mode_code: catalog.resolve(codes::CALL_ARGUMENT_MODE)?,
            no_matching_overload_code: catalog.resolve(codes::NO_MATCHING_OVERLOAD)?,
            ambiguous_call_code: catalog.resolve(codes::AMBIGUOUS_CALL)?,
            generic_call_inference_code: catalog.resolve(codes::GENERIC_CALL_INFERENCE)?,
            invalid_construction_target_code: catalog
                .resolve(codes::INVALID_CONSTRUCTION_TARGET)?,
            construction_inference_code: catalog.resolve(codes::CONSTRUCTION_INFERENCE)?,
            transferable_type_argument_bound_code: catalog
                .resolve(codes::TRANSFERABLE_TYPE_ARGUMENT_BOUND)?,
            hashable_type_argument_bound_code: catalog
                .resolve(codes::HASHABLE_TYPE_ARGUMENT_BOUND)?,
            invalid_iteration_source_code: catalog.resolve(codes::INVALID_ITERATION_SOURCE)?,
            invalid_iteration_pattern_code: catalog.resolve(codes::INVALID_ITERATION_PATTERN)?,
            invalid_container_element_code: catalog.resolve(codes::INVALID_CONTAINER_ELEMENT)?,
            cannot_infer_container_element_code: catalog
                .resolve(codes::CANNOT_INFER_CONTAINER_ELEMENT)?,
            invalid_container_construction_code: catalog
                .resolve(codes::INVALID_CONTAINER_CONSTRUCTION)?,
            invalid_container_index_code: catalog.resolve(codes::INVALID_CONTAINER_INDEX)?,
            immutable_container_place_code: catalog.resolve(codes::IMMUTABLE_CONTAINER_PLACE)?,
            invalid_container_member_code: catalog.resolve(codes::INVALID_CONTAINER_MEMBER)?,
            jump_outside_loop_code: catalog.resolve(codes::JUMP_OUTSIDE_LOOP)?,
            invalid_constant_type_code: catalog.resolve(codes::INVALID_CONSTANT_TYPE)?,
            invalid_constant_expression_code: catalog
                .resolve(codes::INVALID_CONSTANT_EXPRESSION)?,
            invalid_constant_context_code: catalog.resolve(codes::INVALID_CONSTANT_CONTEXT)?,
            constant_cycle_code: catalog.resolve(codes::CONSTANT_DEPENDENCY_CYCLE)?,
            constant_evaluation_code: catalog.resolve(codes::CONSTANT_EVALUATION_FAILURE)?,
            invisible_constant_code: catalog.resolve(codes::INVISIBLE_ASSOCIATED_CONSTANT)?,
            unresolved_constant_code: catalog.resolve(codes::UNRESOLVED_NAME)?,
        })
    }

    fn run(mut self) -> Result<TypedFile, TypeCheckingError> {
        self.collect_nominals()?;
        self.collect_flow_metadata();
        self.collect_enum_cases()?;
        self.collect_nominal_fields()?;
        self.check_type_parameter_bounds()?;
        self.check_direct_interfaces()?;
        self.check_interface_cycles()?;
        self.compute_interface_closures()?;
        self.check_inline_layouts()?;
        self.predeclare_signatures()?;
        self.collect_associated_constants()?;
        self.check_constant_declarations()?;
        self.check_delegations()?;
        self.check_callable_shapes_and_bodies()?;
        for &root in self.parsed.roots() {
            self.check_item(root)?;
        }
        self.validate_type_argument_bounds()?;
        let constants = self.build_constant_facts()?;
        // 后置泛型约束等检查完成后才允许发布阶段计划；recovery 类型仍保留。
        if !self.diagnostics.is_empty() || !self.input_error_spans.is_empty() {
            self.iterations.clear();
            self.ownership_primitives.clear();
            self.integer_operations.clear();
        }
        self.ownership_primitives
            .sort_by_key(|fact| fact.expression().index());
        self.integer_operations
            .sort_by_key(|fact| fact.expression().index());
        self.iterations.sort_by_key(|plan| plan.statement().index());
        let copyabilities = self.all_copyabilities();
        let error = self.error_type();
        let expression_types = self
            .expression_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let type_ref_types = self
            .type_ref_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let symbol_types = self
            .symbol_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let parameter_bindings = self
            .parameter_modes
            .into_iter()
            .enumerate()
            .filter_map(|(index, mode)| {
                mode.map(|mode| ParameterBindingDescriptor::new(SymbolId(index), mode))
            })
            .collect();
        self.non_null_uses
            .sort_by_key(|descriptor| descriptor.expression.index());
        self.null_comparisons
            .sort_by_key(|descriptor| descriptor.expression.index());
        let diagnostics = ordered_diagnostics(self.sources, &self.diagnostics)?
            .into_iter()
            .cloned()
            .collect();
        Ok(TypedFile::new(
            self.parsed.source_id(),
            self.environment.owner().clone(),
            self.name_analysis_owner,
            self.types,
            TypedFileParts {
                constants,
                expression_types,
                type_ref_types,
                symbol_types,
                parameter_bindings,
                non_null_uses: self.non_null_uses,
                null_comparisons: self.null_comparisons,
                nullable_whens: self.nullable_whens,
                non_null_assertions: self.non_null_assertions,
                nominals: self.nominals,
                type_parameters: self.type_parameters,
                delegations: self.delegations,
                callables: self.typed_callables,
                enum_cases: self.enum_cases,
                copyabilities,
                destructurings: self.destructurings,
                iterations: self.iterations,
                expression_categories: self.expression_categories,
                calls: self.calls,
                constructions: self.constructions,
                aggregate_projections: self.aggregate_projections,
                ownership_primitives: self.ownership_primitives,
                rc_operations: self.rc_operations,
                string_operations: self.string_operations,
                integer_operations: self.integer_operations,
                container_constructions: self.container_constructions,
                container_sizes: self.container_sizes,
                container_appends: self.container_appends,
                container_clears: self.container_clears,
                container_remove_ats: self.container_remove_ats,
                container_remove_lasts: self.container_remove_lasts,
                container_remove_firsts: self.container_remove_firsts,
                container_insert_ats: self.container_insert_ats,
                element_places: self.element_places,
                map_descriptors: self.map_descriptors,
            },
            diagnostics,
        ))
    }

    fn ast(&self) -> &SyntaxAst {
        self.parsed.ast()
    }

    fn builtin(&mut self, builtin: BuiltinType) -> TypeId {
        self.types.intern(TypeKind::Builtin(builtin))
    }

    fn error_type(&mut self) -> TypeId {
        self.types.intern(TypeKind::Error)
    }

    fn deferred(&mut self, reason: DeferredReason) -> TypeId {
        self.types.intern(TypeKind::Deferred(reason))
    }

    fn kind(&self, id: TypeId) -> &TypeKind {
        self.types
            .get(id)
            .expect("TypeId is always allocated by this checker")
    }

    fn set_expression(&mut self, id: ExpressionId, ty: TypeId) {
        self.expression_types[id.index()] = Some(ty);
    }

    fn set_expression_category(&mut self, id: ExpressionId, category: ExpressionCategory) {
        self.expression_categories[id.index()] = category;
    }

    fn set_type_ref(&mut self, id: TypeRefId, ty: TypeId) {
        self.type_ref_types[id.index()] = Some(ty);
    }

    fn set_symbol(&mut self, id: SymbolId, ty: TypeId) {
        self.symbol_types[id.index()] = Some(ty);
    }

    fn set_parameter_mode(&mut self, id: SymbolId, mode: ParameterMode) {
        self.parameter_modes[id.index()] = Some(mode);
    }

    fn symbol_type(&self, id: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(id.index()).copied().flatten()
    }

    fn enum_case(&self, id: EnumCaseId) -> Option<&EnumCaseDescriptor> {
        self.enum_case_by_id
            .get(&id)
            .and_then(|&index| self.enum_cases.get(index))
    }

    fn symbol_at(&self, span: Span) -> Option<SymbolId> {
        self.symbols_by_span
            .get(&(span.start(), span.end()))
            .copied()
    }

    fn reference(&self, span: Span, namespace: Namespace) -> Option<&ReferenceTarget> {
        self.references
            .get(&(span.start(), span.end(), namespace_key(namespace)))
    }

    fn external_type(&mut self, id: ExternalSymbolId) -> Result<TypeId, TypeCheckingError> {
        if let Some(ty) = self.external_types.get(&id).copied() {
            return Ok(ty);
        }
        let ty = match self.environment.binding(id).cloned() {
            Some(ExternalTypeBinding::Builtin(builtin)) => self.builtin(builtin),
            Some(ExternalTypeBinding::Capability(capability)) => {
                self.types.intern(TypeKind::Capability(capability))
            }
            Some(ExternalTypeBinding::Intrinsic(_)) => self.error_type(),
            Some(ExternalTypeBinding::Value(ty)) => self.normalize_environment_type(&ty),
            Some(ExternalTypeBinding::Function(signature)) => {
                self.normalize_environment_function(&signature)
            }
            Some(ExternalTypeBinding::IntrinsicCallable(_)) => self.error_type(),
            None => self.deferred(DeferredReason::UnboundExternalType),
        };
        self.external_types.insert(id, ty);
        Ok(ty)
    }

    fn normalize_environment_function(&mut self, signature: &EnvironmentFunction) -> TypeId {
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| FunctionParameterType {
                mode: parameter.mode,
                ty: self.normalize_environment_type(&parameter.ty),
            })
            .collect();
        let return_type = self.normalize_environment_type(&signature.return_type);
        self.types.intern(TypeKind::Function {
            move_only: false,
            parameters,
            return_type,
        })
    }

    fn normalize_environment_type(&mut self, ty: &EnvironmentType) -> TypeId {
        match ty {
            EnvironmentType::Builtin(builtin) => self.builtin(*builtin),
            EnvironmentType::Nullable(inner) => {
                let inner = self.normalize_environment_type(inner);
                self.types.intern(TypeKind::Nullable(inner))
            }
            EnvironmentType::Function {
                move_only,
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| FunctionParameterType {
                        mode: parameter.mode,
                        ty: self.normalize_environment_type(&parameter.ty),
                    })
                    .collect();
                let return_type = self.normalize_environment_type(return_type);
                self.types.intern(TypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
            }
        }
    }

    fn is_error(&self, ty: TypeId) -> bool {
        matches!(self.kind(ty), TypeKind::Error)
    }

    fn is_deferred(&self, ty: TypeId) -> bool {
        matches!(self.kind(ty), TypeKind::Deferred(_))
    }

    fn assignable(&self, actual: TypeId, expected: TypeId) -> bool {
        if actual == expected || self.is_error(actual) || self.is_error(expected) {
            return true;
        }
        if matches!(self.kind(actual), TypeKind::Builtin(BuiltinType::Nothing)) {
            return true;
        }
        match (self.kind(actual), self.kind(expected)) {
            (TypeKind::Builtin(BuiltinType::Nothing), _) => true,
            (TypeKind::EnumCase { root, .. }, _) if *root == expected => true,
            (TypeKind::EnumCase { root, .. }, TypeKind::Nullable(inner)) if root == inner => true,
            (TypeKind::Nullable(inner), TypeKind::Nullable(expected))
                if matches!(self.kind(*inner), TypeKind::Builtin(BuiltinType::Nothing)) =>
            {
                !self.is_deferred(*expected)
            }
            (TypeKind::Nullable(actual), TypeKind::Nullable(expected)) => actual == expected,
            (_, TypeKind::Nullable(inner)) => actual == *inner,
            _ => false,
        }
    }

    fn join(&mut self, left: TypeId, right: TypeId) -> Option<TypeId> {
        if left == right {
            return Some(left);
        }
        if matches!(self.kind(left), TypeKind::Builtin(BuiltinType::Nothing)) {
            return Some(right);
        }
        if matches!(self.kind(right), TypeKind::Builtin(BuiltinType::Nothing)) {
            return Some(left);
        }
        match (self.kind(left).clone(), self.kind(right).clone()) {
            (TypeKind::Nullable(inner), _) if inner == right => Some(left),
            (_, TypeKind::Nullable(inner)) if inner == left => Some(right),
            (TypeKind::Error, _) => Some(right),
            (_, TypeKind::Error) => Some(left),
            _ => None,
        }
    }

    fn type_name(&self, ty: TypeId) -> String {
        match self.kind(ty) {
            TypeKind::Builtin(builtin) => builtin.name().to_owned(),
            TypeKind::Nullable(inner) => format!("{}?", self.type_name(*inner)),
            TypeKind::Function { .. } => "function type".to_owned(),
            TypeKind::Nominal { nominal, .. } => format!("nominal#{}", nominal.symbol().index()),
            TypeKind::Intrinsic {
                constructor,
                arguments,
            } => {
                let name = match constructor {
                    IntrinsicTypeConstructor::Box => "Box",
                    IntrinsicTypeConstructor::Rc => "Rc",
                    IntrinsicTypeConstructor::Array => "Array",
                    IntrinsicTypeConstructor::List => "List",
                    IntrinsicTypeConstructor::MutableList => "MutableList",
                    IntrinsicTypeConstructor::Map => "Map",
                    IntrinsicTypeConstructor::MutableMap => "MutableMap",
                };
                let args = arguments
                    .iter()
                    .map(|argument| self.type_name(*argument))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}<{args}>")
            }
            TypeKind::EnumCase { case, .. } => format!("enum-case#{}", case.index()),
            TypeKind::TypeParameter(symbol) => format!("type-parameter#{}", symbol.index()),
            TypeKind::StaticSelf(interface) => format!("Self<{}>", self.type_name(*interface)),
            TypeKind::Capability(Capability::Copyable) => "Copyable".to_owned(),
            TypeKind::Capability(Capability::Transferable) => "Transferable".to_owned(),
            TypeKind::Capability(Capability::Hashable) => "Hashable".to_owned(),
            TypeKind::IntegerLiteral(_) => "integer literal".to_owned(),
            TypeKind::Error => "<error>".to_owned(),
            TypeKind::Deferred(reason) => format!("<deferred:{reason:?}>"),
        }
    }

    fn emit(
        &mut self,
        code: DiagnosticCode,
        message: &str,
        primary: Span,
    ) -> Result<(), TypeCheckingError> {
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            primary,
        )?);
        Ok(())
    }

    fn emit_with_label(
        &mut self,
        code: DiagnosticCode,
        message: &str,
        primary: Span,
        label: Span,
        label_message: impl Into<String>,
    ) -> Result<(), TypeCheckingError> {
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        diagnostic.add_label(self.sources, label, label_message)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn mismatch(
        &mut self,
        primary: Span,
        expected_span: Option<Span>,
        actual: TypeId,
        expected: TypeId,
    ) -> Result<(), TypeCheckingError> {
        if self.is_error(actual)
            || self.is_deferred(actual)
            || self.is_error(expected)
            || self.is_deferred(expected)
        {
            return Ok(());
        }
        let message = "expression type does not match the expected type";
        if let Some(label) = expected_span {
            self.emit_with_label(
                self.mismatch_code,
                message,
                primary,
                label,
                format!(
                    "expected {}, found {}",
                    self.type_name(expected),
                    self.type_name(actual)
                ),
            )
        } else {
            self.emit(self.mismatch_code, message, primary)
        }
    }
}

fn namespace_key(namespace: Namespace) -> u8 {
    match namespace {
        Namespace::Type => 0,
        Namespace::Value => 1,
    }
}
