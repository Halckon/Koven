//! Narrow Unit-only capability: one outer result lease, a nonempty invariant synchronous consumer sequence.
use lang_frontend::{
    ast::StatementId,
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        BorrowBindingFact, CompilationUnitOwnership, UnitDropPoint, UnitLoanTarget,
    },
    parser::{
        Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
        StringPart, VariableKind,
    },
    type_checking::{
        BorrowReturnOrigin, BuiltinType, CompilationUnitTypes, ExpressionCategory, ParameterMode,
        UnitCallTarget, UnitExpressionId, UnitTypeKind,
    },
};

type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;

pub(in crate::ssa) fn admitted(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    fact: &Binding,
) -> Option<StatementId> {
    if super::ordinary_local::policy(file, typed, owned, fact)
        != Some(super::ordinary_local::Policy::Scoped)
    {
        return None;
    }
    let source = fact.source_loan()?;
    let UnitLoanTarget::Place(root) = fact.origin() else {
        return None;
    };
    if !root.is_root()
        || !matches!(
            typed
                .symbol_type(fact.binding())
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        )
    {
        return None;
    }
    let source_call = file
        .ast()
        .expressions()
        .get(source.call().expression())
        .ok()?;
    let Expression::Call {
        callee, arguments, ..
    } = source_call.payload()
    else {
        return None;
    };
    let descriptor = typed.call(source.call())?;
    let [mapping] = descriptor.arguments() else {
        return None;
    };
    if !file
        .ast()
        .expressions()
        .get(*callee)
        .is_ok_and(|node| matches!(node.payload(), Expression::Name))
        || !matches!(descriptor.target(), UnitCallTarget::Declaration(_))
        || descriptor.receiver().is_some()
        || descriptor.aborts()
        || descriptor.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
        || mapping.argument_index() != 0
        || mapping.parameter_index() != 0
        || mapping.mode() != ParameterMode::Borrow
        || Some(mapping.parameter_type()) != typed.symbol_type(root.root())
        || mapping.crosses_thread()
    {
        return None;
    }
    let [argument] = arguments.as_slice() else {
        return None;
    };
    if source.argument().expression() != argument.value
        || !name(file, names, source.argument(), root.root())
    {
        return None;
    }
    let ends = owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|end| end.binding() == fact.binding())
        .collect::<Vec<_>>();
    let scopes = ends
        .iter()
        .filter_map(|end| match end.point() {
            UnitDropPoint::AfterStatement(scope) => Some(scope),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [scope] = scopes.as_slice() else {
        return None;
    };
    let scope = *scope;
    if scope.source_unit() != fact.binding().source_unit() {
        return None;
    }
    let Statement::Block { elements } = file
        .ast()
        .statements()
        .get(scope.statement())
        .ok()?
        .payload()
    else {
        return None;
    };
    let (binding, while_id, local) = match elements.as_slice() {
        [binding, while_id] => (binding, while_id, false),
        [owner, binding, while_id]
            if local_literal_owner(file, names, typed, *owner, scope.statement(), root.root()) =>
        {
            (binding, while_id, true)
        }
        _ => return None,
    };
    if super::local_if::binding_marker(file, *binding) != Some(fact.marker_span()) {
        return None;
    }
    let Statement::While {
        condition, body, ..
    } = file.ast().statements().get(*while_id).ok()?.payload()
    else {
        return None;
    };
    // The newly admitted local-owner shape has only a dynamic, straight normal body.
    // Existing parameter-source jump/constant support is unchanged.
    if local
        && (!matches!(
            file.ast().expressions().get(*condition).ok()?.payload(),
            Expression::Name
        ) || typed.constants().is_some_and(|constants| {
            constants.uses().iter().any(|usage| {
                usage.expression()
                    == UnitExpressionId::new(fact.binding().source_unit(), *condition)
            })
        }) || tail_jump(file, *while_id).is_some()
            || conditional_jump(file, *while_id).is_some()
            || tail_return(file, *while_id).is_some())
    {
        return None;
    }
    let returned = tail_return(file, *while_id);
    if let Some(returned) = returned {
        if ends.len() != 2
            || ends
                .iter()
                .filter(|end| {
                    end.point()
                        == UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                            fact.binding().source_unit(),
                            returned,
                        ))
                })
                .count()
                != 1
        {
            return None;
        }
    } else if ends.len() != 1 {
        return None;
    }
    let unit = |expression| UnitExpressionId::new(fact.binding().source_unit(), expression);
    if !matches!(
        file.ast().expressions().get(*condition).ok()?.payload(),
        Expression::Name | Expression::Literal(_)
    ) || !matches!(
        typed
            .expression_type(unit(*condition))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
    ) {
        return None;
    }
    let Statement::Block { elements } = file.ast().statements().get(*body).ok()?.payload() else {
        return None;
    };
    if elements.is_empty() {
        return None;
    }
    let conditional = conditional_jump(file, *while_id);
    if let Some(tail) = conditional
        && (!matches!(
            file.ast().expressions().get(tail.condition).ok()?.payload(),
            Expression::Name
        ) || !matches!(
            typed
                .expression_type(unit(tail.condition))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
        ))
    {
        return None;
    }
    let consumers =
        if tail_jump(file, *while_id).is_some() || conditional.is_some() || returned.is_some() {
            &elements[..elements.len() - 1]
        } else {
            elements.as_slice()
        };
    if consumers.is_empty() {
        return None;
    }
    for consumer in consumers {
        let Statement::Expression { expression } =
            file.ast().statements().get(*consumer).ok()?.payload()
        else {
            return None;
        };
        let Expression::Call {
            callee, arguments, ..
        } = file.ast().expressions().get(*expression).ok()?.payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        if !name(file, names, unit(argument.value), fact.binding()) {
            return None;
        }
        let call = typed.call(unit(*expression))?;
        let [mapping] = call.arguments() else {
            return None;
        };
        if !file
            .ast()
            .expressions()
            .get(*callee)
            .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || call.receiver().is_some()
            || call.borrow_return().is_some()
            || call.aborts()
            || mapping.argument_index() != 0
            || mapping.parameter_index() != 0
            || mapping.mode() != ParameterMode::Borrow
            || Some(mapping.parameter_type()) != typed.symbol_type(fact.binding())
            || mapping.crosses_thread()
            || !matches!(
                typed
                    .expression_type(unit(*expression))
                    .and_then(|ty| typed.types().get(ty)),
                Some(UnitTypeKind::Builtin(BuiltinType::Unit))
            )
        {
            return None;
        }
    }
    Some(*while_id)
}

/// Execution admission uses original typed/name facts, never optional observations.
fn local_literal_owner(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    statement: StatementId,
    scope: StatementId,
    root: UnitSymbolId,
) -> bool {
    let check = || -> Option<()> {
        let Statement::LocalVariable { declaration } =
            file.ast().statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(marker),
            initializer,
            ..
        } = file.ast().items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        let symbol = names
            .names()
            .source_units()
            .iter()
            .find(|source| source.source_unit() == root.source_unit())?
            .resolution()
            .symbols()
            .get(root.symbol().index())?;
        let Expression::String { parts } =
            file.ast().expressions().get(*initializer).ok()?.payload()
        else {
            return None;
        };
        let expression = UnitExpressionId::new(root.source_unit(), *initializer);
        if symbol.span() != *marker
            || !parts.iter().all(|part| matches!(part, StringPart::Text(_)))
            || typed.expression_category(expression) != Some(ExpressionCategory::Temporary)
            || typed.expression_type(expression) != typed.symbol_type(root)
            || !matches!(
                typed.symbol_type(root).and_then(|ty| typed.types().get(ty)),
                Some(UnitTypeKind::Builtin(BuiltinType::String))
            )
        {
            return None;
        }
        file.roots().iter().find_map(|item| {
            let Item::Function {
                type_parameters,
                extension_receiver: None,
                form,
                ..
            } = file.ast().items().get(*item).ok()?.payload()
            else {
                return None;
            };
            let block = match form {
                FunctionForm::ImplicitUnitBlock(block)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(block),
                    ..
                } => *block,
                _ => return None,
            };
            (type_parameters.is_empty() && block == scope).then_some(())
        })
    };
    check().is_some()
}

fn name(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    expression: UnitExpressionId,
    symbol: UnitSymbolId,
) -> bool {
    let Ok(node) = file.ast().expressions().get(expression.expression()) else {
        return false;
    };
    if !matches!(node.payload(), Expression::Name) {
        return false;
    }
    let mut references = names
        .names()
        .references()
        .iter()
        .filter(|r| r.source_unit() == expression.source_unit() && r.span() == node.span());
    matches!((references.next(),references.next()),(Some(reference),None) if reference.target()==&UnitReferenceTarget::Symbol(symbol))
}

/// Only the exact final direct jump; eligibility still requires `admitted`.
pub(in crate::ssa) fn tail_jump(
    file: &ParsedFile,
    statement: StatementId,
) -> Option<(lang_frontend::ast::ExpressionId, JumpKind)> {
    let expression = tail_expression(file, statement)?;
    let kind = match file.ast().expressions().get(expression).ok()?.payload() {
        Expression::Continue { .. } => JumpKind::Continue,
        Expression::Break { .. } => JumpKind::Break,
        _ => return None,
    };
    Some((expression, kind))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) enum JumpKind {
    Continue,
    Break,
}

fn tail_expression(
    file: &ParsedFile,
    statement: StatementId,
) -> Option<lang_frontend::ast::ExpressionId> {
    let Statement::While { body, .. } = file.ast().statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Statement::Block { elements } = file.ast().statements().get(*body).ok()?.payload() else {
        return None;
    };
    let Statement::Expression { expression } = file
        .ast()
        .statements()
        .get(*elements.last()?)
        .ok()?
        .payload()
    else {
        return None;
    };
    Some(*expression)
}

#[derive(Clone, Copy)]
pub(in crate::ssa) struct ConditionalJump {
    pub(in crate::ssa) control: lang_frontend::ast::ExpressionId,
    pub(in crate::ssa) condition: lang_frontend::ast::ExpressionId,
    pub(in crate::ssa) jump: lang_frontend::ast::ExpressionId,
    pub(in crate::ssa) kind: JumpKind,
}

/// Exact structural candidate only; typed and per-binding authority comes from `admitted`.
pub(in crate::ssa) fn conditional_jump(
    file: &ParsedFile,
    statement: StatementId,
) -> Option<ConditionalJump> {
    let control = tail_expression(file, statement)?;
    let Expression::If {
        condition,
        then_branch,
        else_branch: None,
        else_span: None,
        ..
    } = file.ast().expressions().get(control).ok()?.payload()
    else {
        return None;
    };
    let Statement::ControlBody { elements } =
        file.ast().statements().get(*then_branch).ok()?.payload()
    else {
        return None;
    };
    let [only] = elements.as_slice() else {
        return None;
    };
    let Statement::Expression { expression: jump } =
        file.ast().statements().get(*only).ok()?.payload()
    else {
        return None;
    };
    let kind = match file.ast().expressions().get(*jump).ok()?.payload() {
        Expression::Break { .. } => JumpKind::Break,
        Expression::Continue { .. } => JumpKind::Continue,
        _ => return None,
    };
    Some(ConditionalJump {
        control,
        condition: *condition,
        jump: *jump,
        kind,
    })
}

pub(in crate::ssa) fn tail_return(
    file: &ParsedFile,
    statement: StatementId,
) -> Option<lang_frontend::ast::ExpressionId> {
    let expression = tail_expression(file, statement)?;
    matches!(
        file.ast().expressions().get(expression).ok()?.payload(),
        Expression::Return { value: None, .. }
    )
    .then_some(expression)
}
