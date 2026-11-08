//! SPEC-0197 compilation-unit overload candidate 的事务化试算状态。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity},
    name_resolution::{SourceUnitId, UnitSymbolId},
    source::Span,
    type_checking::{
        CompilationUnitSignatures, DeferredReason, UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::{
    BodyChecker, CompilationUnitTypeParts, FlowKey, UnitExpressionId, UnitStatementId,
    flow::FlowVersions,
};

#[derive(Clone, Copy)]
pub(super) enum DeferredFact {
    Expression(UnitExpressionId),
    Symbol(UnitSymbolId),
    TypeRef(UnitTypeRefId),
}

/// 一个 unit callable candidate 试算能够改变的完整 typed 状态。
///
/// 名称解析、源码索引与声明图在 body 检查前已经封闭；unit type table、body facts、flow 与
/// diagnostics 必须一起回滚，避免失败候选泄漏后继 ownership 可见的半成品事实。
#[derive(Clone)]
pub(super) struct UnitTrialState {
    signatures: CompilationUnitSignatures,
    flow_facts: BTreeMap<FlowKey, UnitTypeId>,
    flow_versions: FlowVersions,
    pending_immutable_bindings: BTreeSet<UnitSymbolId>,
    replacement_values: BTreeMap<UnitExpressionId, bool>,
    active_nullable_whens: Vec<super::nullable_when::ActiveWhenFrame>,
    pending_loop_exits: Vec<super::while_flow::PendingLoopExits>,
    pending_boolean_edges: BTreeMap<UnitExpressionId, super::while_flow::PendingBooleanEdges>,
    pending_abrupt_sites: BTreeSet<super::while_flow::PendingControlSite>,
    pending_callable_returns: Option<super::call_writeback::PendingCallableReturns>,
    closure_effects: super::closure_writeback::ClosureEffects,
    closure_origins: super::closure_writeback::ClosureOrigins,
    discovered_loop_heads: BTreeSet<UnitStatementId>,
    parts: CompilationUnitTypeParts,
    diagnostics: Vec<Diagnostic>,
    deferred_facts: Vec<DeferredFact>,
}

impl BodyChecker<'_> {
    pub(super) fn note_pending_storage_value(
        &mut self,
        expression: UnitExpressionId,
        ty: UnitTypeId,
    ) {
        self.parts
            .storage_recovery
            .pending_values
            .insert(expression, ty);
        self.deferred_facts
            .push(DeferredFact::Expression(expression));
    }

    pub(super) fn storage_value_is_pending(&self, expression: UnitExpressionId) -> bool {
        self.parts
            .storage_recovery
            .pending_values
            .get(&expression)
            .is_some_and(|ty| self.parts.expression_types.get(&expression) == Some(ty))
    }

    pub(super) fn has_pending_storage_value_since(&self, start: usize) -> bool {
        self.deferred_facts[start..].iter().any(|fact| {
            matches!(fact, DeferredFact::Expression(key) if self.storage_value_is_pending(*key))
        })
    }

    pub(super) fn has_pending_storage_operands(
        &self,
        source: SourceUnitId,
        receiver: ExpressionId,
        index: ExpressionId,
        deferred: usize,
    ) -> bool {
        self.has_pending_storage_value_since(deferred)
            || self.storage_value_is_pending(UnitExpressionId::new(source, receiver))
            || self.storage_value_is_pending(UnitExpressionId::new(source, index))
    }

    /// Return a representative rejection only when the trial added no independent error.
    pub(super) fn repeated_storage_failure_since(
        &self,
        start: usize,
    ) -> Option<(Diagnostic, Span)> {
        let repeated = &self.parts.repeated_storage_diagnostics;
        let mut origin = None;
        for (offset, diagnostic) in self.diagnostics[start..].iter().enumerate() {
            if diagnostic.severity() == Severity::Error {
                let span = *repeated.get(&(start + offset))?;
                origin.get_or_insert((diagnostic, span));
            }
        }
        origin.map(|(diagnostic, span)| (diagnostic.clone(), span))
    }

    pub(super) fn trial_state(&self) -> UnitTrialState {
        UnitTrialState {
            signatures: self.signatures.clone(),
            flow_facts: self.flow_facts.clone(),
            flow_versions: self.flow_versions.clone(),
            pending_immutable_bindings: self.pending_immutable_bindings.clone(),
            replacement_values: self.replacement_values.clone(),
            active_nullable_whens: self.active_nullable_whens.clone(),
            pending_loop_exits: self.pending_loop_exits.clone(),
            pending_boolean_edges: self.pending_boolean_edges.clone(),
            pending_abrupt_sites: self.pending_abrupt_sites.clone(),
            pending_callable_returns: self.pending_callable_returns.clone(),
            closure_effects: self.closure_effects.clone(),
            closure_origins: self.closure_origins.clone(),
            discovered_loop_heads: self.discovered_loop_heads.clone(),
            parts: self.parts.clone(),
            diagnostics: self.diagnostics.clone(),
            deferred_facts: self.deferred_facts.clone(),
        }
    }

    pub(super) fn restore_trial_state(&mut self, state: UnitTrialState) {
        self.signatures = state.signatures;
        self.flow_facts = state.flow_facts;
        self.flow_versions = state.flow_versions;
        self.pending_immutable_bindings = state.pending_immutable_bindings;
        self.replacement_values = state.replacement_values;
        self.active_nullable_whens = state.active_nullable_whens;
        self.pending_loop_exits = state.pending_loop_exits;
        self.pending_boolean_edges = state.pending_boolean_edges;
        self.pending_abrupt_sites = state.pending_abrupt_sites;
        self.pending_callable_returns = state.pending_callable_returns;
        self.closure_effects = state.closure_effects;
        self.closure_origins = state.closure_origins;
        self.discovered_loop_heads = state.discovered_loop_heads;
        self.parts = state.parts;
        self.diagnostics = state.diagnostics;
        self.deferred_facts = state.deferred_facts;
    }

    pub(super) fn note_deferred_fact(
        &mut self,
        fact: DeferredFact,
        previous: Option<UnitTypeId>,
        ty: UnitTypeId,
    ) {
        if previous != Some(ty) && self.is_deferred(ty) {
            self.deferred_facts.push(fact);
        }
    }

    /// Contextual typing can produce a known outer type while children remain
    /// unresolved. Inspect only the deferred writes made within this check.
    pub(super) fn has_deferred_facts_since(&self, start: usize) -> bool {
        self.deferred_facts_since(start, true)
    }

    pub(super) fn has_unresolved_argument_facts_since(&self, start: usize) -> bool {
        // A type ref spelled Any has a known unsupported representation.
        // Preserve its instance root for the existing body storage check.
        self.deferred_facts_since(start, false)
    }

    fn deferred_facts_since(&self, start: usize, include_any_type_ref: bool) -> bool {
        self.deferred_facts[start..].iter().any(|fact| {
            let ty = match fact {
                DeferredFact::Expression(key) => {
                    if self.storage_value_is_pending(*key) {
                        return true;
                    }
                    self.parts.expression_types.get(key)
                }
                DeferredFact::Symbol(key) => self.parts.symbol_types.get(key),
                DeferredFact::TypeRef(key) => self.parts.type_ref_types.get(key),
            };
            matches!(
                ty.and_then(|&ty| self.signatures.types().get(ty)),
                Some(UnitTypeKind::Deferred(reason))
                    if include_any_type_ref
                        || !matches!(fact, DeferredFact::TypeRef(_))
                        || *reason != DeferredReason::AnyValueRepresentation
            )
        })
    }
}
