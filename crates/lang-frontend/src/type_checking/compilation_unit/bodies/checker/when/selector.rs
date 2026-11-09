//! Selector type checking and private writeback edges.

use super::*;

pub(super) struct WhenConditionCheck {
    pub(super) coverage: Vec<CoverageAtom>,
    pub(super) facts: BTreeMap<FlowKey, UnitTypeId>,
    pub(super) valid: bool,
    pub(super) non_null: bool,
    pub(super) operand: Option<(ExpressionId, ExpressionCheck)>,
}

impl BodyChecker<'_> {
    pub(super) fn pending_when_selector_edges(
        &self,
        source: SourceUnitId,
        subjectless: bool,
        checked: &WhenConditionCheck,
    ) -> Result<PendingBooleanEdges, CompilationUnitTypeError> {
        if let Some((operand, result)) = checked.operand {
            if !self.has_pending_expression_exit(source, operand, result)? {
                return Ok(PendingBooleanEdges::default());
            }
            if subjectless && checked.valid {
                return self.boolean_pending_edges(source, operand, &self.flow_versions);
            }
        }
        let keys: BTreeSet<_> = self.flow_versions.pending_keys().collect();
        Ok(PendingBooleanEdges {
            yes: Some(keys.clone()),
            no: Some(keys),
        })
    }

    pub(super) fn when_selector_pending(
        &self,
        source: SourceUnitId,
        condition: &WhenCondition,
        deferred: usize,
    ) -> bool {
        match condition {
            WhenCondition::Expression(expression) | WhenCondition::Contains { expression, .. } => {
                let key = UnitExpressionId::new(source, *expression);
                self.parts
                    .expression_types
                    .get(&key)
                    .is_some_and(|&ty| self.replacement_value_pending(key, ty, deferred))
            }
            WhenCondition::TypeTest { type_ref, .. } => {
                let key = UnitTypeRefId::new(source, *type_ref);
                self.parts
                    .type_ref_types
                    .get(&key)
                    .is_some_and(|&ty| self.contains_unresolved_call_type(ty))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_when_condition(
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
                    operand: Some((*expression, result)),
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
                        operand: None,
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
                    operand: None,
                })
            }
            WhenCondition::Contains {
                operator_span,
                expression,
                ..
            } => {
                let result = self.check_expression(source, *expression, None, None, return_type)?;
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
                    operand: Some((*expression, result)),
                })
            }
        }
    }
}
