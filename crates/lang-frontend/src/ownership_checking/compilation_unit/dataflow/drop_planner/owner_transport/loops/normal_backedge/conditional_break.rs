//! Actual true Break and implicit false continuation, kept distinct from Mixed.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::MixedParameterThreshold;
use crate::ownership_checking::{
    UnitOwnerConditionalBreakBackedgeV1, UnitOwnerReplaceBreakStateV1,
};
use crate::parser::BinaryOperator;

pub(super) struct ConditionalBreakInput {
    pub(super) role: LoopBodyCompletion,
    threshold: MixedParameterThreshold,
    identity: Option<std::sync::Arc<()>>,
    snapshots: Vec<UnitOwnerReplaceBreakStateV1>,
    sealed: bool,
    jump_role: Option<LoopBodyCompletion>,
}
impl ConditionalBreakInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        expression: ExpressionId,
        control: UnitSymbolId,
        loop_condition: ExpressionId,
        parameters: &BTreeSet<UnitSymbolId>,
        callable: UnitOwnerCallable,
    ) -> Option<Self> {
        let ast = checker.parsed.ast();
        let Expression::If {
            condition,
            then_branch,
            else_branch: None,
            ..
        } = ast.expressions().get(expression).ok()?.payload()
        else {
            return None;
        };
        if !exact_type(checker, *condition, BuiltinType::Boolean)
            || checker.ordinary_condition_outcomes(*condition) != (true, true)
        {
            return None;
        }
        let Expression::Binary {
            left,
            operator: BinaryOperator::Less,
            right,
            ..
        } = ast.expressions().get(*condition).ok()?.payload()
        else {
            return None;
        };
        let left_node = ast.expressions().get(*left).ok()?;
        let right_node = ast.expressions().get(*right).ok()?;
        if !matches!(left_node.payload(), Expression::Name)
            || checker.reference_symbol(left_node.span()) != Some(control)
            || !exact_type(checker, *left, BuiltinType::Int)
            || !exact_type(checker, *right, BuiltinType::Int)
        {
            return None;
        }
        if parameters.len() != 2 {
            return None;
        }
        let threshold = match (parameters.len(), right_node.payload()) {
            (2, Expression::Name) => {
                let Expression::Binary {
                    operator: BinaryOperator::Less,
                    right: limit,
                    ..
                } = ast.expressions().get(loop_condition).ok()?.payload()
                else {
                    return None;
                };
                let limit_node = ast.expressions().get(*limit).ok()?;
                if !matches!(limit_node.payload(), Expression::Name) {
                    return None;
                }
                let symbols = [
                    checker.reference_symbol(limit_node.span())?,
                    checker.reference_symbol(right_node.span())?,
                ];
                if symbols[0] == symbols[1]
                    || symbols.contains(&control)
                    || !symbols.iter().all(|id| {
                        parameters.contains(id)
                            && checker
                                .typed
                                .symbol_type(*id)
                                .and_then(|ty| checker.typed.types().get(ty))
                                == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    })
                {
                    return None;
                }
                let declared = parameters.iter().copied().collect::<Vec<_>>();
                Some(MixedParameterThreshold {
                    callable,
                    parameters: [declared[0], declared[1]],
                    symbols,
                    operands: [
                        checker.unit_expression(*limit),
                        checker.unit_expression(*right),
                    ],
                    condition: checker.unit_expression(loop_condition),
                })
            }
            _ => return None,
        };

        let Statement::ControlBody { elements } =
            ast.statements().get(*then_branch).ok()?.payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression: jump } =
            ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        if !matches!(
            ast.expressions().get(*jump).ok()?.payload(),
            Expression::Break { .. }
        ) || !exact_type(checker, *jump, BuiltinType::Nothing)
        {
            return None;
        }
        Some(Self {
            role: LoopBodyCompletion::ConditionalBreak {
                control: checker.unit_expression(expression),
                condition: checker.unit_expression(*condition),
                body: UnitStatementId::new(checker.source_unit, *then_branch),
                jump: checker.unit_expression(*jump),
            },
            threshold: threshold?,
            identity: None,
            snapshots: Vec::new(),
            sealed: false,
            jump_role: None,
        })
    }
    pub(super) fn condition(&self) -> UnitExpressionId {
        let LoopBodyCompletion::ConditionalBreak { condition, .. } = self.role else {
            unreachable!("conditional Break role");
        };
        condition
    }
    pub(super) fn record(
        &mut self,
        phase: usize,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        if self.snapshots.len() != phase
            || phase > 7
            || (phase >= 6 && !self.sealed)
            || self
                .identity
                .as_ref()
                .is_some_and(|i| !std::sync::Arc::ptr_eq(i, identity))
        {
            return false;
        }
        let Some([value]) = boundary_values(state) else {
            return false;
        };
        let Some(snapshot) = super::super::super::replace_consumer::snapshot(value) else {
            return false;
        };
        if self.snapshots.first().is_some_and(|s| *s != snapshot) {
            return false;
        }
        if phase == 0 {
            self.identity = Some(identity.clone());
        }
        self.snapshots.push(snapshot);
        true
    }
    pub(super) fn jump(
        &mut self,
        completion: LoopBodyCompletion,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        let LoopBodyCompletion::ConditionalBreak { jump, .. } = self.role else {
            return false;
        };
        if completion != LoopBodyCompletion::Break(jump)
            || self.jump_role.is_some()
            || !self.record(2, identity, state)
        {
            return false;
        }
        self.jump_role = Some(completion);
        true
    }
    pub(super) fn seal(&mut self) -> bool {
        if self.sealed || self.snapshots.len() != 6 {
            return false;
        }
        self.sealed = true;
        true
    }
    pub(super) fn complete(&mut self, state: &ValueState) -> bool {
        let Some(identity) = self.identity.clone() else {
            return false;
        };
        self.record(7, &identity, state)
    }
    pub(super) fn identity_matches(&self, identity: &std::sync::Arc<()>) -> bool {
        self.identity
            .as_ref()
            .is_some_and(|i| std::sync::Arc::ptr_eq(i, identity))
            && self.sealed
            && self.snapshots.len() == 8
    }
    pub(super) fn finish(self, core: HeaderCore) -> Option<UnitOwnerConditionalBreakBackedgeV1> {
        let LoopBodyCompletion::ConditionalBreak {
            control,
            condition,
            body,
            jump,
        } = self.role
        else {
            return None;
        };
        Some(UnitOwnerConditionalBreakBackedgeV1 {
            core,
            threshold: self.threshold,
            split: control,
            split_condition: condition,
            break_body: body,
            break_jump: jump,
            identity: self.identity?,
            snapshots: self.snapshots,
            split_sealed: self.sealed,
            jump_role: self.jump_role?,
        })
    }
}
