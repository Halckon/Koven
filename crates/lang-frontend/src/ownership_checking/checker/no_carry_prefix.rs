//! A scalar prefix capability is sealed by actual independent arm states, not syntax alone.
use super::{Checker, Flows, State};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{
        Expression, FunctionBody, FunctionForm, IntegerLiteralKind, Item, LiteralKind, Statement,
    },
    type_checking::{BuiltinType, CallableTarget, ParameterMode, TypeKind},
};
#[derive(Clone)]
pub(super) struct Plan {
    pub(super) prefix: ExpressionId,
    pub(super) target: StatementId,
    pub(super) initial: Option<StatementId>,
}
pub(super) struct Completion {
    plan: Plan,
    state: State,
    valid: bool,
    init_entered: bool,
    init_completed: bool,
}
#[derive(Default)]
pub(super) struct Evidence {
    completed: Vec<Completion>,
}
impl Checker<'_> {
    fn prefix_builtin(&self, id: ExpressionId, kind: BuiltinType) -> bool {
        self.typed
            .expression_type(id)
            .and_then(|ty| self.typed.types().get(ty))
            == Some(&TypeKind::Builtin(kind))
    }
    // A signature is not an effect proof. Only a known, local, empty body is
    // admitted; even scalar println or a hidden String allocation remains out.
    fn prefix_empty_callee(&self, target: CallableTarget) -> bool {
        let CallableTarget::Source(symbol) = target else {
            return false;
        };
        let Some((_, item))=self.parsed.ast().items().iter().find(|(_,node)|matches!(node.payload(),Item::Function{name,..} if self.marker_symbol(*name)==Some(symbol))) else {return false};
        let Item::Function { form, .. } = item.payload() else {
            return false;
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return false,
        };
        self.parsed.ast().statements().get(body).is_ok_and(
            |node| matches!(node.payload(),Statement::Block { elements } if elements.is_empty()),
        )
    }
    fn prefix_call(&self, body: StatementId) -> Option<ExpressionId> {
        let Statement::ControlBody { elements } =
            self.parsed.ast().statements().get(body).ok()?.payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression } = self
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .ok()?
            .payload()
        else {
            return None;
        };
        let Expression::Call {
            callee, arguments, ..
        } = self
            .parsed
            .ast()
            .expressions()
            .get(*expression)
            .ok()?
            .payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        if !matches!(
            self.parsed.ast().expressions().get(*callee).ok()?.payload(),
            Expression::Name
        ) || !matches!(
            self.parsed
                .ast()
                .expressions()
                .get(argument.value)
                .ok()?
                .payload(),
            Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
        ) || !self.prefix_builtin(argument.value, BuiltinType::Int)
        {
            return None;
        }
        let call = self.typed.call(*expression)?;
        (call.receiver().is_none()
            && !call.aborts()
            && call.borrow_return().is_none()
            && self.prefix_empty_callee(call.target())
            && self.typed.types().get(call.return_type())
                == Some(&TypeKind::Builtin(BuiltinType::Unit))
            && call.arguments().len() == 1
            && call.arguments().iter().all(|a| {
                a.mode() == ParameterMode::Value
                    && !a.crosses_thread()
                    && self.typed.types().get(a.parameter_type())
                        == Some(&TypeKind::Builtin(BuiltinType::Int))
            }))
        .then_some(*expression)
    }
    pub(super) fn scalar_prefix_parts(
        &self,
        control: ExpressionId,
    ) -> Option<(ExpressionId, [ExpressionId; 2])> {
        let Expression::If {
            condition,
            then_branch,
            else_branch: Some(other),
            ..
        } = self.parsed.ast().expressions().get(control).ok()?.payload()
        else {
            return None;
        };
        let node = self.parsed.ast().expressions().get(*condition).ok()?;
        let symbol = self.reference_symbol(node.span())?;
        if !matches!(node.payload(), Expression::Name)
            || !self.prefix_builtin(*condition, BuiltinType::Boolean)
            || !self
                .names
                .symbols()
                .get(symbol.index())
                .is_some_and(|s| s.kind() == crate::name_resolution::SymbolKind::ValueParameter)
        {
            return None;
        }
        Some((
            *condition,
            [self.prefix_call(*then_branch)?, self.prefix_call(*other)?],
        ))
    }
    pub(super) fn prefix_plan(&self, control: ExpressionId) -> Option<Plan> {
        let _ = self.scalar_prefix_parts(control)?;
        for (_, node) in self.parsed.ast().items().iter() {
            let Item::Function { form, .. } = node.payload() else {
                continue;
            };
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => continue,
            };
            let Statement::Block { elements } =
                self.parsed.ast().statements().get(body).ok()?.payload()
            else {
                continue;
            };
            let (first, initial, target) = match elements.as_slice() {
                [first, initial, target, ..]
                    if matches!(
                        self.parsed.ast().statements().get(*initial).ok()?.payload(),
                        Statement::LocalVariable { .. }
                    ) =>
                {
                    (*first, Some(*initial), *target)
                }
                [first, target, ..] => (*first, None, *target),
                _ => continue,
            };
            if matches!(self.parsed.ast().statements().get(first).ok()?.payload(), Statement::Expression { expression } if *expression==control)
                && matches!(
                    self.parsed.ast().statements().get(target).ok()?.payload(),
                    Statement::While { .. }
                )
            {
                return Some(Plan {
                    prefix: control,
                    target,
                    initial,
                });
            }
        }
        None
    }
    pub(super) fn prefix_unchanged(&self, entry: &State, flows: &Flows) -> bool {
        flows.breaks.is_none()
            && flows.continues.is_none()
            && flows
                .next
                .as_ref()
                .is_some_and(|s| self.resource_snapshot(s) == *entry)
    }
    pub(super) fn prefix_complete(&mut self, plan: Plan, state: State) {
        if self.diagnostics.is_empty() && self.simple_resources(&state) {
            self.no_carry_prefix.completed.push(Completion {
                plan,
                state,
                valid: true,
                init_entered: false,
                init_completed: false,
            });
        }
    }
    pub(super) fn prefix_init_before(&mut self, id: StatementId, state: &State) {
        let actual = self.resource_snapshot(state);
        for c in &mut self.no_carry_prefix.completed {
            if c.plan.initial == Some(id) {
                c.valid &= !c.init_entered && !c.init_completed && c.state == actual;
                c.init_entered = c.valid;
            }
        }
    }
    pub(super) fn prefix_init_after(&mut self, id: StatementId, flows: &Flows) {
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        for c in &mut self.no_carry_prefix.completed {
            if c.plan.initial == Some(id) {
                c.valid &= c.init_entered
                    && !c.init_completed
                    && actual.as_ref() == Some(&c.state)
                    && flows.breaks.is_none()
                    && flows.continues.is_none();
                c.init_completed = c.valid;
            }
        }
    }
    pub(super) fn prefix_ready(
        &self,
        target: StatementId,
        prefix: ExpressionId,
        initial: StatementId,
        entry: &State,
    ) -> bool {
        self.no_carry_prefix
            .completed
            .iter()
            .filter(|c| {
                c.valid
                    && c.plan.target == target
                    && c.plan.prefix == prefix
                    && c.plan.initial == Some(initial)
                    && c.init_completed
                    && c.state == *entry
            })
            .count()
            == 1
    }
}
