//! SPEC-0197 compilation-unit `when` 穷尽性、分支类型与 flow facts。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget, UnitSymbolId},
    parser::{Expression, LiteralKind, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, DeferredReason, ExpressionUse, NominalKind,
        TypeCheckingError, UnitExpressionId, UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::flow::{intersect_facts, merge_flow_versions};
use super::{
    BodyChecker, ExpressionCheck,
    flow::FlowKey,
    while_flow::{PendingBooleanEdges, PendingControlSite},
};

mod selector;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum CoverageAtom {
    Boolean(bool),
    Null,
    Case(UnitSymbolId),
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
        let value_use = self.expression_use(source, expression) == ExpressionUse::Value;
        let deferred = self.deferred_facts.len();
        let subject_result = subject
            .map(|subject| self.check_expression(source, subject, None, None, return_type))
            .transpose()?;
        let subject_type = subject_result.map(|result| result.ty);
        let subject_completes = subject_result.is_none_or(|result| result.falls_through);
        let subject_reaches_arms = match subject.zip(subject_result) {
            Some((subject, result)) => self.has_pending_expression_exit(source, subject, result)?,
            None => true,
        };
        let mut replacement_pending = value_use
            && subject_reaches_arms
            && subject.zip(subject_type).is_some_and(|(subject, ty)| {
                self.replacement_value_pending(UnitExpressionId::new(source, subject), ty, deferred)
            });
        let subject_key = subject.and_then(|subject| self.stable_flow_key(source, subject));
        let domain = subject_type.and_then(|ty| self.closed_domain(ty));
        let nullable_inner = subject_type.and_then(|ty| match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Nullable(inner)) => Some(*inner),
            _ => None,
        });
        let subject_source = subject_key.map(|key| (key, self.flow_version(key)));
        let mut nullable_when = if subject_completes {
            self.begin_nullable_when(
                source,
                expression,
                subject,
                subject_type,
                nullable_inner,
                subject_source,
                entries,
            )?
        } else {
            None
        };
        let baseline = self.flow_facts.clone();
        let baseline_versions = self.flow_versions.clone();
        let mut remaining_facts = baseline.clone();
        let mut remaining_versions = baseline_versions.clone();
        let mut remaining_pending =
            subject_reaches_arms.then(|| baseline_versions.pending_keys().collect::<BTreeSet<_>>());
        let mut remaining_domain = domain
            .clone()
            .map(WhenFlowDomain::Finite)
            .unwrap_or(WhenFlowDomain::Nullable);
        let mut covered = BTreeMap::<CoverageAtom, Span>::new();
        let mut first_else = None;
        let mut has_else = false;
        let mut branches = Vec::new();
        let mut exits = Vec::new();
        let mut pending_exits = None::<BTreeSet<_>>;
        let mut boolean_exits = value_use.then(PendingBooleanEdges::default);
        let mut selector_stops_miss = false;
        let mut closed_after = None;

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
            let mut entry_pending = None::<BTreeSet<_>>;
            let mut body_domain = WhenFlowDomain::Empty;
            let mut entry_source_valid = self.when_source_valid(subject_source);
            if let Some(else_span) = entry.else_span {
                has_else = true;
                body_domain = remaining_domain.clone();
                remaining_domain = WhenFlowDomain::Empty;
                entry_pending = remaining_pending.take();
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
                for (alternative, condition) in entry.conditions.iter().enumerate() {
                    let deferred = self.deferred_facts.len();
                    let condition_span = self.when_condition_span(source, condition)?;
                    self.flow_versions
                        .set_pending_keys(remaining_pending.iter().flatten().copied());
                    let reachable = remaining_pending.is_some() && !remaining_domain.is_empty();
                    let ordinary = subject_completes && !remaining_domain.is_empty();
                    let checked = self.check_loop_paths(ordinary, reachable, |checker| {
                        checker.check_when_condition(
                            source,
                            subject_type,
                            subject_key,
                            domain.as_deref(),
                            condition,
                            return_type,
                        )
                    })?;
                    if value_use && reachable {
                        replacement_pending |= !checked.valid
                            || self.when_selector_pending(source, condition, deferred);
                    }
                    let source_valid = self.when_source_valid(subject_source);
                    let (matched, missed) = if checked
                        .operand
                        .is_none_or(|(_, result)| result.falls_through)
                    {
                        split_domain(
                            &remaining_domain,
                            &checked.coverage,
                            checked.non_null,
                            checked.valid,
                        )
                    } else {
                        // A selector exit stops the unmatched path. Earlier alternatives
                        // have already contributed their independent match paths.
                        selector_stops_miss |= !remaining_domain.is_empty();
                        (WhenFlowDomain::Empty, WhenFlowDomain::Empty)
                    };
                    let pending = if reachable {
                        self.pending_when_selector_edges(source, subject.is_none(), &checked)?
                    } else {
                        PendingBooleanEdges::default()
                    };
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
                        if let Some(keys) = pending.yes {
                            entry_pending.get_or_insert_default().extend(keys);
                        }
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
                    if domain.is_some() && remaining_domain.is_empty() {
                        closed_after.get_or_insert((entry_index + 1, alternative + 1));
                    }
                    remaining_pending = pending.no.filter(|_| !remaining_domain.is_empty());
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
            let body_reachable = entry_pending.is_some();
            self.flow_versions
                .set_pending_keys(entry_pending.into_iter().flatten());
            if entry_source_valid
                && let (Some(key), Some(inner)) = (subject_key, nullable_inner)
                && body_domain.is_non_null()
            {
                self.flow_facts.entry(key).or_insert(inner);
            }
            // A nullable subject can reach one case after earlier alternatives remove null.
            // Keep the existing non-nullable remaining-domain contract unchanged.
            if nullable_inner.is_some()
                && entry_source_valid
                && let WhenFlowDomain::Finite(atoms) = &body_domain
            {
                let facts = self.facts_for_single_case(subject_type, subject_key, atoms)?;
                self.flow_facts.extend(facts);
            }
            let branch_expected = if value_use {
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
            let mut branch_pending = false;
            let ordinary = subject_completes && !body_domain.is_empty();
            let result = self.check_loop_paths(ordinary, body_reachable, |checker| {
                checker.check_value_body_with_replacement(
                    source,
                    entry.body,
                    branch_expected,
                    expected_span,
                    return_type,
                    value_use.then_some(&mut branch_pending),
                )
            });
            self.active_nullable_whens.truncate(frame_base);
            let result = result?;
            super::nullable_when::finish_entry(
                &mut nullable_when,
                nullable_entry,
                &body_domain,
                &remaining_domain,
                result.falls_through,
            );
            if body_reachable && self.has_pending_statement_exit(source, entry.body, result) {
                replacement_pending |= branch_pending;
                pending_exits
                    .get_or_insert_default()
                    .extend(self.flow_versions.pending_keys());
                if let Some(edges) = &mut boolean_exits {
                    if self.is_builtin(result.ty, BuiltinType::Boolean) {
                        self.merge_boolean_branch(edges, source, entry.body, &self.flow_versions)?;
                    } else {
                        boolean_exits = None;
                    }
                }
            }
            if !body_reachable {
                // Static body checking can complete even when no selector can reach it.
                self.note_pending_statement_exit(source, entry.body, false);
            }
            if subject_completes && !body_domain.is_empty() && result.falls_through {
                exits.push((self.flow_facts.clone(), self.flow_versions.clone()));
            }
            branches.push(result);
        }

        let exhaustive = has_else
            || domain
                .as_ref()
                .is_some_and(|domain| domain.iter().all(|atom| covered.contains_key(atom)));
        if value_use && !exhaustive {
            self.emit_non_exhaustive_when(keyword_span, domain.as_deref(), &covered)?;
        }
        if !exhaustive && !remaining_domain.is_empty() {
            if let Some(keys) = remaining_pending {
                pending_exits.get_or_insert_default().extend(keys);
            }
            if subject_completes {
                exits.push((remaining_facts, remaining_versions));
            }
        }
        let falls_through = !exits.is_empty();
        let control = UnitExpressionId::new(source, expression);
        if let Some(closed_after) = closed_after {
            self.parts.when_closed_after.insert(control, closed_after);
        } else {
            self.parts.when_closed_after.remove(&control);
        }
        self.note_pending_exit(
            PendingControlSite::Expression(control),
            pending_exits.is_some(),
        );
        if exhaustive
            && pending_exits.is_some()
            && let Some(edges) = boolean_exits
        {
            self.pending_boolean_edges.insert(control, edges);
        } else {
            self.pending_boolean_edges.remove(&control);
        }
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
        self.flow_versions
            .set_pending_keys(pending_exits.into_iter().flatten());
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
            self.join_when_branches(
                UnitExpressionId::new(source, expression),
                entries,
                &branches,
                expected,
            )?
        };
        // Keep lexical branch diagnostics and the existing static shape of an
        // abrupt subject. A selector that stops the miss path delivers no value.
        let ty = if value_use
            && exhaustive
            && subject_completes
            && selector_stops_miss
            && !falls_through
            && !self.is_error(ty)
            && !self.is_deferred(ty)
        {
            self.builtin(BuiltinType::Nothing)
        } else {
            ty
        };
        if let Some(mut descriptor) = nullable_when {
            descriptor.falls_through = falls_through;
            self.parts.nullable.whens.push(descriptor);
        }
        if value_use && exhaustive {
            self.replacement_values.insert(
                UnitExpressionId::new(source, expression),
                replacement_pending,
            );
        }
        Ok(ExpressionCheck { ty, falls_through })
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
        expression: UnitExpressionId,
        entries: &[WhenEntry],
        branches: &[ExpressionCheck],
        expected: Option<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let source = expression.source_unit();
        let mut joined: Option<(UnitTypeId, usize)> = None;
        let contextual = matches!(
            expected.and_then(|ty| self.signatures.types().get(ty)),
            Some(UnitTypeKind::Deferred(
                DeferredReason::Call | DeferredReason::ContextualArgument
            ))
        );
        let mut unresolved = None;
        let mut failed = false;
        for (index, (entry, branch)) in entries.iter().zip(branches).enumerate() {
            if self.is_error(branch.ty) || failed {
                continue;
            }
            let requirement = self.contextual_branch_requirement(source, entry.body)?;
            let mut branch_ty =
                self.contextual_when_branch_type(source, entry.body, branch.ty, expected)?;
            // A wholly unknown branch cannot erase a requirement proved by
            // another branch, but the result still depends on its resolution.
            if contextual
                && self.is_unresolved_call_type(branch_ty)
                && !self.has_contextual_integer(branch_ty)
            {
                unresolved.get_or_insert(branch_ty);
                let Some(required) = requirement else {
                    continue;
                };
                branch_ty = required;
            }
            let Some((current, origin)) =
                joined.filter(|&(current, _)| !self.is_builtin(current, BuiltinType::Nothing))
            else {
                if let Some(required) = requirement {
                    self.parts
                        .contextual_argument_requirements
                        .insert(expression, required);
                }
                joined = Some((branch_ty, index));
                continue;
            };
            if self.is_builtin(branch.ty, BuiltinType::Nothing) {
                continue;
            }
            let previous = self
                .parts
                .contextual_argument_requirements
                .get(&expression)
                .copied();
            if let Some(ty) = self.join_value_branches(
                expression,
                (entries[origin].body, current),
                (entry.body, branch_ty),
                expected,
            )? {
                let required = self
                    .parts
                    .contextual_argument_requirements
                    .get(&expression)
                    .copied();
                let origin =
                    if required != previous && required == Some(requirement.unwrap_or(branch_ty)) {
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
            self.parts
                .contextual_argument_requirements
                .remove(&expression);
            self.error_type()
        } else if let Some((ty, _)) = joined {
            if let Some(deferred) = unresolved
                && !self.contains_unresolved_call_type(ty)
            {
                self.parts
                    .contextual_argument_requirements
                    .insert(expression, ty);
                deferred
            } else {
                ty
            }
        } else {
            unresolved.unwrap_or_else(|| self.builtin(BuiltinType::Unit))
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
