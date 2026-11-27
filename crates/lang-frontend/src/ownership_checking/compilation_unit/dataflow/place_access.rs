//! Unit name use, place consumption, availability and access diagnostics.
use super::*;
use crate::type_checking::UnitAggregateProjectionKind;

impl Checker<'_> {
    pub(super) fn use_name(
        &mut self,
        expression: ExpressionId,
        span: Span,
        usage: ExpressionUse,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(symbol) = self.reference_symbol(span) else {
            return Ok(());
        };
        if !self.is_place_symbol(symbol) {
            return Ok(());
        }
        let place = UnitOwnershipPlace::new(symbol, Vec::new());
        match usage {
            ExpressionUse::Read => {
                self.access_place(&place, AccessKind::Read, span, None, state)?;
            }
            ExpressionUse::Consume { parameter_span } => {
                self.consume_place(&place, expression, span, parameter_span, state)?;
            }
            ExpressionUse::Place { parameter_span } => {
                self.ensure_available(&place, span, parameter_span, state)?;
            }
        }
        Ok(())
    }

    /// A temporary receiver does not turn its field into an independently movable value.
    pub(super) fn reject_unrooted_field_move(
        &mut self,
        expression: ExpressionId,
        primary: Span,
        parameter_span: Option<Span>,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(projection) = self
            .typed
            .aggregate_projection(self.unit_expression(expression))
        else {
            return Ok(());
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.typed.copyability(projection.ty()) != Copyability::MoveOnly
        {
            return Ok(());
        }
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.partial_move,
            "cannot move a non-Copyable component out of its owner",
            primary,
        )?;
        diagnostic.add_label(
            self.sources,
            self.symbol_span(projection.field())?,
            "non-Copyable component declared here",
        )?;
        add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn consume_place(
        &mut self,
        place: &UnitOwnershipPlace,
        expression: ExpressionId,
        primary: Span,
        parameter_span: Option<Span>,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let ty = self
            .typed
            .expression_type(self.unit_expression(expression))
            .or_else(|| self.typed.symbol_type(place.root()));
        let Some(ty) = ty else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        };
        if self.typed.is_diagnostic_error_type(ty) {
            // An error-typed expression has no proven move/copy effect.
            return Ok(());
        }
        let move_only = match self.typed.copyability(ty) {
            Copyability::Copyable => false,
            Copyability::MoveOnly => true,
            Copyability::Error if self.typed.copyability_error_is_diagnostic(ty) => {
                return Ok(());
            }
            Copyability::Unknown | Copyability::Error => {
                return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                    source_unit: self.source_unit.index(),
                    expression: expression.index(),
                });
            }
        };
        if move_only
            && state.loans.iter().any(|loan| {
                (loan.reserved || matches!(loan.owner, ActiveLoanOwner::IterationSource(_)))
                    && match &loan.target {
                        ActiveLoanTarget::Place(target) => target.overlaps(place),
                        ActiveLoanTarget::This => {
                            self.symbol_kind(place.root()) == Some(SymbolKind::Field)
                        }
                    }
            })
            && !self.access_place(place, AccessKind::Move, primary, parameter_span, state)?
        {
            return Ok(());
        }
        if move_only
            && (self
                .bindings
                .get(&place.root())
                .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
                || state.non_owning.contains_key(&place.root()))
        {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.borrowed_move,
                "cannot move a non-Copyable value out of a borrowed binding",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                state
                    .non_owning
                    .get(&place.root())
                    .copied()
                    .or_else(|| {
                        self.bindings
                            .get(&place.root())
                            .map(|binding| binding.declaration_span())
                    })
                    .unwrap_or(primary),
                "non-owning binding established here",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        let access = if move_only {
            AccessKind::Move
        } else {
            AccessKind::Read
        };
        if !self.access_place(place, access, primary, parameter_span, state)? || !move_only {
            return Ok(());
        }
        if place.element().is_some() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.container_element_move,
                "cannot move a non-Copyable element out of a sequential container",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                self.symbol_span(place.root())?,
                "container owner remains responsible for every initialized element",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        if !place.fields().is_empty() || self.symbol_kind(place.root()) == Some(SymbolKind::Field) {
            let field = place.fields().last().copied().unwrap_or(place.root());
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.partial_move,
                "cannot move a non-Copyable component out of its owner",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                self.symbol_span(field)?,
                "non-Copyable component declared here",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        state.moved.insert(place.root(), primary);
        Ok(())
    }

    pub(super) fn ensure_available(
        &mut self,
        place: &UnitOwnershipPlace,
        primary: Span,
        parameter_span: Option<Span>,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        let Some(origin) = state.moved.get(&place.root()).copied() else {
            return Ok(true);
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.use_after_move,
            "use of moved value",
            primary,
        )?;
        diagnostic.add_label(self.sources, origin, "value was moved here")?;
        add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
        self.diagnostics.push(diagnostic);
        Ok(false)
    }

    pub(super) fn access_place(
        &mut self,
        place: &UnitOwnershipPlace,
        access: AccessKind,
        primary: Span,
        parameter_span: Option<Span>,
        state: &mut State,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.symbol_kind(place.root()) == Some(SymbolKind::Field)
            && !self.ensure_this_available_at(primary, parameter_span, state)?
        {
            return Ok(false);
        }
        // A whole-root assignment restores a moved binding without reading its old value.
        let requires_existing_value = access != AccessKind::Mutation || !place.is_root();
        if requires_existing_value
            && !self.ensure_available(place, primary, parameter_span, state)?
        {
            return Ok(false);
        }
        if (access == AccessKind::Mutation
            || (access == AccessKind::ExclusiveLoan
                && state.non_owning.contains_key(&place.root())))
            && let Some(origin) = state.immutable_capture_origin(place.root())
        {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.loan_conflict,
                "captured bindings are immutable in v1 closures",
                primary,
            )?;
            diagnostic.add_label(self.sources, origin, "conflicting loan starts here")?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(false);
        }
        let canonical = self.canonical_borrow_place(place, state);
        let conflict = state.loans.iter().find(|loan| {
            let overlaps = match &loan.target {
                ActiveLoanTarget::Place(target) => target.overlaps(&canonical),
                ActiveLoanTarget::This => self.symbol_kind(place.root()) == Some(SymbolKind::Field),
            };
            overlaps && loan.conflicts_with(access)
        });
        if let Some(conflict) = conflict {
            let message = match access {
                AccessKind::Read => "read conflicts with an active exclusive loan",
                AccessKind::Move => "move conflicts with an active loan",
                AccessKind::Mutation => "mutation conflicts with an active loan",
                AccessKind::SharedLoan | AccessKind::ExclusiveLoan => {
                    "new loan conflicts with an active loan"
                }
            };
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.loan_conflict,
                message,
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                conflict.origin,
                "conflicting loan starts here",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(false);
        }
        if self.symbol_kind(place.root()) == Some(SymbolKind::Field)
            && matches!(access, AccessKind::Mutation | AccessKind::ExclusiveLoan)
            && !self.require_mutable_this(primary, parameter_span)?
        {
            return Ok(false);
        }
        Ok(true)
    }
}

pub(super) fn add_parameter_label(
    sources: &SourceMap,
    diagnostic: &mut Diagnostic,
    parameter_span: Option<Span>,
) -> Result<(), OwnershipCheckingError> {
    if let Some(parameter_span) = parameter_span {
        diagnostic.add_label(sources, parameter_span, "selected parameter declared here")?;
    }
    Ok(())
}
