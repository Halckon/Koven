//! SPEC-0197 compilation-unit type-test 与稳定 place 流事实。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::{
        Namespace, ReferenceTarget, ScopeKind, SourceUnitId, SymbolKind, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames,
    },
    parser::{
        BinaryOperator, Expression, Item, NameMarker, ParsedFile, PrefixOperator, VariableKind,
    },
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

pub(super) type ConditionFacts = (BTreeMap<FlowKey, UnitTypeId>, BTreeMap<FlowKey, UnitTypeId>);

pub(super) fn collect_stable_flow_symbols(
    files: &[&ParsedFile],
    names: &ValidatedCompilationUnitNames,
) -> BTreeSet<UnitSymbolId> {
    let mut stable = BTreeSet::new();
    for unit in names.names().source_units() {
        let source = unit.source_unit();
        let resolution = unit.resolution();
        let mutable = files[source.index()]
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Variable {
                    kind: VariableKind::Var,
                    name: NameMarker::Present(span),
                    ..
                } => resolution
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.span() == *span)
                    .map(|symbol| symbol.id()),
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
        let target = self.resolve_type_test_ref(source, type_ref)?;
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
                let (left_true, left_false) = self.condition_facts(source, left)?;
                let (right_true, right_false) = self.condition_facts(source, right)?;
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
                let (left_true, left_false) = self.condition_facts(source, left)?;
                let (right_true, right_false) = self.condition_facts(source, right)?;
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
        let node = self.file(source).ast().expressions().get(id).ok()?;
        match node.payload() {
            Expression::Group { expression } => self.stable_flow_key(source, *expression),
            Expression::This if self.current_receiver.is_some() => Some(FlowKey::This),
            Expression::Name => {
                let UnitReferenceTarget::Symbol(symbol) =
                    self.reference(source, node.span(), Namespace::Value)?
                else {
                    return None;
                };
                self.stable_flow_symbols
                    .contains(symbol)
                    .then_some(FlowKey::Symbol(*symbol))
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
