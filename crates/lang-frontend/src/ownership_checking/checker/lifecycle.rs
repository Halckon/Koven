//! Single ownership checker setup, file execution and fact publication.
use super::*;

impl<'a> Checker<'a> {
    pub(super) fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        names: &'a NameResolution,
        typed: &'a TypedFile,
    ) -> Result<Self, OwnershipCheckingError> {
        let symbols_by_span = names
            .symbols()
            .iter()
            .map(|symbol| (span_key(symbol.span()), symbol.id()))
            .collect();
        let references_by_span = names
            .references()
            .iter()
            .filter_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
                _ => None,
            })
            .collect();
        let mut calls_by_expression: BTreeMap<usize, Vec<ParameterMode>> = typed
            .calls()
            .iter()
            .map(|call| {
                let mut modes = vec![ParameterMode::Borrow; call.arguments().len()];
                for argument in call.arguments() {
                    modes[argument.argument_index()] = argument.mode();
                }
                (call.expression().index(), modes)
            })
            .collect();
        let cross_thread_by_expression = typed
            .calls()
            .iter()
            .map(|call| {
                let mut effects = vec![false; call.arguments().len()];
                for argument in call.arguments() {
                    effects[argument.argument_index()] = argument.crosses_thread();
                }
                (call.expression().index(), effects)
            })
            .collect();
        let mut receivers_by_expression = typed
            .calls()
            .iter()
            .filter_map(|call| call.receiver().map(|r| (call.expression().index(), r)))
            .collect();
        container::populate_container_call_contracts(
            typed,
            &mut receivers_by_expression,
            &mut calls_by_expression,
        );
        map::populate_map_call_contracts(
            parsed,
            typed,
            &mut receivers_by_expression,
            &mut calls_by_expression,
        );
        let capture::Analysis {
            captures,
            closures,
            transferabilities,
        } = capture::analyze(parsed, names, typed)?;
        let mut checker = Self {
            iterations: BTreeMap::new(),
            map_iterations: BTreeMap::new(),
            sources,
            parsed,
            names,
            typed,
            symbols_by_span,
            references_by_span,
            calls_by_expression,
            call_descriptors: typed
                .calls()
                .iter()
                .map(|call| (call.expression().index(), call))
                .collect(),
            receivers_by_expression,
            construction: construction::Analysis::new(parsed, typed)?,
            rc_effects: Vec::new(),
            cross_thread_by_expression,
            callable_sources: Default::default(),
            variable_kinds: BTreeMap::new(),
            field_kinds: BTreeMap::new(),
            current_receiver_mode: None,
            checking_borrow_return: false,
            allowed_borrow_call: None,
            no_carry: None,
            no_carry_prefix: Default::default(),
            borrow_results: Default::default(),
            range_producers: super::super::range_producer::collect_single(
                sources, parsed, names, typed,
            )?,
            current_borrow_return: None,
            borrow_return_origins: Vec::new(),
            loop_has_exit: BTreeMap::new(),
            constant_materializations: BTreeMap::new(),
            nullable_whens: BTreeMap::new(),
            non_null_assertions: BTreeMap::new(),
            ownership_primitives: BTreeMap::new(),
            field_replacements: BTreeMap::new(),
            diagnostics: Vec::new(),
            formed_owned_locals: Vec::new(),
            loans: Vec::new(),
            deferred: Vec::new(),
            captures,
            closures,
            transferabilities,
            expression_live_after: Vec::new(),
            statement_live_after: Vec::new(),
            use_after_move_code: codes::catalog()?.resolve(codes::USE_AFTER_MOVE)?,
            partial_move_code: codes::catalog()?.resolve(codes::PARTIAL_MOVE)?,
            borrowed_move_code: codes::catalog()?.resolve(codes::MOVE_FROM_BORROWED_BINDING)?,
            immutable_inout_code: codes::catalog()?.resolve(codes::IMMUTABLE_INOUT_PLACE)?,
            loan_conflict_code: codes::catalog()?.resolve(codes::LOAN_CONFLICT)?,
            container_element_move_code: codes::catalog()?
                .resolve(codes::MOVE_FROM_CONTAINER_ELEMENT)?,
            borrowed_closure_escape_code: codes::catalog()?
                .resolve(codes::BORROWED_CLOSURE_ESCAPE)?,
            illegal_owned_capture_code: codes::catalog()?.resolve(codes::ILLEGAL_OWNED_CAPTURE)?,
            non_transferable_delivery_code: codes::catalog()?
                .resolve(codes::NON_TRANSFERABLE_DELIVERY)?,
        };
        for (item, _) in parsed.ast().items().iter() {
            checker.collect_mutability(item)?;
        }
        Ok(checker)
    }

    pub(super) fn run(mut self) -> Result<OwnershipCheckedFile, OwnershipCheckingError> {
        let map_inputs_valid = self.map_iteration_inputs_valid();
        if map_inputs_valid
            && let Some((statement, reason)) =
                crate::type_checking::map_iteration_validation::file_map_iteration_rows_error(
                    self.parsed,
                    self.names,
                    self.typed,
                )
        {
            return Err(OwnershipCheckingError::InvalidMapIterationFacts {
                statement: Some(statement.index()),
                failure: super::super::IterationFactError {
                    span: self
                        .parsed
                        .ast()
                        .statements()
                        .get(statement)
                        .ok()
                        .map(|node| node.span()),
                    reason,
                },
            });
        }
        let liveness = drop_planner::capture_liveness(&self)?;
        self.expression_live_after = liveness.expression_after;
        self.statement_live_after = liveness.statement_after;
        let mut state = State::default();
        for &root in self.parsed.roots() {
            self.check_item(root, &mut state)?;
        }
        self.reject_unmodeled_own_receivers()?;
        let diagnostics = ordered_diagnostics(self.sources, &self.diagnostics)?
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        if !diagnostics.is_empty() {
            self.borrow_return_origins.clear();
            self.borrow_results = Default::default();
            self.loans.clear();
            self.rc_effects.clear();
        }
        let mut bindings: Vec<_> = self
            .typed
            .parameter_bindings()
            .iter()
            .map(|binding| {
                let kind = match binding.mode() {
                    ParameterMode::Value => OwnershipBindingKind::Owned,
                    ParameterMode::Borrow => OwnershipBindingKind::Shared,
                    ParameterMode::Inout => OwnershipBindingKind::Exclusive,
                };
                OwnershipBindingDescriptor::new(binding.symbol(), kind)
            })
            .collect();
        let drop_plan = if diagnostics.is_empty() {
            drop_planner::plan(&self)?
        } else {
            drop_planner::DropPlan::default()
        };
        let drop_planner::DropPlan {
            cleanup_steps,
            cleanup_conditions,
            drops,
            loan_ends,
            iterations,
            map_iterations,
            map_iteration_required_exits,
            deferred,
            borrow_ends,
        } = drop_plan;
        bindings.extend(
            map_iterations
                .iter()
                .flat_map(|plan| plan.bindings().iter().copied()),
        );
        bindings.sort_by_key(|binding| binding.symbol().index());
        let publish_maps = diagnostics.is_empty()
            && self.deferred.is_empty()
            && map_inputs_valid
            && deferred.is_empty();
        let map_iteration_descriptors = if publish_maps {
            self.typed.map_iterations().to_vec()
        } else {
            Vec::new()
        };
        let map_iteration_templates = if publish_maps {
            self.map_iterations.values().cloned().collect()
        } else {
            Vec::new()
        };
        self.borrow_results.ends = borrow_ends;
        self.deferred.extend(deferred);
        if !self.deferred.is_empty() {
            self.borrow_results = Default::default();
            self.borrow_return_origins.clear();
        }
        self.finish_nullable_drops(&drops)?;
        let callable_provenance = self
            .finish_callable_sources(diagnostics.is_empty() && self.typed.diagnostics().is_empty());
        let constant_materializations = if diagnostics.is_empty()
            && self.deferred.is_empty()
            && self.typed.constants().is_some()
        {
            Some(super::super::ValidatedConstantMaterializations {
                typed_analysis_owner: self.typed.analysis_owner().clone(),
                plans: self.constant_materializations.into_values().collect(),
            })
        } else {
            None
        };
        let construction_plans = self.construction.finish(diagnostics.is_empty());
        let non_null_assertions = if diagnostics.is_empty() {
            self.non_null_assertions.into_values().collect()
        } else {
            Vec::new()
        };
        let nullable_whens = if diagnostics.is_empty() {
            self.nullable_whens.into_values().collect()
        } else {
            Vec::new()
        };
        let ownership_primitives = if diagnostics.is_empty() && self.deferred.is_empty() {
            self.ownership_primitives.into_values().collect()
        } else {
            Vec::new()
        };
        let field_replacements = if diagnostics.is_empty() && self.deferred.is_empty() {
            self.field_replacements.into_values().collect()
        } else {
            Vec::new()
        };
        let captures = if diagnostics.is_empty() {
            self.captures
        } else {
            Vec::new()
        };
        if diagnostics.is_empty() && self.typed.diagnostics().is_empty() && self.deferred.is_empty()
        {
            self.formed_owned_locals.sort();
            self.formed_owned_locals.dedup();
        } else {
            self.formed_owned_locals.clear();
        }
        let owned = OwnershipCheckedFile::new(
            self.parsed.source_id(),
            self.typed.environment_owner().clone(),
            self.typed.analysis_owner().clone(),
            diagnostics,
            OwnershipCheckedParts {
                callable_provenance,
                borrow_results: self.borrow_results,
                borrow_return_origins: self.borrow_return_origins,
                cleanup_steps,
                cleanup_conditions,
                iterations,
                map_iterations,
                map_iteration_templates,
                map_iteration_descriptors,
                map_iteration_required_exits,
                constant_materializations,
                non_null_assertions,
                ownership_primitives,
                field_replacements,
                nullable_whens,
                loan_ends,
                bindings,
                formed_owned_locals: self.formed_owned_locals,
                drops,
                construction_plans,
                string_effects: self
                    .typed
                    .string_operations()
                    .iter()
                    .copied()
                    .filter(|operation| {
                        self.deferred.is_empty()
                            && self.loans.iter().any(|loan| {
                                loan.call() == operation.expression()
                                    && loan.argument() == operation.receiver()
                            })
                    })
                    .map(super::super::StringOwnershipEffect::new)
                    .collect(),
                loans: self.loans,
                rc_effects: self.rc_effects,
                captures,
                closures: self.closures,
                transferabilities: self.transferabilities,
                deferred: self.deferred,
            },
        );
        if owned.diagnostics().is_empty() && owned.deferred().is_empty() && map_inputs_valid {
            super::super::map_iteration_validation::validate_map_iteration_facts(
                self.parsed,
                self.names,
                self.typed,
                &owned,
            )
            .map_err(|failure| OwnershipCheckingError::InvalidMapIterationFacts {
                statement: None,
                failure,
            })?;
        }
        Ok(owned)
    }
}
