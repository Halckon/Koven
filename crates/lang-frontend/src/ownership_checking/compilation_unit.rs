//! SPEC-0198 compilation-unit 所有权产物、身份门禁与 body-local call 数据流。

mod dataflow;

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    name_resolution::{SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames},
    parser::Expression,
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, ParameterMode, TypeEnvironment, UnitCallTarget,
        UnitCallableSignature, UnitExpressionId, UnitTypeId, ValidatedCompilationUnitTypes,
    },
};

use super::{
    ElementIndexIdentity, LoanKind, OwnershipBindingKind, OwnershipCheckingError,
    RcOwnershipEffectKind,
};

/// compilation-unit callable 参数在 Phase 3 中提供的能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitOwnershipBindingDescriptor {
    symbol: UnitSymbolId,
    kind: OwnershipBindingKind,
    declaration_span: Span,
}

impl UnitOwnershipBindingDescriptor {
    const fn new(symbol: UnitSymbolId, kind: OwnershipBindingKind, declaration_span: Span) -> Self {
        Self {
            symbol,
            kind,
            declaration_span,
        }
    }

    /// 返回 source-qualified 参数 symbol。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回 owned/shared/exclusive 能力。
    #[must_use]
    pub const fn kind(self) -> OwnershipBindingKind {
        self.kind
    }

    /// 返回参数声明范围；后续跨文件诊断可直接引用该位置。
    #[must_use]
    pub const fn declaration_span(self) -> Span {
        self.declaration_span
    }
}

/// typed call argument 在 ownership 阶段采用的规范契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitCallArgumentOwnershipKind {
    /// 向 Value 参数交付 owned copy 或 move；具体效果由后续数据流决定。
    Value,
    /// 要求后续 body-local checker 在同步调用期间建立 shared loan。
    SharedLoan,
    /// 要求后续 body-local checker 在同步调用期间建立 exclusive loan。
    ExclusiveLoan,
}

/// 一个 source-qualified call argument 的 ownership 输入契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCallArgumentOwnershipContract {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    parameter_index: usize,
    parameter_type: UnitTypeId,
    category: ExpressionCategory,
    kind: UnitCallArgumentOwnershipKind,
    crosses_thread: bool,
    argument_span: Span,
    call_span: Span,
    parameter_span: Option<Span>,
    loan_begin_span: Span,
}

impl UnitCallArgumentOwnershipContract {
    /// 返回所属 call expression。
    #[must_use]
    pub const fn call(self) -> UnitExpressionId {
        self.call
    }

    /// 返回源码实参 expression。
    #[must_use]
    pub const fn argument(self) -> UnitExpressionId {
        self.argument
    }

    /// 返回声明顺序参数下标。
    #[must_use]
    pub const fn parameter_index(self) -> usize {
        self.parameter_index
    }

    /// 返回 callable 实例化后的参数类型。
    #[must_use]
    pub const fn parameter_type(self) -> UnitTypeId {
        self.parameter_type
    }

    /// 返回类型阶段确认的 place/temporary 类别。
    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    /// 返回 Value/shared-loan/exclusive-loan 契约。
    #[must_use]
    pub const fn kind(self) -> UnitCallArgumentOwnershipKind {
        self.kind
    }

    /// 返回参数是否由 compiler-bound effect 跨线程交付。
    #[must_use]
    pub const fn crosses_thread(self) -> bool {
        self.crosses_thread
    }

    /// 返回实参值表达式范围。
    #[must_use]
    pub const fn argument_span(self) -> Span {
        self.argument_span
    }

    /// 返回同步 call 的完整表达式范围。
    #[must_use]
    pub const fn call_span(self) -> Span {
        self.call_span
    }

    /// 返回源码 callable 参数声明范围；external/function-value/structural-component target
    /// 没有源码参数范围。
    #[must_use]
    pub const fn parameter_span(self) -> Option<Span> {
        self.parameter_span
    }

    /// 返回 loan 的真实起点；Inout 使用调用点 `&`，其余使用 operand。
    #[must_use]
    pub const fn loan_begin_span(self) -> Span {
        self.loan_begin_span
    }
}

/// compilation-unit 中由根 binding、字段路径及可选 terminal element 组成的稳定 place。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitOwnershipPlace {
    root: UnitSymbolId,
    fields: Vec<UnitSymbolId>,
    element: Option<ElementIndexIdentity>,
}

impl UnitOwnershipPlace {
    pub(super) fn new(root: UnitSymbolId, fields: Vec<UnitSymbolId>) -> Self {
        Self {
            root,
            fields,
            element: None,
        }
    }

    pub(super) fn push_field(&mut self, field: UnitSymbolId) -> bool {
        if self.element.is_some() {
            return false;
        }
        self.fields.push(field);
        true
    }

    pub(super) fn push_element(&mut self, element: ElementIndexIdentity) -> bool {
        if self.element.is_some() {
            return false;
        }
        self.element = Some(element);
        true
    }

    /// 返回拥有该 place 的 source-qualified 根 binding。
    #[must_use]
    pub const fn root(&self) -> UnitSymbolId {
        self.root
    }

    /// 返回从根到目标的字段路径。
    #[must_use]
    pub fn fields(&self) -> &[UnitSymbolId] {
        &self.fields
    }

    /// 返回 terminal 顺序容器逻辑索引。
    #[must_use]
    pub const fn element(&self) -> Option<ElementIndexIdentity> {
        self.element
    }

    pub(super) fn overlaps(&self, other: &Self) -> bool {
        if self.root != other.root
            || !self
                .fields
                .iter()
                .zip(&other.fields)
                .all(|(left, right)| left == right)
        {
            return false;
        }
        if self.fields.len() != other.fields.len() {
            return true;
        }
        match (self.element, other.element) {
            (Some(left), Some(right)) => left.may_alias(right),
            (None, _) | (_, None) => true,
        }
    }
}

/// unit loan 的稳定目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitLoanTarget {
    /// 名称、字段或 terminal element place。
    Place(UnitOwnershipPlace),
    /// 延长到同步调用返回的 temporary。
    Temporary(UnitExpressionId),
}

/// 一次成功建立并在同步 call 返回时结束的 source-qualified loan。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanFact {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    target: UnitLoanTarget,
    kind: LoanKind,
    begin_span: Span,
    end_span: Span,
    parameter_span: Option<Span>,
}

impl UnitLoanFact {
    pub(super) const fn new(
        call: UnitExpressionId,
        argument: UnitExpressionId,
        target: UnitLoanTarget,
        kind: LoanKind,
        begin_span: Span,
        end_span: Span,
        parameter_span: Option<Span>,
    ) -> Self {
        Self {
            call,
            argument,
            target,
            kind,
            begin_span,
            end_span,
            parameter_span,
        }
    }

    /// 返回所属同步 call。
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }

    /// 返回建立 loan 的源码实参。
    #[must_use]
    pub const fn argument(&self) -> UnitExpressionId {
        self.argument
    }

    /// 返回 place 或 temporary 目标。
    #[must_use]
    pub const fn target(&self) -> &UnitLoanTarget {
        &self.target
    }

    /// 返回 shared/exclusive loan 种类。
    #[must_use]
    pub const fn kind(&self) -> LoanKind {
        self.kind
    }

    /// 返回 loan 生效位置。
    #[must_use]
    pub const fn begin_span(&self) -> Span {
        self.begin_span
    }

    /// 返回同步 call 结束位置。
    #[must_use]
    pub const fn end_span(&self) -> Span {
        self.end_span
    }

    /// 返回被选择源码参数的声明位置；external/function-value 没有该位置。
    #[must_use]
    pub const fn parameter_span(&self) -> Option<Span> {
        self.parameter_span
    }
}

/// 向 Value 参数交付值时的实际所有权效果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitValueDeliveryKind {
    /// `Copyable` place 按值复制，源仍可用。
    Copy,
    /// MoveOnly owned place 被移动，源随后不可用。
    Move,
    /// 本次求值产生的 temporary 被直接交付。
    Temporary,
}

/// Value delivery 的可追溯来源。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitValueDeliverySource {
    /// 名称、字段或具名容器 element place。
    Place(UnitOwnershipPlace),
    /// 直接产生值的 temporary。
    Temporary(UnitExpressionId),
    /// 由 owner 支撑、但本身不是可移动 place 的 payload/element projection。
    BorrowedProjection {
        /// 保持投影值存活的 owner expression。
        owner: UnitExpressionId,
    },
}

/// 一个已经由 body-local 数据流确认的 Value argument delivery。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitValueDeliveryFact {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    source: UnitValueDeliverySource,
    kind: UnitValueDeliveryKind,
    span: Span,
    parameter_span: Option<Span>,
}

/// 一个 source-qualified intrinsic `Rc<T>` ownership effect。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitRcOwnershipEffect {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    payload_type: UnitTypeId,
    kind: RcOwnershipEffectKind,
}

impl UnitRcOwnershipEffect {
    pub(super) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        payload_type: UnitTypeId,
        kind: RcOwnershipEffectKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            payload_type,
            kind,
        }
    }

    /// 返回产生结果的 source-qualified expression。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回被读取但不消费的 shared-owner receiver。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回 control block payload 的 unit-global 类型。
    #[must_use]
    pub const fn payload_type(self) -> UnitTypeId {
        self.payload_type
    }

    /// 返回 retain 或 payload borrow effect。
    #[must_use]
    pub const fn kind(self) -> RcOwnershipEffectKind {
        self.kind
    }
}

impl UnitValueDeliveryFact {
    pub(super) const fn new(
        call: UnitExpressionId,
        argument: UnitExpressionId,
        source: UnitValueDeliverySource,
        kind: UnitValueDeliveryKind,
        span: Span,
        parameter_span: Option<Span>,
    ) -> Self {
        Self {
            call,
            argument,
            source,
            kind,
            span,
            parameter_span,
        }
    }

    /// 返回所属 call。
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }

    /// 返回源码实参。
    #[must_use]
    pub const fn argument(&self) -> UnitExpressionId {
        self.argument
    }

    /// 返回 place；temporary/borrowed projection 为 `None`。
    #[must_use]
    pub const fn place(&self) -> Option<&UnitOwnershipPlace> {
        match &self.source {
            UnitValueDeliverySource::Place(place) => Some(place),
            UnitValueDeliverySource::Temporary(_)
            | UnitValueDeliverySource::BorrowedProjection { .. } => None,
        }
    }

    /// 返回完整 delivery source。
    #[must_use]
    pub const fn source(&self) -> &UnitValueDeliverySource {
        &self.source
    }

    /// 返回 copy/move/temporary 效果。
    #[must_use]
    pub const fn kind(&self) -> UnitValueDeliveryKind {
        self.kind
    }

    /// 返回交付操作数的源码范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// 返回被选择源码参数的声明位置。
    #[must_use]
    pub const fn parameter_span(&self) -> Option<Span> {
        self.parameter_span
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnitOwnershipProvenance {
    typed_analysis_owner: Arc<()>,
    analysis_owner: Arc<()>,
}

/// SPEC-0198 的 recovery compilation-unit ownership product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitOwnership {
    provenance: UnitOwnershipProvenance,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<UnitOwnershipBindingDescriptor>,
    call_argument_contracts: Vec<UnitCallArgumentOwnershipContract>,
    loans: Vec<UnitLoanFact>,
    value_deliveries: Vec<UnitValueDeliveryFact>,
    rc_effects: Vec<UnitRcOwnershipEffect>,
}

impl CompilationUnitOwnership {
    fn new(
        typed: &CompilationUnitTypes,
        diagnostics: Vec<Diagnostic>,
        bindings: Vec<UnitOwnershipBindingDescriptor>,
        call_argument_contracts: Vec<UnitCallArgumentOwnershipContract>,
        loans: Vec<UnitLoanFact>,
        value_deliveries: Vec<UnitValueDeliveryFact>,
        rc_effects: Vec<UnitRcOwnershipEffect>,
    ) -> Self {
        Self {
            provenance: UnitOwnershipProvenance {
                typed_analysis_owner: Arc::clone(typed.analysis_owner()),
                analysis_owner: Arc::new(()),
            },
            diagnostics,
            bindings,
            call_argument_contracts,
            loans,
            value_deliveries,
            rc_effects,
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

    /// 返回源码/调用顺序稳定的有效同步 loans。
    #[must_use]
    pub fn loans(&self) -> &[UnitLoanFact] {
        &self.loans
    }

    /// 返回在指定 call 结束的 loans。
    pub fn loans_ending_at(&self, call: UnitExpressionId) -> impl Iterator<Item = &UnitLoanFact> {
        self.loans.iter().filter(move |loan| loan.call() == call)
    }

    /// 返回源码/调用顺序稳定的有效 Value deliveries。
    #[must_use]
    pub fn value_deliveries(&self) -> &[UnitValueDeliveryFact] {
        &self.value_deliveries
    }

    /// 返回源码顺序稳定的 intrinsic Rc retain/payload-borrow effects。
    #[must_use]
    pub fn rc_effects(&self) -> &[UnitRcOwnershipEffect] {
        &self.rc_effects
    }
}

/// 建立 source-qualified compilation-unit ownership recovery product。
///
/// 当前发布 callable parameter bindings、call argument contracts、普通 call loan/value
/// deliveries 与 intrinsic Rc effects。constructor/container ordered delivery、drop/capture
/// 与 validated codegen gate 仍由同一 SPEC 的后续切片接入。
pub fn check_compilation_unit_ownership(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<CompilationUnitOwnership, OwnershipCheckingError> {
    let typed = typed.types();
    if !typed.is_compatible_with(sources, inputs, names, environment) {
        return Err(OwnershipCheckingError::MismatchedCompilationUnitTypes);
    }

    let mut bindings = BTreeMap::new();
    for declaration in typed.signatures().declarations() {
        if let Some(callable) = declaration.callable() {
            collect_callable_bindings(callable, names, &mut bindings)?;
        }
        if let Some(nominal) = declaration.nominal() {
            for callable in nominal.members().iter().chain(nominal.companion_members()) {
                collect_callable_bindings(callable, names, &mut bindings)?;
            }
        }
    }
    for (symbol, mode) in typed.body_parameter_modes() {
        let span = unit_symbol_span(names, *symbol)?;
        insert_binding(
            names,
            &mut bindings,
            UnitOwnershipBindingDescriptor::new(*symbol, binding_kind(*mode), span),
        )?;
    }
    let call_argument_contracts = collect_call_argument_contracts(inputs, names, typed)?;
    let mut dataflow = dataflow::analyze(
        sources,
        inputs,
        names,
        typed,
        &bindings,
        &call_argument_contracts,
    )?;
    if !dataflow.diagnostics.is_empty() {
        dataflow.loans.clear();
        dataflow.value_deliveries.clear();
        dataflow.rc_effects.clear();
    }

    Ok(CompilationUnitOwnership::new(
        typed,
        dataflow.diagnostics,
        bindings.into_values().collect(),
        call_argument_contracts,
        dataflow.loans,
        dataflow.value_deliveries,
        dataflow.rc_effects,
    ))
}

fn collect_call_argument_contracts(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<Vec<UnitCallArgumentOwnershipContract>, OwnershipCheckingError> {
    let mut contracts = Vec::new();
    for call in typed.calls() {
        let call_id = call.expression();
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let Expression::Call { arguments, .. } = call_node.payload() else {
            return Err(invalid_unit_call(call_id));
        };
        if arguments.len() != call.arguments().len() {
            return Err(invalid_unit_call(call_id));
        }
        let mut seen_arguments = vec![false; arguments.len()];
        let mut seen_parameters = vec![false; arguments.len()];
        for descriptor in call.arguments() {
            let argument_index = descriptor.argument_index();
            let Some(argument) = arguments.get(argument_index) else {
                return Err(invalid_unit_call_argument(call_id, argument_index));
            };
            if std::mem::replace(&mut seen_arguments[argument_index], true) {
                return Err(invalid_unit_call_argument(call_id, argument_index));
            }
            let parameter_index = descriptor.parameter_index();
            let Some(seen_parameter) = seen_parameters.get_mut(parameter_index) else {
                return Err(invalid_unit_call_parameter(call_id, parameter_index));
            };
            if std::mem::replace(seen_parameter, true) {
                return Err(invalid_unit_call_parameter(call_id, parameter_index));
            }
            let parameter_span =
                source_parameter_span(typed, call.target(), parameter_index, call_id)?;
            contracts.push(UnitCallArgumentOwnershipContract {
                call: call_id,
                argument: UnitExpressionId::new(call_id.source_unit(), argument.value),
                parameter_index,
                parameter_type: descriptor.parameter_type(),
                category: descriptor.category(),
                kind: match descriptor.mode() {
                    ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
                    ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
                    ParameterMode::Inout => UnitCallArgumentOwnershipKind::ExclusiveLoan,
                },
                crosses_thread: descriptor.crosses_thread(),
                argument_span: parsed.ast().expressions().get(argument.value)?.span(),
                call_span: call_node.span(),
                parameter_span,
                loan_begin_span: match argument.mode_marker {
                    Some(crate::parser::ParameterModeMarker::Inout(span)) => span,
                    _ => parsed.ast().expressions().get(argument.value)?.span(),
                },
            });
        }
        if seen_arguments.iter().any(|seen| !seen) {
            return Err(invalid_unit_call(call_id));
        }
        if let Some(parameter) = seen_parameters.iter().position(|seen| !seen) {
            return Err(invalid_unit_call_parameter(call_id, parameter));
        }
    }
    Ok(contracts)
}

fn parsed_for_call<'parsed>(
    inputs: &[SourceUnitInput<'parsed>],
    names: &ValidatedCompilationUnitNames,
    call: UnitExpressionId,
) -> Result<&'parsed crate::parser::ParsedFile, OwnershipCheckingError> {
    let source_unit = call.source_unit();
    let source_id = names
        .names()
        .index()
        .source_units()
        .get(source_unit.index())
        .map(|source| source.source_id())
        .ok_or_else(|| invalid_unit_call(call))?;
    inputs
        .iter()
        .copied()
        .find(|input| input.source_id() == source_id)
        .map(SourceUnitInput::parsed)
        .ok_or_else(|| invalid_unit_call(call))
}

fn source_parameter_span(
    typed: &CompilationUnitTypes,
    target: UnitCallTarget,
    parameter_index: usize,
    call: UnitExpressionId,
) -> Result<Option<Span>, OwnershipCheckingError> {
    let signature = match target {
        UnitCallTarget::Declaration(declaration) => typed
            .signatures()
            .declaration(declaration)
            .and_then(|declaration| declaration.callable()),
        UnitCallTarget::Symbol(symbol) => typed
            .signatures()
            .declarations()
            .iter()
            .flat_map(|declaration| {
                declaration.callable().into_iter().chain(
                    declaration.nominal().into_iter().flat_map(|nominal| {
                        nominal.members().iter().chain(nominal.companion_members())
                    }),
                )
            })
            .find(|callable| {
                callable.target() == crate::type_checking::UnitCallableTarget::Symbol(symbol)
            }),
        UnitCallTarget::External(_)
        | UnitCallTarget::FunctionValue
        | UnitCallTarget::StructuralComponent(_) => return Ok(None),
    };
    signature
        .and_then(|signature| signature.parameters().get(parameter_index))
        .map(|parameter| Some(parameter.span()))
        .ok_or(OwnershipCheckingError::InvalidUnitCallParameter {
            source_unit: call.source_unit().index(),
            expression: call.expression().index(),
            parameter: parameter_index,
        })
}

const fn invalid_unit_call(call: UnitExpressionId) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitCall {
        source_unit: call.source_unit().index(),
        expression: call.expression().index(),
    }
}

const fn invalid_unit_call_argument(
    call: UnitExpressionId,
    argument: usize,
) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitCallArgument {
        source_unit: call.source_unit().index(),
        expression: call.expression().index(),
        argument,
    }
}

const fn invalid_unit_call_parameter(
    call: UnitExpressionId,
    parameter: usize,
) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitCallParameter {
        source_unit: call.source_unit().index(),
        expression: call.expression().index(),
        parameter,
    }
}

fn collect_callable_bindings(
    callable: &UnitCallableSignature,
    names: &ValidatedCompilationUnitNames,
    bindings: &mut BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
) -> Result<(), OwnershipCheckingError> {
    for parameter in callable.parameters() {
        let Some(symbol) = parameter.symbol() else {
            continue;
        };
        insert_binding(
            names,
            bindings,
            UnitOwnershipBindingDescriptor::new(
                symbol,
                binding_kind(parameter.mode()),
                parameter.span(),
            ),
        )?;
    }
    Ok(())
}

fn insert_binding(
    names: &ValidatedCompilationUnitNames,
    bindings: &mut BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
    descriptor: UnitOwnershipBindingDescriptor,
) -> Result<(), OwnershipCheckingError> {
    let symbol = descriptor.symbol();
    unit_symbol_span(names, symbol)?;
    if bindings.insert(symbol, descriptor).is_some() {
        return Err(OwnershipCheckingError::DuplicateUnitBinding {
            source_unit: symbol.source_unit().index(),
            symbol: symbol.symbol().index(),
        });
    }
    Ok(())
}

fn unit_symbol_span(
    names: &ValidatedCompilationUnitNames,
    symbol: UnitSymbolId,
) -> Result<Span, OwnershipCheckingError> {
    names
        .names()
        .source_units()
        .get(symbol.source_unit().index())
        .and_then(|source| source.resolution().symbols().get(symbol.symbol().index()))
        .map(|symbol| symbol.span())
        .ok_or(OwnershipCheckingError::InvalidUnitSymbol {
            source_unit: symbol.source_unit().index(),
            symbol: symbol.symbol().index(),
        })
}

const fn binding_kind(mode: ParameterMode) -> OwnershipBindingKind {
    match mode {
        ParameterMode::Value => OwnershipBindingKind::Owned,
        ParameterMode::Borrow => OwnershipBindingKind::Shared,
        ParameterMode::Inout => OwnershipBindingKind::Exclusive,
    }
}
