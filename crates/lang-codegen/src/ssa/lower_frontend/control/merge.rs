//! Exit-role association and CFG block-parameter merging.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum MergeSlot {
    Result,
    Binding(SymbolId),
    Temporary(usize),
    PendingLoan((usize, usize)),
    CaptureLoan((usize, usize)),
    NonNullView(SymbolId),
    BorrowBinding(SymbolId),
    ResultSource(SymbolId, usize),
    ForSource(usize),
    ForElement(usize),
    MapSource(usize),
    MapCursor(usize),
}

struct MergeProjection {
    slot: MergeSlot,
    span: Span,
}

impl IncomingRole<BranchExit> for MergeProjection {
    fn current(&self, exit: &BranchExit) -> Result<EntityId, LoweringError> {
        match self.slot {
            MergeSlot::Result => lowered_entity(exit.result, self.span),
            MergeSlot::ResultSource(symbol, index) => {
                Ok(EntityId::Loan(exit.result_source_loans[&symbol][index]))
            }
            MergeSlot::CaptureLoan(key) => Ok(EntityId::Loan(exit.capture_loans[&key])),
            MergeSlot::NonNullView(symbol) => Ok(EntityId::Loan(exit.views[&symbol].loan)),
            MergeSlot::Temporary(key) => Ok(EntityId::Value(exit.temporaries[&key])),
            MergeSlot::PendingLoan(key) => exit.loans[&key]
                .map(EntityId::Loan)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.span)),
            MergeSlot::Binding(symbol) => lowered_entity(
                exit.bindings
                    .get(&symbol)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.span))?,
                self.span,
            ),
            MergeSlot::BorrowBinding(symbol) => Ok(EntityId::Loan(exit.borrow_bindings[&symbol])),
            MergeSlot::ForSource(stmt) => Ok(EntityId::Loan(exit.for_sources[&stmt])),
            MergeSlot::ForElement(stmt) => Ok(EntityId::Loan(exit.for_elements[&stmt])),
            MergeSlot::MapSource(_) | MergeSlot::MapCursor(_) => {
                map_frames::map_slot(&exit.map_paths, self.slot, self.span)
            }
        }
    }
}

impl ExpressionLowerer<'_> {
    pub(in crate::ssa) fn merge_exits(
        &mut self,
        exits: Vec<BranchExit>,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(first) = exits.first() else {
            return Ok(LoweredValue::Diverged);
        };
        self.validate_non_null_exit_proofs(&exits, span)?;
        self.validate_pending_field_exits(&exits, span)?;
        let mut map_paths = self.map_merge_paths(&exits, span)?;
        if exits.len() == 1 {
            self.block = first.block;
            self.bindings = first.bindings.clone();
            self.temporaries = first.temporaries.clone();
            self.pending_call_loans = first.loans.clone();
            self.capture_loans = first.capture_loans.clone();
            self.non_null_bindings = first.views.clone();
            self.borrow_bindings = first.borrow_bindings.clone();
            self.result_source_loans = first.result_source_loans.clone();
            for context in &mut self.loops {
                if let Some(ref mut for_data) = context.for_loop {
                    if let Some(&loan) = first.for_sources.get(&for_data.statement.index()) {
                        for_data.rebind_source(loan);
                    }
                    if let Some(&loan) = first.for_elements.get(&for_data.statement.index()) {
                        for_data.guarded_element.set_loan(loan);
                    }
                }
            }
            self.restore_map_paths(&first.map_paths, &first.bindings, first.block, span)?;
            return Ok(first.result);
        }

        let mut slots = Vec::new();
        let mut parameter_types = Vec::new();
        let first_result = first.result;
        if exits.iter().any(|exit| exit.result != first_result) {
            parameter_types
                .push(self.merged_value_type(exits.iter().map(|exit| exit.result), span)?);
            slots.push(MergeSlot::Result);
        }
        for &symbol in baseline.keys() {
            let values = exits
                .iter()
                .map(|exit| exit.bindings.get(&symbol).copied())
                .collect::<Vec<_>>();
            if values.iter().all(Option::is_none) {
                continue;
            }
            let values = values
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let first_value = values[0];
            if values.iter().copied().any(|value| value != first_value) {
                parameter_types.push(self.merged_value_type(values.into_iter(), span)?);
                slots.push(MergeSlot::Binding(symbol));
            }
        }

        let mut temporaries = first.temporaries.clone();
        temporaries.retain(|key, _| exits.iter().all(|exit| exit.temporaries.contains_key(key)));
        for (&key, &owner) in &temporaries {
            parameter_types.push(
                self.function
                    .entity(EntityId::Value(owner))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::Temporary(key));
        }
        let mut loans = first.loans.clone();
        loans.retain(|key, _| exits.iter().all(|exit| exit.loans.contains_key(key)));
        for (&key, &loan) in loans.iter() {
            if let Some(loan) = loan {
                parameter_types.push(
                    self.function
                        .entity(EntityId::Loan(loan))
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty,
                );
                slots.push(MergeSlot::PendingLoan(key));
            }
        }

        let mut capture_loans = first.capture_loans.clone();
        capture_loans.retain(|key, _| {
            exits
                .iter()
                .all(|exit| exit.capture_loans.contains_key(key))
        });
        for (&key, &loan) in &capture_loans {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::CaptureLoan(key));
        }

        for (&symbol, &proof) in &first.views {
            if !baseline.contains_key(&symbol) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            parameter_types.push(EntityType::Loan {
                kind: crate::ssa::model::LoanKind::Shared,
                target: proof.inner,
            });
            slots.push(MergeSlot::NonNullView(symbol));
        }

        let entry_loans = self.entry_loans();
        let mut borrow_bindings = first.borrow_bindings.clone();
        borrow_bindings.retain(|key, loan| {
            !entry_loans.contains(loan)
                && exits
                    .iter()
                    .all(|exit| exit.borrow_bindings.contains_key(key))
        });
        for (&symbol, &loan) in &borrow_bindings {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::BorrowBinding(symbol));
        }

        let mut result_sources = first.result_source_loans.clone();
        if exits.iter().any(|exit| {
            exit.result_source_loans.keys().ne(result_sources.keys())
                || result_sources
                    .iter()
                    .any(|(key, loans)| exit.result_source_loans[key].len() != loans.len())
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        for (&symbol, loans) in &result_sources {
            for (index, loan) in loans.iter().enumerate() {
                parameter_types.push(
                    self.function
                        .entity(EntityId::Loan(*loan))
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                        .ty,
                );
                slots.push(MergeSlot::ResultSource(symbol, index));
            }
        }

        let mut for_sources = first.for_sources.clone();
        for_sources.retain(|key, _| exits.iter().all(|exit| exit.for_sources.contains_key(key)));
        for (&stmt, &loan) in &for_sources {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::ForSource(stmt));
        }

        let mut for_elements = first.for_elements.clone();
        for_elements.retain(|key, _| exits.iter().all(|exit| exit.for_elements.contains_key(key)));
        for (&stmt, &loan) in &for_elements {
            parameter_types.push(
                self.function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
            slots.push(MergeSlot::ForElement(stmt));
        }

        self.map_merge_slots(&exits, &map_paths, &mut slots, &mut parameter_types, span)?;

        // Group 等表达式记录可指向同一实体；所有入边都相同时共享一个参数。
        let mut columns = IncomingColumns::new(&exits);
        let mut unique_types = Vec::new();
        for (&slot, ty) in slots.iter().zip(parameter_types) {
            let choice = columns.associate(&MergeProjection { slot, span })?;
            carry::record_type(
                &mut unique_types,
                choice,
                ty,
                error(LoweringErrorKind::MissingFact, span),
            )?;
        }
        self.validate_pending_field_merge_columns(&exits, columns.columns(), span)?;
        let merge = self
            .function
            .add_block(unique_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        for (exit, arguments) in exits.iter().zip(columns.arguments()) {
            self.function
                .set_terminator(
                    exit.block,
                    TerminatorKind::Branch(Edge {
                        target: merge,
                        arguments,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        }

        let parameters = self
            .function
            .block(merge)
            .expect("new merge block must exist")
            .parameters
            .clone();
        let mut result = first_result;
        let mut bindings = baseline
            .keys()
            .filter_map(|symbol| {
                first
                    .bindings
                    .get(symbol)
                    .copied()
                    .map(|binding| (*symbol, binding))
            })
            .collect::<BTreeMap<_, _>>();
        let mut view_parameters = Vec::new();
        for (slot, column) in slots.into_iter().zip(columns.role_columns()) {
            let parameter = parameters[column.index()];
            if matches!(slot, MergeSlot::MapSource(_) | MergeSlot::MapCursor(_)) {
                map_frames::rebind_merge_slot(&mut map_paths, slot, parameter, span)?;
                continue;
            }
            if let MergeSlot::ResultSource(symbol, index) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                result_sources
                    .get_mut(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?[index] = loan;
                continue;
            }
            if let MergeSlot::PendingLoan(key) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                let child = loans.child(&key);
                loans.rebind(key, loan, child);
                continue;
            }
            if let MergeSlot::CaptureLoan(key) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                capture_loans.insert(key, loan);
                continue;
            }
            if let MergeSlot::NonNullView(symbol) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                view_parameters.push((symbol, loan, first.views[&symbol]));
                continue;
            }
            if let MergeSlot::BorrowBinding(symbol) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                borrow_bindings.insert(symbol, loan);
                continue;
            }
            if let MergeSlot::ForSource(stmt) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for_sources.insert(stmt, loan);
                continue;
            }
            if let MergeSlot::ForElement(stmt) = slot {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for_elements.insert(stmt, loan);
                continue;
            }
            let parameter = LoweredValue::Value(value(parameter));
            match slot {
                MergeSlot::Result => result = parameter,
                MergeSlot::Temporary(key) => {
                    if let LoweredValue::Value(owner) = parameter {
                        temporaries.insert(key, owner);
                    }
                }
                MergeSlot::PendingLoan(_)
                | MergeSlot::CaptureLoan(_)
                | MergeSlot::NonNullView(_)
                | MergeSlot::BorrowBinding(_)
                | MergeSlot::ResultSource(_, _)
                | MergeSlot::ForSource(_)
                | MergeSlot::ForElement(_)
                | MergeSlot::MapSource(_)
                | MergeSlot::MapCursor(_) => {
                    unreachable!("handled loan parameter")
                }
                MergeSlot::Binding(symbol) => {
                    bindings.insert(symbol, parameter);
                }
            }
        }
        let views = self.merged_non_null_proofs(&bindings, view_parameters, span)?;
        self.block = merge;
        self.bindings = bindings;
        self.temporaries = temporaries;
        self.pending_call_loans = loans;
        self.capture_loans = capture_loans;
        self.result_source_loans = result_sources;
        self.non_null_bindings = views;
        self.validate_pending_field_children(
            &self.bindings,
            &self.non_null_bindings,
            &self.pending_call_loans,
            span,
        )?;
        self.borrow_bindings
            .retain(|_, loan| entry_loans.contains(loan));
        self.borrow_bindings.extend(borrow_bindings);
        for context in &mut self.loops {
            if let Some(ref mut for_data) = context.for_loop {
                if let Some(&loan) = for_sources.get(&for_data.statement.index()) {
                    for_data.rebind_source(loan);
                }
                if let Some(&loan) = for_elements.get(&for_data.statement.index()) {
                    for_data.guarded_element.set_loan(loan);
                }
            }
        }
        self.finish_map_merge(&map_paths, span)?;
        Ok(result)
    }

    fn merged_value_type(
        &self,
        values: impl Iterator<Item = LoweredValue>,
        span: Span,
    ) -> Result<EntityType, LoweringError> {
        let mut ty = None;
        for value in values {
            let entity = lowered_entity(value, span)?;
            let current = self
                .function
                .entity(entity)
                .map(|data| data.ty)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            if ty.is_some_and(|expected| expected != current) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            ty = Some(current);
        }
        ty.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }
}
