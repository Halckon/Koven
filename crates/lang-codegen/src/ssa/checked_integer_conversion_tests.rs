//! Ordinary source: identity and cross-width conversion preserve the real LLVM pipeline.
use super::{
    unit_lower::lower_owned_unit_with_entry, unit_lower_test_support::parsed,
    verify::verify_program,
};
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

#[test]
fn single_conversion_identity_reaches_verified_llvm() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("fun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let source = sources.add_source("conversion.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        let inputs = [SourceUnitInput::memory_document(source, &file)];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .unwrap()
            .into_result()
            .unwrap();
        assert!(owned.diagnostics().is_empty());
        let owned = owned.validate().unwrap();
        let mut entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "entry"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.name_span().source_id() == source
            });
        let entry = entries.next().expect("conversion entry exists").id();
        assert!(entries.next().is_none(), "conversion entry is unique");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        let (program, _) = lower_owned_unit_with_entry(&unit, entry).unwrap();
        assert_verified_llvm(&program);
    }
}

#[test]
fn unit_conversion_identity_reaches_verified_llvm() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("package p\nfun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "p/conversion.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "p/conversion.ko",
            source,
            &file,
        )];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .unwrap()
            .into_result()
            .unwrap();
        assert!(owned.diagnostics().is_empty());
        let owned = owned.validate().unwrap();
        let mut entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "entry"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.name_span().source_id() == source
            });
        let entry = entries.next().expect("conversion entry exists").id();
        assert!(entries.next().is_none(), "conversion entry is unique");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        let (program, _) = lower_owned_unit_with_entry(&unit, entry).unwrap();
        assert_verified_llvm(&program);
    }
}

fn assert_verified_llvm(program: &super::model::Program) {
    verify_program(program).unwrap();
    let llvm = crate::llvm::render_verified_program(program).unwrap();
    diagnostic!("verified conversion LLVM: {llvm}");
    assert!(llvm.contains("icmp slt i128"));
    assert!(llvm.contains("icmp sgt i128"));
    assert!(llvm.contains("call void @koven.abort.report"));
}

pub(in crate::ssa) mod map_iteration_exits {
    //! The Map context participates in ordinary CFG and real jump/return dispatch.
    //! Path-local roles restore independently of cumulative exit evidence; ordinary
    //! BranchExit retains the actual break/continue collections.
    use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;

    pub(in crate::ssa) struct Context {
        pub(in crate::ssa) plan: MapIterationOwnershipPlan,
        pub(in crate::ssa) kind: IterationExitKind,
        pub(in crate::ssa) body: BlockId,
        pub(in crate::ssa) frame: Frame,
        pub(in crate::ssa) owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
        pub(in crate::ssa) cursor: ValueId,
        pub(in crate::ssa) cursor_type: SsaTypeId,
        pub(in crate::ssa) next: Option<ValueId>,
        pub(in crate::ssa) cleaned: bool,
        pub(in crate::ssa) returned: Option<ValueId>,
        pub(in crate::ssa) cleanup_start: Option<usize>,
        pub(in crate::ssa) conditional: Option<conditional::State>,
        pub(in crate::ssa) nesting: nested::Nesting,
        pub(in crate::ssa) children: Vec<carrier::Child>,
        pub(in crate::ssa) resumed: Option<checked::Roles>,
    }

    // Ordinary CFG transport restores only these path fields. The qualification
    // plan, source proof and cumulative conditional/nested evidence stay in place.
    impl Context {
        pub(in crate::ssa) fn path_snapshot(&self) -> Option<control::map_frames::Snapshot> {
            let owner = match self.owner {
                Some((_, source, owner)) => Some((path_owner_root(source)?, owner)),
                None => None,
            };
            Some(control::map_frames::Snapshot {
                statement: self.frame.statement,
                source: self.frame.source,
                components: self.frame.components,
                finished: self.frame.finished,
                owner,
                body: self.body,
                cursor: self.cursor,
                cursor_type: self.cursor_type,
                kind: self.kind,
                cleaned: self.cleaned,
                next: self.next,
                returned: self.returned,
                cleanup_start: self.cleanup_start,
            })
        }

        pub(in crate::ssa) fn restore_path(
            &mut self,
            path: control::map_frames::Snapshot,
            span: Span,
        ) -> Result<(), LoweringError> {
            let root = match self.owner {
                Some((_, source, _)) => Some(
                    path_owner_root(source)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                ),
                None => None,
            };
            if self.frame.statement != path.statement
                || self.plan.descriptor().statement() != path.statement
                || self.cursor_type != path.cursor_type
                || root != path.owner.map(|(root, _)| root)
                || self.frame.components.map(|(symbol, _)| symbol)
                    != path.components.map(|(symbol, _)| symbol)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            if let (Some((_, _, owner)), Some((_, current))) = (&mut self.owner, path.owner) {
                *owner = current;
            }
            self.frame.source = path.source;
            self.frame.components = path.components;
            self.frame.finished = path.finished;
            self.body = path.body;
            self.cursor = path.cursor;
            self.kind = path.kind;
            self.cleaned = path.cleaned;
            self.next = path.next;
            self.returned = path.returned;
            self.cleanup_start = path.cleanup_start;
            Ok(())
        }
    }

    fn path_owner_root(source: SourceIdentity) -> Option<control::map_frames::OwnerRoot> {
        use control::map_frames::OwnerRoot;
        Some(match source {
            SourceIdentity::BorrowedBinding(_) => return None,
            SourceIdentity::Parameter(symbol) => OwnerRoot::Parameter(symbol),
            SourceIdentity::Local {
                symbol,
                initializer,
            } => OwnerRoot::Local {
                symbol,
                initializer,
            },
            SourceIdentity::Formed { symbol, birth } => OwnerRoot::Formed { symbol, birth },
            SourceIdentity::Temporary(expression) => OwnerRoot::Temporary(expression),
        })
    }

    impl ExpressionLowerer<'_> {
        /// A planned local Drop must consume its current ordinary CFG binding once.
        /// Unlike the temporary-tail constructor proof this accepts a block parameter
        /// whose producer was transported from an earlier dominating block.
        pub(in crate::ssa) fn emit_flow_map_local_drop(
            &mut self,
            fact: lang_frontend::ownership_checking::DropFact,
            span: Span,
        ) -> Result<(), LoweringError> {
            use lang_frontend::ownership_checking::DropTarget;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let owner = match fact.target() {
                DropTarget::Named(symbol) => match self.bindings.get(&symbol).copied() {
                    Some(LoweredValue::Value(value)) => value,
                    _ => return Err(missing()),
                },
                DropTarget::Temporary(expression) => *self
                    .temporaries
                    .get(&expression.index())
                    .ok_or_else(missing)?,
                _ => return Err(missing()),
            };
            let before = self
                .function
                .block(self.block)
                .ok_or_else(missing)?
                .instructions
                .len();
            conditional::local::validate(
                (self.parsed, self.names, self.typed, self.owned),
                self.function,
                fact,
                owner,
                span,
            )?;
            if !self
                .loops
                .iter()
                .filter_map(|context| context.map_loop.as_ref())
                .any(|state| {
                    state.conditional.is_some()
                        && state.body == self.block
                        && !state.cleaned
                        && state.plan.exits().iter().any(|exit| {
                            exit.point() == fact.point()
                                && exit.actions().contains(&IterationCleanupAction::Drop(fact))
                        })
                })
            {
                return Err(missing());
            }
            let DropTarget::Named(root) = fact.target() else {
                return Err(missing());
            };
            self.end_non_null_proof(root, fact.value_origin())?;
            if self.bindings.remove(&root) != Some(LoweredValue::Value(owner)) {
                return Err(missing());
            }
            self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
            self.release_owner_capture_loans(owner, fact.value_origin())?;

            let emitted = &self
                .function
                .block(self.block)
                .ok_or_else(missing)?
                .instructions[before..];
            let drops = emitted
                .iter()
                .filter_map(|id| self.function.instruction(*id))
                .filter(|instruction| instruction.operation == Operation::Drop { owner })
                .collect::<Vec<_>>();
            if !matches!(drops.as_slice(), [instruction] if instruction.origin == Origin::Source(fact.value_origin()))
            {
                return Err(missing());
            }
            Ok(())
        }
        pub(in crate::ssa) fn lower_map_iteration_break(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            self.clean_map_iteration_body(IterationExitKind::Break(expression), span)?;
            let exit = self.branch_exit(LoweredValue::Unit);
            self.loops
                .last_mut()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .breaks
                .push(exit);
            Ok(LoweredValue::Diverged)
        }

        pub(in crate::ssa) fn lower_map_iteration_continue(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            self.clean_map_iteration_body(IterationExitKind::Continue(expression), span)?;
            self.advance_map_iteration_body(span)?;
            Ok(LoweredValue::Diverged)
        }

        pub(in crate::ssa) fn clean_map_iteration_body(
            &mut self,
            kind: IterationExitKind,
            span: Span,
        ) -> Result<(), LoweringError> {
            if self
                .loops
                .last()
                .and_then(|context| context.map_loop.as_ref())
                .is_some_and(|state| state.conditional.is_some())
            {
                return self.clean_map_flow_point(kind, None, span);
            }
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let mut state = self
                .loops
                .last_mut()
                .and_then(|context| context.map_loop.take())
                .ok_or_else(missing)?;
            if state.kind != kind || self.block != state.body || state.cleaned {
                return Err(missing());
            }
            let exit = unique(state.plan.exits().iter().filter(|exit| exit.kind() == kind))
                .ok_or_else(missing)?;
            state.cleanup_start = Some(
                self.function
                    .block(self.block)
                    .ok_or_else(missing)?
                    .instructions
                    .len(),
            );
            self.emit_map_iteration_cleanup(
                exit,
                &mut state.frame,
                state.owner,
                state.conditional.is_some(),
                span,
            )?;
            let owner_mismatch = if let Some((proof, source, value)) = state.owner {
                source.current_owner(self)
                    != proof
                        .at_checked(exit.point(), self.owned)?
                        .is_none()
                        .then_some(value)
            } else {
                false
            };
            if state
                .frame
                .components
                .iter()
                .any(|(_, loan)| loan.is_some())
                || (exit_shape::terminal(kind) != state.frame.finished)
                || (exit_shape::terminal(kind) != state.frame.source.is_none())
                || owner_mismatch
            {
                return Err(missing());
            }
            state.cleaned = true;
            self.loops.last_mut().ok_or_else(missing)?.map_loop = Some(state);
            Ok(())
        }

        pub(in crate::ssa) fn advance_map_iteration_body(
            &mut self,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let (source, cursor, cursor_type, header, carried) = {
                let context = self.loops.last().ok_or_else(missing)?;
                let state = context.map_loop.as_ref().ok_or_else(missing)?;
                if !state.cleaned
                    || state.next.is_some()
                    || state.frame.finished
                    || state
                        .frame
                        .components
                        .iter()
                        .any(|(_, loan)| loan.is_some())
                    || state.body != self.block
                    || !matches!(
                        state.kind,
                        IterationExitKind::Fallthrough | IterationExitKind::Continue(_)
                    )
                {
                    return Err(missing());
                }
                (
                    state.frame.source.ok_or_else(missing)?,
                    state.cursor,
                    state.cursor_type,
                    context.header,
                    context.carried.clone(),
                )
            };
            let next = self.map_iteration_value(
                Operation::MapIterationNext { source, cursor },
                cursor_type,
                span,
            )?;
            let mut arguments = self.map_iteration_carried_arguments(&carried, span)?;
            arguments.extend([EntityId::Loan(source), EntityId::Value(next)]);
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Branch(Edge {
                        target: header,
                        arguments,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let state = self
                .loops
                .last_mut()
                .and_then(|context| context.map_loop.as_mut())
                .ok_or_else(missing)?;
            state.next = Some(next);
            if let Some(conditional) = &mut state.conditional {
                conditional.exits.last_mut().ok_or_else(missing)?.next = Some(next);
            }
            Ok(())
        }

        /// Called only after the actual return operand has been evaluated/delivered.
        /// Keep the existing Sequential/Numeric dispatch unchanged when no Map is active.
        pub(in crate::ssa) fn emit_active_iteration_return_cleanup(
            &mut self,
            expression: ExpressionId,
            result: LoweredValue,
            span: Span,
        ) -> Result<(), LoweringError> {
            if self
                .loops
                .last()
                .is_some_and(|context| context.map_loop.is_some())
            {
                let value = match result {
                    LoweredValue::Value(value) => Some(value),
                    LoweredValue::Unit => None,
                    LoweredValue::Diverged => return Err(error(LoweringErrorKind::MissingFact, span)),
                };
                if self
                    .loops
                    .last()
                    .and_then(|context| context.map_loop.as_ref())
                    .is_some_and(|state| state.conditional.is_some())
                {
                    return self.clean_map_flow_point(
                        IterationExitKind::Return(expression),
                        value,
                        span,
                    );
                }
                self.clean_map_iteration_body(IterationExitKind::Return(expression), span)?;
                let state = self
                    .loops
                    .last_mut()
                    .and_then(|context| context.map_loop.as_mut())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                state.returned = value;
                if let Some(conditional) = &mut state.conditional {
                    conditional
                        .exits
                        .last_mut()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                        .returned = value;
                }
                return Ok(());
            }
            let exit = self.loops.iter().rev().find_map(|context| {
                context.iteration_plan().and_then(|plan| {
                    plan.exits()
                        .iter()
                        .find(|exit| exit.kind() == IterationExitKind::Return(expression))
                        .cloned()
                })
            });
            if let Some(exit) = exit {
                self.emit_iteration_exit_plan(&exit, span)
            } else {
                self.emit_control_transfer_cleanup(expression)
            }
        }

        pub(in crate::ssa) fn clean_map_flow_point(
            &mut self,
            kind: IterationExitKind,
            returned: Option<ValueId>,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let current = self
                .loops
                .last()
                .and_then(|context| context.map_loop.as_ref())
                .ok_or_else(missing)?;
            let exit = unique(
                current
                    .plan
                    .exits()
                    .iter()
                    .filter(|exit| exit.kind() == kind),
            )
            .ok_or_else(missing)?
            .clone();
            let point = exit.point();
            let actions = self
                .owned
                .cleanup_steps()
                .iter()
                .filter_map(|(actual, action)| (*actual == point).then_some(*action))
                .collect::<Vec<_>>();
            if actions != exit.actions() {
                return Err(missing());
            }
            let mut frames = Vec::new();
            let mut indices = Vec::new();
            for (index, context) in self.loops.iter().enumerate() {
                let Some(state) = context.map_loop.as_ref() else {
                    // Ordinary loop cleanup remains with its own control frame.
                    continue;
                };
                let included = actions.iter().any(|action| matches!(action,
                    IterationCleanupAction::EndBinding { statement, .. } if *statement == state.frame.statement));
                if !included {
                    continue;
                }
                if !matches!(kind, IterationExitKind::Return(_)) && index + 1 != self.loops.len() {
                    return Err(missing());
                }
                let planned = unique(state.plan.exits().iter().filter(|exit| exit.kind() == kind))
                    .ok_or_else(missing)?;
                if planned.point() != point || planned.actions() != actions || state.cleaned {
                    return Err(missing());
                }
                let roles = conditional::transport::roles(self, state, span)?;
                frames.push(conditional::CleanupFrame {
                    statement: state.frame.statement,
                    roles,
                    owner: state.owner,
                });
                indices.push(index);
            }
            if indices.last().copied() != Some(self.loops.len() - 1) {
                return Err(missing());
            }
            let start = self
                .function
                .block(self.block)
                .ok_or_else(missing)?
                .instructions
                .len();
            for action in &actions {
                let statement = match *action {
                    IterationCleanupAction::EndBinding { statement, .. }
                    | IterationCleanupAction::FinishProvider(statement)
                    | IterationCleanupAction::EndSource(statement) => Some(statement),
                    IterationCleanupAction::Drop(fact) => {
                        let mut owners = Vec::new();
                        for frame in &frames {
                            if let Some((proof, _, _)) = frame.owner
                                && proof.at_checked(point, self.owned)? == Some(fact)
                            {
                                owners.push(frame.statement);
                            }
                        }
                        match owners.as_slice() {
                            [] => None,
                            [statement] => Some(*statement),
                            _ => return Err(missing()),
                        }
                    }
                    _ => return Err(missing()),
                };
                if let Some(statement) = statement {
                    let index = *indices
                        .iter()
                        .find(|&&index| {
                            self.loops[index]
                                .map_loop
                                .as_ref()
                                .is_some_and(|state| state.frame.statement == statement)
                        })
                        .ok_or_else(missing)?;
                    let mut state = self.loops[index].map_loop.take().ok_or_else(missing)?;
                    self.emit_map_cleanup_action(
                        action,
                        point,
                        &mut state.frame,
                        state.owner,
                        true,
                        span,
                    )?;
                    self.loops[index].map_loop = Some(state);
                } else if let IterationCleanupAction::Drop(fact) = action {
                    self.emit_flow_map_local_drop(*fact, span)?;
                } else {
                    return Err(missing());
                }
            }
            let end = self
                .function
                .block(self.block)
                .ok_or_else(missing)?
                .instructions
                .len();
            let mut dropped = Vec::new();
            for (proof, _, owner) in frames.iter().filter_map(|frame| frame.owner) {
                if proof.at_checked(point, self.owned)?.is_some() {
                    dropped.push(owner);
                }
            }
            for index in indices {
                let current_owner = self.loops[index]
                    .map_loop
                    .as_ref()
                    .ok_or_else(missing)?
                    .owner
                    .map(|(_, identity, _)| identity.current_owner(self));
                let state = self.loops[index].map_loop.as_mut().ok_or_else(missing)?;
                if state
                    .frame
                    .components
                    .iter()
                    .any(|(_, loan)| loan.is_some())
                    || state.frame.finished != exit_shape::terminal(kind)
                    || state.frame.source.is_none() != exit_shape::terminal(kind)
                {
                    return Err(missing());
                }
                if let Some((_, _, owner)) = state.owner {
                    let expected = (!dropped.contains(&owner)).then_some(owner);
                    if current_owner != Some(expected) {
                        return Err(missing());
                    }
                }
                let frame = frames
                    .iter()
                    .find(|frame| frame.statement == state.frame.statement)
                    .ok_or_else(missing)?;
                state.kind = kind;
                state.cleaned = true;
                state.cleanup_start = Some(start);
                state.returned = returned;
                state
                    .conditional
                    .as_mut()
                    .ok_or_else(missing)?
                    .exits
                    .push(conditional::Exit {
                        kind,
                        block: self.block,
                        roles: frame.roles,
                        frames: frames.clone(),
                        cleanup_start: start,
                        cleanup_end: end,
                        next: None,
                        returned,
                    });
            }
            Ok(())
        }
    }

    pub(in crate::ssa) mod exit_shape {
        //! Bound the Map body to normal straight-line execution or one direct tail transfer.
        //! This describes capability only; the exit/action/owner facts still come from Phase 3.
        use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;
        use lang_frontend::type_checking::MapIterationDescriptor;

        pub(in crate::ssa) fn body_exit(
            parsed: &ParsedFile,
            _names: &NameResolution,
            typed: &TypedFile,
            _descriptor: MapIterationDescriptor,
            body: StatementId,
            span: Span,
        ) -> Result<IterationExitKind, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
            if qualify::straight_statement(parsed, typed, body)? {
                return Ok(IterationExitKind::Fallthrough);
            }
            let node = parsed.ast().statements().get(body).map_err(|_| missing())?;
            let Statement::Block { elements } = node.payload() else {
                return Err(unsupported());
            };
            let Some((last, prefix)) = elements.split_last() else {
                return Err(unsupported());
            };
            for statement in prefix {
                if !qualify::straight_statement(parsed, typed, *statement)? {
                    return Err(unsupported());
                }
            }
            let node = parsed
                .ast()
                .statements()
                .get(*last)
                .map_err(|_| missing())?;
            let Statement::Expression { expression } = node.payload() else {
                return Err(unsupported());
            };
            let node = parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| missing())?;

            match node.payload() {
                Expression::Continue { .. } => Ok(IterationExitKind::Continue(*expression)),
                Expression::Break { .. } => Ok(IterationExitKind::Break(*expression)),
                // Operand evaluation and concrete return ABI belong to ordinary lower_return.
                // Complete FE has already checked the return contract and ownership delivery.
                Expression::Return { .. } => Ok(IterationExitKind::Return(*expression)),
                _ => Err(unsupported()),
            }
        }

        pub(in crate::ssa) fn point(
            kind: IterationExitKind,
            statement: StatementId,
            body: StatementId,
        ) -> DropPoint {
            match kind {
                IterationExitKind::Fallthrough => DropPoint::AfterStatement(body),
                IterationExitKind::Continue(expression)
                | IterationExitKind::Break(expression)
                | IterationExitKind::Return(expression) => DropPoint::ControlTransfer(expression),
                IterationExitKind::Exhaustion => DropPoint::LoopExit(statement),
            }
        }

        pub(in crate::ssa) fn terminal(kind: IterationExitKind) -> bool {
            matches!(
                kind,
                IterationExitKind::Break(_)
                    | IterationExitKind::Return(_)
                    | IterationExitKind::Exhaustion
            )
        }

        pub(in crate::ssa) fn cleanup(
            owned: &OwnershipCheckedFile,
            plan: &MapIterationOwnershipPlan,
            body: StatementId,
            kind: IterationExitKind,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let descriptor = plan.descriptor();
            let statement = descriptor.statement();
            if plan.exits().len() != 2 {
                return Err(missing());
            }
            for expected_kind in [kind, IterationExitKind::Exhaustion] {
                let exit = unique(
                    plan.exits()
                        .iter()
                        .filter(|exit| exit.kind() == expected_kind),
                )
                .ok_or_else(missing)?;
                if exit.point() != point(expected_kind, statement, body)
                    || owned.cleanup_conditions().get(exit.condition())
                        != Some(&CleanupCondition::Always)
                    || owned
                        .cleanup_steps()
                        .iter()
                        .filter_map(|(point, action)| (*point == exit.point()).then_some(*action))
                        .collect::<Vec<_>>()
                        != exit.actions()
                {
                    return Err(missing());
                }
                let map_actions = exit
                    .actions()
                    .iter()
                    .filter(|action| !matches!(action, IterationCleanupAction::Drop(_)))
                    .copied()
                    .collect::<Vec<_>>();
                let mut expected = Vec::new();
                if expected_kind != IterationExitKind::Exhaustion {
                    expected.extend([
                        IterationCleanupAction::EndBinding {
                            statement,
                            symbol: descriptor.value_binding(),
                        },
                        IterationCleanupAction::EndBinding {
                            statement,
                            symbol: descriptor.key_binding(),
                        },
                    ]);
                }
                if terminal(expected_kind) {
                    expected.extend([
                        IterationCleanupAction::FinishProvider(statement),
                        IterationCleanupAction::EndSource(statement),
                    ]);
                }
                if map_actions != expected {
                    return Err(missing());
                }
                // Local drops form one prefix. The Map sequence is contiguous, and
                // only the source owner may follow EndSource in this bounded shape.
                let start = exit
                    .actions()
                    .iter()
                    .position(|action| expected.first() == Some(action))
                    .ok_or_else(missing)?;
                if exit.actions()[..start]
                    .iter()
                    .any(|action| !matches!(action, IterationCleanupAction::Drop(_)))
                    || exit.actions().get(start..start + expected.len()) != Some(expected.as_slice())
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let tail = &exit.actions()[start + expected.len()..];
                if !terminal(expected_kind) && !tail.is_empty() {
                    return Err(missing());
                }
                if plan.source_access() == MapIterationSourceAccess::Shared && !tail.is_empty() {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            Ok(())
        }
    }
}
