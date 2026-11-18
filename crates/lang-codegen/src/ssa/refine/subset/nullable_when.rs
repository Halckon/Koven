//! Nullable When admission consumes the existing same-analysis ownership plan.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{
        BorrowBindingStorage, LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint, UnitLoanFact,
        UnitLoanTarget,
    },
    parser::{AssignmentOperator, Expression},
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, NullableWhenSubjectCategory, ParameterMode,
        UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    /// Presence through a MoveOnly loan does not mint an owned payload value.
    pub(super) fn borrowed_nullable_presence(
        &self,
        symbol: lang_frontend::name_resolution::UnitSymbolId,
        nullable: UnitTypeId,
    ) -> Option<bool> {
        let mode = self.parameter_mode(symbol);
        let lexical = mode.is_none()
            && self.inputs.is_nullable_string(nullable)
            && self.for_borrowed.contains(&symbol)
            && self
                .inputs
                .owned
                .borrow_results()
                .bindings()
                .binary_search_by_key(&symbol, |binding| binding.binding())
                .is_ok_and(|index| {
                    self.inputs.owned.borrow_results().bindings()[index].storage()
                        == BorrowBindingStorage::BorrowedStorage
                });
        ((lexical || matches!(mode, Some(ParameterMode::Borrow | ParameterMode::Inout)))
            && self.copyability_of(nullable) == Copyability::MoveOnly)
            .then(|| {
                matches!(
                    self.kind(nullable),
                    Some(UnitTypeKind::Nullable(inner))
                        if self.map_kv(*inner).is_none()
                            && self.view_inner(*inner)
                            && (mode == Some(ParameterMode::Borrow)
                                || matches!(self.kind(*inner), Some(UnitTypeKind::Builtin(BuiltinType::String)))
                                || self.rc_payload(*inner).is_some())
                )
            })
    }

    /// Admit only an existing pending-drop fact whose real transfer is inside this
    /// owned nullable When and whose active lender encloses that same control.
    pub(super) fn nullable_when_abandons_lender(
        &self,
        call: lang_frontend::type_checking::UnitExpressionId,
        transfer: lang_frontend::type_checking::UnitExpressionId,
    ) -> bool {
        if call.source_unit() != self.inputs.source_unit
            || transfer.source_unit() != self.inputs.source_unit
        {
            return false;
        }
        let ast = self.inputs.parsed.ast();
        let (Ok(lender), Ok(jump)) = (
            ast.expressions().get(call.expression()),
            ast.expressions().get(transfer.expression()),
        ) else {
            return false;
        };
        let encloses = |outer: lang_frontend::source::Span, inner: lang_frontend::source::Span| {
            outer.source_id() == inner.source_id()
                && outer.start() <= inner.start()
                && inner.end() <= outer.end()
        };
        self.inputs.typed.nullable_whens().iter().any(|descriptor| {
            let inner = self.c(descriptor.inner_type());
            descriptor.expression().source_unit() == self.inputs.source_unit
                && (self.class(inner)
                    || self.box_payload(inner).is_some()
                    || self.rc_payload(inner).is_some())
                && super::super::nullable_when::source(self.inputs, descriptor.expression())
                    .is_ok_and(|source| source.loan_kind.is_none())
                && ast
                    .expressions()
                    .get(descriptor.expression().expression())
                    .is_ok_and(|control| encloses(lender.span(), control.span()))
                && descriptor.entries().iter().any(|entry| {
                    ast.statements()
                        .get(entry.body().statement())
                        .is_ok_and(|body| encloses(body.span(), jump.span()))
                })
        })
    }

    /// The frontend supplied the temporary's transfer drop and the same call's
    /// source-prefix end. The earlier auxiliary loan is ended by ordinary return cleanup.
    pub(super) fn pending_nullable_when_rc_argument(
        &self,
        expression: UnitExpressionId,
        transfer: UnitExpressionId,
        loan: &UnitLoanFact,
    ) -> bool {
        let UnitLoanTarget::Temporary(temporary) = loan.target() else {
            return false;
        };
        let call = loan.call();
        let argument = loan.argument();
        if loan.kind() != LoanKind::Shared
            || *temporary != argument
            || [call, argument, expression, transfer]
                .iter()
                .any(|id| id.source_unit() != self.inputs.source_unit)
            || self.ungrouped(argument.expression()) != self.ungrouped(expression.expression())
            || self.inputs.borrow_return.is_some()
            || self.inputs.carrier_return.is_some()
        {
            return false;
        }
        let Some(ty) = self.expression_type(argument.expression()) else {
            return false;
        };
        if self.rc_payload(ty).is_none()
            || !self
                .inputs
                .owned
                .call_argument_contracts_for(call)
                .any(|contract| {
                    contract.argument() == argument
                        && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        && contract.category() == ExpressionCategory::Temporary
                        && self.c(contract.parameter_type()) == ty
                        && !contract.crosses_thread()
                })
        {
            return false;
        }

        let expressions = self.inputs.parsed.ast().expressions();
        let (Ok(lender), Ok(returned)) = (
            expressions.get(call.expression()),
            expressions.get(transfer.expression()),
        ) else {
            return false;
        };
        let Expression::Call { arguments, .. } = lender.payload() else {
            return false;
        };
        let Some(index) = arguments
            .iter()
            .position(|arg| self.id(arg.value) == argument)
        else {
            return false;
        };
        arguments[index + 1..]
            .iter()
            .any(|arg| crate::ssa::unit_lower::span_contains(arg.span, returned.span()))
            && self
                .inputs
                .owned
                .borrow_call_prefix_ends()
                .iter()
                .any(|end| {
                    end.point() == UnitDropPoint::ControlTransfer(transfer)
                        && end.loan().call() == call
                        && end.loan().argument() != argument
                        && self
                            .inputs
                            .owned
                            .nullable_when_use(end.loan().argument())
                            .is_some_and(|fact| fact.loan() == Some(end.loan()))
                })
    }

    /// A real owned When view does not grant the legacy-forbidden loop crossing.
    pub(super) fn when_proof_crosses_loop(
        &self,
        statement: lang_frontend::ast::StatementId,
    ) -> bool {
        use lang_frontend::parser::Statement;
        let ast = self.inputs.parsed.ast();
        let Ok(node) = ast.statements().get(statement) else {
            return false;
        };
        if !matches!(
            node.payload(),
            Statement::While { .. } | Statement::Loop { .. } | Statement::For { .. }
        ) {
            return false;
        }
        let span = node.span();
        self.inputs.typed.nullable_whens().iter().any(|descriptor| {
            descriptor.expression().source_unit() == self.inputs.source_unit
                && descriptor
                    .stable_symbol()
                    .is_some_and(|root| self.view_symbols.contains(&root))
                && descriptor.entries().get(1).is_some_and(|entry| {
                    ast.statements()
                        .get(entry.body().statement())
                        .is_ok_and(|body| {
                            let scope = body.span();
                            scope.source_id() == span.source_id()
                                && scope.start() <= span.start()
                                && span.end() <= scope.end()
                        })
                })
        })
    }

    /// A When use has its own completed ownership contract, not an if-use adaptation.
    pub(super) fn nullable_when_argument(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        mode: ParameterMode,
        parameter: lang_frontend::type_checking::UnitTypeId,
    ) -> Option<bool> {
        use lang_frontend::ownership_checking::UnitNullableWhenUseKind;
        let witness = self.inputs.typed.nullable_when_use(self.id(argument))?;
        if self.copyability_of(self.c(witness.inner_type())) == Copyability::Copyable {
            return None;
        }
        Some(
            self.inputs
                .owned
                .nullable_when_use(self.id(argument))
                .is_some_and(|fact| {
                    fact.call() == self.id(call)
                        && fact.argument() == self.id(argument)
                        && fact.witness() == witness
                        && self.known.contains(&witness.symbol())
                        && self.view_symbols.contains(&witness.symbol())
                        && (self.c(witness.inner_type()) == parameter
                            || super::super::nullable_when::widens_string_argument(
                                self.inputs,
                                witness,
                                mode,
                                parameter,
                            ))
                        && matches!(
                            (mode, fact.kind()),
                            (ParameterMode::Borrow, UnitNullableWhenUseKind::Observe)
                                | (ParameterMode::Value, UnitNullableWhenUseKind::Consume)
                        )
                }),
        )
    }

    pub(super) fn nullable_when(&mut self, expression: ExpressionId) -> bool {
        let Ok(source) = super::super::nullable_when::source(self.inputs, self.id(expression))
        else {
            return false;
        };
        let descriptor = source.plan.descriptor();
        let Some(result_type) = self.expression_type(expression) else {
            return false;
        };
        let nullable = self.c(descriptor.nullable_type());
        let inner = self.c(descriptor.inner_type());
        let presence_only =
            super::super::owned_string_presence_only(self.inputs.typed, source.plan);
        let supported = if source.copyable_scalar {
            self.copyability_of(nullable) == Copyability::Copyable
                && self.copyability_of(inner) == Copyability::Copyable
        } else {
            self.copyability_of(nullable) == Copyability::MoveOnly
                && self.copyability_of(inner) == Copyability::MoveOnly
                && (self.class(inner)
                    || self.box_payload(inner).is_some()
                    || self.rc_payload(inner).is_some()
                    || source.loan_kind.is_some() && self.string(inner)
                    || presence_only)
        };
        if !supported
            || !matches!(self.kind(nullable), Some(UnitTypeKind::Nullable(actual)) if self.c(*actual) == inner)
            || !self.storage_type(nullable)
            || !(self.unit(result_type)
                || self.nothing(result_type)
                || self.copyability_of(result_type) == Copyability::Copyable
                    && self.storage_type(result_type))
        {
            return false;
        }
        if source.loan_kind.is_some()
            || descriptor.source_category() == NullableWhenSubjectCategory::OwnedRoot
        {
            let Some(root) = descriptor.stable_symbol() else {
                return false;
            };
            let mode = self.parameter_mode(root);
            if !self.known.contains(&root)
                || self.view_symbols.contains(&root)
                || !matches!(
                    (source.loan_kind, mode),
                    (
                        Some(crate::ssa::model::LoanKind::Shared),
                        Some(ParameterMode::Borrow)
                    ) | (
                        Some(crate::ssa::model::LoanKind::Exclusive),
                        Some(ParameterMode::Inout)
                    ) | (None, None | Some(ParameterMode::Value))
                )
            {
                return false;
            }
        }
        // A shared subject is a validated transparent root read; lowering branches
        // directly on its loan slot rather than requesting a MoveOnly Value.
        if source.loan_kind.is_none() {
            let Some(leaf) = source.operands.last() else {
                return false;
            };
            if !self.expression(leaf.expression()) {
                return false;
            }
        }
        self.note_nullable(nullable);
        self.migrated = true;
        let outer = self.view_symbols.clone();
        let mut common = outer.clone();
        for (index, entry) in descriptor.entries().iter().enumerate() {
            self.view_symbols.clone_from(&outer);
            if !source.copyable_scalar
                && !presence_only
                && index == 1
                && let Some(root) = descriptor.stable_symbol()
            {
                self.view_symbols.insert(root);
            }
            let admitted = self.statement(entry.body().statement());
            if entry.falls_through() {
                common.retain(|root| self.view_symbols.contains(root));
            }
            self.view_symbols.clone_from(&outer);
            if !admitted {
                return false;
            }
        }
        // Per-operand String reads were checked while the corresponding view was active.
        if !source.copyable_scalar
            && self
                .inputs
                .typed
                .nullable_when_uses()
                .iter()
                .filter(|use_| use_.control() == descriptor.expression())
                .any(|use_| {
                    let reference = use_.reference();
                    let direct_call = source.plan.uses().iter().any(|fact| {
                        fact.reference() == reference && fact.witness().control() == use_.control()
                    });
                    let exclusive_operation =
                        source.loan_kind == Some(crate::ssa::model::LoanKind::Exclusive)
                            && (self.inputs.typed.assignments().iter().any(|assignment| {
                                assignment.target() == reference
                                    && assignment.operator() == AssignmentOperator::Assign
                                    && assignment.target_type() == descriptor.nullable_type()
                            }) || self.inputs.typed.rc_operations().iter().any(|operation| {
                                operation.receiver().source_unit() == self.inputs.source_unit
                                    && self.id(self.ungrouped(operation.receiver().expression()))
                                        == reference
                            }));
                    !self.string_when_reads.contains(&reference)
                        && !direct_call
                        && !exclusive_operation
                        && !self.inputs.owned.borrow_results().bindings().iter().any(|binding| {
                            crate::ssa::borrow_result_support::ordinary_local::narrowed_when_binding(
                                self.inputs.parsed, self.inputs.typed, self.inputs.owned, binding,
                            ).is_some_and(|witness| {
                                witness.reference() == reference
                                    && witness.control() == use_.control()
                            })
                        })
                })
        {
            return false;
        }
        // A branch-only proof never escapes; retained outer proofs must survive every
        // reached successor, including replacement invalidations in either branch.
        self.view_symbols = common;
        true
    }
}
