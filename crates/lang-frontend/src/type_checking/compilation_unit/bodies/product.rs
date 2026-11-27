//! Compilation-unit typed body construction, identity, queries and admission gates.
use super::*;

impl CompilationUnitTypes {
    /// 查询有插值的字符串已解析的执行步骤。纯文本与尚未恢复的插值均无计划。
    #[must_use]
    pub fn interpolation_plan(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&[UnitInterpolationPart]> {
        self.interpolation_plans.get(&expression).map(Vec::as_slice)
    }

    /// 返回 source/statement 顺序稳定的完整 borrowed provider 计划。
    #[must_use]
    pub fn sequential_iterations(&self) -> &[super::super::UnitSequentialIterationDescriptor] {
        &self.iterations
    }

    /// 按 source-qualified statement identity 查询，不接受裸局部 ID。
    #[must_use]
    pub fn sequential_iteration(
        &self,
        statement: UnitStatementId,
    ) -> Option<&super::super::UnitSequentialIterationDescriptor> {
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
            recovery_ownership: parts.recovery_ownership,
            resource_classifications: crate::type_checking::resource::ResourceCache::new(),
            iterations: parts.iterations,
            provenance,
            constant_declaration_count: parts.constant_declaration_count,
            constants: parts.constants,
            constant_selections: parts.constant_selections,
            signatures,
            expression_types: parts.expression_types,
            expression_categories: parts.expression_categories,
            expression_falls_through: parts.expression_falls_through,
            loop_falls_through: parts.loop_falls_through,
            when_closed_after: parts.when_closed_after,
            ordinary_boolean_flow: parts.ordinary_boolean_flow,
            storage_recovery: parts.storage_recovery,
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
            interpolation_plans: parts.interpolation_plans,
            integer_operations: parts.integer_operations,
            integer_range_operations: parts.integer_range_operations,
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

    pub(crate) fn is_compatible_with_raw(
        &self,
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
        names: &CompilationUnitNames,
        environment: &TypeEnvironment,
    ) -> bool {
        self.signatures
            .is_compatible_with_raw(sources, inputs, names, environment)
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

    /// Numeric Range facts in stable source-qualified order.
    pub fn integer_range_operations(&self) -> &[UnitIntegerRangeOperationDescriptor] {
        &self.integer_range_operations
    }
    /// Query one numeric Range operation by source identity.
    pub fn integer_range_operation(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitIntegerRangeOperationDescriptor> {
        self.integer_range_operations
            .iter()
            .find(|fact| fact.expression() == expression)
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

    /// 同一完整判据供能力验证与后续 producer 继承；原 required-fact 校验不缩减。
    pub(crate) fn has_valid_type_facts(&self) -> bool {
        self.signatures.is_valid()
            && self.storage_recovery.pending_iterations.is_empty()
            && !self
                .diagnostics
                .iter()
                .any(|d| d.severity() == Severity::Error)
            && self.ownership_primitives_are_valid()
            && self.integer_operations_are_valid()
            && self.integer_range_operations_are_valid()
            && self.numeric_range_iterations_are_valid()
            && self.map_iterations_are_valid()
    }

    pub(crate) fn has_valid_basic_types(&self) -> bool {
        self.has_valid_type_facts()
            && self.constant_declaration_count == 0
            && self.constant_selections.is_empty()
    }

    pub(crate) fn has_valid_constant_types(&self) -> bool {
        self.has_valid_type_facts() && self.constants.is_some()
    }

    /// 发布独立常量能力；不转换成基础 ownership/native 输入。
    pub fn validate_constants(self) -> Result<ConstEnabledTypedUnit, Box<Self>> {
        if self.has_valid_constant_types() {
            Ok(ConstEnabledTypedUnit(self))
        } else {
            Err(Box::new(self))
        }
    }

    /// 上游 names、signature/body 无 error 且不含常量声明或读取时发布基础 ownership view。
    /// 常量读取仍需独立 const-enabled capability，不能借此绕过其验证。
    pub fn validate(self) -> Result<ValidatedCompilationUnitTypes, Box<Self>> {
        if self.has_valid_basic_types() {
            Ok(ValidatedCompilationUnitTypes(self))
        } else {
            Err(Box::new(self))
        }
    }
}
