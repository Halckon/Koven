//! SPEC-0198 compilation-unit 所有权产物、身份门禁与参数 binding 能力。

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    name_resolution::{SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames},
    parser::Expression,
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, ParameterMode, TypeEnvironment, UnitCallTarget,
        UnitCallableSignature, UnitExpressionId, UnitTypeId, ValidatedCompilationUnitTypes,
    },
};

use super::{OwnershipBindingKind, OwnershipCheckingError};

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
    kind: UnitCallArgumentOwnershipKind,
    crosses_thread: bool,
    argument_span: Span,
    call_span: Span,
    parameter_span: Option<Span>,
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
}

impl CompilationUnitOwnership {
    fn new(
        typed: &CompilationUnitTypes,
        diagnostics: Vec<Diagnostic>,
        bindings: Vec<UnitOwnershipBindingDescriptor>,
        call_argument_contracts: Vec<UnitCallArgumentOwnershipContract>,
    ) -> Self {
        Self {
            provenance: UnitOwnershipProvenance {
                typed_analysis_owner: Arc::clone(typed.analysis_owner()),
                analysis_owner: Arc::new(()),
            },
            diagnostics,
            bindings,
            call_argument_contracts,
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
}

/// 建立 compilation-unit ownership product 的首个纵向切片。
///
/// 当前先发布所有 source/member/lambda callable 参数的 source-qualified binding 能力；
/// call loan、move/drop/capture 将在同一 SPEC 的后续切片接入，不会伪造空 facts。
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

    Ok(CompilationUnitOwnership::new(
        typed,
        Vec::new(),
        bindings.into_values().collect(),
        call_argument_contracts,
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
                kind: match descriptor.mode() {
                    ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
                    ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
                    ParameterMode::Inout => UnitCallArgumentOwnershipKind::ExclusiveLoan,
                },
                crosses_thread: descriptor.crosses_thread(),
                argument_span: parsed.ast().expressions().get(argument.value)?.span(),
                call_span: call_node.span(),
                parameter_span,
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
