//! SPEC-0197 compilation-unit type-test 与稳定 place 流事实。

use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FlowKey {
    Symbol(UnitSymbolId),
    This,
}

#[derive(Clone, Default)]
pub(super) struct FlowVersions {
    versions: BTreeMap<FlowKey, u64>,
    mutable_pending: BTreeMap<FlowKey, bool>,
}

impl FlowVersions {
    pub(super) fn clear(&mut self) {
        self.versions.clear();
        self.mutable_pending.clear();
    }

    pub(super) fn declare_mutable(&mut self, symbol: UnitSymbolId, pending: bool) {
        self.mutable_pending
            .insert(FlowKey::Symbol(symbol), pending);
    }

    pub(super) fn replace_pending(&mut self, key: FlowKey, pending: bool) {
        if let Some(previous) = self.mutable_pending.get_mut(&key) {
            *previous = pending;
        }
    }

    pub(super) fn mutable_is_pending(&self, symbol: UnitSymbolId) -> bool {
        self.mutable_pending
            .get(&FlowKey::Symbol(symbol))
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn merge_pending(&mut self, other: &Self) {
        for (&key, &pending) in &other.mutable_pending {
            *self.mutable_pending.entry(key).or_default() |= pending;
        }
    }

    pub(super) fn take_pending_from(&mut self, other: Self) {
        self.mutable_pending = other.mutable_pending;
    }

    pub(super) fn tracks_mutable(&self, key: FlowKey) -> bool {
        self.mutable_pending.contains_key(&key)
    }

    pub(super) fn pending_keys(&self) -> impl Iterator<Item = FlowKey> + '_ {
        self.mutable_pending
            .iter()
            .filter_map(|(&key, &pending)| pending.then_some(key))
    }

    pub(super) fn mark_pending(&mut self, keys: impl IntoIterator<Item = FlowKey>) -> bool {
        let mut changed = false;
        for key in keys {
            if let Some(pending) = self.mutable_pending.get_mut(&key) {
                changed |= !*pending;
                *pending = true;
            }
        }
        changed
    }

    pub(super) fn clear_pending(&mut self) {
        self.mutable_pending
            .values_mut()
            .for_each(|pending| *pending = false);
    }

    pub(super) fn set_pending_keys(&mut self, keys: impl IntoIterator<Item = FlowKey>) {
        self.clear_pending();
        self.mark_pending(keys);
    }
}

pub(super) fn merge_flow_versions(left: &FlowVersions, right: &FlowVersions) -> FlowVersions {
    let mut merged = FlowVersions {
        versions: crate::type_checking::flow_effects::merge_flow_versions(
            &left.versions,
            &right.versions,
        ),
        mutable_pending: left.mutable_pending.clone(),
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
            .get(&key)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn invalidate_flow_key(&mut self, key: FlowKey) {
        self.flow_facts.remove(&key);
        let version = self.flow_versions.versions.entry(key).or_default();
        // MAX is a permanent invalid source witness, never a reusable snapshot version.
        *version = version.saturating_add(1);
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
        let payload = self
            .file(source)
            .ast()
            .expressions()
            .get(id)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone();
        match payload {
            Expression::Group { expression } => self.condition_facts(source, expression),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                let (when_true, when_false) = self.condition_facts(source, operand)?;
                Ok((when_false, when_true))
            }
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
                operator: BinaryOperator::LogicalAnd,
                right,
                ..
            } => {
                let (mut left_true, left_false) = self.condition_facts(source, left)?;
                let (right_true, right_false) = self.condition_facts(source, right)?;
                let writes = self.expression_flow_writes(source, right)?;
                left_true.retain(|key, _| !writes.contains(key));
                let when_true = extend_facts(&left_true, &right_true);
                let right_false_path = extend_facts(&left_true, &right_false);
                let when_false = intersect_facts(&left_false, &right_false_path);
                Ok((when_true, when_false))
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalOr,
                right,
                ..
            } => {
                let (left_true, mut left_false) = self.condition_facts(source, left)?;
                let (right_true, right_false) = self.condition_facts(source, right)?;
                let writes = self.expression_flow_writes(source, right)?;
                left_false.retain(|key, _| !writes.contains(key));
                let right_true_path = extend_facts(&left_false, &right_true);
                let when_true = intersect_facts(&left_true, &right_true_path);
                let when_false = extend_facts(&left_false, &right_false);
                Ok((when_true, when_false))
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

pub(super) fn extend_facts(
    baseline: &BTreeMap<FlowKey, UnitTypeId>,
    additions: &BTreeMap<FlowKey, UnitTypeId>,
) -> BTreeMap<FlowKey, UnitTypeId> {
    let mut result = baseline.clone();
    for (&key, &ty) in additions {
        match result.get(&key).copied() {
            Some(existing) if existing != ty => {
                result.remove(&key);
            }
            _ => {
                result.insert(key, ty);
            }
        }
    }
    result
}

pub(super) fn intersect_facts(
    left: &BTreeMap<FlowKey, UnitTypeId>,
    right: &BTreeMap<FlowKey, UnitTypeId>,
) -> BTreeMap<FlowKey, UnitTypeId> {
    left.iter()
        .filter_map(|(key, ty)| (right.get(key) == Some(ty)).then_some((*key, *ty)))
        .collect()
}
