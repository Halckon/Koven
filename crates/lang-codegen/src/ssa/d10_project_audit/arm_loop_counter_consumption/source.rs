//! Independent original bounded-counter syntax, typed ABI and dispatch identities.
use super::*;
use lang_frontend::{
    parser::{
        AssignmentOperator, BinaryOperator, Expression, Item, LiteralKind, Statement, VariableKind,
    },
    source::Span,
    type_checking::{BuiltinType, UnitStatementId, UnitTypeKind},
};
pub(super) struct Source {
    pub abi: Vec<(SsaTypeId, ParameterMode)>,
    pub roles: [usize; 3],
    pub conditions: [(UnitExpressionId, Span); 2],
    pub spans: [Span; 5], // initializer, guard, zero, one, decrement
    pub calls: [Vec<Call>; 2],
}
impl Source {
    pub(super) fn collect(
        inputs: &RefineInputs<'_>,
        types: &RefineTypes<'_>,
        wrapper: &UnitLoopCounterBranchNoCarryFact,
    ) -> Option<Self> {
        let fact = wrapper.pair();
        if inputs.parameters.len() != 3
            || inputs.modes.len() != 3
            || !inputs.owned.owns_no_carry_counter_branch(wrapper)
        {
            return None;
        }
        let ast = inputs.parsed.ast();
        let crate::ssa::refine::RefineBody::Block(body) = inputs.body else {
            return None;
        };
        let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
            return None;
        };
        let [initial, statement] = elements.as_slice() else {
            return None;
        };
        if wrapper.initial_statement() != UnitStatementId::new(inputs.source_unit, *initial)
            || fact.arms().iter().any(|a| {
                a.loop_statement() != UnitStatementId::new(inputs.source_unit, *statement)
                    || a.source() != fact.arms()[0].source()
                    || !matches!(a.consumers().len(), 1 | 2)
            })
        {
            return None;
        }
        let Statement::LocalVariable { declaration } =
            ast.statements().get(*initial).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            kind: VariableKind::Var,
            initializer,
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        if wrapper.initializer() != UnitExpressionId::new(inputs.source_unit, *initializer) {
            return None;
        }
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        if wrapper.condition() != UnitExpressionId::new(inputs.source_unit, *condition) {
            return None;
        }
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        let [branch, update] = elements.as_slice() else {
            return None;
        };
        if wrapper.update() != UnitStatementId::new(inputs.source_unit, *update) {
            return None;
        }
        let Statement::Expression {
            expression: assignment,
        } = ast.statements().get(*update).ok()?.payload()
        else {
            return None;
        };
        let Expression::Assignment {
            target,
            operator: AssignmentOperator::Assign,
            value: decrement,
            ..
        } = ast.expressions().get(*assignment).ok()?.payload()
        else {
            return None;
        };
        if wrapper.decrement() != UnitExpressionId::new(inputs.source_unit, *decrement) {
            return None;
        }
        let guard = ast.expressions().get(*condition).ok()?;
        let Expression::Binary {
            left,
            operator: BinaryOperator::Greater,
            right: zero,
            ..
        } = guard.payload()
        else {
            return None;
        };
        let decrement_node = ast.expressions().get(*decrement).ok()?;
        let Expression::Binary {
            left: read,
            operator: BinaryOperator::Subtract,
            right: one,
            ..
        } = decrement_node.payload()
        else {
            return None;
        };
        let reference = |id| {
            let n = ast.expressions().get(id).ok()?;
            if !matches!(n.payload(), Expression::Name) {
                return None;
            }
            inputs
                .references
                .get(&(n.span().start(), n.span().end()))
                .copied()
        };
        if reference(*initializer) != Some(wrapper.rounds())
            || [*left, *read, *target]
                .into_iter()
                .any(|id| reference(id) != Some(wrapper.counter()))
            || wrapper.decrement_operand() != UnitExpressionId::new(inputs.source_unit, *read)
        {
            return None;
        }
        for (id, text) in [(*zero, "0"), (*one, "1")] {
            let node = ast.expressions().get(id).ok()?;
            if !matches!(node.payload(), Expression::Literal(LiteralKind::Integer(_)))
                || inputs.sources.slice(node.span()).ok() != Some(text)
            {
                return None;
            }
        }
        let Statement::Expression { expression } = ast.statements().get(*branch).ok()?.payload()
        else {
            return None;
        };
        if fact.branch() != UnitExpressionId::new(inputs.source_unit, *expression) {
            return None;
        }
        let Expression::If {
            condition: choice,
            then_branch,
            else_branch: Some(other),
            ..
        } = ast.expressions().get(*expression).ok()?.payload()
        else {
            return None;
        };
        if [*then_branch, *other] != fact.arms().clone().map(|a| a.body().statement()) {
            return None;
        }
        let symbols = [
            fact.arms()[0].source(),
            wrapper.rounds(),
            reference(*choice)?,
        ];
        let roles = symbols
            .map(|symbol| inputs.parameters.iter().position(|s| *s == symbol))
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        if roles.iter().copied().collect::<BTreeSet<_>>() != BTreeSet::from([0, 1, 2]) {
            return None;
        }
        let kinds = [BuiltinType::String, BuiltinType::Int, BuiltinType::Boolean];
        let mut abi = Vec::new();
        for (i, symbol) in inputs.parameters.iter().enumerate() {
            let ty = inputs.typed.symbol_type(*symbol)?;
            let role = roles.iter().position(|r| *r == i)?;
            if inputs.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(kinds[role]))
                || (role == 0 && inputs.modes[i] != ParameterMode::Borrow)
                || !matches!(
                    inputs.modes[i],
                    ParameterMode::Borrow | ParameterMode::Value
                )
            {
                return None;
            }
            abi.push((*types.type_ids.get(&ty)?, inputs.modes[i]));
        }
        let counter_ty = inputs.typed.symbol_type(wrapper.counter())?;
        if inputs.typed.types().get(counter_ty) != Some(&UnitTypeKind::Builtin(BuiltinType::Int)) {
            return None;
        }
        let call = |expression| {
            let site = inputs
                .source_plan
                .call_site(inputs.source_token, expression)?;
            if !site.delegation().is_empty() {
                return None;
            }
            let key = site.dispatch_key()?;
            Some((key.clone(), *inputs.function_ids.get(key)?))
        };
        let calls = fact
            .arms()
            .iter()
            .map(|arm| {
                std::iter::once(arm.initializer())
                    .chain(arm.consumers().iter().map(|(_, c, _)| *c))
                    .map(call)
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            abi,
            roles: roles.try_into().ok()?,
            conditions: [
                (wrapper.condition(), guard.span()),
                (
                    UnitExpressionId::new(inputs.source_unit, *choice),
                    ast.expressions().get(*choice).ok()?.span(),
                ),
            ],
            spans: [
                ast.expressions().get(*initializer).ok()?.span(),
                guard.span(),
                ast.expressions().get(*zero).ok()?.span(),
                ast.expressions().get(*one).ok()?.span(),
                decrement_node.span(),
            ],
            calls: calls.try_into().ok()?,
        })
    }
}
