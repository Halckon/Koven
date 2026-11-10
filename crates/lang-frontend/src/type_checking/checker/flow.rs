use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::{Namespace, ReferenceTarget, ScopeKind, SymbolId, SymbolKind},
    parser::{
        BinaryOperator, Expression, Item, LiteralKind, NameMarker, PrefixOperator, VariableKind,
    },
    source::Span,
};

use crate::type_checking::flow_effects::merge_flow_versions;

use super::{pending_boolean::PendingBooleanEdges, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FlowKey {
    This,
    Symbol(SymbolId),
}

type ConditionFacts = (BTreeMap<FlowKey, TypeId>, BTreeMap<FlowKey, TypeId>);

impl Checker<'_> {
    pub(super) fn check_pending_logical_rhs(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        left_result: ExprCheck,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let left_edges = self.pending_boolean_edges(left, left_result.falls_through)?;
        let (entry, bypass) = if operator == BinaryOperator::LogicalAnd {
            (
                left_edges.yes,
                PendingBooleanEdges {
                    yes: None,
                    no: left_edges.no,
                },
            )
        } else {
            (
                left_edges.no,
                PendingBooleanEdges {
                    yes: left_edges.yes,
                    no: None,
                },
            )
        };
        let reaches_right = entry.is_some();
        self.iteration_recovery.bindings = entry.unwrap_or_default();
        let result = self.check_pending_path(reaches_right, |checker| {
            checker.check_expression(right, None, None)
        })?;
        let mut exits = bypass;
        if reaches_right {
            exits.merge(self.pending_boolean_edges(right, result.falls_through)?);
        }
        self.finish_pending_boolean(expression, exits);
        Ok(result)
    }

    pub(super) fn check_non_null_assertion(
        &mut self,
        expression: ExpressionId,
        operand: ExpressionId,
        operator_span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        self.call_recoveries.remove(&expression.index());
        let checked = self.check_expression(operand, None, None)?;
        let recovery = self.call_operand_recovery(operand, checked.ty);
        let ty = match self.kind(checked.ty) {
            TypeKind::Nullable(inner) => {
                let inner = *inner;
                if recovery.is_none() {
                    self.non_null_assertions.push(
                        crate::type_checking::NonNullAssertionDescriptor {
                            expression,
                            operand,
                            operator_span,
                            nullable_type: checked.ty,
                            inner_type: inner,
                            category: self.expression_categories[operand.index()],
                            source_category: self.nullable_when_category(operand)?,
                            copyability: self.copyability_of(inner),
                        },
                    );
                    self.non_null_assertions
                        .sort_by_key(|plan| plan.expression().index());
                }
                inner
            }
            TypeKind::Error => self.error_type(),
            TypeKind::Deferred(_) => self.deferred(DeferredReason::MemberAccess),
            _ => {
                self.emit(
                    self.operands_code,
                    "non-null assertion requires a nullable operand",
                    operator_span,
                )?;
                return Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: checked.falls_through,
                });
            }
        };
        if let Some(recovery) = recovery {
            self.call_recoveries.insert(expression.index(), recovery);
        }
        Ok(ExprCheck {
            ty,
            falls_through: checked.falls_through,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_elvis(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator_span: Span,
        right: ExpressionId,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        self.call_recoveries.remove(&expression.index());
        let left_result = self.check_expression(left, None, None)?;
        let left_completes = self.pending_expression_exit(left, left_result.falls_through);
        let (inner, left_can_bypass_right) = match self.kind(left_result.ty) {
            TypeKind::Nullable(inner) => (
                Some(*inner),
                !matches!(self.kind(*inner), TypeKind::Builtin(BuiltinType::Nothing)),
            ),
            TypeKind::Error => (None, false),
            TypeKind::Deferred(_) => (None, true),
            _ => {
                let right_result = self.check_pending_path(left_completes, |checker| {
                    checker.check_expression(right, expected, expected_span)
                })?;
                self.emit_binary_operand_error(
                    operator_span,
                    left,
                    left_result.ty,
                    right,
                    right_result.ty,
                )?;
                return Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: left_completes,
                });
            }
        };
        let right_expected = inner
            .filter(|inner| !matches!(self.kind(*inner), TypeKind::Builtin(BuiltinType::Nothing)));
        let bypass =
            left_can_bypass_right.then(|| (self.flow_facts.clone(), self.flow_versions.clone()));
        let can_bypass = left_can_bypass_right && left_completes;
        let mut pending_exits = if can_bypass {
            PendingBooleanEdges::unknown(&self.iteration_recovery.bindings)
        } else {
            PendingBooleanEdges::default()
        };
        let right_result = self.check_pending_path(left_completes, |checker| {
            checker.check_expression(right, right_expected.or(expected), expected_span)
        })?;
        let right_completes = self.pending_expression_exit(right, right_result.falls_through);
        if left_completes {
            pending_exits.merge(self.pending_boolean_edges(right, right_completes)?);
        }
        self.finish_pending_boolean(expression, pending_exits);
        if let Some((facts, versions)) = bypass {
            if right_completes {
                self.flow_facts = intersect_facts(&facts, &self.flow_facts);
                self.flow_versions = merge_flow_versions(&versions, &self.flow_versions);
            } else {
                self.flow_facts = facts;
                self.flow_versions = versions;
            }
        }
        // Both operands must type-check, even when the right side is conditional.
        // A usable join type alone does not authorize an enclosing call.
        if let Some(recovery) = self
            .call_operand_recovery(left, left_result.ty)
            .max(self.call_operand_recovery(right, right_result.ty))
        {
            self.call_recoveries.insert(expression.index(), recovery);
        }
        let ty = if let Some(inner) = inner {
            if matches!(self.kind(inner), TypeKind::Builtin(BuiltinType::Nothing)) {
                right_result.ty
            } else {
                inner
            }
        } else {
            self.deferred(DeferredReason::ControlJoin)
        };
        Ok(ExprCheck {
            ty,
            falls_through: left_completes && (left_can_bypass_right || right_completes),
        })
    }

    // A stable narrowed read still compares against its declared nullable storage.
    // This expected type belongs only to the other, literal-null operand.
    pub(super) fn null_comparison_expected(
        &self,
        expression: ExpressionId,
        read_type: TypeId,
    ) -> Option<TypeId> {
        if matches!(self.kind(read_type), TypeKind::Nullable(_)) {
            return Some(read_type);
        }
        if matches!(
            self.kind(read_type),
            TypeKind::Error | TypeKind::Deferred(_)
        ) {
            return None;
        }
        let FlowKey::Symbol(symbol) = self.stable_flow_key(expression)? else {
            return None;
        };
        let declared = self.symbol_type(symbol)?;
        let TypeKind::Nullable(inner) = self.kind(declared) else {
            return None;
        };
        (*inner == read_type && self.flow_facts.get(&FlowKey::Symbol(symbol)) == Some(&read_type))
            .then_some(declared)
    }

    pub(super) fn collect_flow_metadata(&mut self) {
        let variables = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Variable {
                    kind: VariableKind::Var,
                    name: NameMarker::Present(span),
                    ..
                } => Some(*span),
                _ => None,
            })
            .collect::<Vec<_>>();
        for span in variables {
            if let Some(symbol) = self.symbol_at(span) {
                self.mutable_symbols.insert(symbol);
            }
        }
        for (scope, target) in &self.source_references {
            let ReferenceTarget::Symbol(symbol) = target else {
                continue;
            };
            if !self.mutable_symbols.contains(symbol) {
                continue;
            }
            let declaration_scope = self.symbol_scopes[symbol.index()];
            let mut current = Some(*scope);
            while let Some(id) = current {
                if id == declaration_scope {
                    break;
                }
                if self.scope_kinds[id.index()] == ScopeKind::Lambda {
                    self.captured_mutable_symbols.insert(*symbol);
                    break;
                }
                current = self.scope_parents[id.index()];
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_if(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_span: Option<Span>,
        else_branch: Option<StatementId>,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let boolean = self.builtin(BuiltinType::Boolean);
        let checked = self.check_expression(condition, Some(boolean), None)?;
        let edges = self.pending_boolean_edges(condition, checked.falls_through)?;
        let reaches_then = edges.yes.is_some();
        let reaches_else = edges.no.is_some();
        let baseline = self.flow_facts.clone();
        let baseline_versions = self.flow_versions.clone();
        self.iteration_recovery.bindings = edges.yes.unwrap_or_default();
        let (true_facts, false_facts) = self.condition_facts(condition)?;
        self.flow_facts = extend_facts(&baseline, &true_facts);
        let then_expected = if else_branch.is_some() {
            self.control_branch_expected(then_branch, expected)?
        } else {
            expected
        };
        let then_result = self.check_pending_path(reaches_then, |checker| {
            checker.check_value_body(then_branch, then_expected, expected_span)
        })?;
        let then_exits =
            reaches_then && self.pending_statement_exit(then_branch, then_result.falls_through)?;
        let mut pending_exits = PendingBooleanEdges::default();
        if then_exits {
            pending_exits.merge(self.pending_branch_edges(then_branch)?);
        }
        let then_facts = self.flow_facts.clone();
        let then_versions = self.flow_versions.clone();
        let Some(else_branch) = else_branch else {
            if let Some(bindings) = edges.no {
                pending_exits.merge(PendingBooleanEdges::unknown(&bindings));
            }
            self.finish_pending_boolean(expression, pending_exits);
            let false_path = extend_facts(&baseline, &false_facts);
            self.flow_facts = if then_result.falls_through {
                intersect_facts(&then_facts, &false_path)
            } else {
                false_path
            };
            self.flow_versions = if then_result.falls_through {
                merge_flow_versions(&then_versions, &baseline_versions)
            } else {
                baseline_versions
            };
            return Ok(ExprCheck {
                ty: self.builtin(BuiltinType::Unit),
                falls_through: true,
            });
        };
        self.flow_facts = extend_facts(&baseline, &false_facts);
        self.flow_versions = baseline_versions.clone();
        self.iteration_recovery.bindings = edges.no.unwrap_or_default();
        let else_expected = self.control_branch_expected(else_branch, expected)?;
        let else_result = self.check_pending_path(reaches_else, |checker| {
            checker.check_value_body(else_branch, else_expected, expected_span)
        })?;
        let else_exits =
            reaches_else && self.pending_statement_exit(else_branch, else_result.falls_through)?;
        if else_exits {
            pending_exits.merge(self.pending_branch_edges(else_branch)?);
        }
        self.finish_pending_boolean(expression, pending_exits);
        let else_facts = self.flow_facts.clone();
        let else_versions = self.flow_versions.clone();
        self.flow_facts = match (then_result.falls_through, else_result.falls_through) {
            (true, true) => intersect_facts(&then_facts, &else_facts),
            (true, false) => then_facts,
            (false, true) => else_facts,
            (false, false) => baseline,
        };
        self.flow_versions = match (then_result.falls_through, else_result.falls_through) {
            (true, true) => merge_flow_versions(&then_versions, &else_versions),
            (true, false) => then_versions,
            (false, true) => else_versions,
            (false, false) => baseline_versions,
        };
        let ty = if let Some(join) = self.join_control_types(then_result.ty, else_result.ty) {
            join
        } else {
            let primary = else_span.unwrap_or(self.ast().statements().get(else_branch)?.span());
            let first = self.ast().statements().get(then_branch)?.span();
            self.emit_with_label(
                self.branch_type_code,
                "control branches do not have a common type",
                primary,
                first,
                format!("first branch has type {}", self.type_name(then_result.ty)),
            )?;
            self.error_type()
        };
        Ok(ExprCheck {
            ty,
            falls_through: then_result.falls_through || else_result.falls_through,
        })
    }

    pub(super) fn condition_facts(
        &self,
        id: ExpressionId,
    ) -> Result<ConditionFacts, TypeCheckingError> {
        let payload = self.ast().expressions().get(id)?.payload().clone();
        match payload {
            Expression::Group { expression } => self.condition_facts(expression),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                let (when_true, when_false) = self.condition_facts(operand)?;
                Ok((when_false, when_true))
            }
            Expression::TypeTest {
                expression,
                negated,
                type_ref,
                ..
            } => {
                let mut positive = BTreeMap::new();
                if let Some(key) = self.stable_flow_key(expression)
                    && let Some(target) = self.type_ref_types[type_ref.index()]
                    && !self.is_error(target)
                    && !self.is_deferred(target)
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
                let (mut left_true, left_false) = self.condition_facts(left)?;
                let (right_true, right_false) = self.condition_facts(right)?;
                let writes = self.expression_flow_writes(right)?;
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
                let (left_true, mut left_false) = self.condition_facts(left)?;
                let (right_true, right_false) = self.condition_facts(right)?;
                let writes = self.expression_flow_writes(right)?;
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
                self.null_comparison_facts(left, operator, right)
            }
            _ => Ok((BTreeMap::new(), BTreeMap::new())),
        }
    }

    fn null_comparison_facts(
        &self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> Result<ConditionFacts, TypeCheckingError> {
        let left_null = matches!(
            self.ast().expressions().get(left)?.payload(),
            Expression::Literal(LiteralKind::Null)
        );
        let right_null = matches!(
            self.ast().expressions().get(right)?.payload(),
            Expression::Literal(LiteralKind::Null)
        );
        let candidate = match (left_null, right_null) {
            (true, false) => right,
            (false, true) => left,
            _ => return Ok((BTreeMap::new(), BTreeMap::new())),
        };
        let Some(key) = self.stable_flow_key(candidate) else {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        };
        let Some(ty) = self.expression_types[candidate.index()] else {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        };
        let TypeKind::Nullable(inner) = self.kind(ty) else {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        };
        let positive = BTreeMap::from([(key, *inner)]);
        if operator == BinaryOperator::NotEqual {
            Ok((positive, BTreeMap::new()))
        } else {
            Ok((BTreeMap::new(), positive))
        }
    }

    pub(super) fn stable_flow_key(&self, id: ExpressionId) -> Option<FlowKey> {
        let node = self.ast().expressions().get(id).ok()?;
        match node.payload() {
            Expression::This => Some(FlowKey::This),
            Expression::Group { expression } => self.stable_flow_key(*expression),
            Expression::Name => {
                let ReferenceTarget::Symbol(symbol) =
                    self.reference(node.span(), Namespace::Value)?
                else {
                    return None;
                };
                let kind = self.symbol_kinds.get(symbol.index())?;
                let eligible = match kind {
                    SymbolKind::ValueParameter | SymbolKind::LambdaParameter => true,
                    SymbolKind::Variable => {
                        self.scope_kinds[self.symbol_scopes[symbol.index()].index()]
                            != ScopeKind::File
                    }
                    _ => false,
                };
                (eligible && !self.captured_mutable_symbols.contains(symbol))
                    .then_some(FlowKey::Symbol(*symbol))
            }
            _ => None,
        }
    }

    pub(super) fn expression_use(&self, id: ExpressionId) -> ExpressionUse {
        self.expression_uses[id.index()]
    }
}

pub(super) fn extend_facts(
    baseline: &BTreeMap<FlowKey, TypeId>,
    additions: &BTreeMap<FlowKey, TypeId>,
) -> BTreeMap<FlowKey, TypeId> {
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
    left: &BTreeMap<FlowKey, TypeId>,
    right: &BTreeMap<FlowKey, TypeId>,
) -> BTreeMap<FlowKey, TypeId> {
    left.iter()
        .filter_map(|(&key, &ty)| (right.get(&key) == Some(&ty)).then_some((key, ty)))
        .collect()
}
