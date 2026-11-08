//! Source-qualified closure formation、逃逸与跨线程交付。

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    name_resolution::UnitSymbolId,
    parser::{Expression, Statement},
    type_checking::{BuiltinType, Copyability, NominalKind, UnitExpressionId, UnitTypeKind},
};

use crate::ownership_checking::{
    ClosureCaptureEffect, ClosureCaptureMode, LoanKind, Transferability,
    UnitClosureCaptureDescriptor, UnitClosureCaptureSource, UnitOwnershipPlace,
};

use std::collections::{BTreeMap, BTreeSet};

use super::flow::CaptureRestriction;
use super::{
    AccessKind, ActiveLoan, ActiveLoanOwner, ActiveLoanTarget, Checker, ExpressionUse, Flows,
    OwnershipBindingKind, OwnershipCheckingError, State,
};

pub(super) type BranchCaptureReleases =
    BTreeMap<(UnitExpressionId, usize), BTreeMap<UnitSymbolId, BTreeSet<UnitExpressionId>>>;

pub(super) type CallCaptureSources =
    BTreeMap<(UnitExpressionId, UnitExpressionId), BTreeSet<UnitSymbolId>>;

pub(super) struct CaptureIndex<'a> {
    captures: &'a [UnitClosureCaptureDescriptor],
    by_lambda: BTreeMap<UnitExpressionId, Vec<usize>>,
    #[cfg(test)]
    pub(super) descriptor_visits: std::cell::Cell<usize>,
    pub(super) roots: BTreeMap<UnitExpressionId, BTreeSet<UnitSymbolId>>,
    move_only: BTreeSet<UnitExpressionId>,
    pub(super) borrowed_sources: BTreeMap<UnitExpressionId, Vec<UnitSymbolId>>,
}

impl CaptureIndex<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.captures.is_empty()
    }

    pub(super) fn contains_lambda(&self, lambda: UnitExpressionId) -> bool {
        self.by_lambda.contains_key(&lambda)
    }

    pub(super) fn for_lambda(
        &self,
        lambda: UnitExpressionId,
    ) -> impl Iterator<Item = (usize, &UnitClosureCaptureDescriptor)> + '_ {
        self.by_lambda
            .get(&lambda)
            .into_iter()
            .flatten()
            .map(|&ordinal| {
                #[cfg(test)]
                self.descriptor_visits.set(self.descriptor_visits.get() + 1);
                (ordinal, &self.captures[ordinal])
            })
    }
}

pub(super) fn capture_index<'a>(
    captures: &'a [UnitClosureCaptureDescriptor],
    typed: &crate::type_checking::CompilationUnitTypes,
) -> CaptureIndex<'a> {
    let mut roots = BTreeMap::<_, BTreeSet<_>>::new();
    let mut move_only = BTreeSet::new();
    let mut borrowed_sources = BTreeMap::<_, Vec<_>>::new();
    let mut by_lambda = BTreeMap::<_, Vec<_>>::new();
    for (ordinal, capture) in captures.iter().enumerate() {
        by_lambda.entry(capture.lambda()).or_default().push(ordinal);
        if capture.borrow_kind() != Some(LoanKind::Exclusive) {
            continue;
        }
        if let UnitClosureCaptureSource::Symbol(symbol) = capture.source() {
            roots.entry(capture.lambda()).or_default().insert(symbol);
            if typed
                .symbol_type(symbol)
                .is_some_and(|ty| typed.copyability(ty) == Copyability::MoveOnly)
            {
                move_only.insert(capture.lambda());
            }
        }
    }
    for capture in captures {
        if let UnitClosureCaptureSource::Symbol(symbol) = capture.source()
            && capture.borrow_kind().is_some()
        {
            borrowed_sources
                .entry(capture.lambda())
                .or_default()
                .push(symbol);
        }
    }
    CaptureIndex {
        captures,
        by_lambda,
        #[cfg(test)]
        descriptor_visits: std::cell::Cell::new(0),
        roots,
        move_only,
        borrowed_sources,
    }
}

impl Checker<'_> {
    pub(super) fn check_lambda(
        &mut self,
        lambda: ExpressionId,
        body: crate::ast::StatementId,
        mut state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.visited_lambdas.insert(self.unit_expression(lambda));
        let diagnostic_count = self.diagnostics.len();
        let entry_state = state.clone();
        let lambda_id = self.unit_expression(lambda);
        let captures = self.captures_of(lambda).collect::<Vec<_>>();
        for capture in &captures {
            if let UnitClosureCaptureSource::Symbol(symbol) = capture.source()
                && state.borrow_bindings.contains_key(&symbol)
            {
                self.emit_borrow_binding_diagnostic(
                    crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                    "borrow result capture continuation is not yet proven",
                    capture.reference_span(),
                )?;
            }
        }
        let captured_closures: BTreeSet<_> = captures
            .iter()
            .filter_map(|capture| {
                if let UnitClosureCaptureSource::Symbol(symbol) = capture.source() {
                    state.closures.get(&symbol)
                } else {
                    None
                }
            })
            .flatten()
            .copied()
            .collect();
        if self.has_move_only_exclusive_capture(&state, captured_closures.iter().copied()) {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "nested environment cannot yet retain a MoveOnly exclusive capture",
                self.parsed.ast().expressions().get(lambda)?.span(),
            )?;
        }
        for capture in &captures {
            match (capture.mode(), capture.source(), capture.effect()) {
                (ClosureCaptureMode::Shared, UnitClosureCaptureSource::Symbol(symbol), _) => {
                    let place = UnitOwnershipPlace::new(symbol, Vec::new());
                    let requires_exclusive = capture.borrow_kind() == Some(LoanKind::Exclusive);
                    let access = if requires_exclusive {
                        AccessKind::ExclusiveLoan
                    } else {
                        AccessKind::SharedLoan
                    };
                    if self.access_place(
                        &place,
                        access,
                        capture.reference_span(),
                        None,
                        &mut state,
                    )? && (!requires_exclusive || self.supports_capture_write(symbol))
                    {
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Closure(lambda_id),
                            target: ActiveLoanTarget::Place(place),
                            kind: if requires_exclusive {
                                LoanKind::Exclusive
                            } else {
                                LoanKind::Shared
                            },
                            reserved: false,
                            origin: capture.reference_span(),
                        });
                    }
                }
                (ClosureCaptureMode::Shared, UnitClosureCaptureSource::This, _) => {
                    if self.ensure_this_available_at(capture.reference_span(), None, &state)?
                        && self.access_this_at(
                            AccessKind::SharedLoan,
                            capture.reference_span(),
                            None,
                            &state,
                        )?
                    {
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Closure(lambda_id),
                            target: ActiveLoanTarget::This,
                            kind: LoanKind::Shared,
                            reserved: false,
                            origin: capture.reference_span(),
                        });
                    }
                }
                (ClosureCaptureMode::Owned, UnitClosureCaptureSource::This, _) => {
                    self.emit_illegal_owned_capture(
                        capture.reference_span(),
                        capture.reference_span(),
                        "move closure cannot capture this or a field directly",
                    )?;
                }
                (
                    ClosureCaptureMode::Owned,
                    UnitClosureCaptureSource::Symbol(symbol),
                    ClosureCaptureEffect::Move,
                ) => {
                    let binding = self.symbol_span(symbol)?;
                    if self
                        .bindings
                        .get(&symbol)
                        .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
                        || state.non_owning.contains_key(&symbol)
                    {
                        self.emit_illegal_owned_capture(
                            capture.reference_span(),
                            binding,
                            "move closure cannot own a non-Copyable borrowed capture",
                        )?;
                        continue;
                    }
                    let place = UnitOwnershipPlace::new(symbol, Vec::new());
                    if self.access_place(
                        &place,
                        AccessKind::Move,
                        capture.reference_span(),
                        None,
                        &mut state,
                    )? {
                        state.moved.insert(symbol, capture.reference_span());
                        // The nested environment now belongs to the formed outer value.
                        state.closures.remove(&symbol);
                    }
                }
                (
                    ClosureCaptureMode::Owned,
                    UnitClosureCaptureSource::Symbol(symbol),
                    ClosureCaptureEffect::Copy | ClosureCaptureEffect::Unknown,
                ) => {
                    let place = UnitOwnershipPlace::new(symbol, Vec::new());
                    self.access_place(
                        &place,
                        AccessKind::Read,
                        capture.reference_span(),
                        None,
                        &mut state,
                    )?;
                }
                (
                    ClosureCaptureMode::Owned,
                    UnitClosureCaptureSource::Symbol(_),
                    ClosureCaptureEffect::Borrow,
                ) => {
                    return Err(OwnershipCheckingError::InvalidUnitClosureCapture {
                        source_unit: self.source_unit.index(),
                        expression: lambda.index(),
                    });
                }
            }
        }

        if self.diagnostics.len() != diagnostic_count {
            return Ok(Flows::next(entry_state));
        }

        let mut body_state = State::default();
        self.seed_lambda_parameters(lambda, &mut body_state)?;
        for origin in entry_state.closure_tree(captured_closures.iter().copied()) {
            if let Some(nested) = entry_state.closure_captures.get(&origin) {
                body_state.closure_captures.insert(origin, nested.clone());
            }
        }
        for &capture in &captures {
            match capture.source() {
                UnitClosureCaptureSource::This if capture.mode() == ClosureCaptureMode::Shared => {
                    body_state.loans.push(ActiveLoan {
                        owner: ActiveLoanOwner::Closure(lambda_id),
                        target: ActiveLoanTarget::This,
                        kind: LoanKind::Shared,
                        reserved: false,
                        origin: capture.reference_span(),
                    });
                }
                UnitClosureCaptureSource::This => {}
                UnitClosureCaptureSource::Symbol(symbol) => {
                    // Capture values are frozen before any move out of the enclosing binding.
                    if let Some(origins) = entry_state.closures.get(&symbol) {
                        body_state.closures.insert(symbol, origins.clone());
                    }
                    body_state.immutable_captures.insert(
                        symbol,
                        CaptureRestriction {
                            origin: capture.reference_span(),
                            exclusive_assignment: capture.exclusive_assignment(),
                            write_allowed: capture.borrow_kind() == Some(LoanKind::Exclusive)
                                && self.supports_capture_write(symbol),
                        },
                    );
                    if capture.mode() == ClosureCaptureMode::Shared {
                        body_state
                            .non_owning
                            .insert(symbol, capture.reference_span());
                    }
                }
            }
        }
        let result_type = self
            .typed
            .expression_type(lambda_id)
            .and_then(|ty| self.typed.types().get(ty))
            .and_then(|ty| match ty {
                crate::type_checking::UnitTypeKind::Function { return_type, .. } => {
                    Some(*return_type)
                }
                _ => None,
            });
        if let Some(ty) = result_type
            && self.typed.types().get(ty)
                != Some(&crate::type_checking::UnitTypeKind::Builtin(
                    crate::type_checking::BuiltinType::Unit,
                ))
        {
            self.check_control_body(body, body_state, ExpressionUse::Read, true)?;
        } else {
            self.check_statement(body, body_state)?;
        }
        if self.diagnostics.len() != diagnostic_count {
            return Ok(Flows::next(entry_state));
        }
        if let Some((assignment, origin)) = captures
            .iter()
            .filter(|capture| {
                matches!(capture.source(), UnitClosureCaptureSource::Symbol(symbol)
                    if !self.supports_capture_write(symbol))
            })
            .find_map(|capture| {
                capture
                    .exclusive_assignment()
                    .map(|assignment| (assignment, capture.reference_span()))
            })
        {
            // The required capture mode is a property of the body, including
            // assignments whose runtime path does not reach a commit.
            self.emit_exclusive_capture_unavailable(assignment, origin)?;
            return Ok(Flows::next(entry_state));
        }
        state.closure_captures.insert(lambda_id, captured_closures);
        Ok(Flows::next(state))
    }

    fn supports_capture_write(&self, symbol: crate::name_resolution::UnitSymbolId) -> bool {
        let Some(storage) = self.typed.symbol_type(symbol) else {
            return false;
        };
        match self.typed.copyability(storage) {
            Copyability::Copyable => true,
            Copyability::MoveOnly => {
                let mut root = storage;
                while let Some(UnitTypeKind::Nullable(inner)) = self.typed.types().get(root) {
                    root = *inner;
                }
                let opaque = match self.typed.types().get(root) {
                    Some(UnitTypeKind::Builtin(BuiltinType::Any)) => true,
                    Some(UnitTypeKind::Nominal { declaration, .. }) => self
                        .typed
                        .signatures()
                        .declaration(*declaration)
                        .and_then(|declaration| declaration.nominal())
                        .is_none_or(|nominal| nominal.kind() == NominalKind::Interface),
                    _ => false,
                };
                !opaque
                    && self.type_has_concrete_storage(storage)
                    && !self.type_has_closure_storage(storage)
            }
            Copyability::Error | Copyability::Unknown => false,
        }
    }

    pub(super) fn allow_move_only_capture_transport(
        &mut self,
        value: ExpressionId,
        state: &State,
        origins: &BTreeSet<UnitExpressionId>,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.has_move_only_exclusive_capture(state, origins.iter().copied()) {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "MoveOnly exclusive capture needs a directly tracked closure environment",
                self.parsed.ast().expressions().get(value)?.span(),
            )?;
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn has_move_only_exclusive_capture(
        &self,
        state: &State,
        origins: impl IntoIterator<Item = UnitExpressionId>,
    ) -> bool {
        let move_only = &self.capture_index.move_only;
        !move_only.is_empty() && !state.closure_tree(origins).is_disjoint(move_only)
    }

    pub(super) fn emit_exclusive_capture_unavailable(
        &mut self,
        assignment: crate::source::Span,
        origin: crate::source::Span,
    ) -> Result<(), OwnershipCheckingError> {
        use crate::diagnostic::codes;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
            "exclusive closure assignment cannot yet replace this captured storage",
            assignment,
        )?;
        diagnostic.add_label(self.sources, origin, "captured binding is referenced here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn emit_illegal_owned_capture(
        &mut self,
        primary: crate::source::Span,
        binding: crate::source::Span,
        message: &'static str,
    ) -> Result<(), OwnershipCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.illegal_owned_capture,
            message,
            primary,
        )?;
        diagnostic.add_label(self.sources, binding, "capture source is non-owning here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn closure_origins(
        &self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        Ok(match node.payload() {
            Expression::Lambda { .. } => BTreeSet::from([self.unit_expression(expression)]),
            Expression::Group { expression } => self.closure_origins(*expression, state)?,
            Expression::Name => self
                .reference_symbol(node.span())
                .and_then(|s| state.closures.get(&s).cloned())
                .unwrap_or_default(),
            _ => state
                .closure_results
                .get(&self.unit_expression(expression))
                .cloned()
                .unwrap_or_default(),
        })
    }

    pub(super) fn closure_result_origins(
        &self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, OwnershipCheckingError> {
        match self.parsed.ast().expressions().get(expression)?.payload() {
            Expression::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut result = self.closure_body_origins(*then_branch, state)?;
                if let Some(body) = else_branch {
                    result.extend(self.closure_body_origins(*body, state)?);
                }
                Ok(result)
            }
            Expression::When { entries, .. } => {
                let mut result = BTreeSet::new();
                for entry in entries {
                    result.extend(self.closure_body_origins(entry.body, state)?);
                }
                Ok(result)
            }
            _ => self.closure_origins(expression, state),
        }
    }

    fn closure_body_origins(
        &self,
        body: StatementId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, OwnershipCheckingError> {
        match self.parsed.ast().statements().get(body)?.payload() {
            Statement::Expression { expression } => Ok(state
                .closure_results
                .get(&self.unit_expression(*expression))
                .cloned()
                .unwrap_or_default()),
            Statement::ControlBody { elements } => {
                if let Some(tail) = elements.last() {
                    self.closure_body_origins(*tail, state)
                } else {
                    Ok(BTreeSet::new())
                }
            }
            _ => Ok(BTreeSet::new()),
        }
    }

    pub(super) fn expression_root_symbol(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<crate::name_resolution::UnitSymbolId>, OwnershipCheckingError> {
        if self.is_constant_use(expression) {
            return Ok(None);
        }
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name => Ok(self.reference_symbol(node.span())),
            Expression::Group { expression } => self.expression_root_symbol(*expression),
            _ => Ok(None),
        }
    }

    pub(super) fn release_closure(
        &self,
        symbol: crate::name_resolution::UnitSymbolId,
        state: &mut State,
    ) {
        if !state.replacements.contains(&symbol) {
            self.release_closure_except(symbol, &BTreeSet::new(), state);
        }
    }

    pub(super) fn release_closure_except(
        &self,
        symbol: crate::name_resolution::UnitSymbolId,
        retained: &BTreeSet<UnitExpressionId>,
        state: &mut State,
    ) {
        let Some(closure) = state.closures.remove(&symbol) else {
            return;
        };
        for origin in closure {
            self.release_unheld_closure(origin, retained, state);
        }
    }

    fn release_unheld_closure(
        &self,
        closure: UnitExpressionId,
        retained: &BTreeSet<UnitExpressionId>,
        state: &mut State,
    ) {
        let mut pending = vec![closure];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current)
                || state
                    .closure_tree(
                        state
                            .closures
                            .values()
                            .flatten()
                            .copied()
                            .chain(state.pending_closure_calls.values().flatten().copied())
                            .chain(retained.iter().copied()),
                    )
                    .contains(&current)
            {
                continue;
            }
            state
                .loans
                .retain(|loan| loan.owner != ActiveLoanOwner::Closure(current));
            if let Some(captured) = state.closure_captures.remove(&current) {
                pending.extend(captured);
            }
        }
    }

    pub(super) fn has_exclusive_capture(
        &self,
        state: &State,
        origins: impl IntoIterator<Item = UnitExpressionId>,
    ) -> bool {
        let roots = &self.capture_index.roots;
        !roots.is_empty()
            && state
                .closure_tree(origins)
                .iter()
                .any(|lambda| roots.contains_key(lambda))
    }

    pub(super) fn hold_call_closure(
        &mut self,
        call: UnitExpressionId,
        operand: ExpressionId,
        is_callee: bool,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        if let Some(state) = &mut flows.next {
            let origins = self.closure_origins(operand, state)?;
            if !origins.is_empty() {
                let mut exclusive = false;
                if !self.capture_index.roots.is_empty() {
                    let environments = state.closure_tree(origins.iter().copied());
                    exclusive = environments
                        .iter()
                        .any(|lambda| self.capture_index.roots.contains_key(lambda));
                    if exclusive {
                        let operand = self.unit_expression(operand);
                        self.call_capture_sources
                            .entry((call, operand))
                            .or_default()
                            .extend(environments.iter().flat_map(|lambda| {
                                self.capture_index
                                    .borrowed_sources
                                    .get(lambda)
                                    .into_iter()
                                    .flatten()
                                    .copied()
                            }));
                    }
                }
                if exclusive
                    && self.allowed_borrow_call == Some(call.expression())
                    && self.borrowed_call_source(call.expression()) == Some(operand)
                {
                    self.emit_borrow_binding_diagnostic(
                        crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                        "borrow result cannot yet retain an exclusive closure environment",
                        self.parsed.ast().expressions().get(operand)?.span(),
                    )?;
                }
                if exclusive
                    && is_callee
                    && let Some(symbol) = self.expression_root_symbol(operand)?
                {
                    state.loans.push(ActiveLoan {
                        owner: ActiveLoanOwner::Call(call),
                        target: ActiveLoanTarget::Place(UnitOwnershipPlace::new(
                            symbol,
                            Vec::new(),
                        )),
                        kind: LoanKind::Shared,
                        reserved: false,
                        origin: self.parsed.ast().expressions().get(operand)?.span(),
                    });
                }
                state
                    .pending_closure_calls
                    .entry(call)
                    .or_default()
                    .extend(origins);
            }
        }
        Ok(())
    }

    pub(super) fn finish_call_closures(&self, call: UnitExpressionId, flows: &mut Flows) {
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            if let Some(origins) = state.pending_closure_calls.remove(&call) {
                for origin in origins {
                    self.release_unheld_closure(origin, &BTreeSet::new(), state);
                }
            }
        }
    }

    pub(super) fn release_exclusive_branch(
        &mut self,
        control: ExpressionId,
        branch: usize,
        state: &mut State,
    ) {
        let live = self
            .if_branch_live_in
            .get(&control.index())
            .and_then(|arms| arms.get(branch))
            .or_else(|| self.when_entry_live_in.get(&(control.index(), branch)))
            .unwrap_or(&self.expression_live_after[control.index()]);
        for (holder, origins) in self.release_dead_exclusive_captures(live, state) {
            self.branch_capture_releases
                .entry((self.unit_expression(control), branch))
                .or_default()
                .entry(holder)
                .or_default()
                .extend(origins);
        }
    }

    pub(super) fn release_dead_exclusive_captures(
        &self,
        live: &BTreeSet<UnitSymbolId>,
        state: &mut State,
    ) -> Vec<(UnitSymbolId, BTreeSet<UnitExpressionId>)> {
        let exclusive = state
            .loans
            .iter()
            .filter_map(|loan| match loan.owner {
                ActiveLoanOwner::Closure(origin) if loan.kind == LoanKind::Exclusive => {
                    Some(origin)
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        if exclusive.is_empty() {
            return Vec::new();
        }
        let externally_borrowed = state
            .loans
            .iter()
            .filter_map(|loan| match (&loan.owner, &loan.target) {
                (ActiveLoanOwner::Closure(_), _) | (_, ActiveLoanTarget::This) => None,
                (_, ActiveLoanTarget::Place(place)) => Some(place.root()),
            })
            .collect::<BTreeSet<_>>();
        let retain_holder = |holder: &UnitSymbolId| {
            live.contains(holder)
                || state.replacements.contains(holder)
                || externally_borrowed.contains(holder)
        };
        let protected = state.closure_tree(
            state
                .pending_closure_calls
                .values()
                .flatten()
                .copied()
                .chain(
                    state
                        .closures
                        .iter()
                        .filter(|(holder, _)| retain_holder(holder))
                        .flat_map(|(_, origins)| origins.iter().copied()),
                ),
        );
        let dead = state
            .closures
            .iter()
            .filter(|(holder, origins)| {
                !retain_holder(holder)
                    && !state
                        .closure_tree(origins.iter().copied())
                        .is_disjoint(&exclusive)
            })
            .map(|(&symbol, origins)| (symbol, origins.clone()))
            .collect::<Vec<_>>();
        // A live wrapper can retain a dead binding's environment. The loan state
        // retires that binding now; its runtime owner is released with the wrapper.
        for (symbol, _) in &dead {
            self.release_closure(*symbol, state);
        }
        dead.into_iter()
            .filter(|(_, origins)| origins.is_disjoint(&protected))
            .collect()
    }

    pub(super) fn release_dead_control_closures(
        &self,
        control: ExpressionId,
        retained: &BTreeSet<UnitExpressionId>,
        state: &mut State,
    ) {
        loop {
            let dead=state.closures.keys().copied().filter(|symbol| {
                state.moved.contains_key(symbol) && !self.expression_live_after[control.index()].contains(symbol) && !state.replacements.contains(symbol)
                    && !state.loans.iter().any(|loan|matches!(&loan.target,ActiveLoanTarget::Place(place) if place.root()==*symbol))
            }).collect::<Vec<_>>();
            if dead.is_empty() {
                break;
            }
            for symbol in dead {
                self.release_closure_except(symbol, retained, state);
            }
        }
    }

    pub(super) fn release_last_closure_use(
        &self,
        expression: ExpressionId,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        if let Some(symbol) = self.expression_root_symbol(expression)? {
            if self.expression_live_after[expression.index()].contains(&symbol) {
                return Ok(());
            }
            for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                .into_iter()
                .flatten()
            {
                self.release_closure(symbol, state);
            }
        } else {
            for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                .into_iter()
                .flatten()
            {
                // The completed argument owns its actual result, including a consumed branch-local tail.
                let temporaries = self.closure_origins(expression, state)?;
                for closure in temporaries {
                    self.release_unheld_closure(closure, &BTreeSet::new(), state);
                }
            }
        }
        Ok(())
    }

    pub(super) fn reject_borrowed_closure_escape(
        &mut self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let mut pending = self
            .closure_origins(expression, state)?
            .into_iter()
            .collect::<Vec<_>>();
        let mut seen = std::collections::BTreeSet::new();
        let mut borrowed = None;
        while let Some(lambda) = pending.pop() {
            if !seen.insert(lambda) {
                continue;
            }
            let captures = self
                .capture_index
                .for_lambda(lambda)
                .map(|(_, capture)| capture)
                .collect::<Vec<_>>();
            if captures
                .iter()
                .any(|c| c.mode() == ClosureCaptureMode::Shared)
            {
                borrowed = Some(lambda);
                break;
            }
            if let Some(captured) = state.closure_captures.get(&lambda) {
                pending.extend(captured);
            } else {
                // Directly delivered lambdas are checked before their environment is formed.
                pending.extend(
                    captures
                        .iter()
                        .filter_map(|c| {
                            if let UnitClosureCaptureSource::Symbol(s) = c.source() {
                                state.closures.get(&s)
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .copied(),
                );
            }
        }
        let Some(lambda) = borrowed else {
            return Ok(());
        };
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        let lambda_span = self
            .parsed
            .ast()
            .expressions()
            .get(lambda.expression())?
            .span();
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.borrowed_closure_escape,
            "borrowed closure cannot escape its defining callable",
            primary,
        )?;
        diagnostic.add_label(self.sources, lambda_span, "borrowed closure formed here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn check_cross_thread_delivery(
        &mut self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let origins = self.closure_origins(expression, state)?;
        let closure = (origins.len() == 1).then(|| *origins.first().unwrap());
        let transferability = closure
            .and_then(|lambda| {
                self.closures
                    .iter()
                    .find(|descriptor| descriptor.expression() == lambda)
                    .map(|descriptor| descriptor.transferability())
            })
            .or_else(|| {
                self.typed
                    .expression_type(self.unit_expression(expression))
                    .and_then(|ty| self.transferabilities.get(ty.index()).copied())
            });
        if transferability == Some(Transferability::Transferable) {
            return Ok(());
        }
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.non_transferable_delivery,
            "cross-thread delivery requires a Transferable value",
            primary,
        )?;
        let label = if let Some(lambda) = closure {
            self.capture_index
                .for_lambda(lambda)
                .find(|(_, capture)| {
                    capture.mode() == ClosureCaptureMode::Shared
                        || self.transferabilities.get(capture.ty().index()).copied()
                            != Some(Transferability::Transferable)
                })
                .map(|(_, capture)| capture.reference_span())
                .unwrap_or(
                    self.parsed
                        .ast()
                        .expressions()
                        .get(lambda.expression())?
                        .span(),
                )
        } else if let Some(symbol) = self.expression_root_symbol(expression)? {
            self.symbol_span(symbol)?
        } else {
            primary
        };
        diagnostic.add_label(
            self.sources,
            label,
            "non-Transferable source originates here",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn captures_of(
        &self,
        lambda: ExpressionId,
    ) -> impl Iterator<Item = UnitClosureCaptureDescriptor> + '_ {
        self.capture_index
            .for_lambda(self.unit_expression(lambda))
            .map(|(_, capture)| *capture)
    }
}
