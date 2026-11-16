//! Unit ownership fact publication, provenance, queries and validation.
use super::*;

impl CompilationUnitOwnership {
    /// Whole-callable narrow owner transport observations; Unsupported is not a language error.
    #[must_use]
    pub fn owner_observations(&self) -> &[UnitOwnerObservation] {
        &self.owner_observations
    }

    /// 已检查的 source-qualified 一级字段原子置换。
    #[must_use]
    pub fn field_replacements(&self) -> &[super::super::UnitFieldReplaceOwnershipPlan] {
        &self.field_replacements
    }
    /// 按调用 identity 查询独立字段提交能力。
    #[must_use]
    pub fn field_replacement(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::super::UnitFieldReplaceOwnershipPlan> {
        self.field_replacements
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 正常执行前缀后已获准的 source-qualified root 原子 commit。
    #[must_use]
    pub fn ownership_primitives(&self) -> &[super::super::UnitOwnershipPrimitiveOwnershipPlan] {
        &self.ownership_primitives
    }
    /// 按调用 identity 查询正常 root commit。
    #[must_use]
    pub fn ownership_primitive(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::super::UnitOwnershipPrimitiveOwnershipPlan> {
        self.ownership_primitives
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 返回按 source-qualified assertion identity 排序的合法提取计划。
    #[must_use]
    pub fn non_null_assertions(&self) -> &[UnitNonNullAssertionOwnershipPlan] {
        &self.non_null_assertions
    }
    /// 查询仅成功边转移、null 边 Abort 的 ownership plan。
    #[must_use]
    pub fn non_null_assertion(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitNonNullAssertionOwnershipPlan> {
        self.non_null_assertions
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    pub(super) fn new(
        typed: &CompilationUnitTypes,
        bindings: Vec<UnitOwnershipBindingDescriptor>,
        call_argument_contracts: Vec<UnitCallArgumentOwnershipContract>,
        call_receiver_contracts: Vec<UnitCallReceiverOwnershipContract>,
        capture: capture::Analysis,
        dataflow: dataflow::Analysis,
    ) -> Self {
        let successful = dataflow.diagnostics.is_empty() && typed.has_valid_type_facts();
        let mut capture = capture;
        capture.filter_unvisited(&dataflow.visited_lambdas, dataflow.short_circuits.is_some());
        let captures = if successful {
            capture.captures
        } else {
            Vec::new()
        };
        let delegations = if successful {
            typed
                .signatures()
                .delegations()
                .iter()
                .filter(|plan| !plan.forwarders().is_empty())
                .map(|plan| {
                    UnitDelegationOwnershipPlan::new(
                        plan.owner(),
                        plan.target(),
                        plan.delegation_span(),
                        plan.forwarders()
                            .iter()
                            .map(|forwarder| forwarder.requirement())
                            .collect(),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            #[cfg(feature = "d10-catalog-experiment")]
            source_collection_ledger: (successful && dataflow.deferred.is_empty()).then(|| {
                dataflow
                    .source_collection_ledger
                    .with_final_closures(&captures, &capture.closures)
            }),
            iteration_conditional_scopes: dataflow.iteration_conditional_scopes,
            iteration_owner_scopes: dataflow.iteration_owner_scopes,
            iteration_loan_ends: dataflow.iteration_loan_ends,
            iterations: if successful && dataflow.deferred.is_empty() {
                dataflow.iterations
            } else {
                Vec::new()
            },
            map_iterations: if successful && dataflow.deferred.is_empty() {
                dataflow.map_iterations
            } else {
                Vec::new()
            },
            map_iteration_templates: dataflow.map_iteration_templates,
            map_iteration_inputs: typed.map_iterations().to_vec(),
            iteration_templates: dataflow.iteration_templates,
            iteration_required_exits: dataflow.iteration_required_exits,
            short_circuits: (successful && dataflow.deferred.is_empty())
                .then_some(dataflow.short_circuits)
                .flatten(),
            constant_materializations: (successful
                && dataflow.deferred.is_empty()
                && typed.constants().is_some())
            .then_some(dataflow.constant_materializations),
            no_carry: if successful && dataflow.deferred.is_empty() {
                dataflow.no_carry
            } else {
                Default::default()
            },
            borrow_results: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_results
            } else {
                Default::default()
            },
            borrow_return_origins: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_return_origins
            } else {
                Vec::new()
            },
            callable_provenance: super::super::callable_provenance::UnitCallableFacts {
                entered_lambdas: if successful {
                    dataflow.visited_lambdas
                } else {
                    Default::default()
                },
                ..dataflow.callable_provenance
            },
            provenance: UnitOwnershipProvenance {
                typed_analysis_owner: Arc::clone(typed.analysis_owner()),
                typed_basic_is_valid: typed.has_valid_basic_types(),
                typed_const_is_valid: typed.has_valid_constant_types(),
                #[cfg(feature = "d10-catalog-experiment")]
                analysis_owner: dataflow.analysis_owner,
                #[cfg(not(feature = "d10-catalog-experiment"))]
                analysis_owner: Arc::new(()),
                requires_constant_capability: typed
                    .constants()
                    .is_some_and(|facts| !facts.declarations().is_empty()),
            },
            non_null_assertions: dataflow.non_null_assertions,
            nullable_whens: dataflow.nullable_whens,
            field_replacement_types: Self::field_replacement_types(
                typed,
                &dataflow.field_replacements,
            ),
            field_replacements: if successful && dataflow.deferred.is_empty() {
                dataflow.field_replacements
            } else {
                Vec::new()
            },
            ownership_primitives: if successful && dataflow.deferred.is_empty() {
                dataflow.ownership_primitives
            } else {
                Vec::new()
            },
            diagnostics: dataflow.diagnostics,
            bindings,
            formed_owned_locals: if successful && dataflow.deferred.is_empty() {
                dataflow.formed_owned_locals
            } else {
                Vec::new()
            },
            call_argument_contracts,
            call_receiver_contracts,
            receiver_facts: dataflow.receiver_facts,
            conditional_receiver_deliveries: dataflow.conditional_receiver_deliveries,
            delegations,
            string_clone_loan_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.string_clone_loan_ends
            } else {
                Vec::new()
            },
            string_effects: if successful && dataflow.deferred.is_empty() {
                typed
                    .string_operations()
                    .iter()
                    .copied()
                    .filter(|operation| {
                        dataflow.loans.iter().any(|loan| {
                            loan.call() == operation.expression()
                                && loan.argument() == operation.receiver()
                        })
                    })
                    .map(UnitStringOwnershipEffect::new)
                    .collect()
            } else {
                Vec::new()
            },
            owner_observations: owner_transport::publish_observations(
                dataflow.owner_observations,
                successful,
                dataflow.deferred.is_empty(),
            ),
            receiver_call_prefix_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.receiver_call_prefix_ends
            } else {
                Vec::new()
            },
            receiver_call_return_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.receiver_call_return_ends
            } else {
                Vec::new()
            },
            exclusive_call_prefix_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.exclusive_call_prefix_ends
            } else {
                Vec::new()
            },
            borrow_call_prefix_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_call_prefix_ends
            } else {
                Vec::new()
            },
            borrow_forward_transports: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_forward_transports
            } else {
                Vec::new()
            },
            borrow_binding_transports: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_binding_transports
            } else {
                Vec::new()
            },
            borrow_call_transports: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_call_transports
            } else {
                Vec::new()
            },
            borrow_call_handoffs: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_call_handoffs
            } else {
                Vec::new()
            },
            loan_action_observations: if successful && dataflow.deferred.is_empty() {
                dataflow.loan_action_observations
            } else {
                Vec::new()
            },
            direct_call_loan_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.direct_call_loan_ends
            } else {
                Vec::new()
            },
            loans: dataflow.loans,
            value_deliveries: dataflow.value_deliveries,
            rc_effects: dataflow.rc_effects,
            construction_plans: dataflow.construction_plans,
            drops: dataflow.drops,
            conditional_receiver_drops: dataflow.conditional_receiver_drops,
            captures,
            closures: capture.closures,
            transferabilities: capture.transferabilities,
            deferred: dataflow.deferred,
        }
    }

    /// 判断本产物是否来自指定 validated typed unit；克隆 typed/product 保留身份。
    #[must_use]
    pub fn is_compatible_with(&self, typed: &ValidatedCompilationUnitTypes) -> bool {
        Arc::ptr_eq(
            &self.provenance.typed_analysis_owner,
            typed.types().analysis_owner(),
        )
    }

    /// 判断两个 ownership product 是否来自同一次分析。
    #[must_use]
    pub fn is_same_analysis(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.provenance.analysis_owner,
            &other.provenance.analysis_owner,
        )
    }

    /// 返回稳定 source/symbol 顺序的参数能力。
    #[must_use]
    pub fn bindings(&self) -> &[UnitOwnershipBindingDescriptor] {
        &self.bindings
    }

    /// 返回某条分析路径正常完成初始化的具名 val/var local，按 source/symbol 排序去重。
    /// 仅记录形成历史，不证明必执行、当前存活、owner 权限或 nullable proof。
    #[must_use]
    pub fn formed_owned_locals(&self) -> &[UnitSymbolId] {
        &self.formed_owned_locals
    }

    /// 查询 source-qualified 参数 binding 能力。
    #[must_use]
    pub fn binding_kind(&self, symbol: UnitSymbolId) -> Option<OwnershipBindingKind> {
        self.bindings
            .binary_search_by_key(&symbol, |binding| binding.symbol())
            .ok()
            .map(|index| self.bindings[index].kind())
    }

    /// 返回稳定排序的所有权诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 返回稳定 source/call/argument 顺序的 ownership call contracts。
    #[must_use]
    pub fn call_argument_contracts(&self) -> &[UnitCallArgumentOwnershipContract] {
        &self.call_argument_contracts
    }

    /// 返回指定同步 call 的源码顺序 ownership contracts。
    pub fn call_argument_contracts_for(
        &self,
        call: UnitExpressionId,
    ) -> impl Iterator<Item = &UnitCallArgumentOwnershipContract> {
        self.call_argument_contracts
            .iter()
            .filter(move |contract| contract.call() == call)
    }

    /// 返回稳定 source/call 顺序的 receiver ownership contracts。
    #[must_use]
    pub fn call_receiver_contracts(&self) -> &[UnitCallReceiverOwnershipContract] {
        &self.call_receiver_contracts
    }

    /// 查询指定 member call 的 receiver contract。
    #[must_use]
    pub fn call_receiver_contract(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitCallReceiverOwnershipContract> {
        self.call_receiver_contracts
            .iter()
            .copied()
            .find(|contract| contract.call() == call)
    }

    /// 返回稳定 source/call 顺序的成功 receiver ownership facts。
    #[must_use]
    pub fn receiver_facts(&self) -> &[UnitReceiverOwnershipFact] {
        &self.receiver_facts
    }

    /// 查询指定 member call 的 receiver ownership fact。
    #[must_use]
    pub fn receiver_fact(&self, call: UnitExpressionId) -> Option<&UnitReceiverOwnershipFact> {
        self.receiver_facts.iter().find(|fact| fact.call() == call)
    }

    /// 返回源码/调用顺序稳定的 `StaticSelf` Value receiver 条件交付事实。
    #[must_use]
    pub fn conditional_receiver_deliveries(&self) -> &[UnitConditionalReceiverDeliveryFact] {
        &self.conditional_receiver_deliveries
    }

    /// 查询指定 member call 的 `StaticSelf` Value receiver 条件交付事实。
    #[must_use]
    pub fn conditional_receiver_delivery(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitConditionalReceiverDeliveryFact> {
        self.conditional_receiver_deliveries
            .iter()
            .copied()
            .find(|fact| fact.call() == call)
    }

    /// 返回 Borrow-only 委托的 outer receiver/delegate field shared-loan plans。
    #[must_use]
    pub fn delegations(&self) -> &[UnitDelegationOwnershipPlan] {
        &self.delegations
    }

    /// String.clone receiver 正常完成的独立 planner 观察，保持本表遍历顺序。
    /// 不包含普通同名方法、size、未被 planner 遍历或 receiver 未正常完成的 clone。
    /// 错误或 deferred 时整表撤销；不承诺常量条件或全 CFG 的不可达裁剪。
    /// 不是动态执行 trace，也不授予与 drops 或其他 loan-end 表的统一顺序。
    #[must_use]
    pub fn string_clone_loan_ends(&self) -> &[UnitStringCloneLoanEndFact] {
        &self.string_clone_loan_ends
    }

    /// 普通直接调用 Borrow 实参的实际结束事件，保持 planner 的结束动作顺序。
    /// 仅覆盖 owned-result、无 receiver 的 declaration/symbol call：operand 为可分组的
    /// owned val/var 或具名参数根、已完成求值的 String 字面量（含插值）、已验证构造形成的完整 MoveOnly
    /// temporary，或沿 typed Field/Group 链证明的 MoveOnly temporary 父字段。
    /// 构造与字段 target 均须匹配实际 owner。排除普通工厂调用结果、借用绑定、provider
    /// 来源/绑定、其他投影、借用结果、来源移交、Inout、receiver 与 capture end。
    /// 本轮任一 provider 使用的名称根均保守排除，不按调用位置扩大准入。
    /// 空列表不证明其它种类的 loan 仍活跃或已经结束。
    #[must_use]
    pub fn direct_call_loan_ends(&self) -> &[UnitDirectCallLoanEndFact] {
        &self.direct_call_loan_ends
    }

    /// 真实 source token 跨 inner return 的有界同步 consumer 运输；不是完整 loan-end 流。
    #[must_use]
    pub fn borrow_call_transports(&self) -> &[UnitBorrowCallTransportFact] {
        &self.borrow_call_transports
    }

    /// 真实 source token 从直接或透明 Group initializer 运输到 local binding 实际终止；不是完整流。
    #[must_use]
    pub fn borrow_binding_transports(&self) -> &[UnitBorrowBindingTransportFact] {
        &self.borrow_binding_transports
    }

    /// 真实参数 source token 跨 inner return 交接给当前函数返回；不是 source 结束。
    #[must_use]
    pub fn borrow_forward_transports(&self) -> &[UnitBorrowForwardTransportFact] {
        &self.borrow_forward_transports
    }

    /// 已完成的具名 Shared 根来源借用返回交接；不包含完整 loan-end 流。
    #[must_use]
    pub fn borrow_call_handoffs(&self) -> &[UnitBorrowCallHandoffFact] {
        &self.borrow_call_handoffs
    }

    /// 活跃 initializer 被 return 放弃时，已求值来源 token 的结束观察片。
    #[must_use]
    pub fn borrow_call_prefix_ends(&self) -> &[UnitBorrowCallPrefixEndFact] {
        &self.borrow_call_prefix_ends
    }

    /// 无 receiver、owned-result 的 declaration/symbol/external 调用中，Exclusive
    /// owned 名称根实参因 return/break/continue 放弃调用前缀的实际结束事件。
    /// 排除借用绑定、provider 来源/绑定、投影、临时值和借用结果。
    /// 保持真实动作顺序；空列表不证明其它 loan 的状态，abort 不产生 unwind。
    #[must_use]
    pub fn exclusive_call_prefix_ends(&self) -> &[UnitExclusiveCallPrefixEndFact] {
        &self.exclusive_call_prefix_ends
    }

    /// owned/Borrow/Inout 具名根、字段、具体 this 及首层直接根/字段来源借用绑定的 Shared receiver 实际 unwind 观测。
    /// 只观察 return/break/continue 退休的子 token，不结束绑定的来源 lease 或父 loan。
    /// 不提供正常返回、reservation 或完整 receiver loan-end 流。
    #[must_use]
    pub fn receiver_call_prefix_ends(&self) -> &[UnitReceiverCallPrefixEndFact] {
        &self.receiver_call_prefix_ends
    }

    /// owned/Borrow/Inout 具名根、字段、具体 this 及首层直接根/字段来源借用绑定的 Shared receiver 正常退休 token。
    /// 绑定 operand 与 canonical source lease 身份分开，不把子结束当作父 binding 结束。
    /// 不提供跨事务顺序、完整 receiver 流或动态执行次数；前缀退出另行查询。
    #[must_use]
    pub fn receiver_call_return_ends(&self) -> &[UnitReceiverCallReturnEndFact] {
        &self.receiver_call_return_ends
    }

    /// 返回源码/调用顺序稳定的有效同步 loans。
    #[must_use]
    pub fn loans(&self) -> &[UnitLoanFact] {
        &self.loans
    }

    /// 返回由指定 call 建立的 loan 描述符，按所属 call identity 筛选。
    /// 此查询不表示实际 loan-end 事件；借用结果的来源可延续到后续消费边界。
    pub fn loans_ending_at(&self, call: UnitExpressionId) -> impl Iterator<Item = &UnitLoanFact> {
        self.loans.iter().filter(move |loan| loan.call() == call)
    }

    /// 返回源码/调用顺序稳定的有效 Value deliveries。
    #[must_use]
    pub fn value_deliveries(&self) -> &[UnitValueDeliveryFact] {
        &self.value_deliveries
    }

    /// 返回已批准的 String 显式复制效果。
    #[must_use]
    pub fn string_effects(&self) -> &[UnitStringOwnershipEffect] {
        &self.string_effects
    }
    /// 查询已批准的 String 显式复制效果。
    #[must_use]
    pub fn string_effect(&self, expression: UnitExpressionId) -> Option<UnitStringOwnershipEffect> {
        self.string_effects
            .iter()
            .copied()
            .find(|effect| effect.expression() == expression)
    }

    /// 返回源码顺序稳定的 intrinsic Rc retain/payload-borrow effects。
    #[must_use]
    pub fn rc_effects(&self) -> &[UnitRcOwnershipEffect] {
        &self.rc_effects
    }

    /// 返回源码顺序稳定的 construction ordered-delivery/root plans。
    #[must_use]
    pub fn construction_plans(&self) -> &[UnitConstructionOwnershipPlan] {
        &self.construction_plans
    }

    /// 返回 source/control-flow 顺序稳定的 ASAP drop facts。
    #[must_use]
    pub fn drops(&self) -> &[UnitDropFact] {
        &self.drops
    }

    /// 返回源码/控制流顺序稳定的 `StaticSelf` Value receiver 条件析构事实。
    #[must_use]
    pub fn conditional_receiver_drops(&self) -> &[UnitConditionalReceiverDropFact] {
        &self.conditional_receiver_drops
    }

    /// 返回 lambda/source 顺序稳定的 capture 输入事实。
    #[must_use]
    pub fn captures(&self) -> &[UnitClosureCaptureDescriptor] {
        &self.captures
    }

    /// 返回指定 source-qualified lambda 的 capture 输入事实。
    pub fn captures_of(
        &self,
        lambda: UnitExpressionId,
    ) -> impl Iterator<Item = &UnitClosureCaptureDescriptor> {
        self.captures
            .iter()
            .filter(move |capture| capture.lambda() == lambda)
    }

    /// 查询具体 lambda environment 的能力事实。
    #[must_use]
    pub fn closure(&self, expression: UnitExpressionId) -> Option<UnitClosureDescriptor> {
        self.closures
            .iter()
            .copied()
            .find(|closure| closure.expression() == expression)
    }

    /// 返回 source/lambda 顺序稳定的 closure environment 能力事实。
    #[must_use]
    pub fn closures(&self) -> &[UnitClosureDescriptor] {
        &self.closures
    }

    /// 查询一个 unit-global 类型的结构化跨线程转移能力。
    #[must_use]
    pub fn transferability(&self, ty: UnitTypeId) -> Option<Transferability> {
        self.transferabilities.get(ty.index()).copied()
    }

    /// 返回阻止 validated codegen gate 的显式 recovery 边界。
    #[must_use]
    pub fn deferred(&self) -> &[UnitOwnershipDeferredFact] {
        &self.deferred
    }

    /// 无 ownership error、drop plan 完整且不含常量来源时才发布基础 codegen view。
    pub fn validate(self) -> Result<ValidatedCompilationUnitOwnership, Box<Self>> {
        let has_error = self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        if !self.provenance.typed_basic_is_valid
            || has_error
            || !self.deferred.is_empty()
            || self.provenance.requires_constant_capability
            || !self.ownership_primitives_are_valid()
            || !self.field_replacements_are_valid()
            || !self.iterations_are_valid()
        {
            Err(Box::new(self))
        } else {
            Ok(ValidatedCompilationUnitOwnership(self))
        }
    }
}
