use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::source::SourceMap;

use super::{
    model::{
        BinaryOperator, Edge, EntityId, EntityType, LoanKind, ModelError, Operation, Origin,
        Ownership, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
};

fn origins() -> (Origin, Origin) {
    let mut sources = SourceMap::default();
    let source_id = sources
        .add_source("ssa-model.ko", "fun main")
        .expect("test source must be unique");
    let source = Origin::Source(
        sources
            .span(source_id, 0, 4)
            .expect("test source span must be valid"),
    );
    let synthetic = Origin::Synthetic {
        anchor: sources
            .span(source_id, 4, 8)
            .expect("test synthetic anchor must be valid"),
        reason: "cfg split".to_owned(),
    };
    (source, synthetic)
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value entity, got {entity:?}");
    };
    value
}

#[test]
fn type_ids_are_deduplicated_and_reject_cross_program_owners() {
    let mut first = Program::default();
    let first_module = first.add_module("main");
    let module = first
        .module_mut(first_module)
        .expect("new module must be available");
    let unit = module.intern_type(SsaTypeKind::Unit);
    let repeated_unit = module.intern_type(SsaTypeKind::Unit);
    let endpoint = module.intern_type(SsaTypeKind::Opaque {
        name: "Endpoint".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    assert_eq!(unit, repeated_unit);
    assert_ne!(unit, endpoint);
    assert_eq!(module.types.len(), 2);

    let mut second = Program::default();
    let second_module = second.add_module("main");
    let foreign_unit = second
        .module_mut(second_module)
        .expect("new module must be available")
        .intern_type(SsaTypeKind::Unit);

    assert_ne!(first_module, second_module);
    assert_eq!(format!("{first_module:?}"), format!("{second_module:?}"));
    let error = first
        .module_mut(first_module)
        .expect("first module must remain available")
        .add_function("wrong_owner", vec![foreign_unit], origins().0)
        .expect_err("a type from another Program must be rejected");
    assert!(matches!(error, ModelError::WrongTypeOwner { .. }));
}

#[test]
fn builder_rejects_cross_function_entities_and_post_terminator_edits() {
    let (source, _) = origins();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must be available");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let first = module
        .add_function("first", Vec::new(), source.clone())
        .expect("local function signature must be valid");
    let second = module
        .add_function("second", Vec::new(), source.clone())
        .expect("local function signature must be valid");

    let first_block = module
        .function_mut(first)
        .expect("first function must be available")
        .add_block(Vec::new(), source.clone())
        .expect("entry block must be valid");
    let foreign_value = {
        let function = module
            .function_mut(second)
            .expect("second function must be available");
        let block = function
            .add_block(vec![EntityType::Value(integer)], source.clone())
            .expect("entry parameter must be valid");
        value(function.block(block).expect("block must exist").parameters[0])
    };

    let function = module
        .function_mut(first)
        .expect("first function must remain available");
    let error = function
        .append_instruction(
            first_block,
            Operation::Copy {
                source: foreign_value,
            },
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect_err("an entity from another function must be rejected");
    assert!(matches!(error, ModelError::WrongFunctionOwner { .. }));

    function
        .set_terminator(first_block, TerminatorKind::Abort, source.clone())
        .expect("first terminator must be accepted");
    assert!(matches!(
        function.set_terminator(first_block, TerminatorKind::Abort, source.clone()),
        Err(ModelError::TerminatorAlreadySet { .. })
    ));
    assert!(matches!(
        function.append_instruction(
            first_block,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            source,
        ),
        Err(ModelError::BlockAlreadyTerminated { .. })
    ));
}

#[test]
fn scalar_diamond_rendering_is_exact_and_deterministic() {
    let (source, synthetic) = origins();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must be available");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let function_id = module
        .add_function("select", vec![integer], source.clone())
        .expect("function signature must be valid");
    let function = module
        .function_mut(function_id)
        .expect("new function must be available");

    let entry = function
        .add_block(
            vec![EntityType::Value(boolean), EntityType::Value(integer)],
            source.clone(),
        )
        .expect("entry block must be valid");
    let when_true = function
        .add_block(vec![EntityType::Value(integer)], source.clone())
        .expect("true block must be valid");
    let when_false = function
        .add_block(vec![EntityType::Value(integer)], synthetic.clone())
        .expect("false block must be valid");
    let join = function
        .add_block(vec![EntityType::Value(integer)], source.clone())
        .expect("join block must be valid");

    let entry_parameters = &function
        .block(entry)
        .expect("entry block must exist")
        .parameters;
    let condition = value(entry_parameters[0]);
    let input = entry_parameters[1];
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: when_true,
                    arguments: vec![input],
                },
                when_false: Edge {
                    target: when_false,
                    arguments: vec![input],
                },
            },
            synthetic.clone(),
        )
        .expect("conditional terminator must be valid");

    let true_parameter = value(
        function
            .block(when_true)
            .expect("true block must exist")
            .parameters[0],
    );
    let (_, one) = function
        .append_instruction(
            when_true,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect("constant must be appendable");
    let (_, incremented) = function
        .append_instruction(
            when_true,
            Operation::Binary {
                operator: BinaryOperator::Add,
                left: true_parameter,
                right: value(one[0]),
            },
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect("add must be appendable");
    function
        .set_terminator(
            when_true,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: incremented,
            }),
            source.clone(),
        )
        .expect("true branch must be valid");

    let false_parameter = value(
        function
            .block(when_false)
            .expect("false block must exist")
            .parameters[0],
    );
    let (_, one) = function
        .append_instruction(
            when_false,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect("constant must be appendable");
    let (_, decremented) = function
        .append_instruction(
            when_false,
            Operation::Binary {
                operator: BinaryOperator::Subtract,
                left: false_parameter,
                right: value(one[0]),
            },
            vec![EntityType::Value(integer)],
            synthetic.clone(),
        )
        .expect("subtract must be appendable");
    function
        .set_terminator(
            when_false,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: decremented,
            }),
            synthetic.clone(),
        )
        .expect("false branch must be valid");

    let result = value(
        function
            .block(join)
            .expect("join block must exist")
            .parameters[0],
    );
    function
        .set_terminator(
            join,
            TerminatorKind::Return {
                values: vec![result],
            },
            source,
        )
        .expect("return terminator must be valid");

    let rendered = render_program(&program);
    assert_text_eq!(rendered, render_program(&program));
    assert_eq!(
        rendered,
        r#"module "main" {
  !t0 = bool
  !t1 = i64

  func "select"(%v0: value !t0, %v1: value !t1) -> (!t1) @source(SourceId(0):0..4) {
    bb0(%v0: value !t0, %v1: value !t1) @source(SourceId(0):0..4):
      cond %v0, bb1(%v1), bb2(%v1) @synthetic("cfg split", SourceId(0):4..8)
    bb1(%v2: value !t1) @source(SourceId(0):0..4):
      i0: %v5 = const 1 : (value !t1) @source(SourceId(0):0..4)
      i1: %v6 = add %v2, %v5 : (value !t1) @source(SourceId(0):0..4)
      branch bb3(%v6) @source(SourceId(0):0..4)
    bb2(%v3: value !t1) @synthetic("cfg split", SourceId(0):4..8):
      i2: %v7 = const 1 : (value !t1) @source(SourceId(0):0..4)
      i3: %v8 = sub %v3, %v7 : (value !t1) @synthetic("cfg split", SourceId(0):4..8)
      branch bb3(%v8) @synthetic("cfg split", SourceId(0):4..8)
    bb3(%v4: value !t1) @source(SourceId(0):0..4):
      return %v4 @source(SourceId(0):0..4)
  }
}
"#
    );
}

#[test]
fn place_and_loan_entities_keep_distinct_id_spaces() {
    let (source, _) = origins();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must be available");
    let endpoint = module.intern_type(SsaTypeKind::Opaque {
        name: "Endpoint".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let function_id = module
        .add_function("borrow", Vec::new(), source.clone())
        .expect("function signature must be valid");
    let function = module
        .function_mut(function_id)
        .expect("new function must be available");
    let entry = function
        .add_block(vec![EntityType::Value(endpoint)], source.clone())
        .expect("entry block must be valid");
    let owner = value(
        function
            .block(entry)
            .expect("entry block must exist")
            .parameters[0],
    );
    let (_, place) = function
        .append_instruction(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(endpoint)],
            source.clone(),
        )
        .expect("root place must be appendable");
    let EntityId::Place(place) = place[0] else {
        panic!("root place result must use the place ID space");
    };
    let (_, loan) = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: endpoint,
            }],
            source.clone(),
        )
        .expect("loan must be appendable");
    let EntityId::Loan(loan) = loan[0] else {
        panic!("borrow result must use the loan ID space");
    };
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            source.clone(),
        )
        .expect("borrow end must be appendable");
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), source.clone())
        .expect("drop must be appendable");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, source)
        .expect("return must be accepted");

    let rendered = render_program(&program);
    assert!(rendered.contains("%p0 = root_place %v0 : (place !t0)"));
    assert!(rendered.contains("%l0 = borrow.shared %p0 : (loan.shared !t0)"));
    assert!(rendered.contains("end_borrow %l0"));
    assert!(rendered.contains("drop %v0"));
}

pub(in crate::ssa) mod source_owner {
    //! Exact source-owner proof for the observed normal and direct-tail Map shapes.
    //! A source facts were first collected by the ordinary 12-path run on 9244391b.
    //! No shape is inferred from Sequential loops or accepted from a presence bit alone.
    pub(in crate::ssa) use crate::ssa::lower_frontend::map_iteration::*;
    pub(in crate::ssa) use crate::ssa::lower_frontend::orchestrate::nullable_map_guard_support::provenance;
    pub(in crate::ssa) use crate::ssa::ordered_map_storage_tests::source_owner_handoff as handoff;
    pub(in crate::ssa) use delivery::Delivery;
    use delivery::future;
    pub(in crate::ssa) use handoff::delivery;
    pub(in crate::ssa) use lang_frontend::ownership_checking::{
        CleanupConditions, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource, DropFact,
        DropTarget, IterationClosurePhiBinding, IterationPhiBoundary, IterationPhiIncomingBinding,
        IterationPhiIncomingKind, IterationPhiPresenceSource,
    };

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Proof {
        pub(in crate::ssa) flow: Option<Flow>,
        pub(in crate::ssa) disposition: Disposition,
        pub(in crate::ssa) transfer: Option<DropFact>,
        pub(in crate::ssa) conditional_return: Option<DropFact>,
    }

    /// Ownership retention requires one explicit, validated terminal disposition.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) enum Disposition {
        Drop(DropFact),
        Delivery(Delivery),
        Handoff(handoff::Proof),
    }

    /// A small capability handle into the same immutable ownership product.
    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Flow {
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) source: SourceIdentity,
        pub(in crate::ssa) span: Span,
        /// The unique same-epoch lexical Drop proved outside this provider's exits.
        pub(in crate::ssa) late: Option<DropFact>,
    }

    impl Proof {
        pub(in crate::ssa) fn at_checked(
            self,
            point: DropPoint,
            owned: &OwnershipCheckedFile,
        ) -> Result<Option<DropFact>, LoweringError> {
            match self.flow {
                Some(flow) => {
                    let plan = owned
                        .map_iteration(flow.statement)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.origin()))?;
                    if plan.exits().iter().any(|exit| exit.point() == point) {
                        conditional::source_drop(owned, flow, point)
                    } else {
                        flow.late
                            .filter(|fact| fact.point() == point && owned.drops().contains(fact))
                            .map(Some)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, flow.span))
                    }
                }
                None => Ok(self.at(point)),
            }
        }

        pub(in crate::ssa) fn drop(self) -> Option<DropFact> {
            match self.disposition {
                Disposition::Drop(fact) => Some(fact),
                Disposition::Delivery(_) | Disposition::Handoff(_) => None,
            }
        }
        pub(in crate::ssa) fn origin(self) -> Span {
            match self.disposition {
                Disposition::Drop(fact) => fact.value_origin(),
                Disposition::Delivery(delivery) => delivery.origin,
                Disposition::Handoff(proof) => proof.origin,
            }
        }
        pub(in crate::ssa) fn at(self, point: DropPoint) -> Option<DropFact> {
            if self.drop().is_some_and(|drop| drop.point() == point) {
                self.drop()
            } else {
                self.transfer
                    .into_iter()
                    .chain(self.conditional_return)
                    .find(|fact| fact.point() == point)
            }
        }
    }

    pub(in crate::ssa) fn qualify(
        parsed: &ParsedFile,
        names: &NameResolution,
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        symbol: SymbolId,
        body: StatementId,
        body_exit: IterationExitKind,
    ) -> Result<Proof, LoweringError> {
        qualify_with_birth(parsed, names, owned, plan, (symbol, None), body, body_exit)
    }

    pub(in crate::ssa) fn qualify_formed(
        parsed: &ParsedFile,
        names: &NameResolution,
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        root: (SymbolId, &crate::ssa::map_source::Formation<SymbolId>),
        body: StatementId,
        body_exit: IterationExitKind,
    ) -> Result<Proof, LoweringError> {
        qualify_with_birth(
            parsed,
            names,
            owned,
            plan,
            (root.0, Some(root.1)),
            body,
            body_exit,
        )
    }

    fn qualify_with_birth(
        parsed: &ParsedFile,
        names: &NameResolution,
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        root: (
            SymbolId,
            Option<&crate::ssa::map_source::Formation<SymbolId>>,
        ),
        body: StatementId,
        body_exit: IterationExitKind,
    ) -> Result<Proof, LoweringError> {
        let (symbol, birth) = root;
        let descriptor = plan.descriptor();
        let node = parsed
            .ast()
            .statements()
            .get(descriptor.statement())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = node.span();
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let origin = names
            .symbols()
            .get(symbol.index())
            .ok_or_else(missing)?
            .span();
        let transfer_point = match body_exit {
            IterationExitKind::Break(expression) | IterationExitKind::Return(expression) => {
                Some(DropPoint::ControlTransfer(expression))
            }
            IterationExitKind::Fallthrough | IterationExitKind::Continue(_) => None,
            IterationExitKind::Exhaustion => return Err(missing()),
        };
        let exhaustion_point = DropPoint::LoopExit(descriptor.statement());
        let source_drops = if let Some(formation) = birth {
            formed::drops(parsed, owned, symbol, formation)?
        } else {
            owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.target() == DropTarget::Named(symbol))
                .collect::<Vec<_>>()
        };
        if source_drops
            .iter()
            .any(|fact| fact.value_origin() != origin)
        {
            return Err(missing());
        }
        // A later source lifetime is legal frontend output, but is not this proof.
        if source_drops.len() != 1 + usize::from(transfer_point.is_some())
            || source_drops.iter().any(|fact| {
                fact.point() != exhaustion_point && Some(fact.point()) != transfer_point
            })
        {
            return future::qualify(parsed, names, owned, plan, root, body_exit, span);
        }
        let expected_incomings = if matches!(body_exit, IterationExitKind::Return(_)) {
            2
        } else {
            3
        };
        if plan.closure_phis().len() != 2
            || plan.closure_phi_incomings().len() != expected_incomings
            || !plan.capture_graph().nodes().is_empty()
        {
            return Err(unsupported());
        }
        let conditions = owned.cleanup_conditions();
        let header = phi(conditions, plan, symbol, IterationPhiBoundary::Header, span)?;
        let exit = phi(conditions, plan, symbol, IterationPhiBoundary::Exit, span)?;
        if header.owner() == exit.owner()
            || header.availability_selector() == exit.availability_selector()
            || header.availability_condition() == exit.availability_condition()
        {
            return Err(missing());
        }
        let entry = incoming(
            conditions,
            plan,
            header,
            IterationPhiIncomingKind::Entry,
            DropPoint::AfterExpression(descriptor.source()),
            span,
        )?;
        let [seed] = entry.values() else {
            return Err(missing());
        };
        let available = if let Some(birth) = birth {
            formed_condition(parsed, owned, (symbol, birth.birth), seed.source(), span)?
        } else {
            provenance::parameter(owned, symbol, origin, seed.source(), span)?
        };
        if seed.condition() != entry.available_when() || seed.condition() != available {
            return Err(missing());
        }
        match body_exit {
            IterationExitKind::Fallthrough | IterationExitKind::Continue(_) => {
                let kind = match body_exit {
                    IterationExitKind::Continue(expression) => {
                        IterationPhiIncomingKind::Continue(expression)
                    }
                    _ => IterationPhiIncomingKind::Fallthrough,
                };
                let backedge = incoming(
                    conditions,
                    plan,
                    header,
                    kind,
                    exit_shape::point(body_exit, descriptor.statement(), body),
                    span,
                )?;
                let [value] = backedge.values() else {
                    return Err(missing());
                };
                if value.source() != header.owner()
                    || value.condition() != header.availability_condition()
                    || backedge.available_when() != header.availability_condition()
                {
                    return Err(missing());
                }
            }
            IterationExitKind::Break(expression) => {
                let broken = incoming(
                    conditions,
                    plan,
                    exit,
                    IterationPhiIncomingKind::Break(expression),
                    DropPoint::ControlTransfer(expression),
                    span,
                )?;
                absent(conditions, broken, span)?;
            }
            // Return has no phi incoming. The real result and terminator are verified
            // by the Map carrier after the complete ControlTransfer actions execute.
            IterationExitKind::Return(_) => {}
            IterationExitKind::Exhaustion => return Err(missing()),
        }
        let exhausted = incoming(
            conditions,
            plan,
            exit,
            IterationPhiIncomingKind::Exhaustion,
            exhaustion_point,
            span,
        )?;
        absent(conditions, exhausted, span)?;
        let related = owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| {
                source_drops.contains(fact)
                    || fact.owner().is_some_and(|owner| {
                        [seed.source(), header.owner(), exit.owner()].contains(&owner)
                    })
            })
            .collect::<Vec<_>>();
        if related != source_drops {
            return Err(missing());
        }
        for fact in &source_drops {
            if fact.owner() != Some(header.owner())
                || fact.condition() != Some(header.availability_condition())
                || fact.capture_slot().is_some()
                || fact.instance_address().is_some()
            {
                return Err(missing());
            }
        }
        let drop = unique(
            source_drops
                .iter()
                .copied()
                .filter(|fact| fact.point() == exhaustion_point),
        )
        .ok_or_else(missing)?;
        let transfer = transfer_point
            .map(|point| {
                unique(
                    source_drops
                        .iter()
                        .copied()
                        .filter(|fact| fact.point() == point),
                )
                .ok_or_else(missing)
            })
            .transpose()?;
        for (kind, fact) in [
            (body_exit, transfer),
            (IterationExitKind::Exhaustion, Some(drop)),
        ] {
            let exit = unique(plan.exits().iter().filter(|exit| exit.kind() == kind))
                .ok_or_else(missing)?;
            let actual = exit
                .actions()
                .iter()
                .filter_map(|action| match action {
                    IterationCleanupAction::Drop(fact)
                        if fact.target() == DropTarget::Named(symbol) =>
                    {
                        Some(*fact)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if actual != fact.into_iter().collect::<Vec<_>>() {
                return Err(missing());
            }
            if let Some(fact) = fact {
                let terminal = [
                    IterationCleanupAction::FinishProvider(descriptor.statement()),
                    IterationCleanupAction::EndSource(descriptor.statement()),
                    IterationCleanupAction::Drop(fact),
                ];
                if !exit.actions().ends_with(&terminal)
                    || (kind == IterationExitKind::Exhaustion && exit.actions() != terminal)
                {
                    return Err(unsupported());
                }
            }
        }
        Ok(Proof {
            flow: None,
            disposition: source_owner::Disposition::Drop(drop),
            transfer,
            conditional_return: None,
        })
    }

    fn absent(
        conditions: &CleanupConditions,
        incoming: &IterationPhiIncomingBinding,
        span: Span,
    ) -> Result<(), LoweringError> {
        match conditions.get(incoming.available_when()) {
            None => Err(error(LoweringErrorKind::MissingFact, span)),
            Some(CleanupCondition::Never) if incoming.values().is_empty() => Ok(()),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    pub(in crate::ssa) fn phi<'a>(
        conditions: &CleanupConditions,
        plan: &'a MapIterationOwnershipPlan,
        symbol: SymbolId,
        boundary: IterationPhiBoundary,
        span: Span,
    ) -> Result<&'a IterationClosurePhiBinding, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let statement = plan.descriptor().statement();
        let phi = unique(
            plan.closure_phis()
                .iter()
                .filter(|phi| phi.symbol() == symbol && phi.boundary() == boundary),
        )
        .ok_or_else(missing)?;
        if !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
            statement: actual, boundary: actual_boundary, symbol: actual_symbol, origin,
        }) if *actual == statement && *actual_boundary == boundary && *actual_symbol == symbol && *origin == span)
            || !phi.root_nodes().is_empty()
            || !phi.capture_layout().is_empty()
            || !phi.origins().is_empty()
        {
            return Err(missing());
        }
        let selector = conditions
            .selector(phi.availability_selector())
            .ok_or_else(missing)?;
        if !matches!(selector.source(), CleanupSelectorSource::IterationPhi { statement: actual, boundary: actual_boundary, .. }
            if actual == statement && actual_boundary == boundary)
            || selector.selection() != CleanupSelection::IterationPresence
            || selector.origin() != span
            || selector.branch_count() != 2
        {
            return Err(missing());
        }
        let Some(CleanupCondition::Choice { selector, branches }) =
            conditions.get(phi.availability_condition())
        else {
            return Err(missing());
        };
        let [absent, present] = branches.as_slice() else {
            return Err(missing());
        };
        if *selector != phi.availability_selector()
            || conditions.get(*absent) != Some(&CleanupCondition::Never)
            || conditions.get(*present) != Some(&CleanupCondition::Always)
        {
            return Err(missing());
        }
        Ok(phi)
    }

    pub(in crate::ssa) fn incoming<'a>(
        conditions: &CleanupConditions,
        plan: &'a MapIterationOwnershipPlan,
        phi: &IterationClosurePhiBinding,
        kind: IterationPhiIncomingKind,
        point: DropPoint,
        span: Span,
    ) -> Result<&'a IterationPhiIncomingBinding, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == kind),
        )
        .ok_or_else(missing)?;
        if incoming.point() != point
            || incoming.boundary() != phi.boundary()
            || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
        {
            return Err(missing());
        }
        let [binding] = incoming.bindings() else {
            return Err(missing());
        };
        if binding.target() != phi.owner()
            || binding.availability_selector() != phi.availability_selector()
            || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
            || !binding.capture_slots_to_clear().is_empty()
            || !binding.root_sources().is_empty()
            || !binding.selector_writes().is_empty()
            || !binding.origins().is_empty()
        {
            return Err(missing());
        }
        Ok(binding)
    }

    pub(in crate::ssa) fn supported_drop(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: DropFact,
    ) -> bool {
        typed.map_iterations().iter().any(|descriptor| {
            let Ok(qualified) = qualify::qualify(parsed, names, typed, owned, *descriptor) else {
                return false;
            };
            if qualified.owner.is_some_and(|proof| {
                proof
                    .at_checked(fact.point(), owned)
                    .is_ok_and(|actual| actual == Some(fact))
            }) {
                return true;
            }
            let SourceIdentity::Formed { symbol, .. } = qualified.source else {
                return false;
            };
            let Ok(node) = parsed.ast().statements().get(descriptor.statement()) else {
                return false;
            };
            // This is only preflight capability for a different binding epoch. The
            // emitter separately proves its exact Move/Call and current SSA owner.
            formed::other_epoch_drop(
                (parsed, names, typed, owned),
                symbol,
                descriptor.source_type(),
                fact,
                node.span(),
            )
            .is_ok_and(|supported| supported)
        })
    }

    /// Resolve the complete live FE graph to the real local producer, including prior providers.
    pub(in crate::ssa) fn formed_condition(
        parsed: &ParsedFile,
        owned: &OwnershipCheckedFile,
        root: (SymbolId, crate::ssa::map_source::Birth<SymbolId>),
        owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
        span: Span,
    ) -> Result<lang_frontend::ownership_checking::CleanupConditionId, LoweringError> {
        birth_condition(parsed, owned, root, owner, span, false)
    }

    pub(in crate::ssa) fn birth_condition(
        parsed: &ParsedFile,
        owned: &OwnershipCheckedFile,
        root: (SymbolId, crate::ssa::map_source::Birth<SymbolId>),
        owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
        span: Span,
        flow: bool,
    ) -> Result<lang_frontend::ownership_checking::CleanupConditionId, LoweringError> {
        use crate::ssa::map_source::Birth;
        match root.1 {
            Birth::Expression(_) => Err(error(LoweringErrorKind::UnsupportedNode, span)),
            Birth::Call(expression) => {
                let origin = parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
                    .span();
                if flow {
                    provenance::flow(owned, root.0, origin, Some(expression), owner, span)
                } else {
                    provenance::formed(owned, root.0, (expression, origin), owner, span)
                }
            }
            Birth::EntryParameter {
                root: parameter,
                origin,
                ..
            } => provenance::entry(owned, root.0, (parameter, origin), owner, span, flow),
        }
    }

    /// Validate the complete published owner graph against this precise source birth.
    pub(in crate::ssa) fn flow_condition(
        parsed: &ParsedFile,
        names: &NameResolution,
        owned: &OwnershipCheckedFile,
        flow: Flow,
        owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
        span: Span,
    ) -> Result<lang_frontend::ownership_checking::CleanupConditionId, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let plan = owned.map_iteration(flow.statement).ok_or_else(bad)?;
        match flow.source {
            SourceIdentity::BorrowedBinding(_) => Err(bad()),
            SourceIdentity::Parameter(symbol) => {
                let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
                provenance::flow(owned, symbol, origin, None, owner, span)
            }
            SourceIdentity::Local {
                symbol,
                initializer,
            } => birth_condition(
                parsed,
                owned,
                (symbol, crate::ssa::map_source::Birth::Call(initializer)),
                owner,
                span,
                true,
            ),
            SourceIdentity::Formed { symbol, birth } => {
                birth_condition(parsed, owned, (symbol, birth), owner, span, true)
            }
            SourceIdentity::Temporary(expression) => {
                let origin = parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| bad())?
                    .span();
                if owned.cleanup_conditions().owner_value(owner)
                    != Some(&CleanupOwnerValue::Expression { expression, origin })
                {
                    return Err(bad());
                }
                let entry = unique(
                    plan.closure_phi_incomings()
                        .iter()
                        .filter(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry),
                )
                .ok_or_else(bad)?;
                if owned.cleanup_conditions().get(entry.condition())
                    != Some(&CleanupCondition::Always)
                {
                    return Err(bad());
                }
                Ok(entry.condition())
            }
        }
    }
}
