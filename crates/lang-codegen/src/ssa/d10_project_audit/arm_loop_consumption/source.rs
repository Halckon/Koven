//! Original syntax and typed ABI, independent of emitted receipt/SSA.
use super::*;
use lang_frontend::{
    parser::{Expression, Statement},
    source::Span,
    type_checking::{BuiltinType, UnitTypeKind},
};
pub(super) struct Source {
    pub abi: [(SsaTypeId, ParameterMode); 3],
    pub roles: [usize; 3],
    pub conditions: [(UnitExpressionId, Span); 2],
    pub calls: [Vec<Call>; 2],
}
impl Source {
    pub(super) fn collect(
        inputs: &RefineInputs<'_>,
        types: &RefineTypes<'_>,
        fact: &UnitLoopBranchNoCarryFact,
    ) -> Option<Self> {
        if inputs.parameters.len() != 3
            || inputs.modes.len() != 3
            || inputs
                .modes
                .iter()
                .any(|m| !matches!(m, ParameterMode::Borrow | ParameterMode::Value))
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
        let [statement] = elements.as_slice() else {
            return None;
        };
        if fact.arms().iter().any(|a| {
            a.loop_statement().source_unit() != inputs.source_unit
                || a.loop_statement().statement() != *statement
                || a.source() != fact.arms()[0].source()
                || !matches!(a.consumers().len(), 1 | 2)
        }) {
            return None;
        }
        let Statement::While {
            condition: outer,
            body,
            ..
        } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        let [branch] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression } = ast.statements().get(*branch).ok()?.payload()
        else {
            return None;
        };
        if UnitExpressionId::new(inputs.source_unit, *expression) != fact.branch() {
            return None;
        }
        let Expression::If {
            condition,
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
        let source = inputs
            .parameters
            .iter()
            .position(|s| *s == fact.arms()[0].source())?;
        if inputs.modes[source] != ParameterMode::Borrow {
            return None;
        }
        let mut roles = vec![source];
        let mut conditions = Vec::new();
        for e in [outer, condition] {
            let n = ast.expressions().get(*e).ok()?;
            if !matches!(n.payload(), Expression::Name) {
                return None;
            }
            let symbol = inputs.references.get(&(n.span().start(), n.span().end()))?;
            roles.push(inputs.parameters.iter().position(|p| p == symbol)?);
            conditions.push((UnitExpressionId::new(inputs.source_unit, *e), n.span()));
        }
        if roles.iter().copied().collect::<BTreeSet<_>>().len() != 3 {
            return None;
        }
        let abi = inputs
            .parameters
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let t = inputs.typed.symbol_type(*s)?;
                let expected = if i == source {
                    BuiltinType::String
                } else {
                    BuiltinType::Boolean
                };
                if inputs.typed.types().get(t) != Some(&UnitTypeKind::Builtin(expected)) {
                    return None;
                }
                Some((*types.type_ids.get(&t)?, inputs.modes[i]))
            })
            .collect::<Option<Vec<_>>>()?;
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
        let mut calls = Vec::new();
        for arm in fact.arms() {
            calls.push(
                std::iter::once(arm.initializer())
                    .chain(arm.consumers().iter().map(|(_, c, _)| *c))
                    .map(call)
                    .collect::<Option<Vec<_>>>()?,
            );
        }
        Some(Self {
            abi: abi.try_into().ok()?,
            roles: roles.try_into().ok()?,
            conditions: conditions.try_into().ok()?,
            calls: calls.try_into().ok()?,
        })
    }
}
