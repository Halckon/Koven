//! True-arm delivery is not a live owner at the join. Only false retains Q.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::{
    MixedParameterThreshold, ReturnConsumerTarget,
};
use crate::ownership_checking::{
    UnitOwnerConditionalReturnBackedgeV1, UnitOwnerReplaceBreakStateV1,
    UnitOwnerReturnActionV1 as A, UnitOwnerReturnEndpointV1,
};
use crate::parser::BinaryOperator;
pub(super) struct ConditionalReturnInput {
    pub(super) role: LoopBodyCompletion,
    threshold: MixedParameterThreshold,
    target: ReturnConsumerTarget,
    identity: Option<std::sync::Arc<()>>,
    snapshots: Vec<UnitOwnerReplaceBreakStateV1>,
    endpoint: Option<UnitOwnerReturnEndpointV1>,
    true_events: Vec<UnitOwnerTransportEvent>,
    arm_stopped: bool,
    sealed: bool,
}
impl ConditionalReturnInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        expression: ExpressionId,
        control: UnitSymbolId,
        loop_condition: ExpressionId,
        parameters: &BTreeSet<UnitSymbolId>,
        callable: UnitOwnerCallable,
        root: UnitSymbolId,
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
        let target =
            super::super::super::replace_return_consumer::admit(checker, *statement, root)?;
        if target.binding.is_some() || target.replacement.is_some() {
            return None;
        }
        Some(Self {
            role: LoopBodyCompletion::ConditionalReturn {
                control: checker.unit_expression(expression),
                condition: checker.unit_expression(*condition),
                body: UnitStatementId::new(checker.source_unit, *then_branch),
                expression: target.expression,
            },
            threshold: threshold?,
            target,
            identity: None,
            snapshots: Vec::new(),
            endpoint: None,
            true_events: Vec::new(),
            arm_stopped: false,
            sealed: false,
        })
    }
    pub(super) fn condition(&self) -> UnitExpressionId {
        let LoopBodyCompletion::ConditionalReturn { condition, .. } = self.role else {
            unreachable!("conditional Return");
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
            || phase > 5
            || (phase >= 2 && !self.arm_stopped)
            || (phase >= 4 && !self.sealed)
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
    pub(super) fn take(&mut self, id: UnitExpressionId, value: Option<&OwnedValue>) -> bool {
        if id != self.target.payload
            || self.snapshots.len() != 2
            || self.endpoint.is_some()
            || self.arm_stopped
        {
            return false;
        }
        let Some(actual) = value.and_then(super::super::super::replace_consumer::snapshot) else {
            return false;
        };
        if self.snapshots.first() != Some(&actual) {
            return false;
        }
        self.endpoint = Some(UnitOwnerReturnEndpointV1 {
            target: self.target.clone(),
            actions: vec![A::Take {
                expression: id,
                root: actual.symbol,
                owner: actual.owner(),
            }],
            taken: actual,
            cleaned: false,
        });
        true
    }
    pub(super) fn deliver(
        &mut self,
        id: UnitExpressionId,
        payload: UnitExpressionId,
        owner: Option<&UnitOwnerId>,
        state: &ValueState,
    ) -> bool {
        let Some(e) = &mut self.endpoint else {
            return false;
        };
        if id != self.target.expression
            || payload != self.target.payload
            || e.actions.len() != 1
            || e.cleaned
            || !empty_boundary(state)
            || owner != Some(&e.taken.owner)
        {
            return false;
        }
        e.actions.push(A::Return {
            expression: id,
            payload,
            owner: e.owner(),
        });
        true
    }
    pub(super) fn cleanup(&mut self, id: UnitExpressionId, state: &ValueState) -> bool {
        let Some(e) = &mut self.endpoint else {
            return false;
        };
        if id != self.target.expression || e.cleaned || !empty_boundary(state) {
            return false;
        }
        e.cleaned = true;
        e.is_consistent()
    }
    pub(super) fn cleaned(&self, id: UnitExpressionId) -> bool {
        id == self.target.expression && self.endpoint.as_ref().is_some_and(|e| e.is_consistent())
    }
    pub(super) fn finish_arm(&mut self, state: &ValueState) -> bool {
        if self.arm_stopped || !self.cleaned(self.target.expression) || !empty_boundary(state) {
            return false;
        }
        self.arm_stopped = true;
        true
    }
    pub(super) fn seal(&mut self, events: &[UnitOwnerTransportEvent]) -> bool {
        let Some(e) = &self.endpoint else {
            return false;
        };
        if self.sealed
            || !self.arm_stopped
            || self.snapshots.len() != 4
            || events
                != [
                    UnitOwnerTransportEvent::Move {
                        expression: e.payload(),
                        symbol: e.root(),
                        owner: e.owner(),
                    },
                    UnitOwnerTransportEvent::Result {
                        expression: e.payload(),
                        owner: e.owner(),
                    },
                ]
        {
            return false;
        }
        self.true_events = events.to_vec();
        self.sealed = true;
        true
    }
    pub(super) fn complete(&mut self, state: &ValueState) -> bool {
        let Some(i) = self.identity.clone() else {
            return false;
        };
        self.record(5, &i, state)
    }
    pub(super) fn identity_matches(&self, i: &std::sync::Arc<()>) -> bool {
        self.identity
            .as_ref()
            .is_some_and(|actual| std::sync::Arc::ptr_eq(actual, i))
            && self.sealed
            && self.snapshots.len() == 6
    }
    pub(super) fn finish(self, core: HeaderCore) -> Option<UnitOwnerConditionalReturnBackedgeV1> {
        let LoopBodyCompletion::ConditionalReturn {
            control,
            condition,
            body,
            ..
        } = self.role
        else {
            return None;
        };
        Some(UnitOwnerConditionalReturnBackedgeV1 {
            core,
            threshold: self.threshold,
            split: control,
            split_condition: condition,
            return_body: body,
            body_return: self.endpoint?,
            identity: self.identity?,
            snapshots: self.snapshots,
            split_sealed: self.sealed,
            true_events: self.true_events,
            arm_stopped: self.arm_stopped,
        })
    }
}
