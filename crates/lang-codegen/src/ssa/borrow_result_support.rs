//! 首片 CFG 能力边界：不让尚未运输的结果/source loan 隐式跨块。
use std::collections::BTreeSet;

use super::{
    LoweringError, LoweringErrorKind, lowering_support::error,
    unit_lower::span_contains as contains,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement},
    source::Span,
    type_checking::BuiltinType,
};

pub(super) mod closed_while;
pub(super) mod invariant_while;
pub(super) mod local_if;
pub(super) use local_if::ordinary_local;
pub(super) mod branch_no_carry;
pub(super) mod no_carry;
#[cfg(test)]
pub(super) use local_if::stable_map;

mod short_circuit {
    //! Select only already-proven lexical range scopes and normal Boolean uses.
    use super::*;
    use lang_frontend::parser::{PrefixOperator, StringPart, VariableKind};

    pub(super) fn allows(
        parsed: &ParsedFile,
        scope: StatementId,
        expression: ExpressionId,
        builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
        call_allowed: &impl Fn(ExpressionId, CallAccess) -> bool,
    ) -> bool {
        let Ok(node) = parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Binary {
            operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
            left,
            right,
            ..
        } = node.payload()
        else {
            return false;
        };
        if [expression, *left, *right]
            .into_iter()
            .any(|id| builtin(id) != Some(BuiltinType::Boolean))
        {
            return false;
        }
        let Ok(scope_node) = parsed.ast().statements().get(scope) else {
            return false;
        };
        let Statement::Block { elements } = scope_node.payload() else {
            return false;
        };
        if !contains(scope_node.span(), node.span())
            || elements
                .iter()
                .filter_map(|statement| local_if::binding_marker(parsed, *statement))
                .any(|marker| marker.end() > node.span().start())
            || parsed.ast().statements().iter().any(|(_, parent)| {
                contains(parent.span(), node.span())
                    && matches!(
                        parent.payload(),
                        Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
                    )
            })
            || parsed.ast().expressions().iter().any(|(id, parent)| {
                id != expression
                    && contains(parent.span(), node.span())
                    && matches!(
                        parent.payload(),
                        Expression::Lambda { .. }
                            | Expression::Binary {
                                operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
                                ..
                            }
                    )
            })
            || !normal_tree(parsed, expression, expression, builtin, call_allowed)
        {
            return false;
        }
        // Select an entire direct initializer/argument/if condition, not a nested
        // expression or a call-prefix exception that introduces another CFG.
        elements.iter().any(|statement| {
            let Ok(statement) = parsed.ast().statements().get(*statement) else {
                return false;
            };
            match statement.payload() {
                Statement::LocalVariable { declaration } => {
                    let Ok(variable) = parsed.ast().items().get(*declaration) else {
                        return false;
                    };
                    let Item::Variable {
                        kind: VariableKind::Val | VariableKind::Var,
                        initializer,
                        ..
                    } = variable.payload()
                    else {
                        return false;
                    };
                    ungroup(parsed, *initializer, variable.span()).ok() == Some(expression)
                }
                Statement::Expression { expression: outer } => {
                    let Ok(outer) = ungroup(parsed, *outer, statement.span()) else {
                        return false;
                    };
                    let Ok(outer_node) = parsed.ast().expressions().get(outer) else {
                        return false;
                    };
                    match outer_node.payload() {
                        Expression::Call { arguments, .. } => {
                            arguments.iter().any(|argument| {
                                ungroup(parsed, argument.value, argument.span).ok()
                                    == Some(expression)
                            }) && normal_tree(parsed, outer, expression, builtin, call_allowed)
                        }
                        Expression::If { condition, .. } => {
                            ungroup(parsed, *condition, node.span()).ok() == Some(expression)
                                && local_if::allows(
                                    parsed,
                                    scope,
                                    outer,
                                    builtin,
                                    &|id| call_allowed(id, CallAccess::OwnedResult),
                                    None,
                                )
                        }
                        _ => false,
                    }
                }
                _ => false,
            }
        })
    }

    fn normal_tree(
        parsed: &ParsedFile,
        root: ExpressionId,
        selected: ExpressionId,
        builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
        call_allowed: &impl Fn(ExpressionId, CallAccess) -> bool,
    ) -> bool {
        let Ok(node) = parsed.ast().expressions().get(root) else {
            return false;
        };
        parsed.ast().expressions().iter().all(|(id, child)| {
            if !contains(node.span(), child.span()) || id == selected {
                return true;
            }
            if builtin(id) == Some(BuiltinType::Nothing) {
                return false;
            }
            match child.payload() {
                Expression::Name
                | Expression::Literal(_)
                | Expression::Group { .. }
                | Expression::Member { safe: false, .. } => true,
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    builtin(*operand) == Some(BuiltinType::Boolean)
                        && builtin(id) == Some(BuiltinType::Boolean)
                }
                Expression::String { parts } => {
                    parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                }
                Expression::Binary {
                    operator:
                        BinaryOperator::Equal
                        | BinaryOperator::NotEqual
                        | BinaryOperator::Less
                        | BinaryOperator::LessEqual
                        | BinaryOperator::Greater
                        | BinaryOperator::GreaterEqual,
                    left,
                    right,
                    ..
                } => {
                    [*left, *right]
                        .into_iter()
                        .all(|operand| builtin(operand) == Some(BuiltinType::Int))
                        && builtin(id) == Some(BuiltinType::Boolean)
                }
                Expression::Call { .. } => {
                    call_allowed(id, CallAccess::ReadOnly)
                        && matches!(
                            builtin(id),
                            Some(BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Unit)
                        )
                }
                _ => false,
            }
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CallAccess {
    OwnedResult,
    ReadOnly,
}

pub(super) fn range_scope(
    parsed: &ParsedFile,
    marker: Span,
    proven: impl Fn(Span, StatementId) -> bool,
) -> Option<StatementId> {
    local_if::scope(parsed, marker, proven)
}

/// Only discard transparent syntax; callers still validate the published expression facts.
pub(super) fn ungroup(
    parsed: &ParsedFile,
    mut expression: ExpressionId,
    span: Span,
) -> Result<ExpressionId, LoweringError> {
    loop {
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Group { expression: inner } = node.payload() else {
            return Ok(expression);
        };
        expression = *inner;
    }
}

pub(super) fn declaration(parsed: &ParsedFile, marker: Span) -> Result<(), LoweringError> {
    let found = parsed.ast().items().iter().find(|(_, node)| {
        matches!(node.payload(), Item::Function { .. }) && contains(node.span(), marker)
    });
    let Some((_, node)) = found else {
        return Err(error(LoweringErrorKind::MissingFact, marker));
    };
    if !matches!(
        node.payload(),
        Item::Function {
            form: FunctionForm::Explicit {
                body: FunctionBody::Expression { .. },
                ..
            },
            ..
        }
    ) {
        return Err(error(LoweringErrorKind::UnsupportedNode, marker));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn binding(
    parsed: &ParsedFile,
    marker: Span,
    range: bool,
    range_prefixes: &[Span],
    allowed_controls: Option<&BTreeSet<usize>>,
    scope: Option<StatementId>,
    builtin: impl Fn(ExpressionId) -> Option<BuiltinType>,
    call_allowed: impl Fn(ExpressionId, CallAccess) -> bool,
    iteration_allowed: impl Fn(StatementId) -> bool,
) -> Result<(), LoweringError> {
    let function = parsed
        .ast()
        .items()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Item::Function { .. }) && contains(node.span(), marker)
        })
        .map(|(_, node)| node.span())
        .min_by_key(|span| span.end() - span.start())
        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, marker))?;
    for (expression, node) in parsed.ast().expressions().iter() {
        if allowed_controls.is_some_and(|controls| controls.contains(&expression.index())) {
            continue;
        }
        // Checked Int arithmetic uses the existing success/Abort edges and their
        // explicit carrier/root/source-loan transport; other borrow bindings keep
        // the original CFG frontier. Syntax alone is not a scalar type proof.
        if range
            && let Expression::Binary {
                operator,
                left,
                right,
                ..
            } = node.payload()
            && [*left, *right]
                .into_iter()
                .all(|operand| builtin(operand) == Some(BuiltinType::Int))
            && match operator {
                BinaryOperator::Add
                | BinaryOperator::Subtract
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Remainder => builtin(expression) == Some(BuiltinType::Int),
                // Comparisons do not create CFG edges. Both operand types and
                // the distinct Boolean result must come from typed facts.
                BinaryOperator::Equal
                | BinaryOperator::NotEqual
                | BinaryOperator::Less
                | BinaryOperator::LessEqual
                | BinaryOperator::Greater
                | BinaryOperator::GreaterEqual => builtin(expression) == Some(BuiltinType::Boolean),
                _ => false,
            }
        {
            continue;
        }
        if range
            && matches!(
                node.payload(),
                Expression::If { .. } | Expression::Return { .. }
            )
            && range_prefixes
                .iter()
                .any(|prefix| contains(*prefix, node.span()))
        {
            continue;
        }
        if range
            && scope.is_some_and(|scope| {
                local_if::allows(
                    parsed,
                    scope,
                    expression,
                    &builtin,
                    &|id| call_allowed(id, CallAccess::OwnedResult),
                    None,
                ) || short_circuit::allows(parsed, scope, expression, &builtin, &call_allowed)
            })
        {
            continue;
        }
        if contains(function, node.span())
            && matches!(
                node.payload(),
                Expression::If { .. }
                    | Expression::When { .. }
                    | Expression::Return { .. }
                    | Expression::Binary { .. }
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    for (statement, node) in parsed.ast().statements().iter() {
        if contains(function, node.span())
            && (matches!(node.payload(), Statement::Loop { .. })
                || (matches!(node.payload(), Statement::While { .. })
                    && !iteration_allowed(statement))
                || (!range
                    && matches!(node.payload(), Statement::For { .. })
                    && !iteration_allowed(statement)))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
}

/// Declared parameter identity; receivers remain outside the ordinary result slice.
pub(super) fn source_index(
    origin: lang_frontend::type_checking::BorrowReturnOrigin,
    span: Span,
) -> Result<usize, LoweringError> {
    match origin {
        lang_frontend::type_checking::BorrowReturnOrigin::Parameter(index) => Ok(index),
        _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

/// Reject checked unary operations in an ordinary result call's argument subtree.
pub(super) fn argument(parsed: &ParsedFile, span: Span) -> Result<(), LoweringError> {
    for (_, node) in parsed.ast().expressions().iter() {
        if contains(span, node.span())
            && matches!(
                node.payload(),
                Expression::NonNullAssert { .. }
                    | Expression::Prefix {
                        operator: lang_frontend::parser::PrefixOperator::Minus,
                        ..
                    }
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
}

pub(in crate::ssa) mod verify_range {
    //! N1a inline descriptor typing and NullableTake branch-proof contracts.
    use crate::ssa::{
        model::{
            CallableSignature, ClosureCaptureMode, ClosureCaptureOperand, EntityId, EntityType,
            Function, FunctionId, Instruction, LoanId, LoanKind, Module, Operation, SsaTypeId,
            SsaTypeKind, TerminatorKind, ValueId,
        },
        verify::{VerifyError, VerifyErrorKind, VerifyLocation},
        verify_operation::{is_first_class, single_value_result, value_type},
    };

    pub(in crate::ssa) mod nullable {
        use std::collections::{BTreeMap, BTreeSet};

        use crate::ssa::{
            model::{Definition, EntityId, Function, LoanId, TerminatorKind, ValueId},
            verify_ownership::edges,
        };

        /// Follow simultaneous owner and proof rebinding back to real NullableBranch births.
        /// Every path must preserve the pair; every cycle must have a reachable birth witness.
        pub(in crate::ssa) fn has_exact_branch_proof(
            function: &Function,
            owner: ValueId,
            proof: LoanId,
        ) -> bool {
            let mut pending = vec![(owner, proof)];
            let mut dependencies = BTreeMap::new();
            let mut grounded = BTreeSet::new();
            while let Some(pair @ (owner, proof)) = pending.pop() {
                if dependencies.contains_key(&pair) {
                    continue;
                }
                let Some(Definition::BlockParameter { block, index }) = function
                    .entity(EntityId::Loan(proof))
                    .map(|entity| entity.definition)
                else {
                    return false;
                };
                let Some(owner_definition) = function
                    .entity(EntityId::Value(owner))
                    .map(|entity| entity.definition)
                else {
                    return false;
                };
                let mut parents = Vec::new();
                let mut has_incoming = false;
                for source in &function.blocks {
                    let Some(terminator) = &source.terminator else {
                        return false;
                    };
                    for edge in edges(&terminator.kind)
                        .into_iter()
                        .filter(|edge| edge.target == block)
                    {
                        has_incoming = true;
                        let incoming_owner = match owner_definition {
                            Definition::BlockParameter {
                                block: owner_block,
                                index,
                            } if owner_block == block => {
                                let Some(EntityId::Value(value)) = edge.arguments.get(index) else {
                                    return false;
                                };
                                *value
                            }
                            _ => owner,
                        };
                        if let Some(EntityId::Loan(incoming_proof)) = edge.arguments.get(index) {
                            parents.push((incoming_owner, *incoming_proof));
                        } else if let TerminatorKind::NullableBranch {
                            owner: source_owner,
                            when_non_null,
                            view,
                            ..
                        } = &terminator.kind
                        {
                            if !std::ptr::eq(edge, when_non_null)
                                || *view != proof
                                || index != edge.arguments.len()
                                || incoming_owner != *source_owner
                            {
                                return false;
                            }
                            grounded.insert(pair);
                        } else {
                            return false;
                        }
                    }
                }
                if !has_incoming {
                    return false;
                }
                pending.extend(parents.iter().copied());
                dependencies.insert(pair, parents);
            }
            loop {
                let before = grounded.len();
                for (pair, parents) in &dependencies {
                    if parents.iter().any(|parent| grounded.contains(parent)) {
                        grounded.insert(*pair);
                    }
                }
                if grounded.len() == before {
                    return grounded.len() == dependencies.len();
                }
            }
        }
    }

    pub(in crate::ssa) fn parameter(
        module: &Module,
        function: &Function,
    ) -> Option<(LoanId, SsaTypeId, SsaTypeId)> {
        if function.receiver.is_some() || function.borrow_return.is_some() {
            return None;
        }
        let index = function.carrier_return?;
        let [view] = function.return_types.as_slice() else {
            return None;
        };
        let SsaTypeKind::RangeView { source } = module.type_kind(*view)? else {
            return None;
        };
        let entity = *function.blocks.first()?.parameters.get(index)?;
        let EntityId::Loan(loan) = entity else {
            return None;
        };
        (source_root(module, function, loan) == Some(*source)).then_some((loan, *view, *source))
    }

    fn source_root(module: &Module, function: &Function, loan: LoanId) -> Option<SsaTypeId> {
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = function.entity(EntityId::Loan(loan))?.ty
        else {
            return None;
        };
        Some(match module.type_kind(target)? {
            SsaTypeKind::RangeView { source } => *source,
            _ => target,
        })
    }

    pub(in crate::ssa) fn contract(
        module: &Module,
        function: &Function,
        operation: &Operation,
        results: &[EntityType],
    ) -> bool {
        let pair = |view, source| {
            results
                == [
                    EntityType::Value(view),
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: source,
                    },
                ]
        };
        let source_type = |loan| match function.entity(EntityId::Loan(loan)).map(|data| data.ty) {
            Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }) => Some(target),
            _ => None,
        };
        let int = |value| {
            value_type(function, value).is_some_and(|ty| {
                matches!(
                    module.type_kind(ty),
                    Some(SsaTypeKind::Integer {
                        bits: 32,
                        signed: true
                    })
                )
            })
        };
        match operation {
            Operation::RangeElementPlace { view, index } => {
                let Some(SsaTypeKind::RangeView { source }) =
                    source_type(*view).and_then(|ty| module.type_kind(ty))
                else {
                    return false;
                };
                matches!(module.type_kind(*source), Some(SsaTypeKind::SequentialContainer { element, .. }) if results == [EntityType::Place(*element)])
                    && int(*index)
            }
            Operation::RangeConstruct {
                view,
                source,
                begin,
                end,
            } => {
                matches!(module.type_kind(*view),Some(SsaTypeKind::RangeView { source: target }) if Some(*target)==source_root(module,function,*source) && pair(*view,*target))
                    && int(*begin)
                    && int(*end)
            }
            Operation::RangeCall {
                callee,
                arguments,
                source,
            } => {
                let Some(callee) = module.function(*callee) else {
                    return false;
                };
                let Some((_, view, root)) = parameter(module, callee) else {
                    return false;
                };
                let Some(entry) = callee.blocks.first() else {
                    return false;
                };
                arguments.get(callee.carrier_return.unwrap_or(usize::MAX))
                    == Some(&EntityId::Loan(*source))
                    && arguments.len() == entry.parameters.len()
                    && arguments.iter().zip(&entry.parameters).all(|(arg, param)| {
                        function.entity(*arg).map(|d| d.ty) == callee.entity(*param).map(|d| d.ty)
                    })
                    && pair(view, root)
            }
            Operation::RangeLength { view } => {
                let ty = match function.entity(*view).map(|d| d.ty) {
                    Some(
                        EntityType::Value(ty)
                        | EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: ty,
                        },
                    ) => ty,
                    _ => return false,
                };
                matches!(module.type_kind(ty), Some(SsaTypeKind::RangeView { .. }))
                    && single_value_result(results).is_some_and(|ty| {
                        matches!(
                            module.type_kind(ty),
                            Some(SsaTypeKind::Integer {
                                bits: 32,
                                signed: true
                            })
                        )
                    })
            }
            Operation::RangeEnd { view, source } => {
                results.is_empty()
                    && matches!(value_type(function,*view).and_then(|ty|module.type_kind(ty)),Some(SsaTypeKind::RangeView {source:target}) if Some(*target)==source_type(*source))
            }
            _ => false,
        }
    }

    pub(in crate::ssa) fn return_contract(
        module: &Module,
        function: &Function,
        view: ValueId,
        source: LoanId,
        block: crate::ssa::model::BlockId,
        origin: &crate::ssa::model::Origin,
        errors: &mut Vec<VerifyError>,
    ) {
        let valid = parameter(module, function).is_some_and(|(_, expected, root)| {
            value_type(function, view) == Some(expected)
                && function.entity(EntityId::Loan(source)).map(|d| d.ty)
                    == Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: root,
                    })
        });
        if !valid {
            errors.push(VerifyError {
                kind: VerifyErrorKind::ReturnType { index: 0 },
                location: VerifyLocation::Terminator(block),
                origin: Some(origin.clone()),
            });
        }
    }

    pub(in crate::ssa) fn verify_signature(
        module: &Module,
        function: &Function,
        errors: &mut Vec<VerifyError>,
    ) {
        let invalid = function.carrier_return.is_some() && parameter(module,function).is_none()
            || function.return_types.iter().any(|ty|matches!(module.type_kind(*ty),Some(SsaTypeKind::RangeView {..}))) && function.carrier_return.is_none() && function.borrow_return.is_none()
            || function.blocks.first().is_some_and(|entry| entry.parameters.iter().any(|p| matches!(function.entity(*p).map(|d|d.ty),Some(EntityType::Value(ty) | EntityType::Loan {kind:LoanKind::Exclusive,target:ty}) if matches!(module.type_kind(ty),Some(SsaTypeKind::RangeView {..})))))
            || function.blocks.iter().any(|block| matches!(block.terminator.as_ref().map(|t| &t.kind),Some(TerminatorKind::Return {..})) && function.carrier_return.is_some());
        if invalid {
            errors.push(VerifyError {
                kind: VerifyErrorKind::ReturnType { index: 0 },
                location: VerifyLocation::Function(function.id),
                origin: Some(function.origin.clone()),
            });
        }
    }

    pub(in crate::ssa) mod callable {
        //! callable 参数、结果、formation 与签名的一致性合同。
        use super::*;

        pub(in crate::ssa) fn direct_call_contract(
            module: &Module,
            function: &Function,
            instruction: &Instruction,
            results: &[EntityType],
        ) -> bool {
            let Operation::DirectCall {
                callee,
                receiver,
                ref arguments,
            } = instruction.operation
            else {
                return false;
            };
            let Some(callee) = module.function(callee) else {
                return false;
            };
            let Some(entry) = callee.blocks.first() else {
                return false;
            };
            if callee.borrow_return.is_some() || callee.carrier_return.is_some() {
                return false;
            }
            let Some(parameter_types) = entry
                .parameters
                .iter()
                .map(|parameter| callee.entity(*parameter).map(|data| data.ty))
                .collect::<Option<Vec<_>>>()
            else {
                return false;
            };
            let receiver_count = usize::from(callee.receiver.is_some());
            if parameter_types.len() != arguments.len() + receiver_count
                || callee.receiver.is_some() != receiver.is_some()
                || !parameter_types.iter().all(|ty| match ty {
                    EntityType::Value(ty) => is_first_class(module, *ty),
                    EntityType::Loan { target, .. } => is_first_class(module, *target),
                    EntityType::Place(_) => false,
                })
                || !callee
                    .return_types
                    .iter()
                    .all(|ty| is_first_class(module, *ty))
            {
                return false;
            }
            let receiver_matches =
                receiver
                    .zip(callee.receiver)
                    .is_none_or(|(receiver, expected)| {
                        !matches!(receiver, EntityId::Place(_))
                            && function.entity(receiver).map(|entity| entity.ty) == Some(expected)
                    });
            let arguments_match = arguments
                .iter()
                .zip(parameter_types.into_iter().skip(receiver_count))
                .all(|(argument, parameter)| {
                    !matches!(argument, EntityId::Place(_))
                        && function.entity(*argument).map(|entity| entity.ty) == Some(parameter)
                });
            let results_match = results
                == callee
                    .return_types
                    .iter()
                    .copied()
                    .map(EntityType::Value)
                    .collect::<Vec<_>>();
            receiver_matches && arguments_match && results_match
        }

        pub(in crate::ssa) fn function_address_contract(
            module: &Module,
            target: FunctionId,
            results: &[EntityType],
        ) -> bool {
            if module.function(target).is_none_or(|function| {
                function.borrow_return.is_some() || function.carrier_return.is_some()
            }) {
                return false;
            }
            let Some(result) = single_value_result(results) else {
                return false;
            };
            let Some(SsaTypeKind::FunctionPointer { signature }) = module.type_kind(result) else {
                return false;
            };
            function_matches_signature(module, target, signature, None)
        }

        pub(in crate::ssa) fn closure_construct_contract(
            module: &Module,
            function: &Function,
            closure: SsaTypeId,
            thunk: FunctionId,
            operands: &[ClosureCaptureOperand],
            results: &[EntityType],
        ) -> bool {
            let Some(SsaTypeKind::ConcreteClosure {
                signature,
                environment,
                captures,
                ..
            }) = module.type_kind(closure)
            else {
                return false;
            };
            if single_value_result(results) != Some(closure) || operands.len() != captures.len() {
                return false;
            }
            let captures_match = operands.iter().zip(captures).all(|(operand, capture)| {
                match (operand, capture.mode) {
                    (ClosureCaptureOperand::Owned(value), ClosureCaptureMode::Owned) => {
                        value_type(function, *value) == Some(capture.ty)
                    }
                    (ClosureCaptureOperand::Shared(loan), ClosureCaptureMode::Shared) => matches!(
                        function.entity(EntityId::Loan(*loan)).map(|entity| entity.ty),
                        Some(EntityType::Loan { kind: LoanKind::Shared, target }) if target == capture.ty
                    ),
                    _ => false,
                }
            });
            captures_match
                && function_matches_signature(module, thunk, signature, Some(*environment))
        }

        pub(in crate::ssa) fn callable_invoke_contract(
            module: &Module,
            function: &Function,
            callable: ValueId,
            arguments: &[EntityId],
            results: &[EntityType],
        ) -> bool {
            let Some(signature) =
                value_type(function, callable).and_then(|ty| module.callable_signature(ty))
            else {
                return false;
            };
            arguments.len() == signature.parameters.len()
                && arguments
                    .iter()
                    .zip(&signature.parameters)
                    .all(|(argument, expected)| {
                        !matches!(argument, EntityId::Place(_))
                            && function.entity(*argument).map(|entity| entity.ty) == Some(*expected)
                    })
                && results
                    == signature
                        .returns
                        .iter()
                        .copied()
                        .map(EntityType::Value)
                        .collect::<Vec<_>>()
        }

        /// ADR-0034: a shared loan of a function pointer or concrete closure re-borrowed as a shared
        /// loan of the callable view with the same signature.
        pub(in crate::ssa) fn view_of_contract(
            module: &Module,
            function: &Function,
            source: LoanId,
            results: &[EntityType],
        ) -> bool {
            let Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }) = function
                .entity(EntityId::Loan(source))
                .map(|entity| entity.ty)
            else {
                return false;
            };
            let Some(signature) = module.callable_signature(target) else {
                return false;
            };
            matches!(
                results,
                [EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: view,
                }] if module.callable_view_signature(*view) == Some(signature)
            )
        }

        /// ADR-0034: a call through a shared callable view loan.
        pub(in crate::ssa) fn invoke_view_contract(
            module: &Module,
            function: &Function,
            view: LoanId,
            arguments: &[EntityId],
            results: &[EntityType],
        ) -> bool {
            let Some(signature) = view_signature(module, function, view) else {
                return false;
            };
            arguments.len() == signature.parameters.len()
                && arguments
                    .iter()
                    .zip(&signature.parameters)
                    .all(|(argument, expected)| {
                        !matches!(argument, EntityId::Place(_))
                            && function.entity(*argument).map(|entity| entity.ty) == Some(*expected)
                    })
                && results
                    == signature
                        .returns
                        .iter()
                        .copied()
                        .map(EntityType::Value)
                        .collect::<Vec<_>>()
        }

        fn view_signature<'a>(
            module: &'a Module,
            function: &Function,
            view: LoanId,
        ) -> Option<&'a CallableSignature> {
            let EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            } = function.entity(EntityId::Loan(view))?.ty
            else {
                return None;
            };
            module.callable_view_signature(target)
        }

        pub(in crate::ssa) fn call_termination_contract(
            module: &Module,
            function: &Function,
            instruction: &Instruction,
        ) -> bool {
            !call_never_returns(module, function, &instruction.operation)
                || function.block(instruction.block).is_some_and(|block| {
                    block.instructions.last() == Some(&instruction.id)
                        && block
                            .terminator
                            .as_ref()
                            .is_some_and(|term| term.kind == TerminatorKind::Unreachable)
                })
        }

        pub(in crate::ssa) fn call_never_returns(
            module: &Module,
            function: &Function,
            operation: &Operation,
        ) -> bool {
            match *operation {
                Operation::DirectCall { callee, .. } => module
                    .function(callee)
                    .is_some_and(|callee| callee.never_returns),
                Operation::CallableInvoke { callable, .. } => value_type(function, callable)
                    .and_then(|ty| module.callable_signature(ty))
                    .is_some_and(|signature| signature.never_returns),
                Operation::CallableInvokeView { view, .. } => {
                    view_signature(module, function, view)
                        .is_some_and(|signature| signature.never_returns)
                }
                _ => false,
            }
        }

        pub(in crate::ssa) fn function_matches_signature(
            module: &Module,
            target: FunctionId,
            signature: &CallableSignature,
            environment: Option<SsaTypeId>,
        ) -> bool {
            let Some(function) = module.function(target) else {
                return false;
            };
            if function.receiver.is_some() || function.never_returns != signature.never_returns {
                return false;
            }
            let Some(entry) = function.blocks.first() else {
                return false;
            };
            let mut expected = environment
                .into_iter()
                .map(|target| EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                })
                .collect::<Vec<_>>();
            expected.extend(signature.parameters.iter().copied());
            entry.parameters.len() == expected.len()
                && entry
                    .parameters
                    .iter()
                    .zip(expected)
                    .all(|(parameter, ty)| {
                        function.entity(*parameter).map(|data| data.ty) == Some(ty)
                    })
                && function.return_types == signature.returns
        }
    }
}
