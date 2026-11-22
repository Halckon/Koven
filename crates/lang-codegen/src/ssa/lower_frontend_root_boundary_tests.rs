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
    // SPEC-0454 also refines this empty Unit body. Its Boolean/Unit compiler types
    // must be identical without the unused declarations, not mistaken for layouts.
    let baseline = original_unit("fun entry(): Unit {}");
    let (baseline_program, baseline_entry) = baseline
        .lower_entry(baseline.declaration("entry", SymbolKind::Function))
        .expect("declaration-free baseline lowers");
    verify_program(&baseline_program).expect("baseline SSA verifies");
    let baseline_module = baseline_program.module(baseline_entry.module()).unwrap();
    assert_eq!(
        module.types, baseline_module.types,
        "unused declarations add no runtime layouts or compiler types"
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
        let drop =
            unique(drops.iter().copied().filter(|d| Some(*d) != transfer)).ok_or_else(bad)?;
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
                    IterationExitKind::Break(expression) => {
                        IterationPhiIncomingKind::Break(expression)
                    }
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
            if fact.owner() != Some(h.owner())
                || fact.condition() != Some(h.availability_condition())
            {
                return Err(bad());
            }
            let exit =
                unique(plan.exits().iter().filter(|e| e.kind() == body_exit)).ok_or_else(bad)?;
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
}
