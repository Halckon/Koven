//! Isolated source shape, not CFG validity or Carry authorization.
use super::*;
use crate::parser::{
    Expression, FunctionBody, FunctionForm, Item, NameMarker, StringPart, VariableKind,
};
use crate::type_checking::{BuiltinType, UnitTypeKind};

pub(super) fn matches(
    fixture: &Fixture,
    owned: &CompilationUnitOwnership,
    evidence: &CarrySourceEvidence,
) -> Option<()> {
    let ast = fixture.parsed[0].ast();
    let source = fixture.source(0);
    let function = ast
        .items()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Item::Function { form, .. } => Some(form),
            _ => None,
        })?;
    let body = match function {
        FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        }
        | FunctionForm::ImplicitUnitBlock(body) => *body,
        _ => return None,
    };
    let block = |id| match ast.statements().get(id).ok()?.payload() {
        Statement::Block { elements } => Some(elements.as_slice()),
        _ => None,
    };
    let [root_a, root_b, outer, parent, suffix] = block(body)? else {
        return None;
    };
    let Statement::While {
        body: parent_body, ..
    } = ast.statements().get(*parent).ok()?.payload()
    else {
        return None;
    };
    let [inner, child, parent_old, outer_step] = block(*parent_body)? else {
        return None;
    };
    let Statement::While {
        body: child_body, ..
    } = ast.statements().get(*child).ok()?.payload()
    else {
        return None;
    };
    let [child_old, inner_step] = block(*child_body)? else {
        return None;
    };
    let Statement::Expression { expression } = ast.statements().get(*suffix).ok()?.payload() else {
        return None;
    };
    if UnitExpressionId::new(source, *expression) != evidence.suffix {
        return None;
    }
    let resolution = fixture.names.names().source_units()[source.index()].resolution();
    let binding = |statement| {
        let Statement::LocalVariable { declaration } =
            ast.statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            kind,
            name: NameMarker::Present(name),
            initializer,
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        let symbols = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == *name)
            .collect::<Vec<_>>();
        let [symbol] = symbols.as_slice() else {
            return None;
        };
        Some((UnitSymbolId::new(source, symbol.id()), *kind, *initializer))
    };
    let typed = fixture.typed.types();
    let is_type = |symbol, builtin| {
        typed
            .symbol_type(symbol)
            .and_then(|ty| typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(builtin))
    };
    let literal = |expression| {
        matches!(ast.expressions().get(expression).ok()?.payload(), Expression::String { parts } if parts.iter().all(|part| matches!(part, StringPart::Text(_)))).then_some(())
    };
    for (statement, expected) in [(*root_a, evidence.replaced), (*root_b, evidence.carried)] {
        let (symbol, kind, initializer) = binding(statement)?;
        if symbol != expected || kind != VariableKind::Var || !is_type(symbol, BuiltinType::String)
        {
            return None;
        }
        literal(initializer)?;
    }
    // Counter storage is scalar; its exact update/condition relationship remains unproved.
    for statement in [*outer, *inner] {
        let (symbol, kind, initializer) = binding(statement)?;
        if kind != VariableKind::Var
            || !is_type(symbol, BuiltinType::Int)
            || !matches!(
                ast.expressions().get(initializer).ok()?.payload(),
                Expression::Name
            )
        {
            return None;
        }
    }
    let mut primitive_calls = Vec::new();
    for statement in [*child_old, *parent_old] {
        let (symbol, kind, initializer) = binding(statement)?;
        if kind != VariableKind::Val || !is_type(symbol, BuiltinType::String) {
            return None;
        }
        let call = UnitExpressionId::new(source, initializer);
        let primitive = owned.ownership_primitive(call)?;
        if primitive.places().len() != 1 || primitive.places()[0].root() != evidence.replaced {
            return None;
        }
        let Expression::Call { arguments, .. } = ast.expressions().get(initializer).ok()?.payload()
        else {
            return None;
        };
        let [_, incoming] = arguments.as_slice() else {
            return None;
        };
        literal(incoming.value)?;
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(symbol))
            .collect::<Vec<_>>();
        let [drop] = drops.as_slice() else {
            return None;
        };
        if drop.point() != UnitDropPoint::AfterStatement(UnitStatementId::new(source, statement)) {
            return None;
        }
        primitive_calls.push(initializer);
    }
    for statement in [*inner_step, *outer_step] {
        let Statement::Expression { expression } = ast.statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        if !matches!(
            ast.expressions().get(*expression).ok()?.payload(),
            Expression::Assignment { .. }
        ) {
            return None;
        }
    }
    let is_int_expression = |expression| {
        typed
            .expression_type(UnitExpressionId::new(source, expression))
            .and_then(|ty| typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    };
    // Closed expression variants: no branch, early exit, field, nested call producer,
    // interpolation, closure, borrow-result or hidden extra statement is admitted.
    for (id, node) in ast.expressions().iter() {
        match node.payload() {
            Expression::Name | Expression::Literal(_) => {}
            Expression::String { parts }
                if parts.iter().all(|part| matches!(part, StringPart::Text(_))) => {}
            Expression::Call { .. }
                if primitive_calls.contains(&id) || id == evidence.suffix.expression() => {}
            Expression::Binary { left, right, .. }
                if is_int_expression(*left) && is_int_expression(*right) => {}
            Expression::Assignment { target, value, .. }
                if is_int_expression(*target) && is_int_expression(*value) => {}
            _ => return None,
        }
    }
    Some(())
}
