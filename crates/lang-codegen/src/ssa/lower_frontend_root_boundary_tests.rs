//! Original declaration-only roots through strict Unit, with no runtime layouts or actions.

use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use crate::{
    llvm::render_verified_program,
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{
        model::{Origin, TerminatorKind},
        render::render_program,
        verify::verify_program,
    },
    test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic, assert_text_eq},
};

fn original_unit(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original Unit source");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original memory document");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

pub(super) fn assert_unused_declarations(text: &str) {
    let unit = original_unit(text);
    assert_eq!(unit.parsed().roots().len(), 5);
    let declaration = unit.declaration("entry", SymbolKind::Function);
    let lower = || {
        unit.lower_entry(declaration)
            .expect("declarative roots do not initialize")
    };
    let (program, entry) = lower();
    let (repeated, repeated_entry) = lower();
    verify_program(&program).expect("original declaration-root SSA verifies");
    verify_program(&repeated).expect("repeated declaration-root SSA verifies");
    assert_eq!(entry.index(), repeated_entry.index());
    assert_eq!(entry.module().index(), repeated_entry.module().index());
    assert_eq!(program.modules.len(), 1);
    let module = program.module(entry.module()).unwrap();
    assert_eq!(module.functions.len(), 1);
    assert!(
        module.types.is_empty(),
        "unused declarations have no runtime layout"
    );
    let function = module.function(entry).unwrap();
    assert_eq!(
        function.origin,
        Origin::Source(unit.declaration_span(declaration))
    );
    assert_eq!(
        unit.sources().slice(unit.declaration_span(declaration)),
        Ok("fun entry(): Unit {}")
    );
    assert!(function.receiver.is_none());
    assert!(function.return_types.is_empty());
    assert!(
        function.instructions.is_empty(),
        "no initialization or cleanup action"
    );
    assert_eq!(function.blocks.len(), 1);
    assert!(function.blocks[0].parameters.is_empty());
    assert_eq!(
        function.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values: vec![] }
    );
    let ssa = render_program(&program);
    assert_text_eq!(ssa, render_program(&repeated));
    assert_eq_diagnostic!(ssa.matches("func \"").count(), 1, "{ssa}");
    // Unit names are qualified. Bind this one function to the original declaration instead.
    assert_diagnostic!(
        ssa.contains(&format!("func \"{}\"", function.name)),
        "{ssa}"
    );
    let llvm = render_verified_program(&program).expect("declarative roots reach LLVM");
    assert_text_eq!(
        llvm,
        render_verified_program(&repeated).expect("repeated roots reach LLVM")
    );
    assert_eq_diagnostic!(llvm.matches("define internal").count(), 1, "{llvm}");
    for unused in ["Pair", "Holder", "Marker", "Outcome"] {
        assert_diagnostic!(!ssa.contains(unused), "{ssa}");
        assert_diagnostic!(!llvm.contains(unused), "{llvm}");
    }
}

pub(in crate::ssa) mod map_iteration_resource {
    //! An owned Resource Map retains its real parameter or formed birth through lexical cleanup.
    //! Presence is proved from Parameter -> Header -> Exit incoming values, never a bit alone.
    use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;
    use lang_frontend::{
        ownership_checking::{
            CleanupCondition, CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary,
            IterationPhiIncomingKind,
        },
        parser::{FunctionBody, FunctionForm},
    };

    pub(in crate::ssa) fn qualify(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        root: (
            SymbolId,
            Option<&crate::ssa::map_source::Formation<SymbolId>>,
        ),
        body_exit: IterationExitKind,
    ) -> Result<source_owner::Proof, LoweringError> {
        let (symbol, birth) = root;
        let d = plan.descriptor();
        let span = parsed
            .ast()
            .statements()
            .get(d.statement())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
        // Source ownership depends on validated Resource classification and its complete
        // cleanup graph. Existing storage planning owns payload/key representation admission.
        if !matches!(
            body_exit,
            IterationExitKind::Fallthrough
                | IterationExitKind::Continue(_)
                | IterationExitKind::Break(_)
                | IterationExitKind::Return(_)
        ) || plan.source_access() != MapIterationSourceAccess::Owned
            || typed.is_resource_type(d.source_type()) != Some(true)
        {
            return Err(unsupported());
        }
        let scope = scope(parsed, d.statement(), origin, span)?;
        let drops = if let Some(formation) = birth {
            formed::drops(parsed, owned, symbol, formation)?
        } else {
            owned
                .drops()
                .iter()
                .copied()
                .filter(|f| f.target() == DropTarget::Named(symbol))
                .collect::<Vec<_>>()
        };
        let returning = match body_exit {
            IterationExitKind::Return(expression) => Some(expression),
            _ => None,
        };
        let transfer = returning
            .map(|expression| {
                let node = parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| bad())?;
                if !matches!(node.payload(), Expression::Return { .. })
                    || node.span().source_id() != span.source_id()
                    || node.span().start() <= span.start()
                    || node.span().end() >= span.end()
                {
                    return Err(bad());
                }
                unique(
                    drops
                        .iter()
                        .copied()
                        .filter(|d| d.point() == DropPoint::ControlTransfer(expression)),
                )
                .ok_or_else(bad)
            })
            .transpose()?;
        if drops.len() != 1 + usize::from(returning.is_some()) {
            return Err(unsupported());
        }
        let drop = unique(drops.iter().copied().filter(|d| Some(*d) != transfer)).ok_or_else(bad)?;
        if !normal_point(parsed, scope, span, drop.point())
            && !formed::replacement_point(birth, drop.point())
        {
            return Err(bad());
        }
        let conditions = owned.cleanup_conditions();
        if plan.closure_phis().len() != 2
            || plan.closure_phi_incomings().len() != if returning.is_some() { 2 } else { 3 }
            || !plan.capture_graph().nodes().is_empty()
        {
            return Err(unsupported());
        }
        let h = source_owner::phi(conditions, plan, symbol, IterationPhiBoundary::Header, span)?;
        let x = source_owner::phi(conditions, plan, symbol, IterationPhiBoundary::Exit, span)?;
        if h.owner() == x.owner() || h.availability_selector() == x.availability_selector() {
            return Err(bad());
        }
        let entry = source_owner::incoming(
            conditions,
            plan,
            h,
            IterationPhiIncomingKind::Entry,
            DropPoint::AfterExpression(d.source()),
            span,
        )?;
        let [seed] = entry.values() else {
            return Err(bad());
        };
        let available = match birth {
            Some(birth) => source_owner::formed_condition(
                parsed,
                owned,
                (symbol, birth.birth),
                seed.source(),
                span,
            )?,
            None => {
                if conditions.get(entry.available_when()) != Some(&CleanupCondition::Always)
                    || !matches!(conditions.owner_value(seed.source()), Some(CleanupOwnerValue::Parameter { symbol: s, origin: o }) if *s == symbol && *o == origin)
                {
                    return Err(bad());
                }
                entry.available_when()
            }
        };
        if seed.condition() != entry.available_when() || seed.condition() != available {
            return Err(bad());
        }
        let mut incoming_edges = vec![(
            x,
            IterationPhiIncomingKind::Exhaustion,
            DropPoint::LoopExit(d.statement()),
        )];
        if returning.is_none() {
            let backedge =
                unique(plan.exits().iter().filter(|e| e.kind() == body_exit)).ok_or_else(bad)?;
            incoming_edges.push((
                if matches!(body_exit, IterationExitKind::Break(_)) {
                    x
                } else {
                    h
                },
                match body_exit {
                    IterationExitKind::Continue(expression) => {
                        IterationPhiIncomingKind::Continue(expression)
                    }
                    IterationExitKind::Break(expression) => IterationPhiIncomingKind::Break(expression),
                    _ => IterationPhiIncomingKind::Fallthrough,
                },
                backedge.point(),
            ));
        }
        for (phi, kind, point) in incoming_edges {
            let incoming = source_owner::incoming(conditions, plan, phi, kind, point, span)?;
            let [value] = incoming.values() else {
                return Err(bad());
            };
            if value.source() != h.owner()
                || value.condition() != h.availability_condition()
                || incoming.available_when() != h.availability_condition()
            {
                return Err(bad());
            }
        }
        if let Some(fact) = transfer {
            if fact.owner() != Some(h.owner()) || fact.condition() != Some(h.availability_condition()) {
                return Err(bad());
            }
            let exit = unique(plan.exits().iter().filter(|e| e.kind() == body_exit)).ok_or_else(bad)?;
            let suffix = [
                IterationCleanupAction::EndBinding {
                    statement: d.statement(),
                    symbol: d.value_binding(),
                },
                IterationCleanupAction::EndBinding {
                    statement: d.statement(),
                    symbol: d.key_binding(),
                },
                IterationCleanupAction::FinishProvider(d.statement()),
                IterationCleanupAction::EndSource(d.statement()),
                IterationCleanupAction::Drop(fact),
            ];
            if exit.point() != fact.point() || !exit.actions().ends_with(&suffix)
                || exit.actions()[..exit.actions().len()-suffix.len()].iter().any(|a|
                    !matches!(a, IterationCleanupAction::Drop(local) if local.target()!=DropTarget::Named(symbol)))
            { return Err(bad()); }
        }
        let related = owned
            .drops()
            .iter()
            .copied()
            .filter(|f| {
                f.owner()
                    .is_some_and(|o| [seed.source(), h.owner(), x.owner()].contains(&o))
                    || drops.contains(f)
            })
            .collect::<Vec<_>>();
        let (drop_owner, drop_condition) = if let Some(birth) = birth {
            source_owner::formed_condition(parsed, owned, (symbol, birth.birth), h.owner(), span)?;
            source_owner::formed_condition(parsed, owned, (symbol, birth.birth), x.owner(), span)?;
            let owner = drop.owner().ok_or_else(bad)?;
            let condition =
                source_owner::formed_condition(parsed, owned, (symbol, birth.birth), owner, span)?;
            (owner, condition)
        } else {
            (x.owner(), x.availability_condition())
        };
        if related != drops
            || drop.value_origin() != origin
            || drop.owner() != Some(drop_owner)
            || drop.condition() != Some(drop_condition)
            || drops.iter().any(|d| {
                d.value_origin() != origin
                    || d.capture_slot().is_some()
                    || d.instance_address().is_some()
            })
        {
            return Err(bad());
        }
        Ok(source_owner::Proof {
            flow: None,
            disposition: source_owner::Disposition::Drop(drop),
            transfer,
            conditional_return: None,
        })
    }

    // Resolve the callable scope independently of the number or kind of continuation statements.
    pub(in crate::ssa) fn scope(
        parsed: &ParsedFile,
        statement: StatementId,
        origin: Span,
        span: Span,
    ) -> Result<StatementId, LoweringError> {
        let bad = || error(LoweringErrorKind::UnsupportedNode, span);
        let bodies = parsed
            .roots()
            .iter()
            .filter_map(|id| {
                let item = parsed.ast().items().get(*id).ok()?;
                if item.span().source_id() != origin.source_id()
                    || item.span().start() > origin.start()
                    || item.span().end() < origin.end()
                {
                    return None;
                }
                match item.payload() {
                    Item::Function {
                        form:
                            FunctionForm::ImplicitUnitBlock(b)
                            | FunctionForm::Explicit {
                                body: FunctionBody::Block(b),
                                ..
                            },
                        ..
                    } => Some(*b),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let [body] = bodies.as_slice() else {
            return Err(bad());
        };
        // The validated Drop names the actual callable scope. Continuation statements
        // are lowered normally; their AST shape cannot grant or revoke source ownership.
        let node = parsed.ast().statements().get(*body).map_err(|_| bad())?;
        let iteration = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| bad())?;
        if !matches!(node.payload(), Statement::Block { .. })
            || node.span().source_id() != iteration.span().source_id()
            || node.span().start() > iteration.span().start()
            || node.span().end() < iteration.span().end()
        {
            return Err(bad());
        }
        Ok(*body)
    }

    // The fact selects its real cleanup point; statement count and operand shape are irrelevant.
    pub(in crate::ssa) fn normal_point(
        parsed: &ParsedFile,
        scope: StatementId,
        iteration: Span,
        point: DropPoint,
    ) -> bool {
        if point == DropPoint::AfterStatement(scope) {
            return true;
        }
        let DropPoint::ControlTransfer(expression) = point else {
            return false;
        };
        let Ok(node) = parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Ok(body) = parsed.ast().statements().get(scope) else {
            return false;
        };
        let at = node.span();
        let inside = |outer: Span| {
            outer.source_id() == at.source_id()
                && outer.start() <= at.start()
                && outer.end() >= at.end()
        };
        matches!(node.payload(), Expression::Return { .. })
            && inside(body.span())
            && at.start() >= iteration.end()
            && !parsed
                .ast()
                .expressions()
                .iter()
                .any(|(_, n)| matches!(n.payload(), Expression::Lambda { .. }) && inside(n.span()))
            && !parsed.ast().items().iter().any(|(_, n)| {
                matches!(n.payload(), Item::Function { .. } | Item::Deinit { .. })
                    && n.span().start() >= body.span().start()
                    && inside(n.span())
            })
    }

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Carrier {
        fact: DropFact,
        anchor: ValueId,
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn retain_resource_map(
            &mut self,
            owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
            seed: EntityId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some((proof, SourceIdentity::Parameter(root), _)) = owner.filter(|(p, _, _)| {
                // Terminal flow Drops have already executed. Only a separately
                // validated late Drop grants the future-lifetime carrier.
                p.flow.is_none_or(|flow| flow.late == p.drop())
                    && p.drop().is_some_and(|drop| {
                        matches!(
                            drop.point(),
                            DropPoint::AfterStatement(_) | DropPoint::ControlTransfer(_)
                        )
                    })
            }) else {
                return Ok(());
            };
            let bad = || error(LoweringErrorKind::MissingFact, span);
            let EntityId::Value(anchor) = seed else {
                return Err(bad());
            };
            let Some(LoweredValue::Value(current)) = self.bindings.get(&root).copied() else {
                return Err(bad());
            };
            if self.borrow_bindings.contains_key(&root) {
                return Err(bad());
            }
            // Break may merge the retained owner with the independent exhaustion edge.
            // Prove the actual current binding through every incoming to the original seed.
            nested::transport(self.function, EntityId::Value(current), seed, span)?;
            if self
                .map_iteration_state
                .resources
                .insert(
                    root,
                    Carrier {
                        fact: proof.drop().ok_or_else(bad)?,
                        anchor,
                    },
                )
                .is_some()
            {
                return Err(bad());
            }
            Ok(())
        }
        pub(in crate::ssa) fn emit_resource_map_drop(
            &mut self,
            fact: DropFact,
        ) -> Result<bool, LoweringError> {
            let DropTarget::Named(root) = fact.target() else {
                return Ok(false);
            };
            let Some(state) = self.map_iteration_state.resources.get(&root).copied() else {
                return Ok(false);
            };
            let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
            let Some(LoweredValue::Value(current)) = self.bindings.get(&root).copied() else {
                return Err(bad());
            };
            if state.fact != fact
                || self.borrow_bindings.contains_key(&root)
                || self.temporaries.values().any(|v| *v == current)
            {
                return Err(bad());
            }
            nested::transport(
                self.function,
                EntityId::Value(current),
                EntityId::Value(state.anchor),
                fact.value_origin(),
            )?;
            self.bindings.remove(&root);
            self.map_iteration_state.resources.remove(&root);
            self.append(
                Operation::Drop { owner: current },
                Vec::new(),
                fact.value_origin(),
            )?;
            Ok(true)
        }
    }

    pub(in crate::ssa) mod evidence {
        //! Finite all-incoming transport and the real size-result/control continuation.
        use crate::ssa::lower_frontend::map_iteration::local_runtime::*;
        use crate::ssa::lower_frontend::map_iteration::*;
        use crate::ssa::lower_frontend::*;

        pub(in crate::ssa) fn edges(
            evidence: carrier::Evidence<'_>,
            proof: &local::Proof,
        ) -> Result<(), LoweringError> {
            let function = evidence.function;
            let span = evidence.span;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let [preheader, header, body, exhausted] = evidence.blocks;
            if evidence.body_exit != IterationExitKind::Fallthrough
                || evidence.conditional.is_some()
                || !evidence.children.is_empty()
                || evidence.continuation != exhausted
                || evidence
                    .carried
                    .slots
                    .iter()
                    .any(|slot| proof.dead.iter().any(|dead| slot.symbol == Some(dead.root)))
            {
                return Err(missing());
            }
            let owner_column = unique(
                evidence
                    .carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| (slot.symbol == Some(proof.root)).then_some(index)),
            )
            .ok_or_else(missing)?;
            let parameters = |block| -> Result<&[EntityId], LoweringError> {
                function
                    .block(block)
                    .map(|block| block.parameters.as_slice())
                    .ok_or_else(missing)
            };
            for (target, mut expected) in [
                (header, vec![preheader, body]),
                (body, vec![header]),
                (exhausted, vec![header]),
            ] {
                let incoming = nested::incoming(function, target);
                let mut actual = incoming.iter().map(|(block, _)| *block).collect::<Vec<_>>();
                actual.sort();
                expected.sort();
                if actual != expected {
                    return Err(missing());
                }
                for (from, edge) in incoming {
                    let target_parameters = parameters(target)?;
                    if edge.arguments.len() != target_parameters.len() {
                        return Err(missing());
                    }
                    for column in [owner_column, evidence.source_slot, evidence.cursor_slot] {
                        let expected = if from == preheader {
                            [
                                evidence.seed,
                                EntityId::Loan(evidence.source),
                                EntityId::Value(evidence.first),
                            ][[owner_column, evidence.source_slot, evidence.cursor_slot]
                                .iter()
                                .position(|actual| *actual == column)
                                .ok_or_else(missing)?]
                        } else if from == body && column == evidence.cursor_slot {
                            EntityId::Value(evidence.next.ok_or_else(missing)?)
                        } else {
                            *parameters(from)?.get(column).ok_or_else(missing)?
                        };
                        let actual = *target_parameters.get(column).ok_or_else(missing)?;
                        if edge.arguments.get(column) != Some(&expected)
                            || function.entity(actual).map(|data| data.definition)
                                != Some(Definition::BlockParameter {
                                    block: target,
                                    index: column,
                                })
                            || function.entity(actual).map(|data| data.ty)
                                != function.entity(expected).map(|data| data.ty)
                        {
                            return Err(missing());
                        }
                    }
                }
                nested::transport(
                    function,
                    *parameters(target)?.get(owner_column).ok_or_else(missing)?,
                    evidence.seed,
                    span,
                )?;
                nested::transport(
                    function,
                    *parameters(target)?
                        .get(evidence.source_slot)
                        .ok_or_else(missing)?,
                    EntityId::Loan(evidence.source),
                    span,
                )?;
            }
            Ok(())
        }

        /// Tie the two projected loans to the actual qualified source call before either
        /// component is ended; the cursor advances only after the ordered End pair.
        pub(in crate::ssa) fn consumer(
            lowerer: &ExpressionLowerer<'_>,
            evidence: carrier::Evidence<'_>,
            proof: &local::Proof,
        ) -> Result<(), LoweringError> {
            let span = evidence.span;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let call = lowerer.typed.call(proof.consumer).ok_or_else(missing)?;
            let CallableTarget::Source(callee) = call.target() else {
                return Err(missing());
            };
            let callee = *lowerer
                .function_ids
                .get(&FunctionInstanceKey::new(callee, Vec::new()))
                .ok_or_else(missing)?;
            let body = evidence.blocks[2];
            let instructions = carrier::instructions(evidence.function, body).ok_or_else(missing)?;
            let (call_position, actual) =
                unique(instructions.iter().enumerate().filter(|(_, instruction)| {
                    matches!(instruction.operation, Operation::DirectCall { .. })
                }))
                .ok_or_else(missing)?;
            if actual.origin != Origin::Source(lowerer.expression_span(proof.consumer)?)
                || actual.operation
                    != (Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments: vec![EntityId::Loan(evidence.key), EntityId::Loan(evidence.value)],
                    })
            {
                return Err(missing());
            }
            let position = |operation| {
                unique(
                    instructions
                        .iter()
                        .enumerate()
                        .filter_map(|(position, instruction)| {
                            (instruction.operation == operation).then_some(position)
                        }),
                )
                .ok_or_else(missing)
            };
            let parameters = &evidence
                .function
                .block(body)
                .ok_or_else(missing)?
                .parameters;
            let (source, cursor) = roles(parameters, evidence.source_slot, evidence.cursor_slot, span)?;
            let mut counts = [0; 3];
            for instruction in &instructions {
                match instruction.operation {
                    Operation::MapIterationKeyLoan { .. } => counts[0] += 1,
                    Operation::MapIterationValueLoan { .. } => counts[1] += 1,
                    Operation::MapIterationNext { .. } => counts[2] += 1,
                    _ => {}
                }
            }
            if counts != [1, 1, 1] {
                return Err(missing());
            }
            let key = position(Operation::MapIterationKeyLoan { source, cursor })?;
            let value = position(Operation::MapIterationValueLoan { source, cursor })?;
            let value_end = position(Operation::BorrowEnd {
                loan: evidence.value,
            })?;
            let key_end = position(Operation::BorrowEnd {
                loan: evidence.key,
            })?;
            let next = position(Operation::MapIterationNext { source, cursor })?;
            if key >= call_position
                || value >= call_position
                || call_position >= value_end
                || value_end >= key_end
                || key_end >= next
                || instructions
                    .iter()
                    .any(|instruction| instruction.operation == Operation::BorrowEnd { loan: source })
                || lowerer
                    .borrow_bindings
                    .values()
                    .any(|loan| *loan == evidence.key || *loan == evidence.value)
            {
                return Err(missing());
            }
            Ok(())
        }

        pub(in crate::ssa) fn path(
            lowerer: &ExpressionLowerer<'_>,
            path: local::Path,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let previous = unique(lowerer.map_iteration_state.locals.values().filter(|state| {
                state.statement == path.previous && state.proof.late_size == Some(path.size)
            }))
            .ok_or_else(missing)?;
            validate_consumption(lowerer.function, previous, span)?;
            let size = previous.size.ok_or_else(missing)?;
            let (control, condition, success, failure) = size_control(lowerer, path.size, size, span)?;
            if control != path.control
                || condition != path.condition
                || success != path.success
                || failure != path.failure
                || path.branch != 0
            {
                return Err(missing());
            }
            let (block, _, when_true, when_false) =
                control_edges(lowerer, control, condition, size, span)?;
            if when_true.target != lowerer.block
                || !matches!(
                    carrier::terminator(lowerer.function, when_false.target),
                    Some(TerminatorKind::Abort)
                )
            {
                return Err(missing());
            }
            let incoming = nested::incoming(lowerer.function, lowerer.block);
            if !matches!(incoming.as_slice(), [(from, edge)] if *from == block && *edge == when_true) {
                return Err(missing());
            }
            Ok(())
        }

        /// At function completion every later size result must drive the real comparison
        /// and its real success/error arms; a dead MapSize instruction is not a use proof.
        pub(in crate::ssa) fn size_use(
            lowerer: &ExpressionLowerer<'_>,
            state: &Carrier,
            size: ValueId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let expression = state.proof.late_size.ok_or_else(missing)?;
            let (control, condition, _, _) = size_control(lowerer, expression, size, span)?;
            let (block, comparison, _, failure) =
                control_edges(lowerer, control, condition, size, span)?;
            let consumed = state.consumed.ok_or_else(missing)?;
            let comparison = carrier::definition(lowerer.function, EntityId::Value(comparison))
                .ok_or_else(missing)?;
            let read =
                carrier::definition(lowerer.function, EntityId::Value(size)).ok_or_else(missing)?;
            let instructions =
                carrier::instructions(lowerer.function, consumed.block).ok_or_else(missing)?;
            let read_position = instructions
                .iter()
                .position(|instruction| instruction.id == read.id)
                .ok_or_else(missing)?;
            let drop_position = instructions
                .iter()
                .position(|instruction| instruction.id == consumed.instruction)
                .ok_or_else(missing)?;
            let compare_position = instructions
                .iter()
                .position(|instruction| instruction.id == comparison.id)
                .ok_or_else(missing)?;
            if read.operation
                != (Operation::MapSize {
                    owner: EntityId::Value(state.current),
                })
                || read.origin != Origin::Source(lowerer.expression_span(expression)?)
                || read.block != consumed.block
                || block != consumed.block
                || drop_position != read_position + 1
                || drop_position >= compare_position
                || !matches!(
                    carrier::terminator(lowerer.function, failure.target),
                    Some(TerminatorKind::Abort)
                )
            {
                return Err(missing());
            }
            Ok(())
        }

        fn size_control(
            lowerer: &ExpressionLowerer<'_>,
            size_expression: ExpressionId,
            size: ValueId,
            span: Span,
        ) -> Result<(ExpressionId, ExpressionId, StatementId, StatementId), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let (condition, _) = unique(lowerer.parsed.ast().expressions().iter().filter(|(_, node)| {
                matches!(node.payload(), Expression::Binary { left, operator: AstBinaryOperator::Equal, .. }
                    if *left == size_expression)
            })).ok_or_else(missing)?;
            let (control, node) = unique(
                lowerer
                    .parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter(|(_, node)| {
                        matches!(node.payload(), Expression::If { condition: actual, .. } if *actual == condition)
                    }),
            )
            .ok_or_else(missing)?;
            let Expression::If {
                then_branch,
                else_branch: Some(failure),
                ..
            } = node.payload()
            else {
                return Err(missing());
            };
            let read =
                carrier::definition(lowerer.function, EntityId::Value(size)).ok_or_else(missing)?;
            if read.origin != Origin::Source(lowerer.expression_span(size_expression)?) {
                return Err(missing());
            }
            Ok((control, condition, *then_branch, *failure))
        }

        fn control_edges<'a>(
            lowerer: &'a ExpressionLowerer<'_>,
            control: ExpressionId,
            condition: ExpressionId,
            size: ValueId,
            span: Span,
        ) -> Result<(BlockId, ValueId, &'a Edge, &'a Edge), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let control_node = lowerer
                .parsed
                .ast()
                .expressions()
                .get(control)
                .map_err(|_| missing())?;
            let Expression::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } = control_node.payload()
            else {
                return Err(missing());
            };
            let origin = Origin::Source(control_node.span());
            let block = unique(lowerer.function.blocks.iter().filter(|block| {
                block
                    .terminator
                    .as_ref()
                    .is_some_and(|term| term.origin == origin)
            }))
            .ok_or_else(missing)?;
            let Some(TerminatorKind::Conditional {
                condition: result,
                when_true,
                when_false,
            }) = carrier::terminator(lowerer.function, block.id)
            else {
                return Err(missing());
            };
            let statement_origin = |statement| {
                lowerer
                    .parsed
                    .ast()
                    .statements()
                    .get(statement)
                    .map(|node| Origin::Source(node.span()))
                    .map_err(|_| missing())
            };
            if lowerer
                .function
                .block(when_true.target)
                .map(|block| &block.origin)
                != Some(&statement_origin(*then_branch)?)
                || lowerer
                    .function
                    .block(when_false.target)
                    .map(|block| &block.origin)
                    != Some(&statement_origin(*else_branch)?)
            {
                return Err(missing());
            }
            let comparison =
                carrier::definition(lowerer.function, EntityId::Value(*result)).ok_or_else(missing)?;
            let condition_node = lowerer
                .parsed
                .ast()
                .expressions()
                .get(condition)
                .map_err(|_| missing())?;
            let Expression::Binary {
                right,
                operator: AstBinaryOperator::Equal,
                ..
            } = condition_node.payload()
            else {
                return Err(missing());
            };
            let Operation::Compare {
                operator: ComparisonOperator::Equal,
                left,
                right: rhs,
            } = comparison.operation
            else {
                return Err(missing());
            };
            let constant =
                carrier::definition(lowerer.function, EntityId::Value(rhs)).ok_or_else(missing)?;
            if comparison.block != block.id
                || left != size
                || comparison.origin != Origin::Source(condition_node.span())
                || constant.block != block.id
                || constant.origin != Origin::Source(lowerer.expression_span(*right)?)
                || !matches!(constant.operation, Operation::Constant(_))
            {
                return Err(missing());
            }
            Ok((block.id, *result, when_true, when_false))
        }
    }
}
