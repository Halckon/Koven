//! SPEC-0197 compilation-unit `when` 穷尽性、分支类型与 flow facts。

use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget, UnitSymbolId},
    parser::{Expression, LiteralKind, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, ExpressionUse, NominalKind, TypeCheckingError,
        UnitTypeId, UnitTypeKind,
    },
};

use super::flow::{intersect_facts, merge_flow_versions};
use super::{BodyChecker, ExpressionCheck, flow::FlowKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum CoverageAtom {
    Boolean(bool),
    Null,
    Case(UnitSymbolId),
}

struct WhenConditionCheck {
    coverage: Vec<CoverageAtom>,
    facts: BTreeMap<FlowKey, UnitTypeId>,
    valid: bool,
    non_null: bool,
}

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_when(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        keyword_span: Span,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let subject_type = if let Some(subject) = subject {
            Some(
                self.check_expression(source, subject, None, None, return_type)?
                    .ty,
            )
        } else {
            None
        };
        let subject_key = subject.and_then(|subject| self.stable_flow_key(source, subject));
        let domain = subject_type.and_then(|ty| self.closed_domain(ty));
        let nullable_inner = subject_type.and_then(|ty| match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Nullable(inner)) => Some(*inner),
            _ => None,
        });
        let subject_source = subject_key.map(|key| (key, self.flow_version(key)));
        let mut nullable_when = self.begin_nullable_when(
            source,
            expression,
            subject,
            subject_type,
            nullable_inner,
            subject_source,
            entries,
        )?;
        let baseline = self.flow_facts.clone();
        let baseline_versions = self.flow_versions.clone();
        let mut remaining_facts = baseline.clone();
        let mut remaining_versions = baseline_versions.clone();
        let mut remaining_domain = domain
            .clone()
            .map(WhenFlowDomain::Finite)
            .unwrap_or(WhenFlowDomain::Nullable);
        let mut covered = BTreeMap::<CoverageAtom, Span>::new();
        let mut first_else = None;
        let mut has_else = false;
        let mut branches = Vec::new();
        let mut exits = Vec::new();

        for (entry_index, entry) in entries.iter().enumerate() {
            let mut nullable_entry = super::nullable_when::begin_entry(
                nullable_when.as_ref(),
                entry_index,
                entry,
                &remaining_domain,
            );
            self.flow_facts = remaining_facts.clone();
            self.flow_versions = remaining_versions.clone();
            let mut entry_facts = None;
            let mut entry_versions = None;
            let mut body_domain = WhenFlowDomain::Empty;
            let mut entry_source_valid = self.when_source_valid(subject_source);
            if let Some(else_span) = entry.else_span {
                has_else = true;
                body_domain = remaining_domain.clone();
                remaining_domain = WhenFlowDomain::Empty;
                entry_facts = Some(self.flow_facts.clone());
                entry_versions = Some(self.flow_versions.clone());
                if let Some(first) = first_else {
                    self.emit_maybe_label(
                        codes::DUPLICATE_WHEN_ELSE,
                        "when has more than one else entry",
                        else_span,
                        Some(first),
                        "first else entry appears here",
                    )?;
                } else {
                    first_else = Some(else_span);
                }
                if entry_index + 1 != entries.len() {
                    self.emit_maybe_label(
                        codes::NON_FINAL_WHEN_ELSE,
                        "else must be the final when entry",
                        else_span,
                        Some(entries[entry_index + 1].span),
                        "a later entry appears here",
                    )?;
                }
            } else {
                for condition in &entry.conditions {
                    let condition_span = self.when_condition_span(source, condition)?;
                    let checked = self.check_when_condition(
                        source,
                        subject_type,
                        subject_key,
                        domain.as_deref(),
                        condition,
                        return_type,
                    )?;
                    let source_valid = self.when_source_valid(subject_source);
                    let (matched, missed) = split_domain(
                        &remaining_domain,
                        &checked.coverage,
                        checked.non_null,
                        checked.valid,
                    );
                    super::nullable_when::record_alternative(
                        &mut nullable_entry,
                        source,
                        condition,
                        condition_span,
                        &remaining_domain,
                        &matched,
                        &missed,
                    );
                    if !matched.is_empty() {
                        let mut matched_facts = self.flow_facts.clone();
                        entry_source_valid &= source_valid;
                        if source_valid {
                            if let (Some(key), Some(inner)) = (subject_key, nullable_inner)
                                && matched.is_non_null()
                            {
                                matched_facts.entry(key).or_insert(inner);
                            }
                            matched_facts.extend(checked.facts);
                        }
                        entry_facts = Some(match entry_facts {
                            None => matched_facts,
                            Some(previous) => intersect_facts(&previous, &matched_facts),
                        });
                        entry_versions = Some(match entry_versions {
                            None => self.flow_versions.clone(),
                            Some(previous) => merge_flow_versions(&previous, &self.flow_versions),
                        });
                        body_domain = union_domain(body_domain, matched);
                    }
                    // Condition effects remain on the miss edge; a selected body never runs here.
                    remaining_domain = missed;
                    if let (Some(key), Some(inner)) = (subject_key, nullable_inner)
                        && source_valid
                        && remaining_domain.is_non_null()
                    {
                        self.flow_facts.entry(key).or_insert(inner);
                    }
                    if checked.valid && !checked.coverage.is_empty() {
                        let mut first_previous = None;
                        let mut added = false;
                        for atom in checked.coverage.iter().copied() {
                            if let Some(previous) = covered.get(&atom).copied() {
                                first_previous.get_or_insert(previous);
                            } else {
                                covered.insert(atom, condition_span);
                                added = true;
                            }
                        }
                        if !added {
                            self.emit_maybe_label(
                                codes::DUPLICATE_WHEN_COVERAGE,
                                "when condition does not add new finite-domain coverage",
                                condition_span,
                                first_previous,
                                "the same domain was covered here",
                            )?;
                        }
                    }
                }
            }
            remaining_facts = self.flow_facts.clone();
            remaining_versions = self.flow_versions.clone();
            self.flow_facts = entry_facts.unwrap_or_else(|| remaining_facts.clone());
            self.flow_versions = entry_versions.unwrap_or_else(|| remaining_versions.clone());
            if entry_source_valid
                && let (Some(key), Some(inner)) = (subject_key, nullable_inner)
                && body_domain.is_non_null()
            {
                self.flow_facts.entry(key).or_insert(inner);
            }
            let branch_expected = if self.expression_use(source, expression) == ExpressionUse::Value
            {
                self.control_branch_expected(source, entry.body, expected)?
            } else {
                expected
            };
            let frame_base = self.push_nullable_when_frame(
                nullable_when.as_ref(),
                entry_index,
                entry,
                &body_domain,
                entry_source_valid,
            );
            let result = self.check_value_body(
                source,
                entry.body,
                branch_expected,
                expected_span,
                return_type,
            );
            self.active_nullable_whens.truncate(frame_base);
            let result = result?;
            super::nullable_when::finish_entry(
                &mut nullable_when,
                nullable_entry,
                &body_domain,
                &remaining_domain,
                result.falls_through,
            );
            if !body_domain.is_empty() && result.falls_through {
                exits.push((self.flow_facts.clone(), self.flow_versions.clone()));
            }
            branches.push(result);
        }

        let exhaustive = has_else
            || domain
                .as_ref()
                .is_some_and(|domain| domain.iter().all(|atom| covered.contains_key(atom)));
        let value_use = self.expression_use(source, expression) == ExpressionUse::Value;
        if value_use && !exhaustive {
            self.emit_non_exhaustive_when(keyword_span, domain.as_deref(), &covered)?;
        }
        if !exhaustive {
            exits.push((remaining_facts, remaining_versions));
        }
        let falls_through = !exits.is_empty();
        (self.flow_facts, self.flow_versions) = exits
            .into_iter()
            .reduce(
                |(left_facts, left_versions), (right_facts, right_versions)| {
                    (
                        intersect_facts(&left_facts, &right_facts),
                        merge_flow_versions(&left_versions, &right_versions),
                    )
                },
            )
            .unwrap_or((baseline, baseline_versions));
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
            self.join_when_branches(source, entries, &branches)?
        };
        if let Some(mut descriptor) = nullable_when {
            descriptor.falls_through = falls_through;
            self.parts.nullable.whens.push(descriptor);
        }
        Ok(ExpressionCheck { ty, falls_through })
    }

    #[allow(clippy::too_many_arguments)]
    fn check_when_condition(
        &mut self,
        source: SourceUnitId,
        subject: Option<UnitTypeId>,
        subject_key: Option<FlowKey>,
        domain: Option<&[CoverageAtom]>,
        condition: &WhenCondition,
        return_type: UnitTypeId,
    ) -> Result<WhenConditionCheck, CompilationUnitTypeError> {
        match condition {
            WhenCondition::Expression(expression) => {
                let expected = subject.filter(|ty| {
                    matches!(
                        self.file(source)
                            .ast()
                            .expressions()
                            .get(*expression)
                            .map(|node| node.payload()),
                        Ok(Expression::Literal(LiteralKind::Null))
                    ) && matches!(
                        self.signatures.types().get(*ty),
                        Some(UnitTypeKind::Nullable(_))
                    )
                });
                let is_null = matches!(
                    self.file(source)
                        .ast()
                        .expressions()
                        .get(*expression)
                        .map(|node| node.payload()),
                    Ok(Expression::Literal(LiteralKind::Null))
                );
                let result = if is_null {
                    let ty = if let Some(expected) = expected {
                        expected
                    } else {
                        let span = self
                            .file(source)
                            .ast()
                            .expressions()
                            .get(*expression)
                            .map_err(TypeCheckingError::from)?
                            .span();
                        self.emit(
                            codes::CANNOT_INFER_TYPE,
                            "cannot infer the type of null without a nullable expected type",
                            span,
                        )?;
                        self.error_type()
                    };
                    self.record_expression(source, *expression, ty);
                    ExpressionCheck {
                        ty,
                        falls_through: true,
                    }
                } else {
                    self.check_expression(source, *expression, expected, None, return_type)?
                };
                let valid = if let Some(subject) = subject {
                    self.assignable(result.ty, subject) || self.assignable(subject, result.ty)
                } else {
                    self.is_builtin(result.ty, BuiltinType::Boolean)
                };
                if !valid && !self.is_error(result.ty) && !self.is_deferred(result.ty) {
                    let primary = self
                        .file(source)
                        .ast()
                        .expressions()
                        .get(*expression)
                        .map_err(TypeCheckingError::from)?
                        .span();
                    self.emit(
                        codes::INVALID_WHEN_CONDITION,
                        "when condition does not match its subject form or type",
                        primary,
                    )?;
                }
                let valid = valid && !self.is_error(result.ty) && !self.is_deferred(result.ty);
                let non_null = valid
                    && !is_null
                    && !matches!(
                        self.signatures.types().get(result.ty),
                        Some(UnitTypeKind::Nullable(_))
                    );
                let coverage = if subject.is_some() && valid {
                    self.coverage_for_expression(source, *expression)?
                        .into_iter()
                        .collect()
                } else {
                    Vec::new()
                };
                let facts = self.facts_for_single_case(subject, subject_key, &coverage)?;
                Ok(WhenConditionCheck {
                    coverage,
                    facts,
                    valid,
                    non_null,
                })
            }
            WhenCondition::TypeTest {
                operator_span,
                negated,
                type_ref,
            } => {
                let target = self.resolve_type_test_ref(source, *type_ref, subject)?;
                let Some(subject) = subject else {
                    let target_span = self
                        .file(source)
                        .ast()
                        .type_refs()
                        .get(*type_ref)
                        .map_err(TypeCheckingError::from)?
                        .span();
                    self.emit_maybe_label(
                        codes::INVALID_WHEN_CONDITION,
                        "subjectless when cannot use an omitted-subject type test",
                        *operator_span,
                        Some(target_span),
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
                    let target_span = self
                        .file(source)
                        .ast()
                        .type_refs()
                        .get(*type_ref)
                        .map_err(TypeCheckingError::from)?
                        .span();
                    self.emit_maybe_label(
                        codes::INVALID_TYPE_TEST,
                        "type test target is not runtime-testable from the when subject",
                        *operator_span,
                        Some(target_span),
                        "invalid type-test target",
                    )?;
                }
                let valid = valid && !self.is_error(target) && !self.is_deferred(target);
                let non_null = valid
                    && !*negated
                    && !matches!(
                        self.signatures.types().get(target),
                        Some(UnitTypeKind::Nullable(_))
                    );
                let mut coverage = match self.signatures.types().get(target) {
                    Some(UnitTypeKind::EnumCase { case, .. }) if valid => {
                        vec![CoverageAtom::Case(*case)]
                    }
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
                    non_null,
                })
            }
            WhenCondition::Contains {
                operator_span,
                expression,
                ..
            } => {
                self.check_expression(source, *expression, None, None, return_type)?;
                let valid = subject.is_some();
                if !valid {
                    self.emit(
                        codes::INVALID_WHEN_CONDITION,
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

    fn when_source_valid(&self, source: Option<(FlowKey, u64)>) -> bool {
        source
            .is_some_and(|(key, version)| version != u64::MAX && self.flow_version(key) == version)
    }

    fn closed_domain(&self, ty: UnitTypeId) -> Option<Vec<CoverageAtom>> {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Builtin(BuiltinType::Boolean)) => Some(vec![
                CoverageAtom::Boolean(false),
                CoverageAtom::Boolean(true),
            ]),
            Some(UnitTypeKind::Nominal { declaration, .. }) => self
                .signatures
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .filter(|nominal| nominal.kind() == NominalKind::EnumClass)
                .map(|nominal| {
                    nominal
                        .enum_cases()
                        .iter()
                        .map(|case| CoverageAtom::Case(case.type_symbol()))
                        .collect()
                }),
            Some(UnitTypeKind::EnumCase { root, .. }) => self.closed_domain(*root),
            Some(UnitTypeKind::Nullable(inner)) => self.closed_domain(*inner).map(|mut domain| {
                domain.push(CoverageAtom::Null);
                domain
            }),
            _ => None,
        }
    }

    fn coverage_for_expression(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Result<Option<CoverageAtom>, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?;
        match node.payload() {
            Expression::Literal(LiteralKind::Boolean(value)) => {
                Ok(Some(CoverageAtom::Boolean(*value)))
            }
            Expression::Literal(LiteralKind::Null) => Ok(Some(CoverageAtom::Null)),
            Expression::Group { expression } => self.coverage_for_expression(source, *expression),
            Expression::Name => Ok(self.case_for_value_reference(source, node.span())),
            Expression::Member { name_span, .. } => {
                Ok(self.case_for_value_reference(source, *name_span))
            }
            _ => Ok(None),
        }
    }

    fn case_for_value_reference(&self, source: SourceUnitId, span: Span) -> Option<CoverageAtom> {
        let UnitReferenceTarget::Symbol(symbol) = self.reference(source, span, Namespace::Value)?
        else {
            return None;
        };
        self.signatures
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .flat_map(|nominal| nominal.enum_cases())
            .find(|case| case.value_symbol() == *symbol && case.payloads().is_empty())
            .map(|case| CoverageAtom::Case(case.type_symbol()))
    }

    fn facts_for_single_case(
        &mut self,
        subject: Option<UnitTypeId>,
        key: Option<FlowKey>,
        coverage: &[CoverageAtom],
    ) -> Result<BTreeMap<FlowKey, UnitTypeId>, CompilationUnitTypeError> {
        let (Some(subject), Some(key), [CoverageAtom::Case(case)]) = (subject, key, coverage)
        else {
            return Ok(BTreeMap::new());
        };
        let root = match self.signatures.types().get(subject) {
            Some(UnitTypeKind::Nullable(inner)) => *inner,
            Some(UnitTypeKind::EnumCase { root, .. }) => *root,
            Some(UnitTypeKind::Nominal { .. }) => subject,
            _ => return Ok(BTreeMap::new()),
        };
        let ty = self
            .signatures
            .types_mut()
            .intern(UnitTypeKind::EnumCase { case: *case, root });
        Ok(BTreeMap::from([(key, ty)]))
    }

    fn when_condition_span(
        &self,
        source: SourceUnitId,
        condition: &WhenCondition,
    ) -> Result<Span, CompilationUnitTypeError> {
        match condition {
            WhenCondition::Expression(expression) => self
                .file(source)
                .ast()
                .expressions()
                .get(*expression)
                .map(|node| node.span())
                .map_err(TypeCheckingError::from)
                .map_err(CompilationUnitTypeError::from),
            WhenCondition::TypeTest { operator_span, .. }
            | WhenCondition::Contains { operator_span, .. } => Ok(*operator_span),
        }
    }

    fn emit_non_exhaustive_when(
        &mut self,
        keyword_span: Span,
        domain: Option<&[CoverageAtom]>,
        covered: &BTreeMap<CoverageAtom, Span>,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(codes::NON_EXHAUSTIVE_WHEN)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "value-context when is not exhaustive",
            keyword_span,
        )?;
        if let Some(domain) = domain {
            let mut literals = Vec::new();
            for atom in domain.iter().filter(|atom| !covered.contains_key(atom)) {
                match atom {
                    CoverageAtom::Case(case) => {
                        let span = self
                            .enum_case_by_type_symbol(*case)
                            .map(|case| case.name_span())
                            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                        diagnostic.add_label(self.sources, span, "missing enum case")?;
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

    fn enum_case_by_type_symbol(
        &self,
        symbol: UnitSymbolId,
    ) -> Option<&crate::type_checking::UnitEnumCaseSignature> {
        self.signatures
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .flat_map(|nominal| nominal.enum_cases())
            .find(|case| case.type_symbol() == symbol)
    }

    fn join_when_branches(
        &mut self,
        source: SourceUnitId,
        entries: &[WhenEntry],
        branches: &[ExpressionCheck],
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let mut joined: Option<(UnitTypeId, usize)> = None;
        let mut failed = false;
        for (index, (entry, branch)) in entries.iter().zip(branches).enumerate() {
            if self.is_error(branch.ty) || failed {
                continue;
            }
            let Some((current, origin)) = joined else {
                joined = Some((branch.ty, index));
                continue;
            };
            if let Some(ty) = self.join_control_types(current, branch.ty) {
                let origin = if self.is_builtin(current, BuiltinType::Nothing) && ty != current {
                    index
                } else {
                    origin
                };
                joined = Some((ty, origin));
            } else {
                let primary = self
                    .file(source)
                    .ast()
                    .statements()
                    .get(entry.body)
                    .map_err(TypeCheckingError::from)?
                    .span();
                let first = self
                    .file(source)
                    .ast()
                    .statements()
                    .get(entries[origin].body)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_maybe_label(
                    codes::WHEN_BRANCH_TYPE,
                    "when branches do not have a legal common type",
                    primary,
                    Some(first),
                    format!(
                        "previous branches have common type {}",
                        self.type_name(current)
                    ),
                )?;
                failed = true;
            }
        }
        Ok(if failed {
            self.error_type()
        } else {
            joined.map_or_else(|| self.builtin(BuiltinType::Unit), |(ty, _)| ty)
        })
    }
}

/// Private unit domain: enum atoms retain source-qualified unit symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WhenFlowDomain {
    Empty,
    Null,
    NonNull,
    Nullable,
    Finite(Vec<CoverageAtom>),
}

impl WhenFlowDomain {
    pub(super) fn nullability(&self) -> crate::type_checking::UnitNullableWhenDomain {
        use crate::type_checking::UnitNullableWhenDomain as Domain;
        match self {
            Self::Empty => Domain::Empty,
            Self::Null => Domain::Null,
            Self::NonNull => Domain::NonNull,
            Self::Nullable => Domain::Nullable,
            Self::Finite(atoms) if atoms.is_empty() => Domain::Empty,
            Self::Finite(atoms) if atoms == &[CoverageAtom::Null] => Domain::Null,
            Self::Finite(atoms) if !atoms.contains(&CoverageAtom::Null) => Domain::NonNull,
            Self::Finite(_) => Domain::Nullable,
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, Self::Empty) || matches!(self, Self::Finite(atoms) if atoms.is_empty())
    }

    fn is_non_null(&self) -> bool {
        matches!(self, Self::NonNull)
            || matches!(self, Self::Finite(atoms) if !atoms.is_empty() && !atoms.contains(&CoverageAtom::Null))
    }
}

fn split_domain(
    input: &WhenFlowDomain,
    coverage: &[CoverageAtom],
    non_null: bool,
    valid: bool,
) -> (WhenFlowDomain, WhenFlowDomain) {
    if input.is_empty() {
        return (WhenFlowDomain::Empty, input.clone());
    }
    if !valid {
        // Error recovery is neither coverage nor a new proof for the shared body.
        return (input.clone(), input.clone());
    }
    if !coverage.is_empty() {
        if let WhenFlowDomain::Finite(atoms) = input {
            return (
                WhenFlowDomain::Finite(
                    atoms
                        .iter()
                        .copied()
                        .filter(|atom| coverage.contains(atom))
                        .collect(),
                ),
                WhenFlowDomain::Finite(
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
                WhenFlowDomain::Null => (WhenFlowDomain::Null, WhenFlowDomain::Empty),
                WhenFlowDomain::NonNull => (WhenFlowDomain::Empty, WhenFlowDomain::NonNull),
                _ => (WhenFlowDomain::Null, WhenFlowDomain::NonNull),
            };
        }
    }
    let matched = if non_null {
        match input {
            WhenFlowDomain::Null => WhenFlowDomain::Empty,
            WhenFlowDomain::Finite(atoms) => WhenFlowDomain::Finite(
                atoms
                    .iter()
                    .copied()
                    .filter(|atom| *atom != CoverageAtom::Null)
                    .collect(),
            ),
            _ => WhenFlowDomain::NonNull,
        }
    } else {
        input.clone()
    };
    // A nonliteral non-null match does not prove that its miss is null.
    (matched, input.clone())
}

fn union_domain(left: WhenFlowDomain, right: WhenFlowDomain) -> WhenFlowDomain {
    if left.is_empty() {
        return right;
    }
    if right.is_empty() || left == right {
        return left;
    }
    if let (WhenFlowDomain::Finite(left), WhenFlowDomain::Finite(right)) = (&left, &right) {
        let mut atoms = left.clone();
        atoms.extend(right.iter().filter(|atom| !left.contains(atom)).copied());
        atoms.sort();
        return WhenFlowDomain::Finite(atoms);
    }
    if left.is_non_null() && right.is_non_null() {
        WhenFlowDomain::NonNull
    } else {
        WhenFlowDomain::Nullable
    }
}
