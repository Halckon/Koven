//! A scalar prefix capability is sealed by actual independent arm states, not syntax alone.
use super::{Checker, Flows, State};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{
        Expression, FunctionBody, FunctionForm, IntegerLiteralKind, Item, LiteralKind, Statement,
    },
    type_checking::{BuiltinType, ParameterMode, UnitCallTarget, UnitTypeKind},
};
#[derive(Clone)]
pub(super) struct Plan {
    pub(super) prefix: ExpressionId,
    pub(super) condition: ExpressionId,
    pub(super) calls: [ExpressionId; 2],
    pub(super) target: StatementId,
}
pub(super) struct Completion {
    plan: Plan,
    state: State,
}
#[derive(Default)]
pub(super) struct Evidence {
    completed: Vec<Completion>,
    pub(super) entered: Vec<Plan>,
}
impl Checker<'_> {
    fn prefix_builtin(&self, id: ExpressionId, kind: BuiltinType) -> bool {
        self.typed
            .expression_type(self.unit_expression(id))
            .and_then(|ty| self.typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(kind))
    }
    // A signature is not an effect proof. Only a known, local, empty body is
    // admitted; even scalar println or a hidden String allocation remains out.
    fn prefix_empty_callee(&self, target: UnitCallTarget) -> bool {
        let UnitCallTarget::Declaration(id) = target else {
            return false;
        };
        let Some(declaration) = self
            .names
            .index()
            .declarations()
            .iter()
            .find(|d| d.id() == id)
        else {
            return false;
        };
        if declaration.source_unit() != self.source_unit {
            return false;
        }
        let Ok(item) = self.parsed.ast().items().get(declaration.root()) else {
            return false;
        };
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
        let call = self.typed.call(self.unit_expression(*expression))?;
        (call.receiver().is_none()
            && !call.aborts()
            && call.borrow_return().is_none()
            && self.prefix_empty_callee(call.target())
            && self.typed.types().get(call.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            && call.arguments().len() == 1
            && call.arguments().iter().all(|a| {
                a.mode() == ParameterMode::Value
                    && !a.crosses_thread()
                    && self.typed.types().get(a.parameter_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            }))
        .then_some(*expression)
    }
    pub(super) fn prefix_plan(&self, control: ExpressionId) -> Option<Plan> {
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
                .source_units()
                .iter()
                .find(|u| u.source_unit() == symbol.source_unit())?
                .resolution()
                .symbols()
                .get(symbol.symbol().index())
                .is_some_and(|s| s.kind() == crate::name_resolution::SymbolKind::ValueParameter)
        {
            return None;
        }
        let calls = [self.prefix_call(*then_branch)?, self.prefix_call(*other)?];
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
            if let [first, target, ..] = elements.as_slice()
                && matches!(self.parsed.ast().statements().get(*first).ok()?.payload(), Statement::Expression { expression } if *expression==control)
                && matches!(
                    self.parsed.ast().statements().get(*target).ok()?.payload(),
                    Statement::While { .. }
                )
            {
                return Some(Plan {
                    prefix: control,
                    condition: *condition,
                    calls,
                    target: *target,
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
            self.no_carry_prefix
                .completed
                .push(Completion { plan, state });
        }
    }
    pub(super) fn prefix_enter(&mut self, target: StatementId, entry: &State) {
        let mut matches = self
            .no_carry_prefix
            .completed
            .iter()
            .filter(|c| c.plan.target == target && c.state == *entry);
        if let Some(completed) = matches.next()
            && matches.next().is_none()
        {
            self.no_carry_prefix.entered.push(completed.plan.clone());
        }
    }
}
