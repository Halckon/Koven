//! Narrow syntax selection only; ownership identity and termination come from facts.
use super::*;
pub(in crate::ssa) mod metadata;
mod narrowed_binding;
mod statement;
pub(in crate::ssa) mod string_control;
use lang_frontend::{ast::StatementId, parser::VariableKind};

pub(super) fn scope(
    parsed: &ParsedFile,
    marker: Span,
    proven: impl Fn(Span, StatementId) -> bool,
) -> Option<StatementId> {
    statement::scope_for(parsed, marker, false, proven)
}

pub(in crate::ssa) fn binding_marker(parsed: &ParsedFile, statement: StatementId) -> Option<Span> {
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

pub(super) use statement::allows;

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

    use super::narrowed_binding::narrowed_string_binding;
    pub(in crate::ssa) use super::narrowed_binding::{
        narrowed_when_binding, validate_narrowed_when_initializer,
    };

    use super::{
        binding_marker, contains,
        string_control::{statement_control, string_statement_controls},
        ungroup,
    };
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        name_resolution::{SourceUnitId, UnitSymbolId},
        ownership_checking::{
            BorrowBindingFact, BorrowBindingStorage, UnitDropPoint, UnitLoanTarget,
            UnitPreflightFacts,
        },
        parser::{Expression, Item, ParsedFile, Statement, VariableKind},
        source::Span,
        type_checking::{
            BuiltinType, CompilationUnitTypes, Copyability, IntrinsicTypeConstructor,
            NullableWhenSubjectCategory, ParameterMode, UnitExpressionId, UnitTypeKind,
        },
    };

    type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(in crate::ssa) enum Policy {
        Scoped,
        WithOwnershipPrimitive,
        NarrowedString,
        NarrowedNullableString,
        NarrowedWhen,
    }

    pub(in crate::ssa) fn policy(
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        fact: &Binding,
    ) -> Option<Policy> {
        if narrowed_when_binding(file, typed, owned, fact).is_some() {
            return Some(Policy::NarrowedWhen);
        }
        if let Some(policy) = narrowed_string_binding(file, typed, owned, fact) {
            return Some(policy);
        }
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

    fn when_argument_returns(
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        binding: &Binding,
        control: UnitExpressionId,
        ends_binding: impl Fn(ExpressionId) -> bool,
        allowed: &mut BTreeSet<usize>,
    ) {
        let Some(plan) = owned.nullable_when(control) else {
            return;
        };
        let expressions = file.ast().expressions();
        let unit = control.source_unit();
        for use_fact in plan.uses() {
            if use_fact.witness().control() != control
                || use_fact
                    .loan()
                    .is_none_or(|loan| loan.target() != binding.origin())
                || use_fact.call().source_unit() != unit
                || !typed.call(use_fact.call()).is_some_and(|call| {
                    call.result_source()
                        == lang_frontend::type_checking::CallableResultSource::Owned
                })
            {
                continue;
            }
            let Ok(call) = expressions.get(use_fact.call().expression()) else {
                continue;
            };
            let Expression::Call { arguments, .. } = call.payload() else {
                continue;
            };
            let Some(index) = arguments
                .iter()
                .position(|arg| UnitExpressionId::new(unit, arg.value) == use_fact.argument())
            else {
                continue;
            };
            for argument in &arguments[index + 1..] {
                let Ok(id) = ungroup(file, argument.value, argument.span) else {
                    continue;
                };
                let Ok(node) = expressions.get(id) else {
                    continue;
                };
                if matches!(node.payload(), Expression::Return { .. }) && ends_binding(id) {
                    allowed.insert(id.index());
                } else if let Expression::If {
                    then_branch,
                    else_branch,
                    ..
                } = node.payload()
                    && let Some(branches) = statement_control(file, typed, unit, id)
                {
                    let mut returned = std::iter::once(*then_branch)
                        .chain(*else_branch)
                        .filter_map(|branch| tail_return(file, branch))
                        .filter(|&returned| ends_binding(returned))
                        .peekable();
                    if returned.peek().is_some() {
                        allowed.extend(returned.map(|expr| expr.index()));
                        branches.allow(id, file, typed, unit, allowed);
                    }
                }
            }
        }
    }

    /// The typed When must observe this binding or its recorded lexical descendant.
    /// Only a Return with a matching frontend end may leave through an arm.
    fn nullable_when_observers(
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        fact: &Binding,
    ) -> BTreeSet<usize> {
        let mut allowed = BTreeSet::new();
        if fact.storage() != BorrowBindingStorage::BorrowedStorage || fact.source_loan().is_none() {
            return allowed;
        }
        let unit = fact.binding().source_unit();
        let bindings = owned.borrow_results().bindings();
        let is_subject = |symbol| {
            std::iter::successors(Some(symbol), |symbol| {
                let index = bindings
                    .binary_search_by_key(symbol, |binding| binding.binding())
                    .ok()?;
                let binding = &bindings[index];
                if binding.storage() != BorrowBindingStorage::BorrowedStorage
                    || binding.origin() != fact.origin()
                {
                    return None;
                }
                binding.parent()
            })
            .take(bindings.len())
            .any(|binding| binding == fact.binding())
        };
        for descriptor in typed.nullable_whens().iter().filter(|descriptor| {
            descriptor.expression().source_unit() == unit
                && descriptor.source_category() == NullableWhenSubjectCategory::BorrowRoot
                && descriptor.stable_symbol().is_some_and(is_subject)
                && typed.symbol_type(fact.binding()) == Some(descriptor.nullable_type())
                && matches!(
                    typed.types().get(descriptor.inner_type()),
                    Some(
                        UnitTypeKind::Builtin(BuiltinType::String)
                            | UnitTypeKind::Intrinsic {
                                constructor: IntrinsicTypeConstructor::Rc,
                                ..
                            }
                    )
                )
        }) {
            allowed.insert(descriptor.expression().expression().index());
            let ends_binding = |returned| {
                let point = UnitDropPoint::ControlTransfer(UnitExpressionId::new(unit, returned));
                owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|end| end.binding() == fact.binding() && end.point() == point)
            };
            for entry in descriptor.entries() {
                string_statement_controls(
                    file,
                    typed,
                    unit,
                    &[entry.body().statement()],
                    ends_binding,
                    false,
                    &mut allowed,
                );
            }
            if typed.types().get(descriptor.inner_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            {
                when_argument_returns(
                    file,
                    typed,
                    owned,
                    fact,
                    descriptor.expression(),
                    ends_binding,
                    &mut allowed,
                );
            }
        }
        allowed
    }

    /// Admit exact argument returns proved by the source loan's unwind.
    /// For disjoint lexical controls, only follow statement order through ordinary blocks.
    pub(in crate::ssa) fn allowed_controls(
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        fact: &Binding,
        argument_controls: &BTreeMap<UnitExpressionId, BTreeSet<usize>>,
        observers: &BTreeSet<usize>,
    ) -> BTreeSet<usize> {
        let mut allowed = argument_controls
            .get(&fact.initializer())
            .cloned()
            .unwrap_or_default();
        let mut observers = observers.clone();
        observers.extend(nullable_when_observers(file, typed, owned, fact));
        allowed.extend(observers.iter().copied());
        let range = matches!(
            fact.storage(),
            BorrowBindingStorage::NewRangeDescriptor
                | BorrowBindingStorage::BorrowedCarrierMetadata
        );
        let unit = fact.binding().source_unit();
        let string_scope = scoped_string_binding(typed, fact);
        let collect = |statements: &[StatementId], allowed: &mut BTreeSet<usize>| {
            collect_disjoint_controls(
                file,
                statements,
                unit,
                argument_controls,
                range,
                !observers.is_empty(),
                allowed,
            );
            if string_scope {
                string_statement_controls(file, typed, unit, statements, |_| true, true, allowed);
            }
        };
        let mut parents = vec![None; file.ast().statements().len()];
        let mut enclosing_controls = vec![None; parents.len()];
        let mut acquisition = None;
        for (block, node) in file.ast().statements().iter() {
            match node.payload() {
                Statement::Block { elements } | Statement::ControlBody { elements }
                    if string_scope || matches!(node.payload(), Statement::Block { .. }) =>
                {
                    for (index, &statement) in elements.iter().enumerate() {
                        if binding_marker(file, statement) == Some(fact.marker_span()) {
                            acquisition = Some((block, elements.as_slice(), index));
                        }
                        parents[statement.index()] = Some((block, elements.as_slice(), index));
                    }
                }
                Statement::Expression { expression } if string_scope => {
                    let Ok(id) = ungroup(file, *expression, node.span()) else {
                        continue;
                    };
                    if let Some(control) = statement_control(file, typed, unit, id) {
                        for arm in control.arms {
                            enclosing_controls[arm.index()] = Some((block, id));
                        }
                    }
                }
                _ => {}
            }
        }
        let Some((scope, elements, binding_index)) = acquisition else {
            return allowed;
        };
        collect(&elements[..binding_index], &mut allowed);

        let mut active_controls = BTreeSet::new();
        let mut has_proved_return = false;
        if string_scope {
            let ends: BTreeSet<_> = owned
                .borrow_results()
                .ends()
                .iter()
                .filter_map(|end| {
                    let UnitDropPoint::ControlTransfer(expression) = end.point() else {
                        return None;
                    };
                    (end.binding() == fact.binding() && expression.source_unit() == unit)
                        .then_some(expression.expression().index())
                })
                .collect();
            string_statement_controls(
                file,
                typed,
                unit,
                &elements[binding_index + 1..],
                |id| ends.contains(&id.index()),
                true,
                &mut active_controls,
            );
            has_proved_return = ends.iter().any(|end| active_controls.contains(end));
        }
        allowed.extend(active_controls.iter().copied());

        let mut ends = owned.borrow_results().ends().iter().filter(|end| {
            end.binding() == fact.binding()
                && !matches!(end.point(), UnitDropPoint::ControlTransfer(expression)
                    if expression.source_unit() == fact.binding().source_unit()
                        && (range
                            || observers.contains(&expression.expression().index())
                            || active_controls.contains(&expression.expression().index())))
        });
        let remaining = (ends.next().map(|end| end.point()), ends.next());
        // No bound path reaches the lexical successor if the only ends are
        // the already-checked return edges.
        let only_returns = has_proved_return && matches!(remaining, (None, None));
        let end = match remaining {
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
            _ => end == Some(scope) || only_returns,
        };

        let mut block = scope;
        loop {
            if let Some((parent, siblings, index)) = parents[block.index()] {
                collect(&siblings[..index], &mut allowed);
                if ended_in_scope {
                    collect(&siblings[index + 1..], &mut allowed);
                }
                block = parent;
            } else if let Some((statement, id)) = enclosing_controls[block.index()] {
                let Some(control) = statement_control(file, typed, unit, id) else {
                    break;
                };
                control.allow(id, file, typed, unit, &mut allowed);
                for arm in control.arms {
                    if arm != block {
                        collect(&[arm], &mut allowed);
                    }
                }
                block = statement;
            } else {
                break;
            }
        }
        allowed
    }

    fn scoped_string_binding(typed: &CompilationUnitTypes, fact: &Binding) -> bool {
        fact.storage() == BorrowBindingStorage::BorrowedStorage
            && fact.source_loan().is_some()
            && matches!(fact.origin(), UnitLoanTarget::Place(place) if place.is_root())
            && match typed
                .symbol_type(fact.binding())
                .and_then(|ty| typed.types().get(ty))
            {
                Some(UnitTypeKind::Builtin(BuiltinType::String)) => true,
                Some(UnitTypeKind::Nullable(inner)) => {
                    typed.types().get(*inner) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                }
                _ => false,
            }
    }

    // The binding does not exist yet on a Return from an argument. Require the
    // actual end of this call's source token there, independently of its normal path.
    pub(in crate::ssa) fn argument_returns(
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        fact: &Binding,
        allow_copyable_borrow: bool,
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
            let copyable_temporary_borrow = allow_copyable_borrow
                && mapping.mode() == ParameterMode::Borrow
                && mapping.category()
                    == lang_frontend::type_checking::ExpressionCategory::Temporary;
            if (mapping.mode() != ParameterMode::Value && !copyable_temporary_borrow)
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
        let expression = tail_return(file, statement)?;
        matches!(
            file.ast().expressions().get(expression).ok()?.payload(),
            Expression::Return { value: None, .. }
        )
        .then_some(expression)
    }

    pub(in crate::ssa) fn control_tail(
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
        ungroup(file, *expression, tail.span()).ok()
    }

    fn tail_return(file: &ParsedFile, statement: StatementId) -> Option<ExpressionId> {
        let expression = control_tail(file, statement)?;
        matches!(
            file.ast().expressions().get(expression).ok()?.payload(),
            Expression::Return { .. }
        )
        .then_some(expression)
    }

    fn collect_disjoint_controls(
        file: &ParsedFile,
        statements: &[StatementId],
        source_unit: SourceUnitId,
        argument_controls: &BTreeMap<UnitExpressionId, BTreeSet<usize>>,
        range: bool,
        string_observer: bool,
        allowed: &mut BTreeSet<usize>,
    ) {
        let mut pending = statements.to_vec();
        while let Some(id) = pending.pop() {
            let Ok(node) = file.ast().statements().get(id) else {
                continue;
            };
            match node.payload() {
                Statement::Block { elements } | Statement::ControlBody { elements } => {
                    pending.extend_from_slice(elements)
                }
                Statement::For { body, .. } if range => pending.push(*body),
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
                            if range || string_observer {
                                let returned = if string_observer {
                                    tail_return
                                } else {
                                    bare_return
                                };
                                for branch in std::iter::once(*then_branch).chain(*else_branch) {
                                    if let Some(returned) = returned(file, branch) {
                                        allowed.insert(returned.index());
                                    }
                                }
                            }
                        }
                        Expression::Return { .. } if range || string_observer => {
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
