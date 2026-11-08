//! source-qualified 普通 Borrow/Value 参数 shared 结果；非 refine 路径保留 Map 借用 root 绑定。
use super::*;
use lang_frontend::{
    ownership_checking::{BorrowBindingFact, UnitLoanTarget},
    type_checking::BorrowReturnOrigin,
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

fn validate_binding(
    file: &lang_frontend::parser::ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    fact: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
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
    crate::ssa::borrow_result_support::binding(
        file,
        fact.marker_span(),
        fact.storage()
            == lang_frontend::ownership_checking::BorrowBindingStorage::NewRangeDescriptor,
        &prefixes,
        scope,
        |expression| match typed
            .expression_type(UnitExpressionId::new(
                fact.binding().source_unit(),
                expression,
            ))
            .and_then(|ty| typed.types().get(ty))
        {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        },
        |expression, access| {
            typed
                .call(UnitExpressionId::new(
                    fact.binding().source_unit(),
                    expression,
                ))
                .is_some_and(|call| {
                    use crate::ssa::borrow_result_support::CallAccess;
                    let mode_allowed = |mode, ty| {
                        mode == ParameterMode::Borrow
                            || (mode == ParameterMode::Value
                                && typed.copyability(ty)
                                    == lang_frontend::type_checking::Copyability::Copyable)
                    };
                    call.result_source()
                        == lang_frontend::type_checking::CallableResultSource::Owned
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
        },
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
                if fact.storage()
                    == lang_frontend::ownership_checking::BorrowBindingStorage::BorrowedStorage
                    && fact.source_loan().is_none()
                    && fact.parent().is_none()
                    && origin.is_root() =>
            {
                origin
            }
            _ => {
                // Preserve the legacy diagnostic for shapes that cannot lower on this route.
                validate_binding(self.parsed, self.names, self.typed, self.owned, fact)?;
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

#[cfg(test)]
pub(crate) fn validate_bindings_for_test(
    parsed: &[&lang_frontend::parser::ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), LoweringError> {
    for fact in owned.borrow_results().bindings() {
        let file = parsed
            .iter()
            .find(|file| file.source_id() == fact.marker_span().source_id())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, fact.marker_span()))?;
        validate_binding(file, names, typed, owned, fact)?;
    }
    Ok(())
}
