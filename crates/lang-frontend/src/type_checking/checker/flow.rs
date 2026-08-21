use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::{Namespace, ReferenceTarget, ScopeKind, SymbolId, SymbolKind},
    parser::{BinaryOperator, Expression, LiteralKind, ParsedFile, PrefixOperator, Statement},
    source::Span,
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpressionUse {
    Value,
    Statement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FlowKey {
    This,
    Symbol(SymbolId),
}

type ConditionFacts = (BTreeMap<FlowKey, TypeId>, BTreeMap<FlowKey, TypeId>);

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_if(
        &mut self,
        condition: ExpressionId,
        then_branch: StatementId,
        else_span: Option<Span>,
        else_branch: Option<StatementId>,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let boolean = self.builtin(BuiltinType::Boolean);
        self.check_expression(condition, Some(boolean), None)?;
        let baseline = self.flow_facts.clone();
        let (true_facts, false_facts) = self.condition_facts(condition)?;
        self.flow_facts = extend_facts(&baseline, &true_facts);
        let then_result = self.check_value_body(then_branch, expected, expected_span)?;
        let then_facts = self.flow_facts.clone();
        let Some(else_branch) = else_branch else {
            let false_path = extend_facts(&baseline, &false_facts);
            self.flow_facts = if then_result.falls_through {
                intersect_facts(&then_facts, &false_path)
            } else {
                false_path
            };
            return Ok(ExprCheck {
                ty: self.builtin(BuiltinType::Unit),
                falls_through: true,
            });
        };
        self.flow_facts = extend_facts(&baseline, &false_facts);
        let else_result = self.check_value_body(else_branch, expected, expected_span)?;
        let else_facts = self.flow_facts.clone();
        self.flow_facts = match (then_result.falls_through, else_result.falls_through) {
            (true, true) => intersect_facts(&then_facts, &else_facts),
            (true, false) => then_facts,
            (false, true) => else_facts,
            (false, false) => baseline,
        };
        let ty = if self.is_deferred(then_result.ty) || self.is_deferred(else_result.ty) {
            self.deferred(DeferredReason::ControlJoin)
        } else if let Some(join) = self.join(then_result.ty, else_result.ty) {
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
                let (left_true, left_false) = self.condition_facts(left)?;
                let (right_true, right_false) = self.condition_facts(right)?;
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
                let (left_true, left_false) = self.condition_facts(left)?;
                let (right_true, right_false) = self.condition_facts(right)?;
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

pub(super) fn collect_expression_uses(parsed: &ParsedFile) -> Vec<ExpressionUse> {
    let mut uses = vec![ExpressionUse::Value; parsed.ast().expressions().len()];
    for (_, node) in parsed.ast().statements().iter() {
        let elements = match node.payload() {
            Statement::Block { elements } => Some((elements.as_slice(), false)),
            Statement::LambdaBody { elements } | Statement::ControlBody { elements } => {
                Some((elements.as_slice(), true))
            }
            _ => None,
        };
        let Some((elements, tail_is_value)) = elements else {
            continue;
        };
        for (index, &statement) in elements.iter().enumerate() {
            let is_value_tail = tail_is_value && index + 1 == elements.len();
            if is_value_tail {
                continue;
            }
            if let Ok(statement) = parsed.ast().statements().get(statement)
                && let Statement::Expression { expression } = statement.payload()
            {
                mark_statement_expression(parsed, *expression, &mut uses);
            }
        }
    }
    uses
}

fn mark_statement_expression(parsed: &ParsedFile, id: ExpressionId, uses: &mut [ExpressionUse]) {
    uses[id.index()] = ExpressionUse::Statement;
    let Ok(node) = parsed.ast().expressions().get(id) else {
        return;
    };
    match node.payload() {
        Expression::Group { expression } => mark_statement_expression(parsed, *expression, uses),
        Expression::If {
            then_branch,
            else_branch,
            ..
        } => {
            mark_control_tail(parsed, *then_branch, uses);
            if let Some(else_branch) = else_branch {
                mark_control_tail(parsed, *else_branch, uses);
            }
        }
        Expression::When { entries, .. } => {
            for entry in entries {
                mark_control_tail(parsed, entry.body, uses);
            }
        }
        _ => {}
    }
}

fn mark_control_tail(parsed: &ParsedFile, statement: StatementId, uses: &mut [ExpressionUse]) {
    let Ok(node) = parsed.ast().statements().get(statement) else {
        return;
    };
    let Statement::ControlBody { elements } = node.payload() else {
        return;
    };
    let Some(last) = elements.last() else {
        return;
    };
    if let Ok(statement) = parsed.ast().statements().get(*last)
        && let Statement::Expression { expression } = statement.payload()
    {
        mark_statement_expression(parsed, *expression, uses);
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
