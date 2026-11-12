//! Field replacement and owner-loan acquisition in the refined CFG (SPEC-0358): a local
//! owner is lent for the access only; a Borrow/Inout parameter or the current receiver
//! already arrives as a loan of the right kind.
use lang_frontend::{
    ast::ExpressionId, name_resolution::DeclarationId, ownership_checking::UnitDropPoint,
    parser::Expression, source::Span,
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};

/// The owner a field access or receiver goes through.
#[derive(Clone, Copy, Debug)]
pub(super) enum OwnerRef {
    /// An explicit receiver expression: a name, `this`, or a temporary.
    Expression(ExpressionId),
    /// The implicit `this` of the current member of `DeclarationId`.
    This(DeclarationId),
}

/// A loan of an owner; `scoped` loans were begun for this access and end right after it.
#[derive(Clone, Copy, Debug)]
pub(super) struct OwnerLoan {
    pub(super) loan: Slot,
    pub(super) scoped: bool,
}

impl Builder<'_, '_> {
    /// A loan of `owner` of at least `kind`: an exclusive loan also serves a read.
    pub(super) fn owner_loan(
        &mut self,
        owner: OwnerRef,
        kind: LoanKind,
        span: Span,
    ) -> Result<OwnerLoan, LoweringError> {
        let existing = match owner {
            OwnerRef::This(declaration) => {
                if self.inputs.receiver.map(|receiver| receiver.owner) != Some(declaration) {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                self.receiver
            }
            OwnerRef::Expression(mut expression) => {
                while let (Expression::Group { expression: inner }, _) = self.node(expression)? {
                    expression = inner;
                }
                let (payload, expression_span) = self.node(expression)?;
                match payload {
                    Expression::This => self.receiver,
                    Expression::Name => {
                        if let Some(view) = self.narrowed_view(expression)? {
                            if kind == LoanKind::Shared {
                                return Ok(OwnerLoan {
                                    loan: view,
                                    scoped: false,
                                });
                            }
                            return Err(error(LoweringErrorKind::InvalidModel, span));
                        }
                        if let Ok(symbol) = self.reference(expression_span) {
                            match self.borrowed.get(&symbol).copied() {
                                Some(loan) => Some(loan),
                                None => {
                                    let owner = *self.bindings.get(&symbol).ok_or_else(|| {
                                        error(LoweringErrorKind::MissingFact, expression_span)
                                    })?;
                                    return self.lend(owner, kind, span);
                                }
                            }
                        } else {
                            let owner = self
                                .required(expression)?
                                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                            return self.lend(owner, kind, span);
                        }
                    }
                    _ => {
                        let owner = self
                            .required(expression)?
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        return self.lend(owner, kind, span);
                    }
                }
            }
        };
        let loan = existing.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        match (self.body.slots[loan.0], kind) {
            (EntityType::Loan { kind: held, .. }, wanted)
                if held == wanted || held == LoanKind::Exclusive =>
            {
                Ok(OwnerLoan {
                    loan,
                    scoped: false,
                })
            }
            (EntityType::Value(_), LoanKind::Shared) => self.lend(loan, LoanKind::Shared, span),
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }

    /// Lends an owned binding's root place for one access.
    pub(super) fn lend(
        &mut self,
        owner: Slot,
        kind: LoanKind,
        span: Span,
    ) -> Result<OwnerLoan, LoweringError> {
        let EntityType::Value(target) = self.body.slots[owner.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
        let loan = self.value(
            ROp::BorrowBegin { place, kind },
            EntityType::Loan { kind, target },
            span,
        )?;
        Ok(OwnerLoan { loan, scoped: true })
    }

    /// A shared call operand: an exclusive loan held by this function is narrowed to a
    /// call-scoped shared reborrow.
    pub(super) fn shared_operand(
        &mut self,
        loan: OwnerLoan,
        span: Span,
    ) -> Result<OwnerLoan, LoweringError> {
        let EntityType::Loan { kind, target } = self.body.slots[loan.loan.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if kind == LoanKind::Shared {
            return Ok(loan);
        }
        let reborrow = self.value(
            ROp::SharedReborrow { source: loan.loan },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        Ok(OwnerLoan {
            loan: reborrow,
            scoped: true,
        })
    }

    /// Ends a loan begun for one access.
    pub(super) fn end_owner_loan(
        &mut self,
        loan: OwnerLoan,
        span: Span,
    ) -> Result<(), LoweringError> {
        if loan.scoped {
            self.push(ROp::BorrowEnd { loan: loan.loan }, Vec::new(), span)?;
        }
        Ok(())
    }

    pub(super) fn is_field_target(&self, target: ExpressionId) -> bool {
        self.inputs
            .typed
            .aggregate_projection(self.unit(target))
            .is_some()
    }

    /// `owner.f = v` on a plain class owner: a local, an Inout parameter or the Inout
    /// receiver (explicit or implicit `this`).
    pub(super) fn field_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let projection = self
            .inputs
            .typed
            .aggregate_projection(self.unit(target))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (owner, owner_type, _) = self.projection_owner(projection.receiver(), span)?;
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        // Guide: the RHS is evaluated before the old value is dropped and replaced.
        let Some(replacement) = self.required(value)? else {
            let replaced =
                self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
            self.finish_drops(&replaced);
            return Ok(Lowered::Diverged);
        };
        let replacement = self.adapt(replacement, value, self.c(projection.ty()))?;
        self.deliver(value);
        let loan = self.owner_loan(owner, LoanKind::Exclusive, span)?;
        let op = if self.is_value_class(owner_type) {
            ROp::InlineFieldReplace {
                receiver: loan.loan,
                field,
                value: replacement,
            }
        } else {
            ROp::HeapFieldReplace {
                receiver: loan.loan,
                field,
                value: replacement,
            }
        };
        self.push(op, Vec::new(), span)?;
        self.end_owner_loan(loan, span)?;
        let replaced = self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }

    pub(super) fn local_variable(
        &mut self,
        mut declaration: lang_frontend::ast::ItemId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        while let Ok(node) = self.inputs.parsed.ast().items().get(declaration)
            && let lang_frontend::parser::Item::Modified {
                declaration: inner, ..
            } = node.payload()
        {
            declaration = *inner;
        }
        let node = self
            .inputs
            .parsed
            .ast()
            .items()
            .get(declaration)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let lang_frontend::parser::Item::Variable {
            name,
            initializer,
            kind,
            ..
        } = node.payload().clone()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let lang_frontend::parser::NameMarker::Present(name_span) = name else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if matches!(kind, lang_frontend::parser::VariableKind::BorrowVal(_))
            && self
                .inputs
                .typed
                .expression_falls_through(self.unit(initializer))
                == Some(false)
        {
            if self.expression(initializer)? != Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            return Ok(Lowered::Diverged);
        }
        if let Some(fact) = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.initializer() == self.unit(initializer))
        {
            return self.local_borrow_binding(fact.binding(), initializer, span);
        }
        let symbol = self.declared_symbol(name_span)?;
        let declared = self
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.is_unit(declared) {
            if self.expression(initializer)? == Lowered::Diverged {
                return Ok(Lowered::Diverged);
            }
            return Ok(Lowered::Unit);
        }
        let saved_expected = self.expected_nullable;
        if matches!(
            self.inputs.typed.types().get(declared),
            Some(lang_frontend::type_checking::UnitTypeKind::Nullable(_))
        ) {
            self.expected_nullable = self.ssa_type(declared, span).ok();
        } else {
            self.expected_nullable = None;
        }
        let lowered = self.expression(initializer);
        self.expected_nullable = saved_expected;
        let Lowered::Value(value) = lowered? else {
            return Ok(Lowered::Diverged);
        };
        let value = self.adapt_with_span(value, initializer, declared, span)?;
        self.deliver(initializer);
        let EntityType::Value(ty) = self.body.slots[value.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.owner_binding_authorization(self.unit(initializer), symbol, value, span)?;
        let counter =
            self.counter_binding_authorization(self.unit(initializer), symbol, value, span)?;
        let bridge =
            self.bridge_binding_authorization(self.unit(initializer), symbol, value, span)?;
        if bridge.is_some() && counter.is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let binding = self.value(
            ROp::Alias { source: value },
            crate::ssa::model::EntityType::Value(ty),
            span,
        )?;
        self.counter_binding_emitted(counter, binding, span)?;
        self.bridge_emitted(bridge, Some(binding), span)?;
        self.bindings.insert(symbol, binding);
        self.owner_binding_emitted(symbol, binding);
        #[cfg(test)]
        if self.owner_is_suffix(symbol)
            && super::owner_consumer_test_support::take_fault(
                super::owner_consumer_test_support::OwnerFault::DuplicateBind,
            )
        {
            self.owner_binding_authorization(self.unit(initializer), symbol, value, span)?;
        }
        Ok(Lowered::Unit)
    }

    fn declared_symbol(
        &self,
        name_span: Span,
    ) -> Result<lang_frontend::name_resolution::UnitSymbolId, LoweringError> {
        self.inputs
            .names
            .names()
            .source_units()
            .get(self.inputs.source_unit.index())
            .and_then(|source| {
                source.resolution().symbols().iter().find(|symbol| {
                    symbol.span() == name_span
                        && symbol.kind() == lang_frontend::name_resolution::SymbolKind::Variable
                })
            })
            .and_then(|local| {
                self.inputs
                    .typed
                    .body_symbol_types()
                    .keys()
                    .copied()
                    .find(|symbol| {
                        symbol.source_unit() == self.inputs.source_unit
                            && symbol.symbol() == local.id()
                    })
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))
    }
}
