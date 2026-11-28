//! SPEC-0197 compilation-unit type-test 与稳定 place 流事实。

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::{
        Namespace, ReferenceTarget, ScopeKind, SourceUnitId, SymbolKind, UnitReferenceTarget,
        UnitSymbolId,
    },
    parser::{BinaryOperator, Expression, Item, NameMarker, PrefixOperator, VariableKind},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, NominalKind, TypeCheckingError, UnitTypeId,
        UnitTypeKind, UnitTypeRefId,
    },
};

use super::{BodyChecker, ExpressionCheck};

mod snapshot;
use snapshot::Snapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FlowKey {
    Symbol(UnitSymbolId),
    This,
}

/// Narrowings in the current body state. Snapshots share an unchanged map.
#[derive(Clone, Default)]
pub(super) struct FlowFacts(Snapshot<UnitTypeId>);

impl FlowFacts {
    fn set(&mut self, key: FlowKey, ty: Option<UnitTypeId>) -> usize {
        let copies = self.0.set(key, ty);
        #[cfg(test)]
        crate::type_checking::short_circuit_cost_tests::record_flow_fact_copies(copies);
        copies
    }

    pub(super) fn get(&self, key: &FlowKey) -> Option<&UnitTypeId> {
        self.0.get(*key)
    }

    pub(super) fn insert(&mut self, key: FlowKey, ty: UnitTypeId) {
        self.set(key, Some(ty));
    }

    pub(super) fn insert_if_absent(&mut self, key: FlowKey, ty: UnitTypeId) {
        if self.get(&key).is_none() {
            self.insert(key, ty);
        }
    }

    pub(super) fn remove(&mut self, key: &FlowKey) {
        self.set(*key, None);
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn extend(&mut self, facts: BTreeMap<FlowKey, UnitTypeId>) {
        for (key, ty) in facts {
            self.insert(key, ty);
        }
    }

    pub(super) fn with_conditions(&self, conditions: &BTreeMap<FlowKey, UnitTypeId>) -> Self {
        let mut result = self.clone();
        result.apply_conditions(conditions);
        result
    }

    pub(super) fn apply_conditions(&mut self, conditions: &BTreeMap<FlowKey, UnitTypeId>) {
        for (&key, &ty) in conditions {
            if conflicts(self.get(&key).copied(), ty) {
                self.remove(&key);
            } else {
                self.insert(key, ty);
            }
        }
    }

    pub(super) fn intersect(&self, other: &Self) -> Self {
        let (joined, _visits) = self.0.join(&other.0, false, |a, b| (a == b).then_some(a));
        #[cfg(test)]
        crate::type_checking::short_circuit_cost_tests::record_flow_merge_visits(0, _visits);
        Self(joined)
    }
}

#[derive(Default)]
pub(super) struct FlowVersions {
    versions: Snapshot<u64>,
    // Snapshots usually differ only in which tracked bindings are pending.
    tracked_mutables: Arc<BTreeSet<FlowKey>>,
    pending_mutables: BTreeSet<FlowKey>,
}

impl Clone for FlowVersions {
    fn clone(&self) -> Self {
        #[cfg(test)]
        crate::type_checking::short_circuit_cost_tests::record_pending_snapshot_keys(
            self.pending_mutables.len(),
        );
        Self {
            versions: self.versions.clone(),
            tracked_mutables: Arc::clone(&self.tracked_mutables),
            pending_mutables: self.pending_mutables.clone(),
        }
    }
}

impl FlowVersions {
    fn set_version(&mut self, key: FlowKey, version: Option<u64>) -> usize {
        let copies = self.versions.set(key, version);
        #[cfg(test)]
        crate::type_checking::short_circuit_cost_tests::record_flow_version_copies(copies);
        copies
    }

    pub(super) fn copy_version_from(&mut self, key: FlowKey, other: &Self) {
        self.set_version(key, other.versions.get(key).copied());
    }

    pub(super) fn share_versions_from(&mut self, other: &Self) {
        self.versions = other.versions.clone();
    }

    pub(super) fn versions_changed(&self, before: &Self) -> bool {
        self.versions != before.versions
    }

    pub(super) fn restore_bypassed_writes<'a>(
        &mut self,
        facts: &mut FlowFacts,
        before_facts: &FlowFacts,
        before: &Self,
        bypassed: impl IntoIterator<Item = &'a FlowKey>,
    ) {
        for &key in bypassed {
            #[cfg(test)]
            crate::type_checking::short_circuit_cost_tests::record_bypassed_restore_key();
            let previous = before.versions.get(key).copied();
            let current = self.versions.get(key).copied();
            if crate::type_checking::flow_effects::result_edges::can_restore_bypassed_version(
                previous, current,
            ) {
                if let Some(&fact) = before_facts.get(&key) {
                    facts.insert_if_absent(key, fact);
                }
                self.set_version(key, previous);
            }
        }
    }

    pub(super) fn clear(&mut self) {
        self.versions = Snapshot::default();
        self.tracked_mutables = Arc::default();
        self.pending_mutables.clear();
    }

    pub(super) fn declare_mutable(&mut self, symbol: UnitSymbolId, pending: bool) {
        let key = FlowKey::Symbol(symbol);
        if !self.tracks_mutable(key) {
            Arc::make_mut(&mut self.tracked_mutables).insert(key);
        }
        self.replace_pending(key, pending);
    }

    pub(super) fn replace_pending(&mut self, key: FlowKey, pending: bool) {
        if !pending {
            self.pending_mutables.remove(&key);
        } else if self.tracks_mutable(key) {
            self.pending_mutables.insert(key);
        }
    }

    pub(super) fn mutable_is_pending(&self, symbol: UnitSymbolId) -> bool {
        self.pending_mutables.contains(&FlowKey::Symbol(symbol))
    }

    pub(super) fn merge_pending(&mut self, other: &Self) {
        if !Arc::ptr_eq(&self.tracked_mutables, &other.tracked_mutables) {
            Arc::make_mut(&mut self.tracked_mutables)
                .extend(other.tracked_mutables.iter().copied());
        }
        self.pending_mutables
            .extend(other.pending_mutables.iter().copied());
    }

    pub(super) fn take_pending_from(&mut self, other: Self) {
        self.tracked_mutables = other.tracked_mutables;
        self.pending_mutables = other.pending_mutables;
    }

    pub(super) fn tracks_mutable(&self, key: FlowKey) -> bool {
        self.tracked_mutables.contains(&key)
    }

    pub(super) fn pending_keys(&self) -> impl Iterator<Item = FlowKey> + '_ {
        let keys = self.pending_mutables.iter().copied();
        #[cfg(test)]
        let keys = keys.inspect(|_| {
            crate::type_checking::short_circuit_cost_tests::record_pending_scan_keys(1);
        });
        keys
    }

    pub(super) fn mark_pending(&mut self, keys: impl IntoIterator<Item = FlowKey>) -> bool {
        let mut changed = false;
        for key in keys {
            if self.tracks_mutable(key) {
                changed |= self.pending_mutables.insert(key);
            }
        }
        changed
    }

    pub(super) fn clear_pending(&mut self) {
        #[cfg(test)]
        crate::type_checking::short_circuit_cost_tests::record_pending_scan_keys(
            self.pending_mutables.len(),
        );
        self.pending_mutables.clear();
    }

    pub(super) fn set_pending_keys(&mut self, keys: impl IntoIterator<Item = FlowKey>) {
        self.clear_pending();
        self.mark_pending(keys);
    }
}

pub(super) fn merge_flow_versions(left: &FlowVersions, right: &FlowVersions) -> FlowVersions {
    let (versions, _visits) = left
        .versions
        .join(&right.versions, true, |a, b| Some(a.max(b)));
    #[cfg(test)]
    crate::type_checking::short_circuit_cost_tests::record_flow_merge_visits(_visits, 0);
    let mut merged = FlowVersions {
        versions,
        tracked_mutables: Arc::clone(&left.tracked_mutables),
        pending_mutables: left.pending_mutables.clone(),
    };
    merged.merge_pending(right);
    merged
}

pub(super) type ConditionFacts = (BTreeMap<FlowKey, UnitTypeId>, BTreeMap<FlowKey, UnitTypeId>);

pub(super) fn collect_stable_flow_symbols(checker: &BodyChecker<'_>) -> BTreeSet<UnitSymbolId> {
    let mut stable = BTreeSet::new();
    for unit in checker.names.source_units() {
        let source = unit.source_unit();
        let resolution = unit.resolution();
        let mutable = checker
            .file(source)
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Variable {
                    kind: VariableKind::Var,
                    name: NameMarker::Present(span),
                    ..
                } => checker
                    .symbol_at(source, *span, Namespace::Value)
                    .map(UnitSymbolId::symbol),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let mut captured_mutable = BTreeSet::new();
        for reference in resolution.references() {
            let ReferenceTarget::Symbol(symbol) = reference.target() else {
                continue;
            };
            if !mutable.contains(symbol) {
                continue;
            }
            let declaration_scope = resolution.symbols()[symbol.index()].scope();
            let mut current = Some(reference.scope());
            while let Some(scope) = current {
                if scope == declaration_scope {
                    break;
                }
                if resolution.scopes()[scope.index()].kind() == ScopeKind::Lambda {
                    captured_mutable.insert(*symbol);
                    break;
                }
                current = resolution.scopes()[scope.index()].parent();
            }
        }
        stable.extend(resolution.symbols().iter().filter_map(|symbol| {
            let eligible = match symbol.kind() {
                SymbolKind::ValueParameter | SymbolKind::LambdaParameter => true,
                SymbolKind::Variable => {
                    resolution.scopes()[symbol.scope().index()].kind() != ScopeKind::File
                }
                _ => false,
            };
            (eligible && !captured_mutable.contains(&symbol.id()))
                .then_some(UnitSymbolId::new(source, symbol.id()))
        }));
    }
    stable
}

impl BodyChecker<'_> {
    pub(super) fn flow_version(&self, key: FlowKey) -> u64 {
        self.flow_versions
            .versions
            .get(key)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn invalidate_flow_key(&mut self, key: FlowKey) {
        let _fact_copies = self.flow_facts.set(key, None);
        // MAX is a permanent invalid source witness, never a reusable snapshot version.
        let version = self.flow_version(key);
        let _version_copies = if version != u64::MAX {
            self.flow_versions.set_version(key, Some(version + 1))
        } else {
            0
        };
        #[cfg(test)]
        crate::type_checking::short_circuit_cost_tests::record_invalidation_copies(
            _version_copies,
            _fact_copies,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_type_test(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        operator_span: Span,
        type_ref: TypeRefId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let actual = self.check_expression(source, expression, None, None, return_type)?;
        let target = self.resolve_type_test_ref(source, type_ref, Some(actual.ty))?;
        if !self.is_error(actual.ty)
            && !self.is_error(target)
            && !self.valid_type_test_relation(actual.ty, target)
        {
            let target_span = self
                .file(source)
                .ast()
                .type_refs()
                .get(type_ref)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_maybe_label(
                codes::INVALID_TYPE_TEST,
                "type test target is not runtime-testable from the operand type",
                operator_span,
                Some(target_span),
                "invalid type-test target",
            )?;
        }
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Boolean),
            falls_through: actual.falls_through,
        })
    }

    pub(super) fn condition_facts(
        &self,
        source: SourceUnitId,
        id: ExpressionId,
    ) -> Result<ConditionFacts, CompilationUnitTypeError> {
        enum Pending {
            Negate,
            Right(BinaryOperator, ExpressionId),
            Join(BinaryOperator, ExpressionId, ConditionFacts),
        }
        let ast = self.file(source).ast();
        let mut next = id;
        // Leave each frame on the heap until its children have supplied their facts.
        let mut pending = Vec::new();
        loop {
            #[cfg(test)]
            crate::type_checking::short_circuit_cost_tests::record_condition_visit();
            let payload = ast
                .expressions()
                .get(next)
                .map_err(TypeCheckingError::from)?
                .payload();
            let mut facts = match *payload {
                Expression::Group { expression } => {
                    next = expression;
                    continue;
                }
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    pending.push(Pending::Negate);
                    next = operand;
                    continue;
                }
                Expression::Binary {
                    left,
                    operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                    right,
                    ..
                } => {
                    pending.push(Pending::Right(operator, right));
                    next = left;
                    continue;
                }
                _ => self.atomic_condition_facts(source, payload)?,
            };
            loop {
                match pending.pop() {
                    None => return Ok(facts),
                    Some(Pending::Negate) => std::mem::swap(&mut facts.0, &mut facts.1),
                    Some(Pending::Right(operator, right)) => {
                        pending.push(Pending::Join(operator, right, facts));
                        next = right;
                        break;
                    }
                    Some(Pending::Join(operator, right, left_facts)) => {
                        facts = self.join_logical_condition_facts(
                            source, operator, left_facts, right, facts,
                        )?;
                    }
                }
            }
        }
    }

    pub(super) fn join_logical_condition_facts(
        &self,
        source: SourceUnitId,
        operator: BinaryOperator,
        (mut left_true, mut left_false): ConditionFacts,
        right: ExpressionId,
        (right_true, right_false): ConditionFacts,
    ) -> Result<ConditionFacts, CompilationUnitTypeError> {
        // Only the facts on the edge reaching the right operand can be invalidated.
        let carried = if operator == BinaryOperator::LogicalAnd {
            &mut left_true
        } else {
            &mut left_false
        };
        if !carried.is_empty() {
            #[cfg(test)]
            crate::type_checking::short_circuit_cost_tests::record_condition_write_scan();
            let writes = self.expression_flow_writes(source, right)?;
            carried.retain(|key, _| !writes.contains(key));
        }
        if operator == BinaryOperator::LogicalAnd {
            let when_true = extend_facts(&left_true, &right_true);
            let right_false_path = extend_facts(&left_true, &right_false);
            let when_false = intersect_facts(&left_false, &right_false_path);
            Ok((when_true, when_false))
        } else {
            let right_true_path = extend_facts(&left_false, &right_true);
            let when_true = intersect_facts(&left_true, &right_true_path);
            let when_false = extend_facts(&left_false, &right_false);
            Ok((when_true, when_false))
        }
    }

    fn atomic_condition_facts(
        &self,
        source: SourceUnitId,
        payload: &Expression,
    ) -> Result<ConditionFacts, CompilationUnitTypeError> {
        match *payload {
            Expression::TypeTest {
                expression,
                negated,
                type_ref,
                ..
            } => {
                let mut positive = BTreeMap::new();
                let target = self
                    .parts
                    .type_ref_types
                    .get(&UnitTypeRefId::new(source, type_ref))
                    .copied()
                    .or_else(|| {
                        self.signatures
                            .type_ref_type(UnitTypeRefId::new(source, type_ref))
                    });
                let actual = self
                    .parts
                    .expression_types
                    .get(&super::UnitExpressionId::new(source, expression))
                    .copied();
                if let (Some(key), Some(actual), Some(target)) =
                    (self.stable_flow_key(source, expression), actual, target)
                    && !self.is_error(actual)
                    && !self.is_error(target)
                    && self.valid_type_test_relation(actual, target)
                {
                    positive.insert(key, target);
                }
                if negated {
                    Ok((BTreeMap::new(), positive))
                } else {
                    Ok((positive, BTreeMap::new()))
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } if matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) => {
                self.null_comparison_facts(source, left, operator, right)
            }
            _ => Ok((BTreeMap::new(), BTreeMap::new())),
        }
    }

    pub(super) fn stable_flow_key(
        &self,
        source: SourceUnitId,
        id: ExpressionId,
    ) -> Option<FlowKey> {
        self.flow_root(source, id)
            .filter(|&key| self.is_stable_flow_root(key))
    }

    pub(super) fn pending_flow_key(
        &self,
        source: SourceUnitId,
        id: ExpressionId,
    ) -> Option<FlowKey> {
        self.flow_root(source, id)
            .filter(|&key| self.flow_versions.tracks_mutable(key))
    }

    pub(super) fn is_stable_flow_root(&self, key: FlowKey) -> bool {
        match key {
            FlowKey::Symbol(symbol) => self.stable_flow_symbols.contains(&symbol),
            FlowKey::This => self.current_receiver.is_some(),
        }
    }

    pub(super) fn flow_root(&self, source: SourceUnitId, id: ExpressionId) -> Option<FlowKey> {
        let node = self.file(source).ast().expressions().get(id).ok()?;
        match node.payload() {
            Expression::Group { expression } => self.flow_root(source, *expression),
            Expression::This => Some(FlowKey::This),
            Expression::Name => {
                let UnitReferenceTarget::Symbol(symbol) =
                    self.reference(source, node.span(), Namespace::Value)?
                else {
                    return None;
                };
                Some(FlowKey::Symbol(*symbol))
            }
            _ => None,
        }
    }

    pub(super) fn valid_type_test_relation(&self, actual: UnitTypeId, target: UnitTypeId) -> bool {
        match self.signatures.types().get(target) {
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                match self.signatures.types().get(actual) {
                    Some(UnitTypeKind::EnumCase {
                        root: actual_root, ..
                    }) => actual_root == root,
                    Some(UnitTypeKind::Nominal { .. }) => actual == *root,
                    Some(UnitTypeKind::Nullable(inner)) => *inner == *root,
                    _ => false,
                }
            }
            Some(UnitTypeKind::Nominal { declaration, .. }) => {
                let is_interface = self
                    .signatures
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .is_some_and(|nominal| nominal.kind() == NominalKind::Interface);
                !is_interface
                    && (actual == target
                        || matches!(
                            self.signatures.types().get(actual),
                            Some(UnitTypeKind::Nullable(inner)) if *inner == target
                        ))
            }
            _ => false,
        }
    }
}

fn conflicts(existing: Option<UnitTypeId>, addition: UnitTypeId) -> bool {
    existing.is_some_and(|existing| existing != addition)
}

fn extend_facts(
    baseline: &BTreeMap<FlowKey, UnitTypeId>,
    additions: &BTreeMap<FlowKey, UnitTypeId>,
) -> BTreeMap<FlowKey, UnitTypeId> {
    let mut result = baseline.clone();
    for (&key, &ty) in additions {
        if conflicts(result.get(&key).copied(), ty) {
            result.remove(&key);
        } else {
            result.insert(key, ty);
        }
    }
    result
}

fn intersect_facts(
    left: &BTreeMap<FlowKey, UnitTypeId>,
    right: &BTreeMap<FlowKey, UnitTypeId>,
) -> BTreeMap<FlowKey, UnitTypeId> {
    left.iter()
        .filter_map(|(key, ty)| (right.get(key) == Some(ty)).then_some((*key, *ty)))
        .collect()
}
