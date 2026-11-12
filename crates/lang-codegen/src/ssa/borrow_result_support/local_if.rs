//! Narrow syntax selection only; ownership identity and termination come from facts.
use super::*;
use lang_frontend::{
    ast::StatementId,
    parser::{StringPart, VariableKind},
};

/// Require one direct lexical scope whose every borrow binding has matching
/// published new-descriptor origin, continuation and exact scope-end facts.
pub(super) fn scope(
    parsed: &ParsedFile,
    marker: Span,
    proven: impl Fn(Span, StatementId) -> bool,
) -> Option<StatementId> {
    parsed.ast().statements().iter().find_map(|(scope, node)| {
        let Statement::Block { elements } = node.payload() else {
            return None;
        };
        let markers = elements
            .iter()
            .filter_map(|statement| binding_marker(parsed, *statement))
            .collect::<Vec<_>>();
        (markers.contains(&marker) && markers.iter().all(|marker| proven(*marker, scope)))
            .then_some(scope)
    })
}

pub(super) fn binding_marker(parsed: &ParsedFile, statement: StatementId) -> Option<Span> {
    let Statement::LocalVariable { declaration } =
        parsed.ast().statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::BorrowVal(marker),
        ..
    } = parsed.ast().items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    Some(*marker)
}

pub(super) fn allows(
    parsed: &ParsedFile,
    scope: StatementId,
    expression: ExpressionId,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    returned: Option<ExpressionId>,
) -> bool {
    let Ok(node) = parsed.ast().expressions().get(expression) else {
        return false;
    };
    let Expression::If {
        condition,
        then_branch,
        else_branch,
        ..
    } = node.payload()
    else {
        return false;
    };
    if (else_branch.is_none() && returned.is_none())
        || builtin(*condition) != Some(BuiltinType::Boolean)
        || builtin(expression) != Some(BuiltinType::Unit)
    {
        return false;
    }
    let Ok(scope_node) = parsed.ast().statements().get(scope) else {
        return false;
    };
    let Statement::Block { elements } = scope_node.payload() else {
        return false;
    };
    // The selected if is a complete statement, never a Unit-valued argument,
    // initializer, control tail or loop body. Its bindings already exist.
    let is_statement = elements.iter().any(|statement| {
        parsed
            .ast()
            .statements()
            .get(*statement)
            .is_ok_and(|statement| {
                matches!(statement.payload(), Statement::Expression { expression: actual }
                if ungroup(parsed, *actual, statement.span()).ok() == Some(expression))
            })
    });
    if !is_statement
        || (returned.is_none()
            && elements
                .iter()
                .filter_map(|statement| binding_marker(parsed, *statement))
                .any(|marker| marker.end() > node.span().start()))
        || parsed.ast().statements().iter().any(|(_, statement)| {
            contains(statement.span(), node.span())
                && matches!(
                    statement.payload(),
                    Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
                )
        })
        || parsed.ast().expressions().iter().any(|(_, parent)| {
            contains(parent.span(), node.span())
                && matches!(parent.payload(), Expression::Lambda { .. })
        })
    {
        return false;
    }
    let arm_allowed = |branch_id| {
        let Ok(branch) = parsed.ast().statements().get(branch_id) else {
            return false;
        };
        let Statement::ControlBody { elements } = branch.payload() else {
            return matches!(branch.payload(), Statement::Expression { .. })
                && returned.is_some()
                && ordinary_local::bare_return(parsed, branch_id) == returned;
        };
        !elements.is_empty()
            && elements.iter().all(|statement| {
                let Ok(statement) = parsed.ast().statements().get(*statement) else {
                    return false;
                };
                let Statement::Expression { expression } = statement.payload() else {
                    return false;
                };
                let Ok(expression) = ungroup(parsed, *expression, statement.span()) else {
                    return false;
                };
                Some(expression) == returned
                    || (builtin(expression) == Some(BuiltinType::Unit)
                        && owned_call(expression)
                        && parsed
                            .ast()
                            .expressions()
                            .get(expression)
                            .is_ok_and(|node| matches!(node.payload(), Expression::Call { .. })))
            })
    };
    if !std::iter::once(*then_branch)
        .chain(*else_branch)
        .all(arm_allowed)
    {
        return false;
    }
    // Check the entire candidate, including its condition. Only the direct
    // return selected by the caller may terminate this branch.
    parsed.ast().expressions().iter().all(|(id, child)| {
        if !contains(node.span(), child.span()) || id == expression || Some(id) == returned {
            return true;
        }
        if builtin(id) == Some(BuiltinType::Nothing) {
            return false;
        }
        match child.payload() {
            Expression::Name
            | Expression::Literal(_)
            | Expression::Group { .. }
            | Expression::Prefix { .. }
            | Expression::Binary { .. }
            | Expression::Member { safe: false, .. } => true,
            Expression::String { parts } => {
                parts.iter().all(|part| matches!(part, StringPart::Text(_)))
            }
            Expression::Call { .. } => {
                owned_call(id)
                    && matches!(
                        builtin(id),
                        Some(BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Unit)
                    )
            }
            // Hidden nullable, propagation, indexing, capture and interpolation
            // control edges are not proved by the normal-arm selection.
            _ => false,
        }
    })
}

/// A direct return statement or selected return arm needs an exact end fact
/// for this binding. A selected arm still uses the bounded if condition and body.
pub(in crate::ssa) fn return_controls(
    parsed: &ParsedFile,
    marker: Span,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    ends_on_return: impl Fn(ExpressionId) -> bool,
    allowed: &mut BTreeSet<usize>,
) {
    let Some(scope) = scope(parsed, marker, |_, _| true) else {
        return;
    };
    let Ok(node) = parsed.ast().statements().get(scope) else {
        return;
    };
    let Statement::Block { elements } = node.payload() else {
        return;
    };
    for statement in elements {
        let Ok(node) = parsed.ast().statements().get(*statement) else {
            continue;
        };
        let Statement::Expression { expression } = node.payload() else {
            continue;
        };
        let Ok(control) = ungroup(parsed, *expression, node.span()) else {
            continue;
        };
        let Ok(node) = parsed.ast().expressions().get(control) else {
            continue;
        };
        if marker.end() > node.span().start() {
            continue;
        }
        if matches!(node.payload(), Expression::Return { .. }) {
            if ends_on_return(control) {
                allowed.insert(control.index());
            }
            continue;
        }
        let Expression::If {
            then_branch,
            else_branch,
            ..
        } = node.payload()
        else {
            continue;
        };
        let Some(returned) = std::iter::once(*then_branch)
            .chain(*else_branch)
            .find_map(|branch| ordinary_local::bare_return(parsed, branch))
        else {
            continue;
        };
        if ends_on_return(returned)
            && allows(parsed, scope, control, builtin, owned_call, Some(returned))
        {
            allowed.insert(control.index());
            allowed.insert(returned.index());
        }
    }
}

pub(in crate::ssa) mod ordinary_local {
    //! Ordinary call-result bindings keep control outside the published loan extent.
    use std::collections::{BTreeMap, BTreeSet};

    use super::{binding_marker, contains, ungroup};
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        name_resolution::{SourceUnitId, UnitSymbolId},
        ownership_checking::{
            BorrowBindingFact, BorrowBindingStorage, CompilationUnitOwnership, UnitDropPoint,
            UnitLoanTarget,
        },
        parser::{Expression, Item, ParsedFile, Statement, VariableKind},
        source::Span,
        type_checking::{CompilationUnitTypes, Copyability, ParameterMode, UnitExpressionId},
    };

    type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(in crate::ssa) enum Policy {
        Scoped,
        WithOwnershipPrimitive,
    }

    pub(in crate::ssa) fn policy(
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        fact: &Binding,
    ) -> Option<Policy> {
        let source = fact.source_loan()?;
        if fact.storage() != BorrowBindingStorage::BorrowedStorage
            || !matches!(fact.origin(), UnitLoanTarget::Place(place) if place.element().is_none())
            || typed.map_require_value(source.call()).is_some()
            || typed.call(source.call())?.borrow_return().is_none()
        {
            return None;
        }

        let source_unit = fact.binding().source_unit();
        if source.call().source_unit() != source_unit
            || source.argument().source_unit() != source_unit
            || ungroup(file, fact.initializer().expression(), fact.marker_span()).ok()?
                != source.call().expression()
        {
            return None;
        }
        let argument = ungroup(file, source.argument().expression(), fact.marker_span()).ok()?;
        if typed.non_null_use(source.argument()).is_some()
            || typed
                .non_null_use(UnitExpressionId::new(source_unit, argument))
                .is_some()
        {
            return None;
        }

        let owner = callable(file, fact.marker_span())?;
        let has_primitive = typed
            .ownership_primitives()
            .iter()
            .map(|descriptor| descriptor.expression())
            .chain(
                owned
                    .field_replacements()
                    .iter()
                    .map(|plan| plan.descriptor().expression()),
            )
            .filter(|expression| expression.source_unit() == fact.binding().source_unit())
            .any(|expression| {
                file.ast()
                    .expressions()
                    .get(expression.expression())
                    .is_ok_and(|node| {
                        contains(owner, node.span()) && callable(file, node.span()) == Some(owner)
                    })
            });
        Some(if has_primitive {
            Policy::WithOwnershipPrimitive
        } else {
            Policy::Scoped
        })
    }

    fn callable(file: &ParsedFile, span: Span) -> Option<Span> {
        file.ast()
            .items()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Item::Function { .. }))
            .map(|(_, node)| node.span())
            .chain(
                file.ast()
                    .expressions()
                    .iter()
                    .filter(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
                    .map(|(_, node)| node.span()),
            )
            .filter(|candidate| contains(*candidate, span))
            .min_by_key(|candidate| candidate.end() - candidate.start())
    }

    /// Admit exact argument returns proved by the source loan's unwind.
    /// For disjoint lexical controls, only follow statement order through ordinary blocks.
    pub(in crate::ssa) fn allowed_controls(
        file: &ParsedFile,
        owned: &CompilationUnitOwnership,
        fact: &Binding,
        argument_controls: &BTreeMap<UnitExpressionId, BTreeSet<usize>>,
    ) -> BTreeSet<usize> {
        let mut allowed = argument_controls
            .get(&fact.initializer())
            .cloned()
            .unwrap_or_default();
        let range = matches!(
            fact.storage(),
            BorrowBindingStorage::NewRangeDescriptor
                | BorrowBindingStorage::BorrowedCarrierMetadata
        );
        let collect = |statements: &[StatementId], allowed: &mut BTreeSet<usize>| {
            collect_disjoint_controls(
                file,
                statements,
                fact.binding().source_unit(),
                argument_controls,
                range,
                allowed,
            );
        };
        let mut parents = vec![None; file.ast().statements().len()];
        let mut acquisition = None;
        for (block, node) in file.ast().statements().iter() {
            let Statement::Block { elements } = node.payload() else {
                continue;
            };
            for (index, &statement) in elements.iter().enumerate() {
                if binding_marker(file, statement) == Some(fact.marker_span()) {
                    acquisition = Some((block, elements.as_slice(), index));
                }
                parents[statement.index()] = Some((block, elements.as_slice(), index));
            }
        }
        let Some((scope, elements, binding_index)) = acquisition else {
            return allowed;
        };
        collect(&elements[..binding_index], &mut allowed);

        let mut ends = owned.borrow_results().ends().iter().filter(|end| {
            end.binding() == fact.binding()
                && !(range
                    && matches!(end.point(), UnitDropPoint::ControlTransfer(expression)
                        if expression.source_unit() == fact.binding().source_unit()))
        });
        let end = match (ends.next().map(|end| end.point()), ends.next()) {
            (Some(UnitDropPoint::AfterStatement(end)), None)
                if end.source_unit() == fact.binding().source_unit() =>
            {
                Some(end.statement())
            }
            _ => None,
        };
        let ended_in_scope = match end.and_then(|end| parents.get(end.index()).copied().flatten()) {
            Some((block, _, index)) if block == scope && index >= binding_index => {
                collect(&elements[index + 1..], &mut allowed);
                true
            }
            _ => end == Some(scope),
        };

        let mut block = scope;
        while let Some((parent, siblings, index)) = parents[block.index()] {
            collect(&siblings[..index], &mut allowed);
            if ended_in_scope {
                collect(&siblings[index + 1..], &mut allowed);
            }
            block = parent;
        }
        allowed
    }

    // The binding does not exist yet on a Return from an argument. Require the
    // actual end of this call's source token there, independently of its normal path.
    pub(in crate::ssa) fn argument_returns(
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        fact: &Binding,
    ) -> Option<BTreeSet<usize>> {
        let source = fact.source_loan()?;
        let descriptor = typed.call(source.call())?;
        let call = file
            .ast()
            .expressions()
            .get(source.call().expression())
            .ok()?;
        let Expression::Call { arguments, .. } = call.payload() else {
            return None;
        };
        let mut allowed = BTreeSet::new();
        let unit = |id| UnitExpressionId::new(source.call().source_unit(), id);
        for mapping in descriptor.arguments() {
            if mapping.mode() != ParameterMode::Value
                || typed.copyability(mapping.parameter_type()) != Copyability::Copyable
            {
                continue;
            }
            let Some(argument) = arguments.get(mapping.argument_index()) else {
                continue;
            };
            let Ok(control) = ungroup(file, argument.value, argument.span) else {
                continue;
            };
            let Ok(node) = file.ast().expressions().get(control) else {
                continue;
            };
            let Expression::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } = node.payload()
            else {
                continue;
            };
            if typed.expression_falls_through(unit(control)) != Some(true) {
                continue;
            }
            let Some(returned) =
                bare_return(file, *then_branch).or_else(|| bare_return(file, *else_branch))
            else {
                continue;
            };
            if owned.borrow_call_prefix_ends().iter().any(|end| {
                end.point() == UnitDropPoint::ControlTransfer(unit(returned))
                    && end.loan().call() == source.call()
                    && end.loan().argument() == source.argument()
                    && end.loan().target() == fact.origin()
            }) {
                allowed.insert(control.index());
                allowed.insert(returned.index());
            }
        }
        (!allowed.is_empty()).then_some(allowed)
    }

    pub(in crate::ssa::borrow_result_support) fn bare_return(
        file: &ParsedFile,
        statement: StatementId,
    ) -> Option<ExpressionId> {
        let statements = file.ast().statements();
        let node = statements.get(statement).ok()?;
        let tail = match node.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                statements.get(*elements.last()?).ok()?
            }
            _ => node,
        };
        let Statement::Expression { expression } = tail.payload() else {
            return None;
        };
        let expression = ungroup(file, *expression, tail.span()).ok()?;
        matches!(
            file.ast().expressions().get(expression).ok()?.payload(),
            Expression::Return { value: None, .. }
        )
        .then_some(expression)
    }

    fn collect_disjoint_controls(
        file: &ParsedFile,
        statements: &[StatementId],
        source_unit: SourceUnitId,
        argument_controls: &BTreeMap<UnitExpressionId, BTreeSet<usize>>,
        range: bool,
        allowed: &mut BTreeSet<usize>,
    ) {
        let mut pending = statements.to_vec();
        while let Some(id) = pending.pop() {
            let Ok(node) = file.ast().statements().get(id) else {
                continue;
            };
            match node.payload() {
                Statement::Block { elements } => pending.extend_from_slice(elements),
                Statement::Expression { expression } => {
                    let Ok(expression) = ungroup(file, *expression, node.span()) else {
                        continue;
                    };
                    let Ok(control) = file.ast().expressions().get(expression) else {
                        continue;
                    };
                    match control.payload() {
                        Expression::If {
                            then_branch,
                            else_branch,
                            ..
                        } => {
                            allowed.insert(expression.index());
                            if range {
                                for branch in std::iter::once(*then_branch).chain(*else_branch) {
                                    if let Some(returned) = bare_return(file, branch) {
                                        allowed.insert(returned.index());
                                    }
                                }
                            }
                        }
                        Expression::Return { .. } if range => {
                            allowed.insert(expression.index());
                        }
                        _ => {}
                    }
                }
                Statement::LocalVariable { declaration } => {
                    if let Ok(node) = file.ast().items().get(*declaration)
                        && let Item::Variable {
                            kind: VariableKind::BorrowVal(_),
                            initializer,
                            ..
                        } = node.payload()
                        && let Some(controls) =
                            argument_controls.get(&UnitExpressionId::new(source_unit, *initializer))
                    {
                        allowed.extend(controls);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
pub(in crate::ssa) mod stable_map {
    //! Stable Borrow bindings keep their real declaration, scope end and reborrow birth.
    use super::*;
    use crate::ssa::model::{
        Definition, EntityId, EntityType, Function, LoanKind, Operation, Origin,
    };
    use lang_frontend::parser::{NameMarker, VariableKind};

    /// The published end is the enclosing ordinary callable body, not the provider exit.
    pub(in crate::ssa) fn declaration(
        parsed: &ParsedFile,
        marker: Span,
        borrow_marker: Span,
        initializer: ExpressionId,
        scope: StatementId,
        provider: StatementId,
    ) -> Result<Span, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, marker);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, marker);
        let node = parsed.ast().statements().get(scope).map_err(|_| bad())?;
        let Statement::Block { elements } = node.payload() else {
            return Err(unsupported());
        };
        if !parsed.ast().items().iter().any(|(_, node)| {
            matches!(node.payload(),
            Item::Function { form: FunctionForm::ImplicitUnitBlock(body) | FunctionForm::Explicit {
                body: FunctionBody::Block(body), .. }, .. } if *body == scope)
        }) {
            return Err(unsupported());
        }
        let provider_index = elements
            .iter()
            .position(|id| *id == provider)
            .ok_or_else(unsupported)?;
        let mut declarations = Vec::new();
        for (index, &statement) in elements.iter().enumerate() {
            let node = parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|_| bad())?;
            let Statement::LocalVariable { declaration } = node.payload() else {
                continue;
            };
            let mut declaration = *declaration;
            loop {
                let item = parsed.ast().items().get(declaration).map_err(|_| bad())?;
                match item.payload() {
                    Item::Modified {
                        declaration: inner, ..
                    } => declaration = *inner,
                    Item::Variable {
                        kind: VariableKind::BorrowVal(actual),
                        name: NameMarker::Present(name),
                        initializer: expression,
                        ..
                    } if *name == marker
                        && *expression == initializer
                        && *actual == borrow_marker =>
                    {
                        declarations.push((index, node.span()));
                        break;
                    }
                    _ => break,
                }
            }
        }
        let [(index, declaration)] = declarations.as_slice() else {
            return Err(bad());
        };
        if *index >= provider_index {
            return Err(unsupported());
        }
        Ok(*declaration)
    }

    /// Match the original alias instruction, then prove every current incoming column.
    /// No operand is evaluated and no unrelated same-type loan can serve as the seed.
    pub(in crate::ssa) fn seed(
        function: &Function,
        current: EntityId,
        parent: EntityId,
        declaration: Span,
        span: Span,
        transport: impl Fn(&Function, EntityId, EntityId, Span) -> Result<(), LoweringError>,
    ) -> Result<EntityId, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, marker_or(span));
        let Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        }) = function.entity(parent).map(|e| e.ty)
        else {
            return Err(bad());
        };
        let entry = function.entry_block().ok_or_else(bad)?;
        if !matches!(function.entity(parent).map(|e| e.definition),
            Some(Definition::BlockParameter { block, .. }) if block == entry)
        {
            return Err(bad());
        }
        let births = function
            .instructions
            .iter()
            .filter(|i| {
                i.origin == Origin::Source(declaration)
                    && matches!(i.operation, Operation::SharedReborrow { .. })
            })
            .collect::<Vec<_>>();
        let [birth] = births.as_slice() else {
            return Err(bad());
        };
        let Operation::SharedReborrow { source } = birth.operation else {
            return Err(bad());
        };
        let [anchor @ EntityId::Loan(_)] = birth.results.as_slice() else {
            return Err(bad());
        };
        if function.entity(*anchor).map(|e| (e.ty, e.definition))
            != function.entity(parent).map(|e| {
                (
                    e.ty,
                    Definition::InstructionResult {
                        instruction: birth.id,
                        index: 0,
                    },
                )
            })
        {
            return Err(bad());
        }
        transport(function, EntityId::Loan(source), parent, span)?;
        transport(function, current, *anchor, span)?;
        Ok(*anchor)
    }

    const fn marker_or(span: Span) -> Span {
        span
    }
}
