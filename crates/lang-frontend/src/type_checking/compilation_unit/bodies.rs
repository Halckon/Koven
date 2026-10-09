mod call_descriptor;
pub use call_descriptor::UnitCallDescriptor;
use std::{collections::BTreeMap, sync::Arc};

use crate::{
    diagnostic::{Diagnostic, Severity},
    name_resolution::{
        DeclarationId, ExternalSymbolId, SourceUnitInput, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    source::{SourceMap, Span},
    type_checking::{
        Copyability, DestructuringMode, ExpressionCategory, ParameterMode, TypeEnvironment,
    },
};

use super::{
    CompilationUnitSignatures, CompilationUnitTypeError, UnitExpressionId, UnitStatementId,
    UnitTypeId, UnitTypeRefId, UnitTypeTable, ValidatedCompilationUnitSignatures,
};

mod assignment;
mod checker;
mod constants;
mod container;
pub use constants::*;
mod integer;
mod map;
mod non_null_assertion;
mod nullable;
mod ownership_primitive;
mod projection;
mod rc;
mod string;

pub use checker::check_compilation_unit_types;
pub(crate) use checker::copyability::UnitTransferability;
pub use integer::*;
pub use ownership_primitive::*;
pub use {
    assignment::*, container::*, map::*, non_null_assertion::*, nullable::*, projection::*, rc::*,
    string::*,
};

/// 一个 unit body 中成功选择的静态 call target。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitCallTarget {
    /// compilation unit 顶层源码函数。
    Declaration(DeclarationId),
    /// source-local callable。
    Symbol(UnitSymbolId),
    /// 编译器绑定的外部函数。
    External(ExternalSymbolId),
    /// 由函数类型值提供的调用目标。
    FunctionValue,
    /// `value class` 自动结构分量。
    StructuralComponent(UnitSymbolId),
}

/// unit call 的静态 target 与完整类型实参 identity。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCallableInstanceKey {
    target: UnitCallTarget,
    type_arguments: Vec<UnitTypeId>,
}

impl UnitCallableInstanceKey {
    /// 返回唯一静态 call target。
    #[must_use]
    pub const fn target(&self) -> UnitCallTarget {
        self.target
    }

    /// 返回 target 实例化后的完整类型实参。
    #[must_use]
    pub fn type_arguments(&self) -> &[UnitTypeId] {
        &self.type_arguments
    }
}

/// unit call 中一个源码实参到声明参数的映射。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCallArgumentDescriptor {
    argument_index: usize,
    parameter_index: usize,
    category: ExpressionCategory,
    mode: ParameterMode,
    parameter_type: UnitTypeId,
    cross_thread: bool,
}

/// compilation-unit member call 的 receiver 来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitCallReceiverOrigin {
    /// 显式 `receiver.member(...)` receiver。
    Expression(UnitExpressionId),
    /// 裸 member call 复用当前 callable 的 `this`。
    ImplicitThis(DeclarationId),
}

/// 成功 member call 的实例化 receiver 契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCallReceiverDescriptor {
    pub(crate) origin: UnitCallReceiverOrigin,
    pub(crate) mode: ParameterMode,
    pub(crate) category: ExpressionCategory,
    pub(crate) ty: UnitTypeId,
}

impl UnitCallReceiverDescriptor {
    /// 返回显式 expression 或隐式 `this` 来源。
    #[must_use]
    pub const fn origin(self) -> UnitCallReceiverOrigin {
        self.origin
    }

    /// 返回已选择 callable 的 receiver mode。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回 receiver 的 place/temporary 类别。
    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    /// 返回 owner 实参替换后的 receiver 类型。
    #[must_use]
    pub const fn ty(self) -> UnitTypeId {
        self.ty
    }
}

impl UnitCallArgumentDescriptor {
    /// 返回实参在源码顺序中的下标。
    #[must_use]
    pub const fn argument_index(self) -> usize {
        self.argument_index
    }

    /// 返回实参映射到的声明参数下标。
    #[must_use]
    pub const fn parameter_index(self) -> usize {
        self.parameter_index
    }

    /// 返回实参的类型层面 place/temporary 类别。
    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    /// 返回声明参数的规范化交付模式。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回实例化后的声明参数类型。
    #[must_use]
    pub const fn parameter_type(self) -> UnitTypeId {
        self.parameter_type
    }

    /// 返回参数是否由 compiler-bound effect 跨线程交付。
    #[must_use]
    pub const fn crosses_thread(self) -> bool {
        self.cross_thread
    }
}

/// compilation unit 中 construction target 的稳定 identity。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitConstructionTarget {
    /// `class` 或 `value class` 的主构造器。
    Nominal(DeclarationId),
    /// `enum class` 的 case；使用值命名空间 symbol 标识。
    EnumCase(UnitSymbolId),
    /// 编译器绑定的 intrinsic `Box`。
    IntrinsicBox,
    /// 编译器绑定的 intrinsic `Rc`。
    IntrinsicRc,
}

/// 构造目标与完整类型实参组成的实例 identity。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstructionInstanceKey {
    pub(crate) target: UnitConstructionTarget,
    pub(crate) type_arguments: Vec<UnitTypeId>,
}

impl UnitConstructionInstanceKey {
    /// 返回唯一 construction target。
    #[must_use]
    pub const fn target(&self) -> UnitConstructionTarget {
        self.target
    }

    /// 返回实例化后的完整类型实参。
    #[must_use]
    pub fn type_arguments(&self) -> &[UnitTypeId] {
        &self.type_arguments
    }
}

/// 一个构造实参到声明 field/payload 的 Value 映射。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstructionArgumentDescriptor {
    pub(crate) parameter_index: usize,
    pub(crate) parameter_symbol: Option<UnitSymbolId>,
    pub(crate) parameter_name: String,
    pub(crate) parameter_type: UnitTypeId,
    pub(crate) argument: UnitExpressionId,
    pub(crate) evaluation_index: usize,
    pub(crate) category: ExpressionCategory,
}

impl UnitConstructionArgumentDescriptor {
    /// 返回声明顺序参数下标。
    #[must_use]
    pub const fn parameter_index(&self) -> usize {
        self.parameter_index
    }

    /// 返回 source-qualified field/payload symbol；intrinsic 参数没有源码 symbol。
    #[must_use]
    pub const fn parameter_symbol(&self) -> Option<UnitSymbolId> {
        self.parameter_symbol
    }

    /// 返回源码参数名。
    #[must_use]
    pub fn parameter_name(&self) -> &str {
        &self.parameter_name
    }

    /// 返回实例化后的参数类型。
    #[must_use]
    pub const fn parameter_type(&self) -> UnitTypeId {
        self.parameter_type
    }

    /// 构造参数固定采用 Value delivery。
    #[must_use]
    pub const fn mode(&self) -> ParameterMode {
        ParameterMode::Value
    }

    /// 返回 source-qualified 实参 expression。
    #[must_use]
    pub const fn argument(&self) -> UnitExpressionId {
        self.argument
    }

    /// 返回实参的源码求值顺序下标。
    #[must_use]
    pub const fn evaluation_index(&self) -> usize {
        self.evaluation_index
    }

    /// 返回实参 place/temporary 类别。
    #[must_use]
    pub const fn category(&self) -> ExpressionCategory {
        self.category
    }
}

/// 一次已成功类型化的 construction。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstructionDescriptor {
    pub(crate) expression: UnitExpressionId,
    pub(crate) instance: UnitConstructionInstanceKey,
    pub(crate) result_type: UnitTypeId,
    pub(crate) arguments: Vec<UnitConstructionArgumentDescriptor>,
}

impl UnitConstructionDescriptor {
    /// 返回 source-qualified construction expression。
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }

    /// 返回目标与完整类型实参组成的实例 identity。
    #[must_use]
    pub const fn instance(&self) -> &UnitConstructionInstanceKey {
        &self.instance
    }

    /// 返回唯一 construction target。
    #[must_use]
    pub const fn target(&self) -> UnitConstructionTarget {
        self.instance.target()
    }

    /// 返回构造结果类型。
    #[must_use]
    pub const fn result_type(&self) -> UnitTypeId {
        self.result_type
    }

    /// 返回声明参数顺序的 Value operand mappings。
    #[must_use]
    pub fn arguments(&self) -> &[UnitConstructionArgumentDescriptor] {
        &self.arguments
    }
}

/// compilation unit 中一个已类型化的结构化解构分量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDestructuringComponent {
    symbol: UnitSymbolId,
    ty: UnitTypeId,
}

impl UnitDestructuringComponent {
    pub(crate) const fn new(symbol: UnitSymbolId, ty: UnitTypeId) -> Self {
        Self { symbol, ty }
    }

    /// 返回接收分量的 source-qualified binding symbol。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回替换实际泛型实参后的分量类型。
    #[must_use]
    pub const fn ty(self) -> UnitTypeId {
        self.ty
    }
}

/// compilation unit 中一次精确、有效的局部 value-class 结构化解构。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDestructuringDescriptor {
    statement: UnitStatementId,
    source_type: UnitTypeId,
    mode: DestructuringMode,
    components: Vec<UnitDestructuringComponent>,
}

/// concrete ordinary-class owner 的一个已替换 runtime field。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitRuntimeFieldLayoutField {
    symbol: UnitSymbolId,
    template_type: UnitTypeId,
    concrete_type: UnitTypeId,
    span: Span,
}

impl UnitRuntimeFieldLayoutField {
    pub(crate) const fn new(
        symbol: UnitSymbolId,
        template_type: UnitTypeId,
        concrete_type: UnitTypeId,
        span: Span,
    ) -> Self {
        Self {
            symbol,
            template_type,
            concrete_type,
            span,
        }
    }

    /// 返回源码 field identity。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回 owner declaration 中的字段模板类型。
    #[must_use]
    pub const fn template_type(self) -> UnitTypeId {
        self.template_type
    }

    /// 返回按当前 owner arguments 递归替换后的字段类型。
    #[must_use]
    pub const fn concrete_type(self) -> UnitTypeId {
        self.concrete_type
    }

    /// 返回字段名称范围。
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }
}

/// owner-instance-qualified ordinary-class runtime field layout。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitRuntimeFieldLayoutDescriptor {
    owner_type: UnitTypeId,
    declaration: DeclarationId,
    arguments: Vec<UnitTypeId>,
    fields: Vec<UnitRuntimeFieldLayoutField>,
}

impl UnitRuntimeFieldLayoutDescriptor {
    pub(crate) fn new(
        owner_type: UnitTypeId,
        declaration: DeclarationId,
        arguments: Vec<UnitTypeId>,
        fields: Vec<UnitRuntimeFieldLayoutField>,
    ) -> Self {
        Self {
            owner_type,
            declaration,
            arguments,
            fields,
        }
    }

    /// 返回完整 concrete owner type identity。
    #[must_use]
    pub const fn owner_type(&self) -> UnitTypeId {
        self.owner_type
    }

    /// 返回 owner classifier declaration identity。
    #[must_use]
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    /// 返回完整 owner type arguments。
    #[must_use]
    pub fn arguments(&self) -> &[UnitTypeId] {
        &self.arguments
    }

    /// 返回主构造器源码字段顺序的 concrete layout。
    #[must_use]
    pub fn fields(&self) -> &[UnitRuntimeFieldLayoutField] {
        &self.fields
    }
}

impl UnitDestructuringDescriptor {
    pub(crate) fn new(
        statement: UnitStatementId,
        source_type: UnitTypeId,
        mode: DestructuringMode,
        components: Vec<UnitDestructuringComponent>,
    ) -> Self {
        Self {
            statement,
            source_type,
            mode,
            components,
        }
    }

    /// 返回带 source-unit 限定的 statement identity。
    #[must_use]
    pub const fn statement(&self) -> UnitStatementId {
        self.statement
    }

    /// 返回 initializer 的规范化源类型。
    #[must_use]
    pub const fn source_type(&self) -> UnitTypeId {
        self.source_type
    }

    /// 返回 Copy 或 Consume 原子模式。
    #[must_use]
    pub const fn mode(&self) -> DestructuringMode {
        self.mode
    }

    /// 返回主构造器字段顺序的 binding / component type。
    #[must_use]
    pub fn components(&self) -> &[UnitDestructuringComponent] {
        &self.components
    }
}

/// body checker 交给 recovery product 的最小、source-qualified facts。
#[derive(Clone, Default)]
pub(crate) struct CompilationUnitTypeParts {
    pub(crate) iterations: Vec<super::UnitSequentialIterationDescriptor>,
    pub(crate) constant_declaration_count: usize,
    pub(crate) constants: Option<UnitConstantFacts>,
    pub(crate) constant_selections: BTreeMap<UnitExpressionId, UnitSymbolId>,
    pub(crate) expression_types: BTreeMap<UnitExpressionId, UnitTypeId>,
    pub(crate) expression_categories: BTreeMap<UnitExpressionId, ExpressionCategory>,
    pub(crate) expression_falls_through: BTreeMap<UnitExpressionId, bool>,
    pub(crate) type_ref_types: BTreeMap<UnitTypeRefId, UnitTypeId>,
    pub(crate) symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    pub(crate) parameter_modes: BTreeMap<UnitSymbolId, ParameterMode>,
    pub(crate) assignments: Vec<UnitAssignmentDescriptor>,
    pub(crate) calls: Vec<UnitCallDescriptor>,
    pub(crate) aggregate_projections: Vec<UnitAggregateProjectionDescriptor>,
    pub(crate) constructions: Vec<UnitConstructionDescriptor>,
    pub(crate) destructurings: Vec<UnitDestructuringDescriptor>,
    pub(crate) runtime_field_layouts: Vec<UnitRuntimeFieldLayoutDescriptor>,
    pub(crate) ownership_primitives: Vec<UnitOwnershipPrimitiveDescriptor>,
    pub(crate) rc_operations: Vec<UnitRcOperationDescriptor>,
    pub(crate) string_operations: Vec<UnitStringOperationDescriptor>,
    pub(crate) integer_operations: Vec<UnitIntegerOperationDescriptor>,
    pub(crate) container_constructions: Vec<UnitContainerConstructionDescriptor>,
    container_sizes: Vec<UnitContainerSizeDescriptor>,
    pub(crate) range_sizes:
        Vec<crate::type_checking::RangeSizeDescriptor<UnitExpressionId, UnitTypeId>>,
    pub(crate) container_appends: Vec<UnitContainerAppendDescriptor>,
    pub(crate) container_clears: Vec<UnitContainerClearDescriptor>,
    pub(crate) container_remove_ats: Vec<UnitContainerRemoveAtDescriptor>,
    pub(crate) container_remove_lasts: Vec<UnitContainerRemoveLastDescriptor>,
    pub(crate) container_remove_firsts: Vec<UnitContainerRemoveFirstDescriptor>,
    pub(crate) container_insert_ats: Vec<UnitContainerInsertAtDescriptor>,
    pub(crate) element_places: Vec<UnitElementPlaceDescriptor>,
    pub(crate) map_descriptors: UnitMapDescriptors,
    pub(crate) nullable: UnitNullableFacts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BodyTypeProvenance {
    analysis_owner: Arc<()>,
}

/// SPEC-0197 body 阶段的 recovery typed product。
///
/// 类型表仍由内含的 signature product 唯一拥有；body facts 只使用
/// [`UnitTypeId`] 并且所有源码 identity 都带 [`SourceUnitId`] 限定。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitTypes {
    iterations: Vec<super::UnitSequentialIterationDescriptor>,
    pub(in crate::type_checking) resource_classifications:
        crate::type_checking::resource::ResourceCache<
            crate::type_checking::resource::UnitResourceClassifier,
        >,
    constant_declaration_count: usize,
    constants: Option<UnitConstantFacts>,
    constant_selections: BTreeMap<UnitExpressionId, UnitSymbolId>,
    provenance: BodyTypeProvenance,
    signatures: CompilationUnitSignatures,
    expression_types: BTreeMap<UnitExpressionId, UnitTypeId>,
    expression_categories: BTreeMap<UnitExpressionId, ExpressionCategory>,
    type_ref_types: BTreeMap<UnitTypeRefId, UnitTypeId>,
    symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    parameter_modes: BTreeMap<UnitSymbolId, ParameterMode>,
    assignments: Vec<UnitAssignmentDescriptor>,
    calls: Vec<UnitCallDescriptor>,
    aggregate_projections: Vec<UnitAggregateProjectionDescriptor>,
    constructions: Vec<UnitConstructionDescriptor>,
    destructurings: Vec<UnitDestructuringDescriptor>,
    runtime_field_layouts: Vec<UnitRuntimeFieldLayoutDescriptor>,
    ownership_primitives: Vec<UnitOwnershipPrimitiveDescriptor>,
    rc_operations: Vec<UnitRcOperationDescriptor>,
    string_operations: Vec<UnitStringOperationDescriptor>,
    integer_operations: Vec<UnitIntegerOperationDescriptor>,
    container_constructions: Vec<UnitContainerConstructionDescriptor>,
    container_sizes: Vec<UnitContainerSizeDescriptor>,
    pub(crate) range_sizes:
        Vec<crate::type_checking::RangeSizeDescriptor<UnitExpressionId, UnitTypeId>>,
    container_appends: Vec<UnitContainerAppendDescriptor>,
    container_clears: Vec<UnitContainerClearDescriptor>,
    container_remove_ats: Vec<UnitContainerRemoveAtDescriptor>,
    container_remove_lasts: Vec<UnitContainerRemoveLastDescriptor>,
    container_remove_firsts: Vec<UnitContainerRemoveFirstDescriptor>,
    container_insert_ats: Vec<UnitContainerInsertAtDescriptor>,
    element_places: Vec<UnitElementPlaceDescriptor>,
    pub(crate) map_descriptors: UnitMapDescriptors,
    nullable: UnitNullableFacts,
    body_diagnostics: Vec<Diagnostic>,
    diagnostics: Vec<Diagnostic>,
}

impl CompilationUnitTypes {
    /// 返回 source/statement 顺序稳定的完整 borrowed provider 计划。
    #[must_use]
    pub fn sequential_iterations(&self) -> &[super::UnitSequentialIterationDescriptor] {
        &self.iterations
    }

    /// 按 source-qualified statement identity 查询，不接受裸局部 ID。
    #[must_use]
    pub fn sequential_iteration(
        &self,
        statement: UnitStatementId,
    ) -> Option<&super::UnitSequentialIterationDescriptor> {
        self.iterations
            .binary_search_by_key(&statement, |plan| plan.statement())
            .ok()
            .map(|index| &self.iterations[index])
    }

    pub(crate) fn new(
        signatures: CompilationUnitSignatures,
        parts: CompilationUnitTypeParts,
        body_diagnostics: Vec<Diagnostic>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        let provenance = BodyTypeProvenance {
            analysis_owner: Arc::new(()),
        };
        Self {
            resource_classifications: crate::type_checking::resource::ResourceCache::new(),
            iterations: parts.iterations,
            provenance,
            constant_declaration_count: parts.constant_declaration_count,
            constants: parts.constants,
            constant_selections: parts.constant_selections,
            signatures,
            expression_types: parts.expression_types,
            expression_categories: parts.expression_categories,
            type_ref_types: parts.type_ref_types,
            symbol_types: parts.symbol_types,
            parameter_modes: parts.parameter_modes,
            assignments: parts.assignments,
            calls: parts.calls,
            aggregate_projections: parts.aggregate_projections,
            constructions: parts.constructions,
            destructurings: parts.destructurings,
            runtime_field_layouts: parts.runtime_field_layouts,
            ownership_primitives: parts.ownership_primitives,
            rc_operations: parts.rc_operations,
            string_operations: parts.string_operations,
            integer_operations: parts.integer_operations,
            container_constructions: parts.container_constructions,
            container_sizes: parts.container_sizes,
            range_sizes: parts.range_sizes,
            container_appends: parts.container_appends,
            container_clears: parts.container_clears,
            container_remove_ats: parts.container_remove_ats,
            container_remove_lasts: parts.container_remove_lasts,
            container_remove_firsts: parts.container_remove_firsts,
            container_insert_ats: parts.container_insert_ats,
            element_places: parts.element_places,
            map_descriptors: parts.map_descriptors,
            nullable: parts.nullable,
            body_diagnostics,
            diagnostics,
        }
    }

    /// 检查该产物是否来自给定 inputs、names 与 type environment 身份链。
    #[must_use]
    pub fn is_compatible_with(
        &self,
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
        names: &ValidatedCompilationUnitNames,
        environment: &TypeEnvironment,
    ) -> bool {
        self.signatures
            .is_compatible_with(sources, inputs, names, environment)
    }

    /// 判断两个 body typed product 是否来自同一次分析；克隆保留身份。
    #[must_use]
    pub fn is_same_analysis(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.provenance.analysis_owner,
            &other.provenance.analysis_owner,
        )
    }

    pub(crate) const fn analysis_owner(&self) -> &Arc<()> {
        &self.provenance.analysis_owner
    }

    /// 返回 body 阶段沿用的 unit-wide signatures。
    #[must_use]
    pub const fn signatures(&self) -> &CompilationUnitSignatures {
        &self.signatures
    }

    /// 返回 signature 与 body 共享的唯一 unit-global type table。
    #[must_use]
    pub const fn types(&self) -> &UnitTypeTable {
        self.signatures.types()
    }

    /// 返回 unit-global 类型的条件 `Copyable` 判定；与 body 类型检查共用同一算法。
    #[must_use]
    pub fn copyability(&self, ty: UnitTypeId) -> Copyability {
        checker::copyability::unit_copyability(&self.signatures, ty)
    }

    pub(crate) fn transferability(&self, ty: UnitTypeId) -> UnitTransferability {
        checker::copyability::unit_transferability(&self.signatures, ty)
    }

    /// 查询 source-qualified expression 的规范类型。
    #[must_use]
    pub fn expression_type(&self, expression: UnitExpressionId) -> Option<UnitTypeId> {
        self.expression_types.get(&expression).copied()
    }

    /// 返回 source-qualified expression typed facts。
    #[must_use]
    pub const fn expression_types(&self) -> &BTreeMap<UnitExpressionId, UnitTypeId> {
        &self.expression_types
    }

    /// 查询 source-qualified expression 的类型层面类别。
    #[must_use]
    pub fn expression_category(&self, expression: UnitExpressionId) -> Option<ExpressionCategory> {
        self.expression_categories.get(&expression).copied()
    }

    /// 返回源码稳定顺序的成功普通替换赋值事实。
    #[must_use]
    pub fn assignments(&self) -> &[UnitAssignmentDescriptor] {
        &self.assignments
    }

    /// 查询一个成功普通替换赋值 expression 的 descriptor。
    #[must_use]
    pub fn assignment(&self, expression: UnitExpressionId) -> Option<UnitAssignmentDescriptor> {
        self.assignments
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的成功 call facts。
    #[must_use]
    pub fn calls(&self) -> &[UnitCallDescriptor] {
        &self.calls
    }

    /// 查询一个成功 call expression 的 descriptor。
    #[must_use]
    pub fn call(&self, expression: UnitExpressionId) -> Option<&UnitCallDescriptor> {
        self.calls
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的聚合分量投影事实。
    #[must_use]
    pub fn aggregate_projections(&self) -> &[UnitAggregateProjectionDescriptor] {
        &self.aggregate_projections
    }

    /// 查询一个字段访问或结构分量 call 的投影事实。
    #[must_use]
    pub fn aggregate_projection(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitAggregateProjectionDescriptor> {
        self.aggregate_projections
            .iter()
            .copied()
            .find(|projection| projection.expression() == expression)
    }

    /// 返回源码稳定顺序的 String intrinsic 操作。
    #[must_use]
    pub fn string_operations(&self) -> &[UnitStringOperationDescriptor] {
        &self.string_operations
    }

    /// 查询已绑定 receiver 的显式 String 操作。
    #[must_use]
    pub fn string_operation(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitStringOperationDescriptor> {
        self.string_operations
            .iter()
            .copied()
            .find(|operation| operation.expression() == expression)
    }

    /// 返回源码稳定顺序的 Integer intrinsic 操作。
    #[must_use]
    pub fn integer_operations(&self) -> &[UnitIntegerOperationDescriptor] {
        &self.integer_operations
    }

    /// 查询已绑定 receiver 的显式 Integer 操作。
    #[must_use]
    pub fn integer_operation(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitIntegerOperationDescriptor> {
        self.integer_operations
            .iter()
            .copied()
            .find(|operation| operation.expression() == expression)
    }

    /// 返回源码稳定顺序的原子所有权原语静态事实；错误产物不发布此表。
    #[must_use]
    pub fn ownership_primitives(&self) -> &[UnitOwnershipPrimitiveDescriptor] {
        &self.ownership_primitives
    }

    /// 查询 compiler-bound 原语；普通同名源码调用返回 None。
    #[must_use]
    pub fn ownership_primitive(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitOwnershipPrimitiveDescriptor> {
        self.ownership_primitives
            .iter()
            .copied()
            .find(|fact| fact.expression() == expression)
    }

    /// 返回源码稳定顺序的 intrinsic `Rc<T>` operation facts。
    #[must_use]
    pub fn rc_operations(&self) -> &[UnitRcOperationDescriptor] {
        &self.rc_operations
    }

    /// 查询指定 source-qualified expression 的 intrinsic `Rc<T>` operation。
    #[must_use]
    pub fn rc_operation(&self, expression: UnitExpressionId) -> Option<UnitRcOperationDescriptor> {
        self.rc_operations
            .iter()
            .copied()
            .find(|operation| operation.expression() == expression)
    }

    /// 返回源码稳定顺序的成功 construction facts。
    #[must_use]
    pub fn constructions(&self) -> &[UnitConstructionDescriptor] {
        &self.constructions
    }

    /// 查询一个成功 construction expression 的 descriptor。
    #[must_use]
    pub fn construction(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitConstructionDescriptor> {
        self.constructions
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的有效 value-class 解构事实。
    #[must_use]
    pub fn destructurings(&self) -> &[UnitDestructuringDescriptor] {
        &self.destructurings
    }

    /// 查询一条 source-qualified statement 的有效解构事实。
    #[must_use]
    pub fn destructuring(
        &self,
        statement: UnitStatementId,
    ) -> Option<&UnitDestructuringDescriptor> {
        self.destructurings
            .iter()
            .find(|descriptor| descriptor.statement() == statement)
    }

    /// 返回按 owner `UnitTypeId` 稳定排序的 ordinary-class runtime field layouts。
    #[must_use]
    pub fn runtime_field_layouts(&self) -> &[UnitRuntimeFieldLayoutDescriptor] {
        &self.runtime_field_layouts
    }

    /// 查询一个 concrete ordinary-class owner 的 runtime field layout。
    #[must_use]
    pub fn runtime_field_layout(
        &self,
        owner_type: UnitTypeId,
    ) -> Option<&UnitRuntimeFieldLayoutDescriptor> {
        self.runtime_field_layouts
            .binary_search_by_key(&owner_type.index(), |layout| layout.owner_type().index())
            .ok()
            .map(|index| &self.runtime_field_layouts[index])
    }

    /// 查询 source-qualified type reference 的规范类型。
    #[must_use]
    pub fn type_ref_type(&self, type_ref: UnitTypeRefId) -> Option<UnitTypeId> {
        self.type_ref_types
            .get(&type_ref)
            .copied()
            .or_else(|| self.signatures.type_ref_type(type_ref))
    }

    /// 返回 source-qualified type-reference typed facts。
    #[must_use]
    pub const fn type_ref_types(&self) -> &BTreeMap<UnitTypeRefId, UnitTypeId> {
        &self.type_ref_types
    }

    /// 查询 source-qualified symbol 的 body 类型，再回退到 signature 类型。
    #[must_use]
    pub fn symbol_type(&self, symbol: UnitSymbolId) -> Option<UnitTypeId> {
        self.symbol_types
            .get(&symbol)
            .copied()
            .or_else(|| self.signatures.symbol_type(symbol))
    }

    /// 返回 body 阶段新增的 source-qualified symbol typed facts。
    #[must_use]
    pub const fn body_symbol_types(&self) -> &BTreeMap<UnitSymbolId, UnitTypeId> {
        &self.symbol_types
    }

    /// 查询已采用 expected callable contract 的 lambda 参数模式。
    #[must_use]
    pub fn body_parameter_mode(&self, symbol: UnitSymbolId) -> Option<ParameterMode> {
        self.parameter_modes.get(&symbol).copied()
    }

    /// 返回按 source-qualified symbol 排序的 lambda 参数模式事实。
    #[must_use]
    pub const fn body_parameter_modes(&self) -> &BTreeMap<UnitSymbolId, ParameterMode> {
        &self.parameter_modes
    }

    /// 返回已经按 stable source key、byte span 与 code 排序的 body 类型诊断。
    ///
    /// signature 诊断仍由 [`Self::signatures`] 暴露；完整 driver 接线后再统一发布聚合集合。
    #[must_use]
    pub fn body_diagnostics(&self) -> &[Diagnostic] {
        &self.body_diagnostics
    }

    /// 返回 signature 与 body 类型阶段统一排序后的诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 返回原子发布的常量事实；错误或未完成求值时为 None。
    pub const fn constants(&self) -> Option<&UnitConstantFacts> {
        self.constants.as_ref()
    }

    /// 发布独立常量能力；不转换成基础 ownership/native 输入。
    pub fn validate_constants(self) -> Result<ConstEnabledTypedUnit, Box<Self>> {
        if self.ownership_primitives_are_valid()
            && self.integer_operations_are_valid()
            && self.constants.is_some()
            && !self
                .diagnostics
                .iter()
                .any(|d| d.severity() == Severity::Error)
        {
            Ok(ConstEnabledTypedUnit(self))
        } else {
            Err(Box::new(self))
        }
    }

    /// signature/body 无 error 且不含常量声明或读取时才发布基础 ownership view。
    /// 常量读取需要 SPEC-0210 的独立 const-enabled capability，不能借此绕过其验证。
    pub fn validate(self) -> Result<ValidatedCompilationUnitTypes, Box<Self>> {
        let has_error = self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        // Selection is a recovery fact, not the const-enabled capability required by SPEC-0210.
        if has_error
            || self.constant_declaration_count > 0
            || !self.constant_selections.is_empty()
            || !self.ownership_primitives_are_valid()
            || !self.integer_operations_are_valid()
        {
            Err(Box::new(self))
        } else {
            Ok(ValidatedCompilationUnitTypes(self))
        }
    }
}

/// 不可伪造的无错误 compilation-unit body typed product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCompilationUnitTypes(CompilationUnitTypes);

impl ValidatedCompilationUnitTypes {
    /// 返回 recovery product 的只读视图。
    #[must_use]
    pub const fn types(&self) -> &CompilationUnitTypes {
        &self.0
    }

    /// 解包 recovery product。
    #[must_use]
    pub fn into_types(self) -> CompilationUnitTypes {
        self.0
    }
}

/// 核对 body 阶段的 source inputs、validated names、type environment 与 signature owner。
///
/// 该入口只验证分析身份链，不执行 body 类型检查。
pub fn validate_compilation_unit_body_inputs(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    signatures: &ValidatedCompilationUnitSignatures,
) -> Result<(), CompilationUnitTypeError> {
    let unit_names = names.names();
    let rebuilt = crate::name_resolution::index_compilation_unit(sources, inputs)
        .map_err(|_| CompilationUnitTypeError::MismatchedInputs)?;
    if &rebuilt != unit_names.index() {
        return Err(CompilationUnitTypeError::MismatchedInputs);
    }
    if unit_names.source_units().len() != unit_names.index().source_units().len()
        || unit_names.source_units().iter().any(|source| {
            source.resolution().source_id()
                != unit_names.index().source_units()[source.source_unit().index()].source_id()
                || !Arc::ptr_eq(source.resolution().environment_owner(), environment.owner())
        })
    {
        return Err(CompilationUnitTypeError::MismatchedNameEnvironment);
    }
    if !signatures
        .signatures()
        .is_compatible_with(sources, inputs, names, environment)
    {
        return Err(CompilationUnitTypeError::MismatchedSignatures);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        diagnostic::{Diagnostic, Severity, codes},
        lexer::lex,
        name_resolution::{
            NameEnvironment, SourceUnitInput, index_compilation_unit,
            resolve_compilation_unit_names,
        },
        parser::{AssignmentOperator, ParsedFile, parse_file},
        source::{SourceId, SourceMap},
        type_checking::{
            BuiltinType, TypeEnvironment, UnitStatementId, collect_compilation_unit_signatures,
            standard_environments,
        },
    };

    use super::*;

    fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
        let source = sources.add_source(name, text).expect("unique source");
        let lexed = lex(sources, source).expect("lexing succeeds internally");
        let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        (source, parsed)
    }

    fn validated_names<'a>(
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'a>],
        environment: &NameEnvironment,
    ) -> crate::name_resolution::ValidatedCompilationUnitNames {
        let index = index_compilation_unit(sources, inputs).expect("valid unit input");
        resolve_compilation_unit_names(sources, inputs, &index, environment)
            .expect("name resolution succeeds internally")
            .validate()
            .expect("valid names")
    }

    #[test]
    fn source_qualified_ast_identities_do_not_collide() {
        let mut sources = SourceMap::new();
        let (left_source, left) = parsed(
            &mut sources,
            "left.ko",
            "package p\nfun left(): Int { return 1 }",
        );
        let (right_source, right) = parsed(
            &mut sources,
            "right.ko",
            "package p\nfun right(): Int { return 2 }",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/left.ko", left_source, &left),
            SourceUnitInput::new("root", "p/right.ko", right_source, &right),
        ];
        let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
        let left_unit = index.source_units()[0].id();
        let right_unit = index.source_units()[1].id();
        let left_expression = left.ast().expressions().iter().next().expect("literal").0;
        let right_expression = right.ast().expressions().iter().next().expect("literal").0;
        let left_statement = left.ast().statements().iter().next().expect("body").0;
        let right_statement = right.ast().statements().iter().next().expect("body").0;
        let left_type_ref = left.ast().type_refs().iter().next().expect("return type").0;
        let right_type_ref = right
            .ast()
            .type_refs()
            .iter()
            .next()
            .expect("return type")
            .0;

        assert_eq!(left_expression.index(), right_expression.index());
        assert_ne!(
            UnitExpressionId::new(left_unit, left_expression),
            UnitExpressionId::new(right_unit, right_expression)
        );
        assert_eq!(left_statement.index(), right_statement.index());
        assert_ne!(
            UnitStatementId::new(left_unit, left_statement),
            UnitStatementId::new(right_unit, right_statement)
        );
        assert_eq!(left_type_ref.index(), right_type_ref.index());
        assert_ne!(
            UnitTypeRefId::new(left_unit, left_type_ref),
            UnitTypeRefId::new(right_unit, right_type_ref)
        );
    }

    #[test]
    fn product_queries_use_the_unit_type_space_and_preserve_analysis_identity() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "main.ko",
            "package app\nfun answer(): Int = 42",
        );
        let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
                .expect("signature collection succeeds");
        let source_unit = names.names().index().source_units()[0].id();
        let expression = file.ast().expressions().iter().next().expect("literal").0;
        let type_ref = file.ast().type_refs().iter().next().expect("return type").0;
        let int = signatures
            .types()
            .builtin(BuiltinType::Int)
            .expect("Int seed");
        let mut parts = CompilationUnitTypeParts::default();
        parts
            .expression_types
            .insert(UnitExpressionId::new(source_unit, expression), int);
        parts.assignments.push(UnitAssignmentDescriptor::new(
            UnitExpressionId::new(source_unit, expression),
            UnitExpressionId::new(source_unit, expression),
            UnitExpressionId::new(source_unit, expression),
            AssignmentOperator::Assign,
            int,
            true,
        ));
        let product = CompilationUnitTypes::new(signatures, parts, Vec::new(), Vec::new());

        assert_eq!(
            product.expression_type(UnitExpressionId::new(source_unit, expression)),
            Some(int)
        );
        assert_eq!(
            product
                .assignment(UnitExpressionId::new(source_unit, expression))
                .map(UnitAssignmentDescriptor::target_type),
            Some(int)
        );
        assert_eq!(product.assignments().len(), 1);
        assert_eq!(
            product.type_ref_type(UnitTypeRefId::new(source_unit, type_ref)),
            Some(int)
        );
        assert!(product.type_ref_types().is_empty());
        assert_eq!(
            product.types().get(int),
            Some(&super::super::UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert!(product.is_compatible_with(&sources, &inputs, &names, &type_environment));
        assert!(product.is_same_analysis(&product.clone()));
        assert!(product.clone().validate().is_ok());
    }

    #[test]
    fn body_input_gate_rejects_foreign_names_environment_inputs_and_signatures() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "main.ko",
            "package app\nfun answer(): Int = 42",
        );
        let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
                .expect("signature collection succeeds")
                .validate()
                .expect("valid signatures");

        assert!(
            validate_compilation_unit_body_inputs(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &signatures,
            )
            .is_ok()
        );

        let repeated_names = validated_names(&sources, &inputs, &name_environment);
        assert!(matches!(
            validate_compilation_unit_body_inputs(
                &sources,
                &inputs,
                &repeated_names,
                &type_environment,
                &signatures,
            ),
            Err(CompilationUnitTypeError::MismatchedSignatures)
        ));

        let unrelated_types = TypeEnvironment::new(&NameEnvironment::new());
        assert!(matches!(
            validate_compilation_unit_body_inputs(
                &sources,
                &inputs,
                &names,
                &unrelated_types,
                &signatures,
            ),
            Err(CompilationUnitTypeError::MismatchedNameEnvironment)
        ));

        let changed_inputs = [SourceUnitInput::new("root", "other/main.ko", source, &file)];
        assert!(matches!(
            validate_compilation_unit_body_inputs(
                &sources,
                &changed_inputs,
                &names,
                &type_environment,
                &signatures,
            ),
            Err(CompilationUnitTypeError::MismatchedInputs)
        ));
    }

    #[test]
    fn validated_gate_rejects_body_and_signature_errors() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "main.ko",
            "package app\nfun answer(): Int = 42",
        );
        let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
                .expect("signature collection succeeds");
        let primary = file
            .ast()
            .expressions()
            .iter()
            .next()
            .expect("literal")
            .1
            .span();
        let diagnostic = Diagnostic::new(
            &sources,
            Severity::Error,
            codes::catalog()
                .expect("production catalog")
                .resolve(codes::TYPE_MISMATCH)
                .expect("registered code"),
            "body type mismatch",
            primary,
        )
        .expect("valid diagnostic");
        let product = CompilationUnitTypes::new(
            signatures,
            CompilationUnitTypeParts::default(),
            vec![diagnostic.clone()],
            vec![diagnostic],
        );

        assert_eq!(product.body_diagnostics().len(), 1);
        assert!(product.validate().is_err());

        let (duplicate_source, duplicate_file) = parsed(
            &mut sources,
            "duplicate.ko",
            "package duplicate\nfun same(input: Int): Int { return input }\nfun same(input: Int): String { return \"x\" }",
        );
        let duplicate_inputs = [SourceUnitInput::new(
            "root",
            "duplicate/duplicate.ko",
            duplicate_source,
            &duplicate_file,
        )];
        let duplicate_names = validated_names(&sources, &duplicate_inputs, &name_environment);
        let invalid_signatures = collect_compilation_unit_signatures(
            &sources,
            &duplicate_inputs,
            &duplicate_names,
            &type_environment,
        )
        .expect("signature errors remain a recovery product");
        assert!(invalid_signatures.clone().validate().is_err());
        let diagnostics = invalid_signatures.diagnostics().to_vec();
        assert!(
            CompilationUnitTypes::new(
                invalid_signatures,
                CompilationUnitTypeParts::default(),
                Vec::new(),
                diagnostics,
            )
            .validate()
            .is_err()
        );
    }

    #[test]
    fn body_model_does_not_reuse_the_single_file_type_id() {
        fn accepts_unit(_: UnitTypeId) {}
        fn accepts_local(_: crate::type_checking::TypeId) {}

        let _ = accepts_unit as fn(UnitTypeId);
        let _ = accepts_local as fn(crate::type_checking::TypeId);
    }
}
