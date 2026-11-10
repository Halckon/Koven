//! Field replacement in the refined CFG (SPEC-0358): the RHS is evaluated first, then the
//! field is replaced under an exclusive loan of the owner; the replacement drops
//! the old MoveOnly value, so the published `BeforeReplacement` drop is consumed by it.
use lang_frontend::{ast::ExpressionId, ownership_checking::UnitDropPoint, source::Span};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::LoanKind,
    refine::body::ROp,
};

impl Builder<'_, '_> {
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
        let crate::ssa::model::EntityType::Value(ty) = self.body.slots[value.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.owner_binding_authorization(self.unit(initializer), symbol, value, span)?;
        let binding = self.value(
            ROp::Alias { source: value },
            crate::ssa::model::EntityType::Value(ty),
            span,
        )?;
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
