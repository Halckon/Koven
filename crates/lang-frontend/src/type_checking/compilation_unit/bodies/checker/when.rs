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

use super::flow::{extend_facts, intersect_facts};
use super::{BodyChecker, ExpressionCheck, flow::FlowKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CoverageAtom {
    Boolean(bool),
    Null,
    Case(UnitSymbolId),
}

struct WhenConditionCheck {
    coverage: Vec<CoverageAtom>,
    facts: BTreeMap<FlowKey, UnitTypeId>,
    valid: bool,
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
        let baseline = self.flow_facts.clone();
        let mut covered = BTreeMap::<CoverageAtom, Span>::new();
        let mut first_else = None;
        let mut has_else = false;
        let mut branches = Vec::new();
        let mut exits = Vec::new();

        for (entry_index, entry) in entries.iter().enumerate() {
            let mut entry_facts = None;
            if let Some(else_span) = entry.else_span {
                has_else = true;
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
                    entry_facts = Some(match entry_facts {
                        None => checked.facts,
                        Some(previous) => intersect_facts(&previous, &checked.facts),
                    });
                }
            }
            self.flow_facts = if let Some(ref facts) = entry_facts {
                extend_facts(&baseline, facts)
            } else {
                baseline.clone()
            };
            let result =
                self.check_value_body(source, entry.body, expected, expected_span, return_type)?;
            if result.falls_through {
                exits.push(self.flow_facts.clone());
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
            exits.push(baseline.clone());
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
            self.join_when_branches(source, entries, &branches)?
        };
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
                })
            }
            WhenCondition::TypeTest {
                operator_span,
                negated,
                type_ref,
            } => {
                let target = self.resolve_type_test_ref(source, *type_ref)?;
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
                })
            }
        }
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
                            .get(entries[0].body)
                            .map_err(TypeCheckingError::from)?
                            .span();
                        self.emit_maybe_label(
                            codes::WHEN_BRANCH_TYPE,
                            "when branches do not have a legal common type",
                            primary,
                            Some(first),
                            format!("first branch has type {}", self.type_name(current)),
                        )?;
                        self.error_type()
                    }
                },
            });
        }
        Ok(joined.unwrap_or_else(|| self.builtin(BuiltinType::Unit)))
    }

    fn join_when_types(&self, left: UnitTypeId, right: UnitTypeId) -> Option<UnitTypeId> {
        if left == right {
            return Some(left);
        }
        if self.is_builtin(left, BuiltinType::Nothing) {
            return Some(right);
        }
        if self.is_builtin(right, BuiltinType::Nothing) {
            return Some(left);
        }
        match (
            self.signatures.types().get(left),
            self.signatures.types().get(right),
        ) {
            (Some(UnitTypeKind::Error), _) => Some(right),
            (_, Some(UnitTypeKind::Error)) => Some(left),
            (Some(UnitTypeKind::Nullable(_)), _) if self.assignable(right, left) => Some(left),
            (_, Some(UnitTypeKind::Nullable(_))) if self.assignable(left, right) => Some(right),
            (Some(UnitTypeKind::EnumCase { root, .. }), _) if *root == right => Some(right),
            (_, Some(UnitTypeKind::EnumCase { root, .. })) if left == *root => Some(left),
            (
                Some(UnitTypeKind::EnumCase {
                    root: left_root, ..
                }),
                Some(UnitTypeKind::EnumCase {
                    root: right_root, ..
                }),
            ) if left_root == right_root => Some(*left_root),
            _ if !self.is_deferred(left) && !self.is_deferred(right) => {
                Some(self.builtin(BuiltinType::Any))
            }
            _ => None,
        }
    }
}
