//! Preserve source parameter identity while checking an unresolved call candidate.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{ExternalSymbolId, SourceUnitId, UnitSymbolId},
    parser::{BinaryOperator, CallArgument, Expression},
    source::Span,
    type_checking::{BuiltinType, DeferredReason, TypeCheckingError, UnitTypeId, UnitTypeKind},
};

use super::{
    BodyChecker, CompilationUnitTypeError, ExpressionExpected,
    lambda::PartialLambdaContext,
    pending_types::invariant_type_children,
    type_graph::{CandidateTypes, map_type_children},
};

mod explicit_types;

pub(super) struct PendingConstraints {
    formal_types: Vec<UnitTypeId>,
    substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
    explicit: BTreeMap<UnitSymbolId, TypeRefId>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Constraint {
    Parameter(UnitSymbolId),
    External(ExternalSymbolId),
}

type TypePair = (UnitTypeId, UnitTypeId);
type MergeMemo = BTreeMap<TypePair, Option<UnitTypeId>>;
type Evidence = BTreeMap<Constraint, BTreeSet<UnitTypeId>>;

impl PendingConstraints {
    pub(super) fn new(
        checker: &BodyChecker<'_>,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        explicit: impl Iterator<Item = (UnitSymbolId, TypeRefId)>,
        formal_types: impl FnOnce() -> Vec<UnitTypeId>,
    ) -> Option<Self> {
        let substitutions: BTreeMap<_, _> = substitutions
            .iter()
            .filter_map(|(&parameter, &ty)| {
                checker
                    .contains_unresolved_call_type(ty)
                    .then_some((parameter, ty))
            })
            .collect();
        (!substitutions.is_empty()).then(|| {
            let explicit = explicit
                .filter(|(parameter, _)| substitutions.contains_key(parameter))
                .collect();
            Self {
                formal_types: formal_types(),
                substitutions,
                explicit,
            }
        })
    }

    pub(super) fn explicit_lambda_header_mismatch(
        &self,
        checker: &mut BodyChecker<'_>,
        private_types: Option<&CandidateTypes>,
        source: SourceUnitId,
        parameter: usize,
        expression: ExpressionId,
    ) -> Result<Option<Span>, CompilationUnitTypeError> {
        if self.explicit.is_empty() {
            return Ok(None);
        }
        let checkpoint = checker.signatures.types().len();
        let result = (|| {
            let formal = materialize(checker, private_types, self.formal_types[parameter])?;
            let Some(UnitTypeKind::TypeParameter(parameter)) =
                checker.signatures.types().get(formal)
            else {
                return Ok(None);
            };
            let Some(&reference) = self.explicit.get(parameter) else {
                return Ok(None);
            };
            let Some(pattern) = checker.explicit_constraint_type(source, reference)? else {
                return Ok(None);
            };
            Ok(
                match checker.partial_lambda_context(
                    source,
                    expression,
                    ExpressionExpected::Canonical(pattern),
                ) {
                    PartialLambdaContext::WrongHeader(span) => Some(span),
                    _ => None,
                },
            )
        })();
        checker.signatures.types_mut().discard_after(checkpoint);
        result
    }

    /// This scratch transaction runs after argument facts are recorded. Merged
    /// types are evidence of compatibility only and cannot enter the product.
    pub(super) fn first_conflict(
        &self,
        checker: &mut BodyChecker<'_>,
        private_types: Option<&CandidateTypes>,
        source: SourceUnitId,
        arguments: &[CallArgument],
        mapping: &[usize],
        actuals: &[Option<UnitTypeId>],
    ) -> Result<Option<usize>, CompilationUnitTypeError> {
        let checkpoint = checker.signatures.types().len();
        let result =
            self.check_arguments(checker, private_types, source, arguments, mapping, actuals);
        checker.signatures.types_mut().discard_after(checkpoint);
        result
    }

    fn check_arguments(
        &self,
        checker: &mut BodyChecker<'_>,
        private_types: Option<&CandidateTypes>,
        source: SourceUnitId,
        arguments: &[CallArgument],
        mapping: &[usize],
        actuals: &[Option<UnitTypeId>],
    ) -> Result<Option<usize>, CompilationUnitTypeError> {
        let mut evidence = Evidence::new();
        for (&parameter, &ty) in &self.substitutions {
            evidence.insert(
                Constraint::Parameter(parameter),
                BTreeSet::from([materialize(checker, private_types, ty)?]),
            );
        }
        let mut patterns = BTreeMap::new();
        for (&parameter, &reference) in &self.explicit {
            if let Some(pattern) = checker.explicit_constraint_type(source, reference)? {
                patterns.insert(parameter, pattern);
            }
        }
        let mut merged = MergeMemo::new();
        for (index, (&parameter, &actual)) in mapping.iter().zip(actuals).enumerate() {
            let Some(actual) = actual else { continue };
            if checker.provisional_argument_type(source, arguments[index].value)? {
                continue;
            }
            let formal = materialize(checker, private_types, self.formal_types[parameter])?;
            let literal_widths = match checker.signatures.types().get(formal) {
                Some(UnitTypeKind::TypeParameter(parameter))
                    if self.substitutions.contains_key(parameter) =>
                {
                    checker.contextual_integer_types(source, arguments[index].value)?
                }
                _ => Vec::new(),
            };
            if !checker.add_argument_evidence(
                formal,
                actual,
                literal_widths,
                &patterns,
                &mut evidence,
                &mut merged,
            )? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

fn materialize(
    checker: &mut BodyChecker<'_>,
    private_types: Option<&CandidateTypes>,
    ty: UnitTypeId,
) -> Result<UnitTypeId, CompilationUnitTypeError> {
    match private_types {
        Some(types) => types.materialize_expected(checker.signatures.types_mut(), ty),
        None => Ok(ty),
    }
}

impl BodyChecker<'_> {
    /// These roots can use a later expectation to choose a different type.
    /// Their default type alone cannot prove that a pending call is impossible.
    fn provisional_argument_type(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        let expressions = self.file(source).ast().expressions();
        loop {
            let node = expressions
                .get(expression)
                .map_err(TypeCheckingError::from)?;
            match node.payload() {
                Expression::Group { expression: child } => expression = *child,
                Expression::If { .. }
                | Expression::When { .. }
                | Expression::Binary {
                    operator: BinaryOperator::Elvis,
                    ..
                } => return Ok(true),
                Expression::Call {
                    callee,
                    type_arguments,
                    ..
                } if type_arguments.is_empty() => {
                    return self.is_list_form_callee(source, *callee);
                }
                _ => return Ok(false),
            }
        }
    }

    fn add_argument_evidence(
        &mut self,
        formal: UnitTypeId,
        actual: UnitTypeId,
        literal_widths: Vec<UnitTypeId>,
        patterns: &BTreeMap<UnitSymbolId, UnitTypeId>,
        evidence: &mut Evidence,
        merged: &mut MergeMemo,
    ) -> Result<bool, CompilationUnitTypeError> {
        if self.is_error(actual)
            || self.is_deferred(actual)
            || self.is_builtin(actual, BuiltinType::Nothing)
        {
            return Ok(true);
        }
        // Only the argument root admits a promotion. Once a root alternative
        // is chosen, all its child positions compare invariantly.
        let mut alternatives = Vec::new();
        for option in self
            .argument_type_options(actual)
            .into_iter()
            .chain(literal_widths)
        {
            if let Some(narrowed) =
                self.invariant_argument_evidence(formal, option, patterns, evidence, merged)?
            {
                alternatives.push(narrowed);
            }
        }
        let mut alternatives = alternatives.into_iter();
        let Some(mut narrowed) = alternatives.next() else {
            return Ok(false);
        };
        for alternative in alternatives {
            // A parameter left unconstrained by any viable root alternative
            // must keep all its previous possibilities.
            narrowed.retain(|parameter, types| {
                if let Some(other) = alternative.get(parameter) {
                    types.extend(other.iter().copied());
                    true
                } else {
                    false
                }
            });
        }
        evidence.extend(narrowed);
        Ok(true)
    }

    fn invariant_argument_evidence(
        &mut self,
        formal: UnitTypeId,
        actual: UnitTypeId,
        patterns: &BTreeMap<UnitSymbolId, UnitTypeId>,
        evidence: &Evidence,
        merged: &mut MergeMemo,
    ) -> Result<Option<Evidence>, CompilationUnitTypeError> {
        let mut narrowed = Evidence::new();
        let mut pending = vec![(formal, actual, false)];
        let mut visited = BTreeSet::new();
        while let Some((formal, actual, explicit_pattern)) = pending.pop() {
            if !visited.insert((formal, actual, explicit_pattern))
                || self.is_error(actual)
                || self.is_error(formal)
                || self.is_unresolved_call_type(actual)
            {
                continue;
            }
            let types = self.signatures.types();
            let formal_kind = types.get(formal);
            let constraint = match formal_kind {
                Some(UnitTypeKind::TypeParameter(parameter)) if !explicit_pattern => {
                    let parameter = *parameter;
                    if let Some(&pattern) = patterns.get(&parameter) {
                        // Parameters named in the explicit source type belong
                        // to the caller, even in a recursive call to itself.
                        pending.push((pattern, actual, true));
                    }
                    Constraint::Parameter(parameter)
                }
                Some(UnitTypeKind::Deferred(DeferredReason::ExternalConstraint(external))) => {
                    Constraint::External(*external)
                }
                _ => {
                    if self.is_unresolved_call_type(formal) {
                        continue;
                    }
                    if let Some(children) = invariant_type_children(formal_kind, types.get(actual))
                    {
                        pending.extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|(formal, actual)| (formal, actual, explicit_pattern)),
                        );
                    } else if formal != actual {
                        return Ok(None);
                    }
                    continue;
                }
            };
            let previous = narrowed
                .get(&constraint)
                .or_else(|| evidence.get(&constraint));
            if previous.is_none() && matches!(constraint, Constraint::External(_)) {
                narrowed.insert(constraint, BTreeSet::from([actual]));
            } else if let Some(previous) = previous {
                let mut compatible = BTreeSet::new();
                for &previous in previous {
                    if let Some(ty) = self.merge_type_evidence(previous, actual, merged)? {
                        compatible.insert(ty);
                    }
                }
                if compatible.is_empty() {
                    return Ok(None);
                }
                narrowed.insert(constraint, compatible);
            }
        }
        Ok(Some(narrowed))
    }

    fn argument_type_options(&mut self, actual: UnitTypeId) -> Vec<UnitTypeId> {
        let kind = self.signatures.types().get(actual);
        if matches!(kind, Some(UnitTypeKind::Nullable(inner)) if self.is_builtin(*inner, BuiltinType::Nothing))
        {
            let unresolved = self.deferred_type(DeferredReason::Call);
            return vec![
                self.signatures
                    .types_mut()
                    .intern(UnitTypeKind::Nullable(unresolved)),
            ];
        }
        let (root, nullable_root) = match kind {
            Some(UnitTypeKind::EnumCase { root, .. }) => (Some(*root), Some(*root)),
            Some(UnitTypeKind::StaticSelf(root)) => (Some(*root), None),
            _ => (None, None),
        };
        let mut options = vec![actual];
        options.extend(root);
        for inner in std::iter::once(actual).chain(nullable_root) {
            options.push(
                self.signatures
                    .types_mut()
                    .intern(UnitTypeKind::Nullable(inner)),
            );
        }
        options
    }

    fn merge_type_evidence(
        &mut self,
        left: UnitTypeId,
        right: UnitTypeId,
        merged: &mut MergeMemo,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let root = (left, right);
        let mut pending = vec![(root, false)];
        while let Some((pair @ (left, right), finish)) = pending.pop() {
            if merged.contains_key(&pair) {
                continue;
            }
            if left == right || self.is_unresolved_call_type(right) {
                merged.insert(pair, Some(left));
                continue;
            }
            if self.is_unresolved_call_type(left) {
                merged.insert(pair, Some(right));
                continue;
            }
            let types = self.signatures.types();
            let left_kind = types.get(left);
            let Some(children) = invariant_type_children(left_kind, types.get(right)) else {
                merged.insert(pair, None);
                continue;
            };
            if !finish {
                pending.push((pair, true));
                pending.extend(children.into_iter().rev().map(|child| (child, false)));
                continue;
            }
            let Some(children) = children
                .into_iter()
                .map(|child| merged[&child])
                .collect::<Option<Vec<_>>>()
            else {
                merged.insert(pair, None);
                continue;
            };
            let mut children = children.into_iter();
            let mut next = |_| {
                children
                    .next()
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
            };
            let left_kind = left_kind
                .cloned()
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let kind = match left_kind {
                UnitTypeKind::EnumCase { case, root } => UnitTypeKind::EnumCase {
                    case,
                    root: next(root)?,
                },
                kind => map_type_children(kind, next)?,
            };
            merged.insert(pair, Some(self.signatures.types_mut().intern(kind)));
        }
        Ok(merged[&root])
    }
}
