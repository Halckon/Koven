//! SPEC-0198 compilation-unit 所有权产物、身份门禁与参数 binding 能力。

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    name_resolution::{SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames},
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, ParameterMode, TypeEnvironment, UnitCallableSignature,
        ValidatedCompilationUnitTypes,
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
}

impl CompilationUnitOwnership {
    fn new(
        typed: &CompilationUnitTypes,
        diagnostics: Vec<Diagnostic>,
        bindings: Vec<UnitOwnershipBindingDescriptor>,
    ) -> Self {
        Self {
            provenance: UnitOwnershipProvenance {
                typed_analysis_owner: Arc::clone(typed.analysis_owner()),
                analysis_owner: Arc::new(()),
            },
            diagnostics,
            bindings,
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

    Ok(CompilationUnitOwnership::new(
        typed,
        Vec::new(),
        bindings.into_values().collect(),
    ))
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
