//! Live-Q split evidence collected at real arm and jump completion hooks.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::MixedParameterThreshold;
use crate::ownership_checking::{UnitOwnerMixedBackedgeV1, UnitOwnerReplaceBreakStateV1};
use crate::parser::{BinaryOperator, LiteralKind};

type Snapshot = UnitOwnerReplaceBreakStateV1;
#[derive(Default)]
struct Arm {
    entry: Option<Snapshot>,
    jump: Option<Snapshot>,
    role: Option<LoopBodyCompletion>,
    completion: Option<Snapshot>,
}
pub(super) struct MixedInput {
    parameter_count: usize,
    threshold: Option<MixedParameterThreshold>,
    pub(super) role: LoopBodyCompletion,
    identity: Option<std::sync::Arc<()>>,
    entry: Option<Snapshot>,
    arms: [Arm; 2],
}
impl MixedInput {
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
            else_branch: Some(other),
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
        let threshold = match (parameters.len(), right_node.payload()) {
            (1, Expression::Literal(LiteralKind::Integer(kind))) => {
                if crate::type_checking::integer_literal_magnitude(
                    checker.sources.slice(right_node.span()).ok()?,
                    *kind,
                ) != Some(2)
                {
                    return None;
                }
                None
            }
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
        let arm = |body: StatementId, is_continue: bool| {
            let Statement::ControlBody { elements } = ast.statements().get(body).ok()?.payload()
            else {
                return None;
            };
            let [statement] = elements.as_slice() else {
                return None;
            };
            let Statement::Expression { expression } =
                ast.statements().get(*statement).ok()?.payload()
            else {
                return None;
            };
            if !exact_type(checker, *expression, BuiltinType::Nothing) {
                return None;
            }
            let node = ast.expressions().get(*expression).ok()?;
            let jump = checker.unit_expression(*expression);
            let exit = match (node.payload(), is_continue) {
                (Expression::Continue { .. }, true) => LoopBranchExit::Continue(jump),
                (Expression::Break { .. }, false) => LoopBranchExit::Break(jump),
                _ => return None,
            };
            Some(LoopBranchCompletion {
                body: UnitStatementId::new(checker.source_unit, body),
                exit,
            })
        };
        Some(Self {
            parameter_count: parameters.len(),
            threshold,
            role: LoopBodyCompletion::Conditional {
                control: checker.unit_expression(expression),
                condition: checker.unit_expression(*condition),
                branches: [arm(*then_branch, true)?, arm(*other, false)?],
            },
            identity: None,
            entry: None,
            arms: [Arm::default(), Arm::default()],
        })
    }
    fn snapshot(state: &ValueState) -> Option<Snapshot> {
        let [value] = boundary_values(state)? else {
            return None;
        };
        super::super::super::replace_consumer::snapshot(value)
    }
    pub(super) fn enter(&mut self, identity: &std::sync::Arc<()>, state: &ValueState) -> bool {
        if self.entry.is_some() {
            return false;
        }
        let Some(snapshot) = Self::snapshot(state) else {
            return false;
        };
        self.entry = Some(snapshot);
        self.identity = Some(identity.clone());
        true
    }
    pub(super) fn arm_entry(
        &mut self,
        branch: usize,
        control: UnitExpressionId,
        body: UnitStatementId,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        let LoopBodyCompletion::Conditional {
            control: expected,
            branches,
            ..
        } = self.role
        else {
            return false;
        };
        if control != expected
            || branches.get(branch).is_none_or(|a| a.body != body)
            || self
                .identity
                .as_ref()
                .is_none_or(|i| !std::sync::Arc::ptr_eq(i, identity))
        {
            return false;
        }
        let Some(snapshot) = Self::snapshot(state) else {
            return false;
        };
        let Some(arm) = self.arms.get_mut(branch) else {
            return false;
        };
        if self.entry.as_ref() != Some(&snapshot) || arm.entry.is_some() {
            return false;
        }
        arm.entry = Some(snapshot);
        true
    }
    pub(super) fn jump(
        &mut self,
        branch: usize,
        completion: LoopBodyCompletion,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        let LoopBodyCompletion::Conditional { branches, .. } = self.role else {
            return false;
        };
        if branches.get(branch).and_then(|a| a.completion()) != Some(completion)
            || self
                .identity
                .as_ref()
                .is_none_or(|i| !std::sync::Arc::ptr_eq(i, identity))
        {
            return false;
        }
        let Some(snapshot) = Self::snapshot(state) else {
            return false;
        };
        let Some(arm) = self.arms.get_mut(branch) else {
            return false;
        };
        if arm.entry.as_ref() != Some(&snapshot) || arm.jump.is_some() {
            return false;
        }
        arm.jump = Some(snapshot);
        arm.role = Some(completion);
        true
    }
    pub(super) fn arm_complete(
        &mut self,
        branch: usize,
        continues: bool,
        state: &ValueState,
    ) -> bool {
        if continues {
            return false;
        }
        let Some(snapshot) = Self::snapshot(state) else {
            return false;
        };
        let Some(arm) = self.arms.get_mut(branch) else {
            return false;
        };
        if arm.jump.as_ref() != Some(&snapshot) || arm.completion.is_some() {
            return false;
        }
        arm.completion = Some(snapshot);
        true
    }
    pub(super) fn complete(&self) -> bool {
        self.entry.is_some()
            && self.identity.is_some()
            && self.arms.iter().all(|a| {
                a.entry == self.entry && a.jump == self.entry && a.completion == self.entry
            })
    }
    pub(super) fn completed_snapshot(&self) -> Option<&Snapshot> {
        self.complete().then_some(self.entry.as_ref()).flatten()
    }
    pub(super) fn identity_matches(&self, identity: &std::sync::Arc<()>) -> bool {
        self.identity
            .as_ref()
            .is_some_and(|i| std::sync::Arc::ptr_eq(i, identity))
    }
    pub(super) fn condition(&self) -> UnitExpressionId {
        let LoopBodyCompletion::Conditional { condition, .. } = self.role else {
            unreachable!("mixed role");
        };
        condition
    }
    pub(super) fn finish(self, core: HeaderCore) -> Option<UnitOwnerMixedBackedgeV1> {
        if !self.complete() {
            return None;
        }
        let LoopBodyCompletion::Conditional {
            control,
            condition,
            branches,
        } = self.role
        else {
            return None;
        };
        let [a, b] = self.arms;
        Some(UnitOwnerMixedBackedgeV1 {
            parameter_count: self.parameter_count,
            threshold: self.threshold,
            core,
            split: control,
            split_condition: condition,
            bodies: [branches[0].body, branches[1].body],
            jumps: [branches[0].jump()?, branches[1].jump()?],
            identity: self.identity?,
            split_state: self.entry?,
            entries: [a.entry?, b.entry?],
            transferred: [a.jump?, b.jump?],
            roles: [a.role?, b.role?],
            completed: [a.completion?, b.completion?],
            stopped: true,
        })
    }
}
