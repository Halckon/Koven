#[allow(unused_imports)] pub(in crate::ssa) use crate::ssa::map_provider_exits_pipeline_tests::temporary_forms::graph::structure as structure;
// Original conditional break source, admitted through the same Unit analysis.
use super::unit_iteration::{Control, assert_facts};

fn source(live: bool) -> String {
    format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); if (key == 1) {{ break }} }}; {} }}",
        if live { "cache.contains(1)" } else { "" }
    )
}

fn check(live: bool) {
    let text = source(live);
    let (program, analysis) = super::lower_map_unit(&text, "main");
    let origins = assert_facts(&analysis, Control::Break, live);
    structure::assert_exit_structure(&program, origins.statement, origins.source, live, true);
}

#[test]
fn single_map_break_live_source_red() {
    check(true);
}

#[test]
fn single_map_break_dead_source_red() {
    check(false);
}

fn assert_unit_boundary(live: bool, constants: bool, remove: bool) {
    use crate::ssa::unit_lower_test_support::{declaration, parsed};
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        },
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let body = source(live);
    let body = if remove {
        body.replace("cache.put(key, \"value\".clone())", "cache.remove(key)")
    } else {
        body
    };
    let text = format!("package p\n{body}");
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source_id, &file)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let entry = declaration(&names, "p", "main");
    let result = if constants {
        let typed = result.validate_constants().unwrap();
        let owned =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let typed = result.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    let (program, entry) = result.unwrap_or_else(|error| {
        panic!("unit constants={constants}, live={live}, remove={remove}: {error:?}")
    });
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let (loop_span, source_expression) = file
        .ast()
        .statements()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            lang_frontend::parser::Statement::For { source, .. } => Some((node.span(), *source)),
            _ => None,
        })
        .unwrap();
    let source_span = file
        .ast()
        .expressions()
        .get(source_expression)
        .unwrap()
        .span();
    // Unit EndElement/EndSource carry the provider statement's actual origin.
    structure::assert_exit_structure(&program, loop_span, source_span, live, true);
}

#[test]
fn unit_map_break_basic_live_is_independent() {
    assert_unit_boundary(true, false, false);
}
#[test]
fn unit_map_break_basic_dead_has_exit_cleanup() {
    assert_unit_boundary(false, false, false);
}
#[test]
fn unit_map_break_constant_live_is_independent() {
    assert_unit_boundary(true, true, false);
}
#[test]
fn unit_map_break_constant_dead_has_exit_cleanup() {
    assert_unit_boundary(false, true, false);
}

#[test]
fn single_map_break_unconditional_and_multiple_exits_remain_explicit() {
    for body in ["break", "if (key == 1) { break }; if (key == 2) { break }"] {
        let text = source(true).replace("if (key == 1) { break }", body);
        crate::ssa::map_unit_consumer_test_support::assert_source(&text, &["main"]);
    }
}

#[test]
fn single_map_break_matrix_has_real_ssa_and_separate_owner_drops() {
    for (text, counts) in [
        (
            include_str!("../../tests/fixtures/map-conditional-break-0310/live-dead.ko"),
            vec![
                ("removeDead", 2),
                ("removeLive", 1),
                ("putDead", 2),
                ("putLive", 1),
            ],
        ),
        (
            include_str!("../../tests/fixtures/map-conditional-break-0310/distinct-owners.ko"),
            vec![("parameterOwner", 1), ("separateOwners", 2)],
        ),
    ] {
        let (program, analysis) = super::lower_map_unit(text, "main");
        super::assert_unit_map_drop_counts(&program, &analysis, &counts);
    }
}

#[test]
fn unit_map_break_remove_uses_current_owner_on_each_exit() {
    for constants in [false, true] {
        for live in [false, true] {
            assert_unit_boundary(live, constants, true);
        }
    }
}

#[test]
fn unit_map_break_original_full_fixture_has_separate_owner_drops() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let source = include_str!("../../tests/fixtures/map-conditional-break-0310/live-dead.ko");
    let analysis = UnitSource::analyze("map-break-live-dead.ko", source).unwrap();
    analysis.assert_frontend_clean();
    let entry = analysis.declaration("main", SymbolKind::Function);
    let (program, entry) = analysis.lower_entry(entry).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let expected = [
        ("removeDead", 2),
        ("removeLive", 1),
        ("putDead", 2),
        ("putLive", 1),
    ]
    .into_iter()
    .map(|(name, count)| {
        let functions = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| {
                let span = function.origin.span();
                matches!(&function.origin, crate::ssa::model::Origin::Source(_))
                    && source[span.start()..span.end()].starts_with(&format!("fun {name}("))
            })
            .collect::<Vec<_>>();
        assert_eq!(functions.len(), 1, "one real source definition for {name}");
        (functions[0].name.as_str(), count)
    })
    .collect::<Vec<_>>();
    super::assert_map_drop_counts(&program, &expected);
}

pub(in crate::ssa) mod loop_control {
    //! `while` / `loop` 的 backedge、jump 与 loop-carried binding lowering。

    #[path = "lower_frontend/loop_control/iteration.rs"]
    pub(in crate::ssa) mod iteration;
    pub(in crate::ssa) use crate::ssa::carry::columns::nullable_while;
    pub(in crate::ssa) use crate::ssa::lowering_support_tests::numeric_iteration;

    use std::collections::BTreeMap;

    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        name_resolution::SymbolId,
        source::Span,
    };

    use crate::ssa::lower_frontend::{
        ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind,
        control::{BranchExit, LinearBindingSlot, LinearBindings},
        error,
    };
    use crate::ssa::model::{
        BlockId, Edge, EntityId, EntityType, LoanId, LoanKind, Operation, Origin, SsaTypeId,
        TerminatorKind, ValueId,
    };

    pub(in crate::ssa) struct LoopJump {
        pub(in crate::ssa) exit: BranchExit,
        pub(in crate::ssa) span: Span,
    }

    pub(in crate::ssa) struct ForLoopData {
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) provider_header: crate::ssa::provider::ProviderHeader,
        pub(in crate::ssa) guarded_element: crate::ssa::provider::GuardedElement,
        pub(in crate::ssa) int_type: SsaTypeId,
        pub(in crate::ssa) plan: lang_frontend::ownership_checking::IterationOwnershipPlan,
        pub(in crate::ssa) new_source_loan: Option<LoanId>,
        pub(in crate::ssa) active_source_loan: Option<LoanId>,
        pub(in crate::ssa) body_source: LoanId,
        pub(in crate::ssa) body_cursor: ValueId,
        pub(in crate::ssa) source_slot: usize,
        pub(in crate::ssa) length_slot: usize,
        pub(in crate::ssa) cursor_slot: usize,
        pub(in crate::ssa) is_carried_source: bool,
    }

    impl ForLoopData {
        pub(in crate::ssa) fn rebind_source(&mut self, loan: LoanId) {
            self.body_source = loan;
            // A sibling may have ended its own source. Restore this path from ownership,
            // not the previous path's active state; borrowed sources remain caller-owned.
            self.active_source_loan =
                (self.new_source_loan.is_some() && !self.is_carried_source).then_some(loan);
        }
    }

    pub(in crate::ssa) struct LoopContext {
        pub(in crate::ssa) entry_views: BTreeMap<SymbolId, crate::ssa::lower_frontend::NonNullProof>,
        pub(in crate::ssa) outer_borrowed_while: Option<crate::ssa::lower_frontend::outer_borrowed_while::Relation>,
        pub(in crate::ssa) header: BlockId,
        pub(in crate::ssa) carried: LinearBindings,
        pub(in crate::ssa) continues: Vec<LoopJump>,
        pub(in crate::ssa) breaks: Vec<BranchExit>,
        pub(in crate::ssa) for_loop: Option<ForLoopData>,
        pub(in crate::ssa) numeric_loop: Option<numeric_iteration::NumericLoopData>,
        pub(in crate::ssa) map_loop: Option<crate::ssa::lower_frontend::map_iteration::Context>,
    }

    impl LoopContext {
        pub(in crate::ssa) fn iteration_plan(
            &self,
        ) -> Option<&lang_frontend::ownership_checking::IterationOwnershipPlan> {
            self.for_loop
                .as_ref()
                .map(|data| &data.plan)
                .or_else(|| self.numeric_loop.as_ref().map(|data| &data.plan))
        }
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_while(
            &mut self,
            statement: StatementId,
            condition: ExpressionId,
            body: StatementId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let inherited = if let Some((descriptor, subject)) =
                self.direct_nullable_while_condition(condition, span)?
            {
                match self.qualify_nullable_while_entry(statement, descriptor, subject, span)? {
                    nullable_while::NullableWhileEntry::Readonly(root, proof) => Some((root, proof)),
                    nullable_while::NullableWhileEntry::Direct(entry) => {
                        return self
                            .lower_direct_nullable_while(statement, condition, body, entry, span);
                    }
                }
            } else {
                None
            };
            let baseline = self.bindings.clone();
            let context = self.create_loop_header(&baseline, span)?;
            let condition = self.require_value(condition)?;
            let condition_bindings = self.bindings.clone();
            let carried = self.linear_binding_slots(&condition_bindings, span)?;
            let parameter_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
            let body_block = self
                .function
                .add_block(parameter_types.clone(), Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let false_block = self
                .function
                .add_block(parameter_types, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Conditional {
                        condition,
                        when_true: loop_edge(body_block, &carried),
                        when_false: loop_edge(false_block, &carried),
                    },
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

            // Save the false edge before body lowering changes any owner or proof state.
            self.block = false_block;
            self.bindings =
                self.rebind_linear_bindings(&condition_bindings, false_block, &carried, span)?;
            let false_exit = self.loop_exit();
            self.block = body_block;
            self.bindings =
                self.rebind_linear_bindings(&condition_bindings, body_block, &carried, span)?;
            self.loops.push(context);
            let body_result = self.lower_statement(body)?;
            if !matches!(body_result, LoweredValue::Diverged) {
                self.record_continue(span)?;
            }
            let context = self.loops.pop().expect("while context must be balanced");
            if let Some((root, proof)) = inherited {
                for jump in &context.continues {
                    self.validate_readonly_while_exit(root, proof, &jump.exit, jump.span)?;
                }
            }
            self.finish_loop_continues(&context)?;
            for exit in context.breaks.iter().chain(std::iter::once(&false_exit)) {
                self.validate_pending_field_loop_exit(&context.carried, exit, span)?;
            }
            let mut exits = context.breaks;
            exits.push(false_exit);
            let result = self.finish_loop_exits(statement, exits, &baseline, inherited, None, span)?;
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
            }
            Ok(result)
        }

        pub(in crate::ssa) fn lower_loop(
            &mut self,
            statement: StatementId,
            body: StatementId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let baseline = self.bindings.clone();
            let context = self.create_loop_header(&baseline, span)?;
            self.loops.push(context);
            let body_result = self.lower_statement(body)?;
            if !matches!(body_result, LoweredValue::Diverged) {
                self.record_continue(span)?;
            }
            let context = self.loops.pop().expect("loop context must be balanced");
            self.finish_loop_continues(&context)?;
            self.finish_loop_exits(statement, context.breaks, &baseline, None, None, span)
        }

        pub(in crate::ssa) fn finish_loop_exits(
            &mut self,
            statement: StatementId,
            mut exits: Vec<BranchExit>,
            baseline: &BTreeMap<SymbolId, LoweredValue>,
            inherited: Option<(SymbolId, crate::ssa::lower_frontend::NonNullProof)>,
            provider: Option<&crate::ssa::lower_frontend::outer_borrowed_while::Relation>,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            if let Some(provider) = provider {
                self.validate_outer_borrowed_exit_states(provider, &exits, span)?;
            }
            // Different paths may already have consumed an owner; discharge each exit before joining.
            for exit in &mut exits {
                if let Some(provider) = provider {
                    self.restore_outer_borrowed_state(provider, exit, span)?;
                }
                if let Some((root, proof)) = inherited {
                    self.validate_readonly_while_exit(root, proof, exit, span)?;
                }
                self.block = exit.block;
                self.bindings.clone_from(&exit.bindings);
                self.temporaries.clone_from(&exit.temporaries);
                self.pending_call_loans.clone_from(&exit.loans);
                self.capture_loans.clone_from(&exit.capture_loans);
                self.borrow_bindings.clone_from(&exit.borrow_bindings);
                self.result_source_loans
                    .clone_from(&exit.result_source_loans);
                self.non_null_bindings.clone_from(&exit.views);
                self.emit_drops(lang_frontend::ownership_checking::DropPoint::LoopExit(
                    statement,
                ))?;
                if let Some(provider) = provider {
                    self.validate_outer_borrowed_exit_states(provider, &[self.loop_exit()], span)?;
                }
                *exit = self.loop_exit();
                if let Some((root, proof)) = inherited {
                    self.validate_readonly_while_exit(root, proof, exit, span)?;
                }
            }
            self.merge_exits(exits, baseline, span)
        }

        pub(in crate::ssa) fn loop_exit(&self) -> BranchExit {
            self.branch_exit(LoweredValue::Unit)
        }

        pub(in crate::ssa) fn lower_break(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
            if !self.retire_outer_borrowed_break(span)? {
                self.retire_while_proofs(false, span)?;
            }
            let exit = self.loop_exit();
            let context = self
                .loops
                .last_mut()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            context.breaks.push(exit);
            Ok(LoweredValue::Diverged)
        }

        pub(in crate::ssa) fn lower_continue(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
            self.record_continue(span)?;
            Ok(LoweredValue::Diverged)
        }

        pub(in crate::ssa) fn record_continue(&mut self, span: Span) -> Result<(), LoweringError> {
            self.retire_while_proofs(false, span)?;
            let jump = LoopJump {
                exit: self.loop_exit(),
                span,
            };
            let context = self
                .loops
                .last_mut()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            context.continues.push(jump);
            Ok(())
        }

        pub(in crate::ssa) fn finish_loop_continues(&mut self, context: &LoopContext) -> Result<(), LoweringError> {
            for jump in &context.continues {
                let arguments = context
                    .carried
                    .slots
                    .iter()
                    .map(|slot| loop_slot_entity(self, slot, &jump.exit, jump.span))
                    .collect::<Result<Vec<_>, _>>()?;
                self.validate_pending_field_edge(&context.carried, &jump.exit, &arguments, jump.span)?;
                self.function
                    .set_terminator(
                        jump.exit.block,
                        TerminatorKind::Branch(Edge {
                            target: context.header,
                            arguments,
                        }),
                        Origin::Source(jump.span),
                    )
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, jump.span))?;
            }
            Ok(())
        }

        pub(in crate::ssa) fn create_loop_header(
            &mut self,
            baseline: &BTreeMap<SymbolId, LoweredValue>,
            span: Span,
        ) -> Result<LoopContext, LoweringError> {
            let mut carried = self.linear_binding_slots(baseline, span)?;
            // Scalar bindings may change on each iteration, so loops carry them too.
            for (&symbol, &binding) in baseline {
                match binding {
                    LoweredValue::Unit => {}
                    LoweredValue::Value(value) => {
                        if carried.slots.iter().any(|slot| slot.symbol == Some(symbol)) {
                            continue;
                        }
                        let source = EntityId::Value(value);
                        let ty = self
                            .function
                            .entity(source)
                            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                            .ty;
                        crate::ssa::carry::seed_slot(
                            &mut carried.slots,
                            LinearBindingSlot::new(Some(symbol), source, ty),
                        );
                    }
                    LoweredValue::Diverged => {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                }
            }
            let header = self
                .function
                .add_block(
                    carried.slots.iter().map(|slot| slot.ty).collect(),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Branch(loop_edge(header, &carried)),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.bindings = self.rebind_linear_bindings(baseline, header, &carried, span)?;
            self.block = header;
            Ok(LoopContext {
                entry_views: self.non_null_bindings.clone(),
                outer_borrowed_while: None,
                header,
                carried,
                continues: Vec::new(),
                breaks: Vec::new(),
                for_loop: None,
                numeric_loop: None,
                map_loop: None,
            })
        }

        pub(in crate::ssa) fn lower_for_break(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            if self
                .loops
                .last()
                .is_some_and(|context| context.map_loop.is_some())
            {
                return self.lower_map_iteration_break(expression, span);
            }
            if self
                .loops
                .last()
                .is_some_and(|context| context.numeric_loop.is_some())
            {
                return self.lower_numeric_break(expression, span);
            }
            let (plan, statement) = {
                let context = self
                    .loops
                    .last()
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let for_data = context
                    .for_loop
                    .as_ref()
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                (for_data.plan.clone(), for_data.statement)
            };
            let break_exit = plan
                .exits()
                .iter()
                .find(|e| {
                    matches!(
                        e.kind(),
                        lang_frontend::ownership_checking::IterationExitKind::Break(expr) if expr == expression
                    )
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_iteration_exit_plan(break_exit, span)?;
            self.finish_map_loop_break(statement, expression, span)?;
            let mut exit = self.loop_exit();
            exit.for_sources.remove(&statement.index());
            exit.for_elements.remove(&statement.index());
            let context = self.loops.last_mut().expect("loop context present");
            context.breaks.push(exit);
            Ok(LoweredValue::Diverged)
        }

        pub(in crate::ssa) fn lower_for_continue(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            if self
                .loops
                .last()
                .is_some_and(|context| context.map_loop.is_some())
            {
                return self.lower_map_iteration_continue(expression, span);
            }
            if self
                .loops
                .last()
                .is_some_and(|context| context.numeric_loop.is_some())
            {
                return self.lower_numeric_continue(expression, span);
            }
            let (
                plan,
                header_block,
                int_type,
                body_cursor,
                step,
                body_source,
                is_carried_source,
                header_length,
            ) = {
                let context = self
                    .loops
                    .last()
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let for_data = context
                    .for_loop
                    .as_ref()
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                (
                    for_data.plan.clone(),
                    context.header,
                    for_data.int_type,
                    for_data.body_cursor,
                    for_data.guarded_element.step(),
                    for_data.body_source,
                    for_data.is_carried_source,
                    for_data.provider_header.length,
                )
            };
            let continue_exit = plan
                .exits()
                .iter()
                .find(|e| {
                    matches!(
                        e.kind(),
                        lang_frontend::ownership_checking::IterationExitKind::Continue(expr) if expr == expression
                    )
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_iteration_exit_plan(continue_exit, span)?;

            let (_, next_res) = self.append(
                Operation::Binary {
                    operator: crate::ssa::model::BinaryOperator::Add,
                    left: body_cursor,
                    right: step,
                },
                vec![EntityType::Value(int_type)],
                span,
            )?;
            let EntityId::Value(next_cursor) = next_res[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };

            let current_exit = self.loop_exit();
            let context = self.loops.last().expect("loop context present");
            let mut backedge_args = context
                .carried
                .slots
                .iter()
                .map(|slot| loop_slot_entity(self, slot, &current_exit, span))
                .collect::<Result<Vec<_>, _>>()?;
            if !is_carried_source {
                backedge_args.push(EntityId::Loan(body_source));
            }
            backedge_args.push(EntityId::Value(header_length));
            backedge_args.push(EntityId::Value(next_cursor));

            let backedge = Edge {
                target: header_block,
                arguments: backedge_args,
            };
            self.record_map_loop_backedge(
                plan.descriptor().statement(),
                lang_frontend::ownership_checking::IterationPhiIncomingKind::Continue(expression),
                &backedge,
                span,
            )?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Branch(backedge),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

            Ok(LoweredValue::Diverged)
        }

        pub(in crate::ssa) fn emit_iteration_exit_plan(
            &mut self,
            exit: &lang_frontend::ownership_checking::IterationExitPlan,
            span: Span,
        ) -> Result<(), LoweringError> {
            self.preflight_outer_borrowed_iteration(exit, span)?;
            self.preflight_borrowed_while_exit(exit, span)?;
            for action in exit.actions() {
                self.emit_iteration_cleanup_action(action, span)?;
            }
            Ok(())
        }

        pub(in crate::ssa) fn emit_iteration_cleanup_action(
            &mut self,
            action: &lang_frontend::ownership_checking::IterationCleanupAction,
            span: Span,
        ) -> Result<(), LoweringError> {
            use lang_frontend::ownership_checking::{DropTarget, IterationCleanupAction as Action};
            match action {
                Action::Drop(fact) => {
                    if self.emit_local_map_drop(*fact)? {
                        return Ok(());
                    }
                    if self.emit_outer_borrowed_drop(*fact)? {
                        return Ok(());
                    }
                    if fact.condition().is_some()
                        && !crate::ssa::lower_frontend::map_loop_owner::supported_drop(
                            self.parsed,
                            self.names,
                            self.typed,
                            self.owned,
                            *fact,
                        )
                    {
                        return Err(error(
                            LoweringErrorKind::UnsupportedNode,
                            fact.value_origin(),
                        ));
                    }
                    let owner = match fact.target() {
                        DropTarget::Named(symbol) => {
                            self.end_non_null_proof(symbol, fact.value_origin())?;
                            match self.bindings.remove(&symbol) {
                                Some(LoweredValue::Value(value)) => value,
                                _ if fact.condition().is_some()
                                    && !crate::ssa::lower_frontend::constant_presence::constant_drop(
                                        self.parsed,
                                        self.names,
                                        self.typed,
                                        self.owned,
                                        *fact,
                                    ) =>
                                {
                                    return Err(error(
                                        LoweringErrorKind::MissingFact,
                                        fact.value_origin(),
                                    ));
                                }
                                _ => return Ok(()),
                            }
                        }
                        DropTarget::Temporary(expr) => match self.temporaries.remove(&expr.index()) {
                            Some(value) => {
                                self.temporaries.retain(|_, candidate| *candidate != value);
                                value
                            }
                            None => return Ok(()),
                        },
                        _ => return Ok(()),
                    };
                    self.record_map_loop_drop(*fact, owner)?;
                    self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
                    self.release_owner_capture_loans(owner, fact.value_origin())?;
                }
                Action::EndBinding { symbol, statement } => {
                    if let Some(plan) = self.owned.iteration(*statement)
                        && let lang_frontend::type_checking::IterationProvider::NumericRange(endpoint) =
                            plan.descriptor().provider()
                    {
                        if plan.source_kind()
                            != &lang_frontend::ownership_checking::IterationSource::OwnedSnapshot(
                                plan.descriptor().source(),
                            )
                        {
                            return Err(error(LoweringErrorKind::MissingFact, span));
                        }
                        let endpoint_type = self.ssa_builtin(endpoint, span)?;
                        let Some(LoweredValue::Value(value)) = self.bindings.remove(symbol) else {
                            return Err(error(LoweringErrorKind::MissingFact, span));
                        };
                        if self
                            .function
                            .entity(EntityId::Value(value))
                            .map(|entity| entity.ty)
                            != Some(EntityType::Value(endpoint_type))
                        {
                            return Err(error(LoweringErrorKind::MissingFact, span));
                        }
                        return Ok(());
                    }
                    if let Some(loan) = self.borrow_bindings.remove(symbol) {
                        let is_element_loan = self.loops.iter().rev().any(|c| {
                            c.for_loop.as_ref().is_some_and(|f| {
                                f.statement == *statement && f.guarded_element.loan() == loan
                            })
                        });
                        if !is_element_loan {
                            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                        }
                    }
                }
                Action::EndElement(stmt) => {
                    if let Some(context) = self
                        .loops
                        .iter()
                        .rev()
                        .find(|c| c.for_loop.as_ref().is_some_and(|f| f.statement == *stmt))
                        && let Some(ref for_data) = context.for_loop
                    {
                        let loan = for_data.guarded_element.loan();
                        self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                    }
                }
                Action::FinishProvider(_) => {}
                Action::EndSource(stmt) => {
                    if let Some(context) = self
                        .loops
                        .iter_mut()
                        .rev()
                        .find(|c| c.for_loop.as_ref().is_some_and(|f| f.statement == *stmt))
                        && let Some(ref mut for_data) = context.for_loop
                        && let Some(loan) = for_data.active_source_loan.take()
                    {
                        self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                    }
                    if let Some(source) = self
                        .owned
                        .iteration(*stmt)
                        .map(|plan| plan.descriptor().source())
                        && self
                            .owned
                            .borrow_results()
                            .range_uses()
                            .iter()
                            .any(|fact| fact.expression() == source)
                    {
                        self.finish_short_range(source, span)?;
                    }
                }
                Action::EndCallLoan(fact) => {
                    let key = (fact.call().index(), fact.argument().index());
                    if self.pending_call_loans.contains_key(&key) {
                        self.end_pending_source_loan(key, span)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    pub(in crate::ssa) fn loop_edge(target: BlockId, carried: &LinearBindings) -> Edge {
        Edge {
            target,
            arguments: carried.slots.iter().map(|slot| slot.source).collect(),
        }
    }

    /// Resolve header slots from the exit snapshot, after nested CFG has rebound IDs.
    pub(in crate::ssa) fn loop_slot_entity(
        lowerer: &ExpressionLowerer<'_>,
        slot: &LinearBindingSlot,
        exit: &BranchExit,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        for &(symbol, entry_proof) in &slot.views {
            let proof = exit
                .views
                .get(&symbol)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            lowerer.validate_non_null_proof(symbol, proof, &exit.bindings, span)?;
            if entry_proof.inner != proof.inner
                || entry_proof.origin != proof.origin
                || slot.source != EntityId::Loan(entry_proof.loan)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        let entity = if let Some(symbol) = slot.symbol {
            match exit.bindings.get(&symbol) {
                Some(LoweredValue::Value(value)) => Some(EntityId::Value(*value)),
                _ => None,
            }
        } else if let Some(key) = slot.temporaries.first() {
            exit.temporaries.get(key).copied().map(EntityId::Value)
        } else if let Some(key) = slot.loans.first() {
            exit.loans.get(key).copied().flatten().map(EntityId::Loan)
        } else if let Some(key) = slot.captures.first() {
            exit.capture_loans.get(key).copied().map(EntityId::Loan)
        } else if let Some((symbol, _)) = slot.views.first() {
            exit.views
                .get(symbol)
                .map(|proof| EntityId::Loan(proof.loan))
        } else if let Some(symbol) = slot.borrow_symbols.first() {
            exit.borrow_bindings
                .get(symbol)
                .copied()
                .map(EntityId::Loan)
        } else if let Some((symbol, index)) = slot.result_sources.first() {
            exit.result_source_loans
                .get(symbol)
                .and_then(|loans| loans.get(*index))
                .copied()
                .map(EntityId::Loan)
        } else if let Some(stmt) = slot.for_sources.first() {
            exit.for_sources.get(stmt).copied().map(EntityId::Loan)
        } else if let Some(stmt) = slot.for_elements.first() {
            exit.for_elements.get(stmt).copied().map(EntityId::Loan)
        } else {
            None
        };
        let entity = entity.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if slot.views.iter().any(|(symbol, _)| {
            exit.views
                .get(symbol)
                .is_none_or(|proof| entity != EntityId::Loan(proof.loan))
        }) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(entity)
    }
}
