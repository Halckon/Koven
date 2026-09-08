use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity},
    name_resolution::{Namespace, ReferenceTarget},
    parser::{Expression, LiteralKind, WhenCondition, WhenEntry},
    source::Span,
};

use super::{
    flow::{FlowKey, intersect_facts},
    *,
};

use crate::type_checking::{
    NullableWhenAlternativeDescriptor, NullableWhenDescriptor, NullableWhenEntryDescriptor,
    NullableWhenSubjectCategory, WhenDomain, WhenDomainAtom,
};
use WhenDomainAtom as CoverageAtom;

struct WhenConditionCheck {
    coverage: Vec<CoverageAtom>,
    facts: BTreeMap<FlowKey, TypeId>,
    valid: bool,
    non_null: bool,
}

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_when(
        &mut self,
        id: ExpressionId,
        keyword_span: Span,
        subject: Option<ExpressionId>,
        entries: Vec<WhenEntry>,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let subject_type = if let Some(subject) = subject {
            Some(self.check_expression(subject, None, None)?.ty)
        } else {
            None
        };
        let subject_key = subject.and_then(|subject| self.stable_flow_key(subject));
        let domain = subject_type.and_then(|ty| self.closed_domain(ty));
        let nullable_inner = subject_type.and_then(|ty| match self.kind(ty) {
            TypeKind::Nullable(inner) => Some(*inner),
            _ => None,
        });
        let diagnostic_start = self.diagnostics.len();
        let baseline = self.flow_facts.clone();
        let subject_version =
            subject_key.map(|key| self.flow_versions.get(&key).copied().unwrap_or_default());
        let mut remaining_versions = self.flow_versions.clone();
        let mut exit_versions = self.flow_versions.clone();
        let mut remaining_facts = self.flow_facts.clone();
        let mut remaining_domain = domain
            .clone()
            .map(WhenDomain::Finite)
            .unwrap_or(WhenDomain::Nullable);
        let mut plans = Vec::new();
        let mut covered = BTreeMap::<CoverageAtom, Span>::new();
        let mut first_else = None;
        let mut has_else = false;
        let mut branches = Vec::new();
        let mut exits = Vec::new();

        for (entry_index, entry) in entries.iter().enumerate() {
            self.flow_facts = remaining_facts.clone();
            self.flow_versions = remaining_versions.clone();
            let input_domain = remaining_domain.clone();
            let mut body_domain = WhenDomain::Empty;
            let mut alternative_plans = Vec::new();
            let mut entry_facts = None;
            let mut entry_source_valid = subject_key.is_some_and(|key| {
                Some(self.flow_versions.get(&key).copied().unwrap_or_default()) == subject_version
            });
            if let Some(else_span) = entry.else_span {
                has_else = true;
                body_domain = input_domain.clone();
                remaining_domain = WhenDomain::Empty;
                entry_facts = Some(self.flow_facts.clone());
                if let Some(first) = first_else {
                    self.emit_with_label(
                        self.duplicate_when_else_code,
                        "when has more than one else entry",
                        else_span,
                        first,
                        "first else entry appears here",
                    )?;
                } else {
                    first_else = Some(else_span);
                }
                if entry_index + 1 != entries.len() {
                    self.emit_with_label(
                        self.non_final_when_else_code,
                        "else must be the final when entry",
                        else_span,
                        entries[entry_index + 1].span,
                        "a later entry appears here",
                    )?;
                }
            } else {
                for condition in &entry.conditions {
                    let condition_span = self.when_condition_span(condition)?;
                    let checked = self.check_when_condition(
                        subject_type,
                        subject_key,
                        domain.as_deref(),
                        condition,
                    )?;
                    let source_valid = subject_key.is_some_and(|key| {
                        Some(self.flow_versions.get(&key).copied().unwrap_or_default())
                            == subject_version
                    });
                    let (matched, missed) = split_domain(
                        &input_domain,
                        &checked.coverage,
                        checked.non_null,
                        checked.valid,
                    );
                    // Alternatives are judged against the entry input, while condition effects
                    // persist along evaluation. Never restore pre-condition source proofs.
                    let mut matched_facts = self.flow_facts.clone();
                    if !matched.is_empty() {
                        entry_source_valid &= source_valid;
                        if let (Some(key), Some(inner)) = (subject_key, nullable_inner)
                            && source_valid
                            && matched.is_non_null()
                        {
                            matched_facts.insert(key, inner);
                        }
                        for (key, ty) in &checked.facts {
                            if source_valid {
                                matched_facts.insert(*key, *ty);
                            }
                        }
                        entry_facts = Some(match entry_facts {
                            None => matched_facts,
                            Some(previous) => intersect_facts(&previous, &matched_facts),
                        });
                        body_domain = union_domain(body_domain, matched.clone());
                    }
                    alternative_plans.push(NullableWhenAlternativeDescriptor {
                        span: condition_span,
                        match_domain: matched,
                        fallthrough_domain: missed,
                    });
                    remaining_domain = split_domain(
                        &remaining_domain,
                        &checked.coverage,
                        checked.non_null,
                        checked.valid,
                    )
                    .1;
                    if checked.valid && !checked.coverage.is_empty() {
                        let mut first_previous = None;
                        let mut added = false;
                        for atom in checked.coverage {
                            if let Some(previous) = covered.get(&atom).copied() {
                                first_previous.get_or_insert(previous);
                            } else {
                                covered.insert(atom, condition_span);
                                added = true;
                            }
                        }
                        if !added {
                            self.emit_with_label(
                                self.duplicate_when_coverage_code,
                                "when condition does not add new finite-domain coverage",
                                condition_span,
                                first_previous.expect("non-empty duplicate coverage"),
                                "the same domain was covered here",
                            )?;
                        }
                    }
                }
            }
            if let (Some(key), Some(inner)) = (subject_key, nullable_inner)
                && Some(self.flow_versions.get(&key).copied().unwrap_or_default())
                    == subject_version
                && remaining_domain.is_non_null()
            {
                self.flow_facts.insert(key, inner);
            }
            remaining_facts = self.flow_facts.clone();
            remaining_versions = self.flow_versions.clone();
            self.flow_facts = entry_facts.unwrap_or_else(|| remaining_facts.clone());
            let body_type = if body_domain.is_non_null() {
                nullable_inner
            } else {
                None
            };
            if let (Some(key), Some(inner)) = (subject_key, body_type)
                && entry_source_valid
                && !self.flow_facts.contains_key(&key)
            {
                self.flow_facts.insert(key, inner);
            }
            // Coverage is relative to the remaining domain, not the original nullable root.
            // A single surviving enum case carries its payload type even after a negated test.
            if entry_source_valid && let WhenDomain::Finite(atoms) = &body_domain {
                let facts = self.facts_for_single_case(subject_type, subject_key, atoms)?;
                self.flow_facts.extend(facts);
            }
            let stable_symbol = match subject_key {
                Some(FlowKey::Symbol(symbol)) if entry_source_valid => Some(symbol),
                _ => None,
            };
            plans.push(NullableWhenEntryDescriptor {
                span: entry.span,
                input_domain,
                remaining_domain: remaining_domain.clone(),
                alternatives: alternative_plans,
                body_domain,
                body_type,
                stable_symbol,
            });
            let result = self.check_value_body(entry.body, expected, expected_span)?;
            if result.falls_through {
                exits.push(self.flow_facts.clone());
            }
            for (&key, &version) in &self.flow_versions {
                let previous = exit_versions.entry(key).or_default();
                *previous = (*previous).max(version);
            }
            branches.push(result);
        }

        self.flow_versions = exit_versions;
        let exhaustive = has_else
            || domain
                .as_ref()
                .is_some_and(|domain| domain.iter().all(|atom| covered.contains_key(atom)));
        let value_use = self.expression_use(id) == ExpressionUse::Value;
        if value_use && !exhaustive {
            self.emit_non_exhaustive_when(keyword_span, domain.as_deref(), &covered)?;
        }
        if !exhaustive {
            exits.push(remaining_facts);
        }
        self.flow_facts = exits
            .into_iter()
            .reduce(|left, right| intersect_facts(&left, &right))
            .unwrap_or(baseline);

        let falls_through = !exhaustive || branches.iter().any(|branch| branch.falls_through);
        let ty = if !value_use {
            self.builtin(BuiltinType::Unit)
        } else if branches
            .iter()
            .all(|branch| self.is_builtin(branch.ty, BuiltinType::Nothing))
        {
            self.builtin(BuiltinType::Nothing)
        } else if let Some(expected) = expected.filter(|ty| !self.is_deferred(*ty)) {
            expected
        } else {
            self.join_when_branches(&entries, &branches)?
        };
        if nullable_inner.is_some()
            && self.diagnostics.len() == diagnostic_start
            && let (Some(subject), Some(subject_type)) = (subject, subject_type)
        {
            let category = self.nullable_when_category(subject)?;
            let native_eligible = matches!(
                category,
                NullableWhenSubjectCategory::OwnedRoot | NullableWhenSubjectCategory::Temporary
            ) && nullable_inner.is_some_and(|inner| match self.kind(inner) {
                TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Box | IntrinsicTypeConstructor::Rc,
                    ..
                } => true,
                TypeKind::Nominal { nominal, .. } => self
                    .nominals
                    .iter()
                    .any(|item| item.id() == *nominal && item.kind() == NominalKind::Class),
                _ => false,
            });
            self.nullable_whens.retain(|plan| plan.expression() != id);
            self.nullable_whens.push(NullableWhenDescriptor {
                expression: id,
                subject,
                span: keyword_span,
                subject_type,
                category,
                stable_symbol: match subject_key {
                    Some(FlowKey::Symbol(symbol)) => Some(symbol),
                    _ => None,
                },
                native_eligible,
                entries: plans,
            });
            self.nullable_whens
                .sort_by_key(|plan| plan.expression().index());
        }
        Ok(ExprCheck { ty, falls_through })
    }

    fn check_when_condition(
        &mut self,
        subject: Option<TypeId>,
        subject_key: Option<FlowKey>,
        domain: Option<&[CoverageAtom]>,
        condition: &WhenCondition,
    ) -> Result<WhenConditionCheck, TypeCheckingError> {
        match condition {
            WhenCondition::Expression(expression) => {
                let expected = subject.filter(|ty| {
                    matches!(
                        self.ast()
                            .expressions()
                            .get(*expression)
                            .map(|node| node.payload()),
                        Ok(Expression::Literal(LiteralKind::Null))
                    ) && matches!(self.kind(*ty), TypeKind::Nullable(_))
                });
                let result = self.check_expression(*expression, expected, None)?;
                let valid = if let Some(subject) = subject {
                    self.assignable(result.ty, subject) || self.assignable(subject, result.ty)
                } else {
                    self.is_builtin(result.ty, BuiltinType::Boolean)
                };
                if !valid && !self.is_error(result.ty) && !self.is_deferred(result.ty) {
                    self.emit(
                        self.invalid_when_condition_code,
                        "when condition does not match its subject form or type",
                        self.ast().expressions().get(*expression)?.span(),
                    )?;
                }
                let coverage = if subject.is_some() && valid {
                    self.coverage_for_expression(*expression)?
                        .into_iter()
                        .collect()
                } else {
                    Vec::new()
                };
                let facts = self.facts_for_single_case(subject, subject_key, &coverage)?;
                Ok(WhenConditionCheck {
                    non_null: valid
                        && !matches!(
                            self.kind(result.ty),
                            TypeKind::Nullable(_) | TypeKind::Error | TypeKind::Deferred(_)
                        ),
                    coverage,
                    facts,
                    valid,
                })
            }
            WhenCondition::TypeTest {
                operator_span,
                negated,
                type_ref,
            } => {
                let target = self.resolve_type_test_ref(*type_ref)?;
                let Some(subject) = subject else {
                    self.emit_with_label(
                        self.invalid_when_condition_code,
                        "subjectless when cannot use an omitted-subject type test",
                        *operator_span,
                        self.ast().type_refs().get(*type_ref)?.span(),
                        "type-test target appears here",
                    )?;
                    return Ok(WhenConditionCheck {
                        coverage: Vec::new(),
                        facts: BTreeMap::new(),
                        valid: false,
                        non_null: false,
                    });
                };
                let valid = self.is_error(target) || self.valid_type_test_relation(subject, target);
                if !valid {
                    self.emit_with_label(
                        self.invalid_type_test_code,
                        "type test target is not runtime-testable from the when subject",
                        *operator_span,
                        self.ast().type_refs().get(*type_ref)?.span(),
                        "invalid type-test target",
                    )?;
                }
                let mut coverage = match self.kind(target) {
                    TypeKind::EnumCase { case, .. } if valid => vec![CoverageAtom::Case(*case)],
                    _ => Vec::new(),
                };
                if *negated
                    && let Some(domain) = domain
                    && !coverage.is_empty()
                {
                    coverage = domain
                        .iter()
                        .copied()
                        .filter(|atom| !coverage.contains(atom))
                        .collect();
                }
                let facts = self.facts_for_single_case(Some(subject), subject_key, &coverage)?;
                Ok(WhenConditionCheck {
                    coverage,
                    facts,
                    valid,
                    non_null: valid
                        && !negated
                        && !matches!(self.kind(target), TypeKind::Nullable(_)),
                })
            }
            WhenCondition::Contains {
                operator_span,
                expression,
                ..
            } => {
                self.check_expression(*expression, None, None)?;
                let valid = subject.is_some();
                if !valid {
                    self.emit(
                        self.invalid_when_condition_code,
                        "subjectless when cannot use an omitted-subject contains condition",
                        *operator_span,
                    )?;
                }
                Ok(WhenConditionCheck {
                    coverage: Vec::new(),
                    facts: BTreeMap::new(),
                    valid,
                    non_null: false,
                })
            }
        }
    }

    fn closed_domain(&self, ty: TypeId) -> Option<Vec<CoverageAtom>> {
        match self.kind(ty) {
            TypeKind::Builtin(BuiltinType::Boolean) => Some(vec![
                CoverageAtom::Boolean(false),
                CoverageAtom::Boolean(true),
            ]),
            TypeKind::Nominal { nominal, .. } => self
                .nominals
                .iter()
                .find(|descriptor| {
                    descriptor.id() == *nominal && descriptor.kind() == NominalKind::EnumClass
                })
                .map(|_| {
                    self.enum_cases
                        .iter()
                        .filter(|case| case.root() == *nominal)
                        .map(|case| CoverageAtom::Case(case.id()))
                        .collect()
                }),
            TypeKind::EnumCase { root, .. } => self.closed_domain(*root),
            TypeKind::Nullable(inner) => self.closed_domain(*inner).map(|mut domain| {
                domain.push(CoverageAtom::Null);
                domain
            }),
            _ => None,
        }
    }

    fn coverage_for_expression(
        &self,
        id: ExpressionId,
    ) -> Result<Option<CoverageAtom>, TypeCheckingError> {
        let node = self.ast().expressions().get(id)?;
        match node.payload() {
            Expression::Literal(LiteralKind::Boolean(value)) => {
                Ok(Some(CoverageAtom::Boolean(*value)))
            }
            Expression::Literal(LiteralKind::Null) => Ok(Some(CoverageAtom::Null)),
            Expression::Group { expression } => self.coverage_for_expression(*expression),
            Expression::Name => {
                let case = match self.reference(node.span(), Namespace::Value) {
                    Some(ReferenceTarget::Symbol(symbol)) => {
                        self.enum_case_by_value_symbol.get(symbol)
                    }
                    _ => None,
                };
                Ok(case.and_then(|case| {
                    self.enum_case(*case)
                        .is_some_and(|descriptor| descriptor.payloads().is_empty())
                        .then_some(CoverageAtom::Case(*case))
                }))
            }
            Expression::Member { name_span, .. } => {
                let case = match self.reference(*name_span, Namespace::Value) {
                    Some(ReferenceTarget::Symbol(symbol)) => {
                        self.enum_case_by_value_symbol.get(symbol)
                    }
                    _ => None,
                };
                Ok(case.and_then(|case| {
                    self.enum_case(*case)
                        .is_some_and(|descriptor| descriptor.payloads().is_empty())
                        .then_some(CoverageAtom::Case(*case))
                }))
            }
            _ => Ok(None),
        }
    }

    fn facts_for_single_case(
        &mut self,
        subject: Option<TypeId>,
        key: Option<FlowKey>,
        coverage: &[CoverageAtom],
    ) -> Result<BTreeMap<FlowKey, TypeId>, TypeCheckingError> {
        let (Some(subject), Some(key), [CoverageAtom::Case(case)]) = (subject, key, coverage)
        else {
            return Ok(BTreeMap::new());
        };
        let root = match self.kind(subject) {
            TypeKind::Nullable(inner) => *inner,
            TypeKind::EnumCase { root, .. } => *root,
            TypeKind::Nominal { .. } => subject,
            _ => return Ok(BTreeMap::new()),
        };
        let ty = self.types.intern(TypeKind::EnumCase { case: *case, root });
        Ok(BTreeMap::from([(key, ty)]))
    }

    fn when_condition_span(&self, condition: &WhenCondition) -> Result<Span, TypeCheckingError> {
        match condition {
            WhenCondition::Expression(expression) => {
                Ok(self.ast().expressions().get(*expression)?.span())
            }
            WhenCondition::TypeTest { operator_span, .. }
            | WhenCondition::Contains { operator_span, .. } => Ok(*operator_span),
        }
    }

    fn emit_non_exhaustive_when(
        &mut self,
        keyword_span: Span,
        domain: Option<&[CoverageAtom]>,
        covered: &BTreeMap<CoverageAtom, Span>,
    ) -> Result<(), TypeCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.non_exhaustive_when_code,
            "value-context when is not exhaustive",
            keyword_span,
        )?;
        if let Some(domain) = domain {
            let mut literals = Vec::new();
            for atom in domain.iter().filter(|atom| !covered.contains_key(atom)) {
                match atom {
                    CoverageAtom::Case(case) => {
                        let descriptor = self
                            .enum_case(*case)
                            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                        diagnostic.add_label(
                            self.sources,
                            self.symbol_spans[descriptor.value_symbol().index()],
                            "missing enum case",
                        )?;
                    }
                    CoverageAtom::Boolean(value) => literals.push(value.to_string()),
                    CoverageAtom::Null => literals.push("null".to_owned()),
                }
            }
            if !literals.is_empty() {
                diagnostic.add_note(format!("missing values: {}", literals.join(", ")))?;
            }
        } else {
            diagnostic.add_help("add a final else entry")?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn join_when_branches(
        &mut self,
        entries: &[WhenEntry],
        branches: &[StatementCheck],
    ) -> Result<TypeId, TypeCheckingError> {
        let mut joined = None;
        for (entry, branch) in entries.iter().zip(branches) {
            if self.is_error(branch.ty) {
                continue;
            }
            joined = Some(match joined {
                None => branch.ty,
                Some(current) => match self.join_when_types(current, branch.ty) {
                    Some(ty) => ty,
                    None => {
                        self.emit_with_label(
                            self.when_branch_type_code,
                            "when branches do not have a legal common type",
                            self.ast().statements().get(entry.body)?.span(),
                            self.ast().statements().get(entries[0].body)?.span(),
                            format!("first branch has type {}", self.type_name(current)),
                        )?;
                        self.error_type()
                    }
                },
            });
        }
        Ok(joined.unwrap_or_else(|| self.builtin(BuiltinType::Unit)))
    }

    fn join_when_types(&mut self, left: TypeId, right: TypeId) -> Option<TypeId> {
        if let Some(joined) = self.join(left, right) {
            return Some(joined);
        }
        let root = |kind: &TypeKind| match kind {
            TypeKind::EnumCase { root, .. } => Some(*root),
            _ => None,
        };
        match (root(self.kind(left)), root(self.kind(right))) {
            (Some(left_root), Some(right_root)) if left_root == right_root => Some(left_root),
            (Some(root), None) if root == right => Some(root),
            (None, Some(root)) if root == left => Some(root),
            _ if !self.is_deferred(left) && !self.is_deferred(right) => {
                Some(self.builtin(BuiltinType::Any))
            }
            _ => None,
        }
    }
}

/// 对内部单次求值 subject 分裂域，不把未知条件的失败误作完整补集。
fn split_domain(
    input: &WhenDomain,
    coverage: &[CoverageAtom],
    non_null: bool,
    valid: bool,
) -> (WhenDomain, WhenDomain) {
    if !valid || input.is_empty() {
        return (WhenDomain::Empty, input.clone());
    }
    if !coverage.is_empty() {
        if let WhenDomain::Finite(atoms) = input {
            return (
                WhenDomain::Finite(
                    atoms
                        .iter()
                        .copied()
                        .filter(|atom| coverage.contains(atom))
                        .collect(),
                ),
                WhenDomain::Finite(
                    atoms
                        .iter()
                        .copied()
                        .filter(|atom| !coverage.contains(atom))
                        .collect(),
                ),
            );
        }
        if coverage == [CoverageAtom::Null] {
            return match input {
                WhenDomain::Null => (WhenDomain::Null, WhenDomain::Empty),
                WhenDomain::NonNull => (WhenDomain::Empty, WhenDomain::NonNull),
                _ => (WhenDomain::Null, WhenDomain::NonNull),
            };
        }
    }
    let matched = if non_null {
        match input {
            WhenDomain::Null => WhenDomain::Empty,
            WhenDomain::Finite(atoms) => WhenDomain::Finite(
                atoms
                    .iter()
                    .copied()
                    .filter(|atom| *atom != CoverageAtom::Null)
                    .collect(),
            ),
            _ => WhenDomain::NonNull,
        }
    } else {
        input.clone()
    };
    (matched, input.clone())
}

fn union_domain(left: WhenDomain, right: WhenDomain) -> WhenDomain {
    if left.is_empty() {
        return right;
    }
    if right.is_empty() || left == right {
        return left;
    }
    if let (WhenDomain::Finite(left), WhenDomain::Finite(right)) = (&left, &right) {
        let mut atoms = left.clone();
        atoms.extend(right.iter().filter(|atom| !left.contains(atom)).copied());
        atoms.sort();
        return WhenDomain::Finite(atoms);
    }
    WhenDomain::Nullable
}

impl Checker<'_> {
    fn nullable_when_category(
        &self,
        expression: ExpressionId,
    ) -> Result<NullableWhenSubjectCategory, TypeCheckingError> {
        use NullableWhenSubjectCategory as Category;
        if let Expression::Group { expression } =
            self.ast().expressions().get(expression)?.payload()
        {
            return self.nullable_when_category(*expression);
        }
        if self
            .element_places
            .iter()
            .any(|place| place.expression() == expression)
        {
            return Ok(Category::ContainerElement);
        }
        if self.aggregate_projections.iter().any(|place| {
            place.expression() == expression && place.kind() == AggregateProjectionKind::Field
        }) {
            return Ok(Category::OrdinaryField);
        }
        let source_key = self.stable_flow_key(expression).or_else(|| {
            let node = self.ast().expressions().get(expression).ok()?;
            if matches!(node.payload(), Expression::Name)
                && let Some(ReferenceTarget::Symbol(symbol)) =
                    self.reference(node.span(), Namespace::Value)
            {
                Some(FlowKey::Symbol(*symbol))
            } else {
                None
            }
        });
        if let Some(key) = source_key {
            return Ok(match key {
                FlowKey::Symbol(symbol) => match self.parameter_modes[symbol.index()] {
                    Some(ParameterMode::Borrow) => Category::BorrowRoot,
                    Some(ParameterMode::Inout) => Category::InoutRoot,
                    _ => Category::OwnedRoot,
                },
                FlowKey::This => match self.current_receiver_mode {
                    Some(ParameterMode::Inout) => Category::InoutRoot,
                    Some(ParameterMode::Value) => Category::OwnedRoot,
                    _ => Category::BorrowRoot,
                },
            });
        }
        Ok(Category::Temporary)
    }
}
