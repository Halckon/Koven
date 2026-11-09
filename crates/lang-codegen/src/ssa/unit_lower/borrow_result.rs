//! 普通 Borrow/Value 参数 shared 结果的准入，以及非 refine 路径的 Map 借用 root 绑定。
use super::*;
use crate::ssa::borrow_result_support::{CallAccess, local_if, ordinary_local};
use lang_frontend::{
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, OwnershipBindingKind, UnitLoanTarget,
    },
    type_checking::{
        BorrowReturnOrigin, CallableResultSource, IntrinsicTypeConstructor,
        UnitAggregateProjectionReceiver,
    },
};

pub(super) fn validate_declarations(
    parsed: &[&lang_frontend::parser::ParsedFile],
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
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
            crate::ssa::borrow_result_support::declaration(file, contract.marker_span())?;
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
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
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
            ))
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
            && super::map_iteration::stable_borrow_provider(
                file,
                names,
                typed,
                owned,
                statement,
                fact.binding(),
            )
    }))
}

// The lexical exception applies only to descriptor owners and direct metadata
// children that the refine range builder can recognize.
fn is_local_range_binding(
    file: &lang_frontend::parser::ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
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

fn validate_binding(
    file: &lang_frontend::parser::ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    fact: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
    mut allowed_controls: Option<std::collections::BTreeSet<usize>>,
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
            let statement = UnitStatementId::new(fact.binding().source_unit(), statement);
            super::map_iteration::stable_borrow_provider(
                file,
                names,
                typed,
                owned,
                statement,
                fact.binding(),
            )
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
    pub(super) fn lower_result_binding(
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
                validate_binding(self.parsed, self.names, self.typed, self.owned, fact, None)?;
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
        self.result_source_loans.insert(symbol, Vec::new());
        Ok(LoweredValue::Unit)
    }

    pub(super) fn end_result_bindings(
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
            self.result_source_loans.remove(&fact.binding());
        }
        Ok(())
    }
}

pub(crate) fn validate_bindings(
    parsed: &[&lang_frontend::parser::ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
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
            let controls = ordinary_local::argument_returns(file?, typed, owned, fact)?;
            Some((fact.initializer(), controls))
        })
        .collect();
    for (fact, file, policy) in bindings {
        let file =
            file.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, fact.marker_span()))?;
        match policy {
            Some(ordinary_local::Policy::Scoped) => {
                let allowed =
                    ordinary_local::allowed_controls(file, owned, fact, &argument_controls);
                validate_binding(file, names, typed, owned, fact, Some(allowed))?;
            }
            None if legacy_gate
                && !has_dedicated_binding_path(file, names, typed, owned, fact)? =>
            {
                let allowed = is_local_range_binding(file, names, typed, owned, fact).then(|| {
                    ordinary_local::allowed_controls(file, owned, fact, &argument_controls)
                });
                validate_binding(file, names, typed, owned, fact, allowed)?;
            }
            _ => {}
        }
    }
    Ok(())
}
