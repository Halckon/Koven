//! 普通 Borrow/Value 参数 shared 结果的准入，以及非 refine 路径的 Map 借用 root 绑定。
mod entry_string;
mod nullable_presence;

pub(in crate::ssa) use entry_string::forwards_string_entry;
use entry_string::string_entry_block_return;

use crate::ssa::borrow_result_support::{CallAccess, local_if, ordinary_local};
use crate::ssa::unit_lower::*;
use lang_frontend::ownership_checking::UnitPreflightFacts;
use lang_frontend::{
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, BorrowSourceLoan, OwnershipBindingKind,
        UnitLoanTarget,
    },
    type_checking::{
        BorrowReturnOrigin, CallableResultSource, IntrinsicTypeConstructor,
        UnitAggregateProjectionReceiver, UnitCallTarget, UnitCallableSignature,
    },
};

use super::{
    bounded_declaration, branch_declaration, multi_continuation_declaration, nested_declaration,
    single_continuation_declaration,
};
pub(in crate::ssa) fn validate_declarations(
    parsed: &[&lang_frontend::parser::ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
) -> Result<(), LoweringError> {
    for callable in typed
        .signatures()
        .declarations()
        .iter()
        .flat_map(|declaration| {
            declaration.callable().into_iter().chain(
                declaration.nominal().into_iter().flat_map(|nominal| {
                    nominal.members().iter().chain(nominal.companion_members())
                }),
            )
        })
    {
        if let Some(contract) = callable.borrow_return() {
            let file = parsed
                .iter()
                .find(|file| file.source_id() == contract.marker_span().source_id())
                .ok_or_else(|| {
                    lowering_error(LoweringErrorKind::MissingFact, contract.marker_span())
                })?;
            if string_entry_block_return(file, names, typed, owned, callable).is_none()
                && !branch_declaration::allows(file, names, typed, owned, callable)
                && !nested_declaration::allows(file, names, typed, owned, callable)
                && !bounded_declaration::allows(file, names, typed, owned, callable)
                && !single_continuation_declaration::allows(file, names, typed, owned, callable)
                && !multi_continuation_declaration::allows(file, names, typed, owned, callable)
            {
                crate::ssa::borrow_result_support::declaration(file, contract.marker_span())?;
            }
            if !matches!(contract.origin(), BorrowReturnOrigin::Parameter(index) if callable.parameters().get(index).is_some_and(|parameter| parameter.mode() == ParameterMode::Borrow))
                    || callable.receiver().is_some()
                    || callable.parameters().iter().any(|parameter| parameter.mode() == ParameterMode::Inout)
                    || !owned.borrow_return_origins().iter().any(|fact| {
                        fact.declaration_span() == contract.marker_span()
                            && matches!(fact.origin(), UnitLoanTarget::Place(place) if place.element().is_none())
                    })
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        contract.marker_span(),
                    ));
                }
        }
    }
    Ok(())
}

// These binding shapes have dedicated lowering checks. Other bindings still require
// the older whole-function CFG frontier, including RangeView and carrier aliases.
fn has_dedicated_binding_path(
    file: &lang_frontend::parser::ParsedFile,
    _names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    fact: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> Result<bool, LoweringError> {
    let UnitLoanTarget::Place(place) = fact.origin() else {
        return Ok(false);
    };
    if fact.storage() != BorrowBindingStorage::BorrowedStorage
        || fact.source_loan().is_some()
        || place.element().is_some()
    {
        return Ok(false);
    }
    let source_unit = fact.binding().source_unit();
    let parent = fact.parent();
    if !place.is_root() {
        if parent.is_some() {
            return Ok(false);
        }
        let expression = crate::ssa::borrow_result_support::ungroup(
            file,
            fact.initializer().expression(),
            fact.marker_span(),
        )?;
        return Ok(typed
                .aggregate_projection(UnitExpressionId::new(source_unit, expression))
                .is_some_and(|projection| {
                    matches!(projection.receiver(), UnitAggregateProjectionReceiver::Expression(receiver)
                        if receiver.source_unit() == source_unit)
                        && place.fields().last() == Some(&projection.field())
                }));
    }
    let root = place.root();
    let initializer = fact.initializer();
    let source_symbol = parent.unwrap_or(root);
    if root.source_unit() == source_unit
        && source_symbol.source_unit() == source_unit
        && initializer.source_unit() == source_unit
        && (owned.formed_owned_locals().binary_search(&root).is_ok()
            || matches!(
                owned.binding_kind(root),
                Some(OwnershipBindingKind::Owned | OwnershipBindingKind::Shared)
            )
            || (parent.is_some()
                && owned.binding_kind(root) == Some(OwnershipBindingKind::Exclusive)))
    {
        let expression = crate::ssa::borrow_result_support::ungroup(
            file,
            initializer.expression(),
            fact.marker_span(),
        )?;
        let source = UnitExpressionId::new(source_unit, expression);
        // The sealed frontend fact establishes the root. The reached lowerer
        // and verifier check storage and the actual loan extent.
        if let Some(ty) = typed.symbol_type(root)
            && typed.symbol_type(source_symbol) == Some(ty)
            && typed.expression_type(source) == Some(ty)
            && file
                .ast()
                .expressions()
                .get(expression)
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
        {
            if let Some(parent) = parent {
                if matches!(
                    typed.types().get(ty),
                    Some(UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::Map
                            | IntrinsicTypeConstructor::MutableMap,
                        ..
                    })
                ) {
                    return Ok(false);
                }
                // Unit publishes these facts ordered by their source-qualified binding.
                let bindings = owned.borrow_results().bindings();
                return Ok(bindings
                    .binary_search_by_key(&parent, |binding| binding.binding())
                    .is_ok_and(|index| {
                        let parent = &bindings[index];
                        parent.storage() == BorrowBindingStorage::BorrowedStorage
                            && parent.origin() == fact.origin()
                    }));
            }
            return Ok(true);
        }
    }
    if parent.is_some() {
        return Ok(false);
    }
    Ok(owned.map_iterations().iter().any(|plan| {
        let statement = plan.descriptor().statement();
        statement.source_unit() == source_unit
            && plan.source() == Some(fact.origin())
            && typed.map_iteration(statement).is_some()
    }))
}

// The lexical exception applies only to descriptor owners and direct metadata
// children that the refine range builder can recognize.
fn is_local_range_binding(
    file: &lang_frontend::parser::ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    fact: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> bool {
    let source_unit = fact.binding().source_unit();
    if fact.initializer().source_unit() != source_unit
        || !matches!(fact.origin(), UnitLoanTarget::Place(place) if place.is_root())
    {
        return false;
    }
    let Ok(expression) = crate::ssa::borrow_result_support::ungroup(
        file,
        fact.initializer().expression(),
        fact.marker_span(),
    ) else {
        return false;
    };
    let id = UnitExpressionId::new(source_unit, expression);
    let ty = typed.expression_type(id);
    if ty != typed.symbol_type(fact.binding())
        || !matches!(
            ty.and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                ..
            })
        )
    {
        return false;
    }
    match fact.storage() {
        BorrowBindingStorage::NewRangeDescriptor => {
            fact.source_loan().is_some_and(|loan| loan.call() == id)
                && typed.call(id).is_some_and(|call| {
                    matches!(call.result_source(), CallableResultSource::Carrier(_))
                })
        }
        BorrowBindingStorage::BorrowedCarrierMetadata if fact.source_loan().is_none() => {
            let Some(parent) = fact
                .parent()
                .filter(|parent| parent.source_unit() == source_unit)
            else {
                return false;
            };
            let bindings = owned.borrow_results().bindings();
            let Ok(index) = bindings.binary_search_by_key(&parent, |binding| binding.binding())
            else {
                return false;
            };
            let parent_fact = &bindings[index];
            parent_fact.storage() == BorrowBindingStorage::NewRangeDescriptor
                && parent_fact.origin() == fact.origin()
                && typed.symbol_type(parent) == ty
                && file.ast().expressions().get(expression).is_ok_and(|node| {
                    matches!(node.payload(), Expression::Name)
                        && names.names().references().iter().any(|reference| {
                            reference.source_unit() == source_unit
                                && reference.span() == node.span()
                                && reference.target() == &UnitReferenceTarget::Symbol(parent)
                        })
                })
        }
        _ => false,
    }
}

/// The canonical string operations observe the existing result loan. A return
/// may cross the binding only at a frontend-published loan end.
fn admit_string_result_observers(
    file: &lang_frontend::parser::ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    fact: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> Option<std::collections::BTreeSet<usize>> {
    use lang_frontend::parser::{BinaryOperator, WhenCondition};

    let mut allowed = std::collections::BTreeSet::new();
    let builtin = |expression| {
        let id = UnitExpressionId::new(fact.binding().source_unit(), expression);
        match typed
            .expression_type(id)
            .and_then(|ty| typed.types().get(ty))
        {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        }
    };
    if fact.storage() != BorrowBindingStorage::BorrowedStorage || fact.source_loan().is_none() {
        return None;
    }
    let string = builtin(fact.initializer().expression()) == Some(BuiltinType::String)
        && matches!(fact.origin(), UnitLoanTarget::Place(place) if place.is_root());
    if !string {
        return None;
    }
    let expressions = file.ast().expressions();
    let root = |expression| {
        crate::ssa::borrow_result_support::ungroup(file, expression, fact.marker_span()).ok()
    };
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == fact.binding().source_unit()
                && reference.target() == &UnitReferenceTarget::Symbol(fact.binding())
        })
        .map(|reference| (reference.span().start(), reference.span().end()))
        .collect::<std::collections::BTreeSet<_>>();
    let observes_binding = |expression| {
        root(expression)
            .and_then(|leaf| expressions.get(leaf).ok())
            .is_some_and(|node| {
                matches!(node.payload(), Expression::Name)
                    && references.contains(&(node.span().start(), node.span().end()))
            })
    };
    let comparison = |expression| {
        builtin(expression) == Some(BuiltinType::Boolean)
            && root(expression)
                .and_then(|leaf| expressions.get(leaf).ok())
                .is_some_and(|node| {
                    matches!(
                        node.payload(),
                        Expression::Binary {
                            operator: BinaryOperator::Equal | BinaryOperator::NotEqual,
                            left,
                            right,
                            ..
                        } if builtin(*left) == Some(BuiltinType::String)
                            && builtin(*right) == Some(BuiltinType::String)
                            && (observes_binding(*left) || observes_binding(*right))
                    )
                })
    };
    for (expression, node) in expressions.iter() {
        let admitted = match node.payload() {
            Expression::Binary { .. } => comparison(expression),
            Expression::If { condition, .. } => comparison(*condition),
            Expression::When { subject: Some(subject), entries, .. } => {
                builtin(*subject) == Some(BuiltinType::String)
                    && observes_binding(*subject)
                    && entries.iter().all(|entry| {
                        entry.conditions.iter().all(|condition| {
                            matches!(condition, WhenCondition::Expression(selector)
                                if root(*selector)
                                    .and_then(|leaf| expressions.get(leaf).ok())
                                    .is_some_and(|node| matches!(node.payload(), Expression::String { .. })))
                        })
                    })
            }
            _ => false,
        };
        if admitted {
            allowed.insert(expression.index());
        }
    }
    Some(allowed)
}

fn extend_observer_returns(
    file: &lang_frontend::parser::ParsedFile,
    owned: &dyn UnitPreflightFacts,
    fact: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
    allowed: &mut std::collections::BTreeSet<usize>,
) {
    if !allowed.is_empty() {
        allowed.extend(
            owned
                .borrow_results()
                .ends()
                .iter()
                .filter(|end| end.binding() == fact.binding())
                .filter_map(|end| match end.point() {
                    UnitDropPoint::ControlTransfer(expression)
                        if expression.source_unit() == fact.binding().source_unit()
                            && file
                                .ast()
                                .expressions()
                                .get(expression.expression())
                                .is_ok_and(|node| {
                                    matches!(node.payload(), Expression::Return { .. })
                                }) =>
                    {
                        Some(expression.expression().index())
                    }
                    _ => None,
                }),
        );
    }
}

fn validate_binding(
    file: &lang_frontend::parser::ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    fact: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
    mut allowed_controls: Option<std::collections::BTreeSet<usize>>,
    observer_whiles: &std::collections::BTreeSet<usize>,
) -> Result<(), LoweringError> {
    if file.source_id() != fact.marker_span().source_id() {
        return Err(lowering_error(
            LoweringErrorKind::MissingFact,
            fact.marker_span(),
        ));
    }
    let prefixes = owned
        .borrow_results()
        .range_uses()
        .iter()
        .filter_map(|use_fact| {
            let lang_frontend::ownership_checking::RangeUseSite::Call(call) = use_fact.site()
            else {
                return None;
            };
            (use_fact.origin() == fact.origin()
                && call.source_unit() == fact.binding().source_unit())
            .then(|| {
                file.ast()
                    .expressions()
                    .get(call.expression())
                    .ok()
                    .map(|node| node.span())
            })
            .flatten()
        })
        .collect::<Vec<_>>();
    let scope = crate::ssa::borrow_result_support::range_scope(
        file,
        fact.marker_span(),
        |marker, scope| {
            let expected_end =
                UnitDropPoint::AfterStatement(lang_frontend::type_checking::UnitStatementId::new(
                    fact.binding().source_unit(),
                    scope,
                ));
            let Some(other) = owned
                .borrow_results()
                .bindings()
                .iter()
                .find(|other| other.marker_span() == marker)
            else {
                return false;
            };
            let mut ends = owned
                .borrow_results()
                .ends()
                .iter()
                .filter(|end| end.binding() == other.binding());
            other.storage()
                == lang_frontend::ownership_checking::BorrowBindingStorage::NewRangeDescriptor
                && other.origin() == fact.origin()
                && other.source_loan().is_some()
                && ends.next().is_some_and(|end| end.point() == expected_end)
                && ends.next().is_none()
        },
    );
    let builtin = |expression| match typed
        .expression_type(UnitExpressionId::new(
            fact.binding().source_unit(),
            expression,
        ))
        .and_then(|ty| typed.types().get(ty))
    {
        Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
        _ => None,
    };
    let call_allowed = |expression, access| {
        typed
            .call(UnitExpressionId::new(
                fact.binding().source_unit(),
                expression,
            ))
            .is_some_and(|call| {
                let mode_allowed = |mode, ty| {
                    mode == ParameterMode::Borrow
                        || (mode == ParameterMode::Value
                            && typed.copyability(ty)
                                == lang_frontend::type_checking::Copyability::Copyable)
                };
                call.result_source() == lang_frontend::type_checking::CallableResultSource::Owned
                    && (access == CallAccess::OwnedResult
                        || (!call.aborts()
                            && !matches!(
                                call.target(),
                                lang_frontend::type_checking::UnitCallTarget::FunctionValue
                            )
                            && call.arguments().iter().all(|argument| {
                                !argument.crosses_thread()
                                    && mode_allowed(argument.mode(), argument.parameter_type())
                            })
                            && call.receiver().is_none_or(|receiver| {
                                mode_allowed(receiver.mode(), receiver.ty())
                            })))
            })
    };
    if let Some(allowed) = &mut allowed_controls
        && matches!(
            fact.storage(),
            BorrowBindingStorage::NewRangeDescriptor
                | BorrowBindingStorage::BorrowedCarrierMetadata
        )
    {
        local_if::return_controls(
            file,
            fact.marker_span(),
            &builtin,
            &|id| call_allowed(id, CallAccess::ReadOnly),
            |returned| {
                let point = UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                    fact.binding().source_unit(),
                    returned,
                ));
                owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|end| end.binding() == fact.binding() && end.point() == point)
            },
            allowed,
        );
    }
    let invariant = crate::ssa::borrow_result_support::invariant_while::admitted(
        file, names, typed, owned, fact,
    );
    if let Some(statement) = invariant
        && let Some(tail) =
            crate::ssa::borrow_result_support::invariant_while::conditional_jump(file, statement)
        && let Some(allowed) = &mut allowed_controls
    {
        allowed.insert(tail.control.index());
    }
    if let Some(statement) = invariant
        && let Some(returned) =
            crate::ssa::borrow_result_support::invariant_while::tail_return(file, statement)
        && let Some(allowed) = &mut allowed_controls
    {
        allowed.insert(returned.index());
    }
    let branch_no_carry =
        crate::ssa::borrow_result_support::branch_no_carry::admitted(file, owned, fact);
    if let Some((_, control)) = branch_no_carry {
        allowed_controls
            .get_or_insert_with(Default::default)
            .insert(control.index());
        for tail in owned.no_carry_break_branches().iter().filter(|b| {
            owned.owns_no_carry_break_branch(b)
                && b.pair().branch().expression() == control
                && b.pair().branch().source_unit() == fact.binding().source_unit()
        }) {
            allowed_controls
                .get_or_insert_with(Default::default)
                .insert(tail.break_expression().expression().index());
        }
        for prefix in owned.no_carry_break_prefixes().iter().filter(|p| {
            owned.owns_no_carry_break_prefix(p)
                && p.pair().pair().branch().expression() == control
                && p.prefix().source_unit() == fact.binding().source_unit()
        }) {
            allowed_controls
                .get_or_insert_with(Default::default)
                .insert(prefix.prefix().expression().index());
        }
        for cb in
            crate::ssa::borrow_result_support::branch_no_carry::counter_breaks(owned).filter(|b| {
                crate::ssa::borrow_result_support::branch_no_carry::owns_counter_break(owned, b)
                    && b.counter().pair().branch().expression() == control
                    && b.counter().pair().branch().source_unit() == fact.binding().source_unit()
            })
        {
            let allowed = allowed_controls.get_or_insert_with(Default::default);
            allowed.insert(cb.break_control().expression().index());
            allowed.insert(cb.break_expression().expression().index());
            allowed.insert(cb.counter().condition().expression().index());
            allowed.insert(cb.counter().decrement().expression().index());
        }
        for prefix in owned.no_carry_prefix_counter_breaks().iter().filter(|p| {
            owned.owns_no_carry_prefix_counter_break(p)
                && p.counter_break().counter().pair().branch().expression() == control
                && p.prefix().source_unit() == fact.binding().source_unit()
        }) {
            allowed_controls
                .get_or_insert_with(Default::default)
                .insert(prefix.prefix().expression().index());
        }
        // Only the separately owned bounded fact admits its exact guard/update.
        // Other binary expressions retain the ordinary borrow-binding frontier.
        for counter in owned.no_carry_counter_branches().iter().filter(|c| {
            owned.owns_no_carry_counter_branch(c)
                && c.pair().branch().expression() == control
                && c.pair().branch().source_unit() == fact.binding().source_unit()
        }) {
            let allowed = allowed_controls.get_or_insert_with(Default::default);
            allowed.insert(counter.condition().expression().index());
            allowed.insert(counter.decrement().expression().index());
        }
    }
    #[cfg(any(
        feature = "two-source-unit",
        all(test, feature = "e2-two-source-test-support")
    ))]
    let experimental = crate::ssa::two_source_binding::admitted(file, owned, fact);
    #[cfg(any(
        feature = "two-source-unit",
        all(test, feature = "e2-two-source-test-support")
    ))]
    if let Some((controls, _)) = experimental {
        allowed_controls
            .get_or_insert_with(Default::default)
            .extend(controls.map(|id| id.index()));
    }
    let no_carry = crate::ssa::borrow_result_support::no_carry::admitted(file, owned, fact);
    let no_carry_siblings =
        crate::ssa::borrow_result_support::no_carry::siblings(file, owned, fact);
    let closed_whiles =
        crate::ssa::borrow_result_support::closed_while::following(file, typed, owned, fact);
    crate::ssa::borrow_result_support::binding(
        file,
        fact.marker_span(),
        fact.storage()
            == lang_frontend::ownership_checking::BorrowBindingStorage::NewRangeDescriptor,
        &prefixes,
        allowed_controls.as_ref(),
        scope,
        builtin,
        call_allowed,
        |statement| {
            #[cfg(any(
                feature = "two-source-unit",
                all(test, feature = "e2-two-source-test-support")
            ))]
            if experimental.is_some_and(|(_, approved)| approved == statement) {
                return true;
            }
            if observer_whiles.contains(&statement.index())
                || closed_whiles.contains(&statement)
                || invariant == Some(statement)
                || no_carry == Some(statement)
                || branch_no_carry.is_some_and(|(s, _)| s == statement)
                || no_carry_siblings.contains(&statement)
            {
                return true;
            }
            let statement = UnitStatementId::new(fact.binding().source_unit(), statement);
            owned.map_iterations().iter().any(|plan| {
                plan.descriptor().statement() == statement && plan.source() == Some(fact.origin())
            })
        },
    )?;
    if !matches!(fact.origin(), UnitLoanTarget::Place(place) if place.element().is_none()) {
        return Err(lowering_error(
            LoweringErrorKind::UnsupportedNode,
            fact.marker_span(),
        ));
    }
    Ok(())
}

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa) fn lower_result_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, initializer);
        let fact = self
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        // Only direct root Borrow parameter aliases remain here (SPEC-0393).
        let origin = match fact.origin() {
            UnitLoanTarget::Place(origin)
                if fact.storage() == BorrowBindingStorage::BorrowedStorage
                    && fact.source_loan().is_none()
                    && fact.parent().is_none()
                    && origin.is_root() =>
            {
                origin
            }
            _ => {
                // Preserve the legacy diagnostic for shapes that cannot lower on this route.
                validate_binding(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    fact,
                    None,
                    &Default::default(),
                )?;
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        if self.direct_name_symbol(initializer, span)? != Some(origin.root()) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(source) = self.borrow_bindings.get(&origin.root()).copied() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target: self.expression_ssa_type(initializer, span)?,
        };
        if self
            .function
            .entity(EntityId::Loan(source))
            .map(|data| data.ty)
            != Some(ty)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::SharedReborrow { source },
                vec![ty],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.borrow_bindings.insert(symbol, loan);
        Ok(LoweredValue::Unit)
    }

    pub(in crate::ssa) fn end_result_bindings(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let ends = self
            .owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|fact| fact.point() == point)
            .copied()
            .collect::<Vec<_>>();
        for fact in ends {
            let Some(loan) = self.borrow_bindings.remove(&fact.binding()) else {
                continue;
            };
            let span = self
                .function
                .entity(EntityId::Loan(loan))
                .ok_or_else(|| {
                    lowering_error(LoweringErrorKind::MissingFact, self.function.origin.span())
                })?
                .origin
                .span();
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        Ok(())
    }
}

pub(crate) fn validate_bindings(
    parsed: &[&lang_frontend::parser::ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    legacy_gate: bool,
) -> Result<(), LoweringError> {
    let bindings: Vec<_> = owned
        .borrow_results()
        .bindings()
        .iter()
        .map(|fact| {
            let file = parsed
                .iter()
                .copied()
                .find(|file| file.source_id() == fact.marker_span().source_id());
            let policy = file.and_then(|file| ordinary_local::policy(file, typed, owned, fact));
            (fact, file, policy)
        })
        .collect();
    // Each initializer keeps its own proof. A sibling may use it only through
    // the ordinary lexical disjointness walk.
    let argument_controls = bindings
        .iter()
        .filter_map(|&(fact, file, policy)| {
            if policy != Some(ordinary_local::Policy::Scoped) {
                return None;
            }
            let controls = ordinary_local::argument_returns(file?, typed, owned, fact, false)?;
            Some((fact.initializer(), controls))
        })
        .collect();
    let mut nullable = nullable_presence::Observers::new(typed, owned, &bindings);
    for (fact, file, policy) in bindings {
        let file =
            file.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, fact.marker_span()))?;
        match policy {
            Some(ordinary_local::Policy::Scoped) => {
                let (mut observers, loops) = nullable
                    .take(file, names, typed, owned, fact)
                    .unwrap_or_default();
                extend_observer_returns(file, owned, fact, &mut observers);
                let allowed = ordinary_local::allowed_controls(
                    file,
                    typed,
                    owned,
                    fact,
                    &argument_controls,
                    &observers,
                );
                validate_binding(file, names, typed, owned, fact, Some(allowed), &loops)?;
            }
            Some(ordinary_local::Policy::NarrowedWhen) => {
                ordinary_local::validate_narrowed_when_initializer(file, typed, owned, fact)?;
            }
            None if legacy_gate
                && !has_dedicated_binding_path(file, names, typed, owned, fact)? =>
            {
                let allowed = is_local_range_binding(file, names, typed, owned, fact).then(|| {
                    ordinary_local::allowed_controls(
                        file,
                        typed,
                        owned,
                        fact,
                        &argument_controls,
                        &Default::default(),
                    )
                });
                validate_binding(
                    file,
                    names,
                    typed,
                    owned,
                    fact,
                    allowed,
                    &Default::default(),
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}
