//! TypedFile construction, provenance and descriptor queries.
use super::*;

impl TypedFile {
    pub(crate) fn new(
        source_id: SourceId,
        environment_owner: Arc<()>,
        environment_snapshot_owner: Arc<()>,
        name_analysis_owner: Arc<()>,
        types: TypeTable,
        parts: TypedFileParts,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            resource_classifications: super::super::resource::ResourceCache::new(),
            source_id,
            environment_owner,
            environment_snapshot_owner,
            name_analysis_owner,
            analysis_owner: parts
                .constants
                .as_ref()
                .map_or_else(|| Arc::new(()), |facts| facts.analysis_owner.clone()),
            constants: parts.constants,
            types,
            expression_types: parts.expression_types,
            type_ref_types: parts.type_ref_types,
            symbol_types: parts.symbol_types,
            parameter_bindings: parts.parameter_bindings,
            non_null_uses: parts.non_null_uses,
            null_comparisons: parts.null_comparisons,
            nullable_whens: parts.nullable_whens,
            non_null_assertions: parts.non_null_assertions,
            nominals: parts.nominals,
            type_parameters: parts.type_parameters,
            delegations: parts.delegations,
            callables: parts.callables,
            enum_cases: parts.enum_cases,
            copyabilities: parts.copyabilities,
            destructurings: parts.destructurings,
            iterations: parts.iterations,
            expression_categories: parts.expression_categories,
            calls: parts.calls,
            constructions: parts.constructions,
            aggregate_projections: parts.aggregate_projections,
            ownership_primitives: parts.ownership_primitives,
            rc_operations: parts.rc_operations,
            string_operations: parts.string_operations,
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
            diagnostics,
        }
    }

    #[must_use]
    /// 返回来源文件身份。
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// 返回该 typed 产物是否与名称产物共享源码和显式环境身份。
    #[must_use]
    pub fn is_compatible_with_names(&self, names: &NameResolution) -> bool {
        self.source_id == names.source_id()
            && Arc::ptr_eq(&self.environment_owner, names.environment_owner())
            && Arc::ptr_eq(&self.name_analysis_owner, names.analysis_owner())
    }

    pub(crate) fn environment_owner(&self) -> &Arc<()> {
        &self.environment_owner
    }

    pub(in crate::type_checking) fn environment_snapshot_owner(&self) -> &Arc<()> {
        &self.environment_snapshot_owner
    }

    pub(crate) fn analysis_owner(&self) -> &Arc<()> {
        &self.analysis_owner
    }

    #[must_use]
    /// 返回本产物的规范化类型表。
    pub const fn types(&self) -> &TypeTable {
        &self.types
    }

    #[must_use]
    /// 查询表达式类型。
    pub fn expression_type(&self, id: ExpressionId) -> Option<TypeId> {
        self.expression_types.get(id.index()).copied()
    }

    #[must_use]
    /// 查询 TypeRef 类型。
    pub fn type_ref_type(&self, id: TypeRefId) -> Option<TypeId> {
        self.type_ref_types.get(id.index()).copied()
    }

    #[must_use]
    /// 查询源码 symbol 类型。
    pub fn symbol_type(&self, id: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(id.index()).copied()
    }

    /// 查询一个具名函数或已采用 expected contract 的 lambda 参数模式。
    #[must_use]
    pub fn parameter_mode(&self, id: SymbolId) -> Option<ParameterMode> {
        self.parameter_bindings
            .binary_search_by_key(&id.index(), |binding| binding.symbol().index())
            .ok()
            .map(|index| self.parameter_bindings[index].mode())
    }

    /// 返回稳定 symbol 顺序的 callable 参数绑定事实。
    #[must_use]
    pub fn parameter_bindings(&self) -> &[ParameterBindingDescriptor] {
        &self.parameter_bindings
    }

    /// 返回按 expression identity 排序的已证明非空使用点。
    #[must_use]
    pub fn non_null_uses(&self) -> &[NonNullUseDescriptor] {
        &self.non_null_uses
    }

    /// 查询一个表达式是否是已证明非空的稳定 symbol 使用点。
    #[must_use]
    pub fn non_null_use(&self, expression: ExpressionId) -> Option<NonNullUseDescriptor> {
        self.non_null_uses
            .binary_search_by_key(&expression.index(), |descriptor| {
                descriptor.expression.index()
            })
            .ok()
            .map(|index| self.non_null_uses[index])
    }

    /// 查询一个 equality expression 发布的 null/non-null edge fact。
    #[must_use]
    pub fn null_comparison(&self, expression: ExpressionId) -> Option<NullComparisonDescriptor> {
        self.null_comparisons
            .binary_search_by_key(&expression.index(), |descriptor| {
                descriptor.expression.index()
            })
            .ok()
            .map(|index| self.null_comparisons[index])
    }

    /// 返回源码声明顺序的名义类型描述符。
    #[must_use]
    pub fn nominals(&self) -> &[NominalDescriptor] {
        &self.nominals
    }

    /// 返回源码 symbol 顺序的类型参数描述符。
    #[must_use]
    pub fn type_parameters(&self) -> &[TypeParameterDescriptor] {
        &self.type_parameters
    }

    /// 返回源码顺序的有效静态接口委托计划。
    #[must_use]
    pub fn delegations(&self) -> &[DelegationPlan] {
        &self.delegations
    }

    /// 返回源码声明顺序的已知 callable 签名。
    #[must_use]
    pub fn callables(&self) -> &[CallableDescriptor] {
        &self.callables
    }

    /// 返回源码声明顺序的 typed enum case 描述符。
    #[must_use]
    pub fn enum_cases(&self) -> &[EnumCaseDescriptor] {
        &self.enum_cases
    }

    /// 查询一个本产物 TypeId 的静态复制能力。
    #[must_use]
    pub fn copyability(&self, id: TypeId) -> Option<Copyability> {
        self.copyabilities.get(id.index()).copied()
    }

    /// 已完成类型检查的 for 计划，按 StatementId 排序；不授予所有权能力。
    #[must_use]
    pub fn sequential_iterations(&self) -> &[crate::type_checking::SequentialIterationDescriptor] {
        &self.iterations.sequential
    }
    /// 查询指定 for 的完整计划；非法或 poisoned for 不发布。
    #[must_use]
    pub fn sequential_iteration(
        &self,
        statement: StatementId,
    ) -> Option<&crate::type_checking::SequentialIterationDescriptor> {
        self.sequential_iterations()
            .iter()
            .find(|plan| plan.statement() == statement)
    }

    /// 返回源码 statement 顺序的有效 value-class 解构描述符。
    #[must_use]
    pub fn destructurings(&self) -> &[DestructuringDescriptor] {
        &self.destructurings
    }

    /// 查询指定 statement 的有效 value-class 解构描述符。
    #[must_use]
    pub fn destructuring(&self, statement: StatementId) -> Option<&DestructuringDescriptor> {
        self.destructurings
            .iter()
            .find(|descriptor| descriptor.statement() == statement)
    }

    /// 查询一个表达式的 Phase 2 place/temporary 类别。
    #[must_use]
    pub fn expression_category(&self, expression: ExpressionId) -> Option<ExpressionCategory> {
        self.expression_categories.get(expression.index()).copied()
    }

    /// 返回源码 call expression 顺序的成功选择描述符。
    #[must_use]
    pub fn calls(&self) -> &[CallDescriptor] {
        &self.calls
    }

    /// 查询指定 call expression 的成功选择描述符。
    #[must_use]
    pub fn call(&self, expression: ExpressionId) -> Option<&CallDescriptor> {
        self.calls
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码 expression 顺序的成功 construction descriptors。
    #[must_use]
    pub fn constructions(&self) -> &[ConstructionDescriptor] {
        &self.constructions
    }

    /// 查询指定 expression 的成功 construction descriptor。
    #[must_use]
    pub fn construction(&self, expression: ExpressionId) -> Option<&ConstructionDescriptor> {
        self.constructions
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码 expression 顺序的聚合分量投影。
    #[must_use]
    pub fn aggregate_projections(&self) -> &[AggregateProjectionDescriptor] {
        &self.aggregate_projections
    }

    /// 查询指定 expression 的聚合分量投影。
    #[must_use]
    pub fn aggregate_projection(
        &self,
        expression: ExpressionId,
    ) -> Option<AggregateProjectionDescriptor> {
        self.aggregate_projections
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 String intrinsic 操作。
    #[must_use]
    pub fn string_operations(&self) -> &[StringOperationDescriptor] {
        &self.string_operations
    }

    /// 查询已绑定 receiver 的显式 String 操作。
    #[must_use]
    pub fn string_operation(&self, expression: ExpressionId) -> Option<StringOperationDescriptor> {
        self.string_operations
            .iter()
            .copied()
            .find(|operation| operation.expression() == expression)
    }

    /// 返回源码稳定顺序的 Integer intrinsic 操作。
    #[must_use]
    pub fn integer_operations(&self) -> &[IntegerOperationDescriptor] {
        &self.integer_operations
    }

    /// Numeric Range facts in stable expression order.
    pub fn integer_range_operations(&self) -> &[IntegerRangeOperationDescriptor] {
        &self.integer_range_operations
    }
    /// Query one numeric Range operation by source identity.
    pub fn integer_range_operation(
        &self,
        expression: ExpressionId,
    ) -> Option<&IntegerRangeOperationDescriptor> {
        self.integer_range_operations
            .iter()
            .find(|fact| fact.expression() == expression)
    }

    /// 查询已绑定 receiver 的显式 Integer 操作。
    #[must_use]
    pub fn integer_operation(
        &self,
        expression: ExpressionId,
    ) -> Option<IntegerOperationDescriptor> {
        self.integer_operations
            .iter()
            .copied()
            .find(|operation| operation.expression() == expression)
    }

    /// 返回源码稳定顺序的原子所有权原语静态事实；错误产物不发布此表。
    #[must_use]
    pub fn ownership_primitives(&self) -> &[OwnershipPrimitiveDescriptor] {
        &self.ownership_primitives
    }

    /// 查询 compiler-bound 原语；普通同名源码调用返回 None。
    #[must_use]
    pub fn ownership_primitive(
        &self,
        expression: ExpressionId,
    ) -> Option<OwnershipPrimitiveDescriptor> {
        self.ownership_primitives
            .iter()
            .copied()
            .find(|fact| fact.expression() == expression)
    }

    /// 返回源码 expression 顺序的 intrinsic `Rc<T>` 操作。
    #[must_use]
    pub fn rc_operations(&self) -> &[RcOperationDescriptor] {
        &self.rc_operations
    }

    /// 查询指定 expression 的 intrinsic `Rc<T>` 操作。
    #[must_use]
    pub fn rc_operation(&self, expression: ExpressionId) -> Option<RcOperationDescriptor> {
        self.rc_operations
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    #[must_use]
    /// 返回稳定排序的类型诊断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl TypedFile {
    /// 完整常量事实；上游或类型诊断、未完成求值时不发布半成品。
    #[must_use]
    pub fn constants(&self) -> Option<&super::super::ValidatedConstants> {
        self.constants.as_ref()
    }

    /// 返回按 when expression identity 排序的 nullable flow plans。
    #[must_use]
    pub fn nullable_whens(&self) -> &[super::super::NullableWhenDescriptor] {
        &self.nullable_whens
    }
    /// 查询单次 subject 求值对应的 nullable flow plan。
    #[must_use]
    pub fn nullable_when(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::super::NullableWhenDescriptor> {
        self.nullable_whens
            .iter()
            .find(|plan| plan.expression() == expression)
    }
}

impl TypedFile {
    /// 按 assertion AST identity 排序的单次求值 extraction 候选。
    #[must_use]
    pub fn non_null_assertions(&self) -> &[super::super::NonNullAssertionDescriptor] {
        &self.non_null_assertions
    }

    /// 查询 compiler-bound assertion，不经过普通 callable 名称选择。
    #[must_use]
    pub fn non_null_assertion(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::super::NonNullAssertionDescriptor> {
        self.non_null_assertions
            .iter()
            .find(|plan| plan.expression() == expression)
    }
}
