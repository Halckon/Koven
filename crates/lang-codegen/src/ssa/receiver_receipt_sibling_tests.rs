//! One analysis containing P2/P4 and both existing owner consumers, without native execution.
use super::*;
#[test]
fn receiver_consumer_and_owner_siblings_keep_production_receipts_separate() {
    let text = r#"class Node { fun touch(): Int = 1 }
fun receiver(own node: Node): Unit { node.touch() }
fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun loan(): Unit { val owner = "kept"; consume(view(owner)) }
fun sink(own item: String): Unit {}
fun owner_call(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; sink(held) }
fun owner_drop(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }
fun entry(): Unit { receiver(Node()); loan(); owner_call(false); owner_drop(false) }"#;
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "receipt-siblings.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receipt-siblings.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let ((((lowered, calls), drops), consumer), receiver) =
            super::super::refine::receiver_receipt::observe(|| {
                super::super::refine::loan_receipt::observe(|| {
                    crate::ssa::observe_owner_drops(|| {
                        crate::ssa::observe_owner_calls(|| {
                            super::super::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, &owned, entry,
                            )
                        })
                    })
                })
            });
        let (program, _) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        let [p2] = receiver.as_slice() else {
            panic!("one P2 receipt");
        };
        let [p4] = consumer.as_slice() else {
            panic!("one P4 receipt");
        };
        assert_ne!(p2.caller, p4.caller);
        assert_ne!(p2.call, p4.call);
        assert_ne!(p2.callable, p4.callable);
        assert!(p2.ids.iter().all(|id| !p4.ids.contains(id)));
        assert!(
            p2.entities
                .iter()
                .all(|entity| !p4.entities.contains(entity))
        );
        assert_eq!(
            drops.iter().filter(|drop| drop.3.is_some()).count(),
            1,
            "only B owner Drop authorization"
        );
        assert_eq!(
            calls.iter().filter(|call| call.3.is_some()).count(),
            1,
            "only own String Call authorization"
        );
        for (protocol, ids, call) in [(2, &p2.ids, p2.call), (4, &p4.ids, p4.call)] {
            let matches = owned
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|o| o.regions())
                .filter(|r| r.protocol_version() == protocol && r.call() == call)
                .collect::<Vec<_>>();
            let [region] = matches.as_slice() else {
                panic!("one same-analysis region");
            };
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(*ids, region.action_ids().collect::<Vec<_>>());
        }
    }
}

pub(in crate::ssa) mod map_iteration {
    //! Bounded two-component Map provider; independent from Sequential snapshots.
    pub(in crate::ssa) use crate::ssa::lower_frontend::control::map_frames::carrier;
    pub(in crate::ssa) use crate::ssa::map_provider_exits_pipeline_tests::nested_exits::query_inputs::query_cleanup::map_iteration_conditional as conditional;
    pub(in crate::ssa) use conditional::checked;
    pub(in crate::ssa) use checked::nested;
    pub(in crate::ssa) use crate::ssa::checked_integer_conversion_tests::map_iteration_exits as exits;
    pub(in crate::ssa) use exits::exit_shape;
    pub(in crate::ssa) use crate::ssa::unit_lower_control_result_tests::map_iteration_formed as formed;
    pub(in crate::ssa) use crate::ssa::mixed_borrow_boundary_tests::map_iteration_local as local;
    pub(in crate::ssa) use crate::ssa::lower_frontend::nullable_if_cleanup::local_runtime;
    pub(in crate::ssa) use crate::ssa::lower_frontend_tests::root_boundaries::map_iteration_resource as resource;
    pub(in crate::ssa) use crate::ssa::multi_borrow_range_boundary_tests::map_iteration_source as source;
    pub(in crate::ssa) use source::borrowed;
    pub(in crate::ssa) use crate::ssa::tests::source_owner;
    pub(in crate::ssa) use crate::ssa::borrowed_container_lowering_tests::map_iteration_temporary as temporary;
    pub(in crate::ssa) use temporary::{guard, qualify};

    pub(in crate::ssa) use crate::ssa::lower_frontend::*;
    pub(in crate::ssa) use crate::ssa::model::{Definition, MapContainerKind, Module, SsaTypeKind};
    pub(in crate::ssa) use control::{LinearBindingSlot, LinearBindings};
    pub(in crate::ssa) use lang_frontend::ownership_checking::{
        CleanupCondition, IterationCleanupAction, IterationExitKind, IterationExitPlan,
        MapIterationOwnershipPlan, MapIterationSourceAccess,
    };

    pub(in crate::ssa) use exits::Context;
    pub(in crate::ssa) use qualify::validate;
    pub(in crate::ssa) use source::SourceIdentity;
    pub(in crate::ssa) fn supported_drop(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: lang_frontend::ownership_checking::DropFact,
    ) -> bool {
        local::supported_drop(parsed, names, typed, owned, fact)
            || source_owner::supported_drop(parsed, names, typed, owned, fact)
            || conditional::supported_local_drop(parsed, names, typed, owned, fact)
            || typed.map_iterations().iter().any(|d| {
                let Some(plan) = owned.map_iteration(d.statement()) else {
                    return false;
                };
                let Ok(node) = parsed.ast().statements().get(d.statement()) else {
                    return false;
                };
                formed::retained::proof((parsed, names, typed, owned), plan, fact, node.span())
                    .is_ok_and(|proof| proof.is_some())
            })
    }

    type Storage = BTreeMap<SsaTypeId, (MapContainerKind, SsaTypeId, SsaTypeId)>;

    /// Read the actual Module only after ordinary type planning has finished.
    pub(in crate::ssa) fn storage(module: &Module, types: &BTreeMap<TypeId, SsaTypeId>) -> Storage {
        types
            .values()
            .filter_map(|&ty| module.map_container(ty).map(|layout| (ty, layout)))
            .collect()
    }

    #[derive(Default)]
    pub(in crate::ssa) struct State {
        pub(in crate::ssa) entries: BTreeMap<SymbolId, EntityId>,
        pub(in crate::ssa) locals: BTreeMap<SymbolId, local_runtime::Carrier>,
        pub(in crate::ssa) resources: BTreeMap<SymbolId, resource::Carrier>,
        pub(in crate::ssa) storage: Storage,
    }
    impl State {
        pub(in crate::ssa) fn new(
            bindings: &BTreeMap<SymbolId, LoweredValue>,
            loans: &BTreeMap<SymbolId, LoanId>,
            storage: &Storage,
        ) -> Self {
            Self {
                entries: bindings
                    .iter()
                    .filter_map(|(&symbol, binding)| match binding {
                        LoweredValue::Value(owner) => Some((symbol, EntityId::Value(*owner))),
                        _ => None,
                    })
                    .chain(
                        loans
                            .iter()
                            .map(|(&symbol, &loan)| (symbol, EntityId::Loan(loan))),
                    )
                    .collect(),
                storage: storage.clone(),
                locals: BTreeMap::new(),
                resources: BTreeMap::new(),
            }
        }
    }

    pub(in crate::ssa) fn unique<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
        let result = values.next()?;
        values.next().is_none().then_some(result)
    }

    /// One path-local provider frame; false is rebuilt from its independent snapshot.
    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Frame {
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) source: Option<LoanId>,
        pub(in crate::ssa) components: [(SymbolId, Option<LoanId>); 2],
        pub(in crate::ssa) finished: bool,
    }

    /// Reuse complete provider qualification for this exact stable binding's For.
    pub(in crate::ssa) fn stable_borrow_provider(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        statement: StatementId,
        binding: SymbolId,
    ) -> bool {
        typed.map_iteration(statement).is_some_and(|descriptor| {
            qualify::qualify(parsed, names, typed, owned, *descriptor).is_ok_and(|qualified|
                matches!(qualified.source, SourceIdentity::BorrowedBinding(proof) if proof.binding == binding))
        })
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_map_iteration(
            &mut self,
            statement: StatementId,
            source: ExpressionId,
            body: StatementId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let descriptor = *self
                .typed
                .map_iteration(statement)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let qualified =
                qualify::qualify(self.parsed, self.names, self.typed, self.owned, descriptor)?;
            if descriptor.source() != source || qualified.body != body {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.validate_map_parent(qualified.nesting, span)?;
            if let Some(lambda) = self.thunk_expression {
                let missing = || error(LoweringErrorKind::MissingFact, span);
                if qualified.local.is_none() {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let owner = self.source_token.ok_or_else(missing)?;
                let plan = self
                    .source_closures
                    .lambdas
                    .get(&(owner, lambda.index()))
                    .ok_or_else(missing)?;
                if plan.owner != owner
                    || plan.expression != lambda
                    || plan.thunk != self.function.id
                    || plan.return_type != self.return_type
                    || !plan.captures.is_empty()
                    || builtin_type(self.typed, plan.return_type) != Some(BuiltinType::Unit)
                    || local::lambda_body(self.parsed, self.typed, self.owned, plan.body, span)?
                        != lambda
                    || !matches!(self.parsed.ast().statements().get(plan.body).map_err(|_| missing())?.payload(),
                        Statement::LambdaBody { elements } if elements.contains(&statement))
                {
                    return Err(missing());
                }
            }
            if self.readonly_receiver.is_some()
                || !self.pending_call_loans.is_empty()
                || self
                    .temporaries
                    .iter()
                    .any(|(&key, &value)| !self.active_map_temporary(key, value))
                || !self.non_null_bindings.is_empty()
                || !self.capture_loans.is_empty()
                || self.result_source_loans.iter().any(|(binding, loans)| {
                    !matches!(qualified.source, SourceIdentity::BorrowedBinding(proof)
                        if proof.binding == *binding && loans.is_empty())
                })
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let plan = qualified.plan.clone();
            let body_exit = qualified.body_exit;
            let source_identity = qualified.source;
            let owner_proof = qualified.owner;
            let source_origin = qualified.origin;
            if let Some(local) = &qualified.local {
                self.validate_local_map_entry(local, span)?;
            }
            let mapped = |ty| {
                self.resolve_type(ty, span).and_then(|ty| {
                    self.type_ids
                        .get(&ty)
                        .copied()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
                })
            };
            let container = mapped(descriptor.source_type())?;
            let key_type = mapped(descriptor.key_type())?;
            let value_type = mapped(descriptor.value_type())?;
            let expected_kind = match descriptor.kind() {
                lang_frontend::type_checking::MapIterationKind::Map => MapContainerKind::Map,
                lang_frontend::type_checking::MapIterationKind::MutableMap => {
                    MapContainerKind::MutableMap
                }
            };
            if self.map_iteration_state.storage.get(&container)
                != Some(&(expected_kind, key_type, value_type))
                || self.ssa_types.get(container.index())
                    != Some(&SsaTypeKind::MapContainer {
                        kind: expected_kind,
                        key: key_type,
                        value: value_type,
                    })
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let cursor_type = self.ssa_builtin(BuiltinType::ULong, span)?;
            let bool_type = self.ssa_builtin(BuiltinType::Boolean, span)?;
            if self.ssa_types.get(cursor_type.index())
                != Some(&SsaTypeKind::Integer {
                    bits: 64,
                    signed: false,
                })
                || self.ssa_types.get(bool_type.index()) != Some(&SsaTypeKind::Boolean)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let seed = self.map_iteration_seed(source_identity, container, source_origin, span)?;
            let source_loan = match (plan.source_access(), seed) {
                (
                    MapIterationSourceAccess::Owned | MapIterationSourceAccess::Temporary,
                    EntityId::Value(owner),
                ) => {
                    if self.function.entity(seed).map(|entity| entity.ty)
                        != Some(EntityType::Value(container))
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    if source_identity.current_owner(self) != Some(owner) {
                        let kind = match source_identity {
                            SourceIdentity::Parameter(_) => LoweringErrorKind::UnsupportedNode,
                            SourceIdentity::BorrowedBinding(_)
                            | SourceIdentity::Local { .. }
                            | SourceIdentity::Formed { .. }
                            | SourceIdentity::Temporary(_) => LoweringErrorKind::MissingFact,
                        };
                        return Err(error(kind, span));
                    }
                    let (_, places) = self.append(
                        Operation::RootPlace { owner },
                        vec![EntityType::Place(container)],
                        span,
                    )?;
                    let [EntityId::Place(place)] = places.as_slice() else {
                        return Err(error(LoweringErrorKind::InvalidModel, span));
                    };
                    self.map_iteration_loan(
                        Operation::BorrowBegin {
                            place: *place,
                            kind: LoanKind::Shared,
                        },
                        container,
                        span,
                    )?
                }
                (MapIterationSourceAccess::Shared, EntityId::Loan(loan)) => {
                    let symbol = match source_identity {
                        SourceIdentity::Parameter(symbol) => symbol,
                        SourceIdentity::BorrowedBinding(proof) => proof.binding,
                        _ => return Err(error(LoweringErrorKind::MissingFact, span)),
                    };
                    if self.borrow_bindings.get(&symbol) != Some(&loan)
                        || self.function.entity(seed).map(|entity| entity.ty)
                            != Some(EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: container,
                            })
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    self.map_iteration_loan(
                        Operation::SharedReborrow { source: loan },
                        container,
                        span,
                    )?
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            let first = self.map_iteration_value(
                Operation::MapIterationFirst {
                    source: source_loan,
                },
                cursor_type,
                span,
            )?;
            let baseline = self.bindings.clone();
            let mut carried = self.linear_binding_slots(&baseline, span)?;
            for (&binding, &lowered) in &baseline {
                let LoweredValue::Value(owner) = lowered else {
                    continue;
                };
                if carried
                    .slots
                    .iter()
                    .any(|slot| slot.symbol == Some(binding))
                {
                    continue;
                }
                let entity = EntityId::Value(owner);
                let ty = self
                    .function
                    .entity(entity)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                    .ty;
                crate::ssa::carry::seed_slot(
                    &mut carried.slots,
                    LinearBindingSlot::new(Some(binding), entity, ty),
                );
            }
            // An entry Borrow source has a real caller column too. Reuse ordinary
            // bindings so all later providers see the transported current root loan.
            if let (SourceIdentity::Parameter(symbol), EntityId::Loan(loan)) =
                (source_identity, seed)
            {
                let entity = EntityId::Loan(loan);
                let choice = crate::ssa::carry::associate_source(
                    self.function,
                    &mut carried.slots,
                    entity,
                    crate::ssa::carry::TypeAtMiss::Query,
                    error(LoweringErrorKind::MissingFact, span),
                )?;
                if let crate::ssa::carry::ColumnMatch::Added(column) = choice {
                    carried.slots[column.index()].borrow_symbols.push(symbol);
                }
            }
            let source_slot = carried.slots.len();
            let cursor_slot = source_slot + 1;
            let mut types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
            types.extend([
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: container,
                },
                EntityType::Value(cursor_type),
            ]);
            let preheader = self.block;
            let header = self
                .function
                .add_block(types.clone(), Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let body_block = self
                .function
                .add_block(types.clone(), Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let exhausted = self
                .function
                .add_block(types, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let mut entry_args = carried
                .slots
                .iter()
                .map(|slot| slot.source)
                .collect::<Vec<_>>();
            entry_args.extend([EntityId::Loan(source_loan), EntityId::Value(first)]);
            self.function
                .set_terminator(
                    preheader,
                    TerminatorKind::Branch(Edge {
                        target: header,
                        arguments: entry_args,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.block = header;
            self.bindings = self.rebind_linear_bindings(&baseline, header, &carried, span)?;
            let header_parameters = self.map_iteration_parameters(header, span)?;
            let (header_source, header_cursor) =
                roles(&header_parameters, source_slot, cursor_slot, span)?;
            let condition = self.map_iteration_value(
                Operation::MapIterationHasNext {
                    source: header_source,
                    cursor: header_cursor,
                },
                bool_type,
                span,
            )?;
            self.function
                .set_terminator(
                    header,
                    TerminatorKind::Conditional {
                        condition,
                        when_true: Edge {
                            target: body_block,
                            arguments: header_parameters.clone(),
                        },
                        when_false: Edge {
                            target: exhausted,
                            arguments: header_parameters,
                        },
                    },
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.block = exhausted;
            self.bindings = self.rebind_linear_bindings(&baseline, exhausted, &carried, span)?;
            let false_exit = self.branch_exit(LoweredValue::Unit);
            self.block = body_block;
            self.bindings = self.rebind_linear_bindings(&baseline, body_block, &carried, span)?;
            let body_parameters = self.map_iteration_parameters(body_block, span)?;
            let (body_source, body_cursor) =
                roles(&body_parameters, source_slot, cursor_slot, span)?;
            let key = self.map_iteration_loan(
                Operation::MapIterationKeyLoan {
                    source: body_source,
                    cursor: body_cursor,
                },
                key_type,
                span,
            )?;
            let value = self.map_iteration_loan(
                Operation::MapIterationValueLoan {
                    source: body_source,
                    cursor: body_cursor,
                },
                value_type,
                span,
            )?;
            if key == value
                || self
                    .borrow_bindings
                    .insert(descriptor.key_binding(), key)
                    .is_some()
                || self
                    .borrow_bindings
                    .insert(descriptor.value_binding(), value)
                    .is_some()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let body_owner = self.map_iteration_owner(
                owner_proof,
                source_identity,
                &carried,
                &body_parameters,
                span,
            )?;
            self.loops.push(loop_control::LoopContext {
                entry_views: self.non_null_bindings.clone(),
                outer_borrowed_while: None,
                header,
                carried: carried.clone(),
                continues: Vec::new(),
                breaks: Vec::new(),
                for_loop: None,
                numeric_loop: None,
                map_loop: Some(Context {
                    plan: plan.clone(),
                    kind: body_exit,
                    body: body_block,
                    frame: Frame {
                        statement,
                        source: Some(body_source),
                        components: [
                            (descriptor.key_binding(), Some(key)),
                            (descriptor.value_binding(), Some(value)),
                        ],
                        finished: false,
                    },
                    owner: body_owner,
                    cursor: body_cursor,
                    cursor_type,
                    next: None,
                    cleaned: false,
                    returned: None,
                    cleanup_start: None,
                    conditional: qualified.conditional.map(conditional::State::new),
                    nesting: qualified.nesting,
                    children: Vec::new(),
                    resumed: None,
                }),
            });
            // The exact selected exit owns the body cleanup point. A real transfer
            // is already Diverged and must not acquire a synthetic fallthrough/Next.
            let result = self.lower_statement_inner(body)?;
            if (qualified.conditional.is_none()
                && self.block
                    != self
                        .loops
                        .last()
                        .and_then(|c| c.map_loop.as_ref())
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                        .body)
                || (qualified.conditional.is_none()
                    && (result == LoweredValue::Diverged)
                        == (body_exit == IterationExitKind::Fallthrough))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            if result != LoweredValue::Diverged {
                self.clean_map_iteration_body(IterationExitKind::Fallthrough, span)?;
                self.advance_map_iteration_body(span)?;
            }
            let mut context = self
                .loops
                .pop()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let state = context
                .map_loop
                .take()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if !state.cleaned
                || !context.continues.is_empty()
                || context.breaks.len()
                    != if qualified.conditional.is_some() {
                        plan.exits()
                            .iter()
                            .filter(|exit| matches!(exit.kind(), IterationExitKind::Break(_)))
                            .count()
                    } else {
                        usize::from(matches!(body_exit, IterationExitKind::Break(_)))
                    }
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            // Restore every ordinary owner/loan slot from the independent false edge.
            if self.merge_exits(vec![false_exit], &baseline, span)? != LoweredValue::Unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let exhausted_parameters = self.map_iteration_parameters(exhausted, span)?;
            let (exhausted_source, _) =
                roles(&exhausted_parameters, source_slot, cursor_slot, span)?;
            let mut frame = Frame {
                statement,
                source: Some(exhausted_source),
                components: [
                    (descriptor.key_binding(), None),
                    (descriptor.value_binding(), None),
                ],
                finished: false,
            };
            let owner = self.map_iteration_owner(
                owner_proof,
                source_identity,
                &carried,
                &exhausted_parameters,
                span,
            )?;
            let exhaustion = unique(
                plan.exits()
                    .iter()
                    .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
            )
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_map_iteration_cleanup(
                exhaustion,
                &mut frame,
                owner,
                qualified.conditional.is_some(),
                span,
            )?;
            let owner_mismatch = if let Some((proof, source, value)) = owner {
                let live = proof.at_checked(exhaustion.point(), self.owned)?.is_none();
                source.current_owner(self) != live.then_some(value)
            } else {
                false
            };
            if !frame.finished || frame.source.is_some() || owner_mismatch {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            context.breaks.push(self.branch_exit(LoweredValue::Unit));
            self.merge_exits(context.breaks, &baseline, span)?;
            let evidence = carrier::Evidence {
                parsed: self.parsed,
                names: self.names,
                typed: self.typed,
                owned: self.owned,
                exhaustion_point: exhaustion.point(),
                function: self.function,
                conditional: state.conditional.as_ref(),
                children: &state.children,
                cleanup_body: state.body,
                seed,
                source: source_loan,
                first,
                next: state.next,
                body_exit,
                // Conditional/nested validators start from the original body owner;
                // only a checked-success carrier records a replacement body role.
                body_owner: if state.resumed.is_some() && qualified.conditional.is_none() {
                    state.owner
                } else {
                    body_owner
                },
                resumed: state.resumed,
                returned: state.returned,
                cleanup_start: state.cleanup_start,
                continuation: self.block,
                key,
                value,
                blocks: [preheader, header, body_block, exhausted],
                carried: &carried,
                source_slot,
                cursor_slot,
                owner,
                source_origin,
                span,
            };
            evidence.validate()?;
            if let Some(local) = qualified.local {
                let completed = self.local_map_completion(local, evidence)?;
                if self
                    .map_iteration_state
                    .locals
                    .insert(completed.root(), completed)
                    .is_some()
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            if let Some(parent) = self
                .loops
                .last_mut()
                .and_then(|context| context.map_loop.as_mut())
            {
                parent.children.extend(state.children.iter().copied());
                parent.children.push(carrier::Child {
                    statement,
                    blocks: [preheader, header, body_block, exhausted],
                    source: source_loan,
                    first,
                    source_slot,
                    cursor_slot,
                });
            }
            if !self
                .loops
                .iter()
                .filter_map(|context| context.map_loop.as_ref())
                .any(|active| {
                    active
                        .owner
                        .is_some_and(|(_, source, _)| source == source_identity)
                })
            {
                self.retain_resource_map(owner, seed, span)?;
            }
            Ok(LoweredValue::Unit)
        }

        fn map_iteration_owner(
            &self,
            proof: Option<source_owner::Proof>,
            source: SourceIdentity,
            carried: &LinearBindings,
            parameters: &[EntityId],
            span: Span,
        ) -> Result<Option<(source_owner::Proof, SourceIdentity, ValueId)>, LoweringError> {
            proof
                .map(|proof| {
                    let slot = unique(
                        carried
                            .slots
                            .iter()
                            .enumerate()
                            .filter(|(_, slot)| source.matches_slot(slot)),
                    )
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                    .0;
                    let Some(EntityId::Value(value)) = parameters.get(slot) else {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    };
                    if source.current_owner(self) != Some(*value) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    Ok((proof, source, *value))
                })
                .transpose()
        }

        pub(in crate::ssa) fn map_iteration_value(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
            match results.as_slice() {
                [EntityId::Value(value)] => Ok(*value),
                _ => Err(error(LoweringErrorKind::InvalidModel, span)),
            }
        }
        fn map_iteration_loan(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoanId, LoweringError> {
            let (_, results) = self.append(
                operation,
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }],
                span,
            )?;
            match results.as_slice() {
                [EntityId::Loan(loan)] => Ok(*loan),
                _ => Err(error(LoweringErrorKind::InvalidModel, span)),
            }
        }
        fn map_iteration_parameters(
            &self,
            block: BlockId,
            span: Span,
        ) -> Result<Vec<EntityId>, LoweringError> {
            self.function
                .block(block)
                .map(|block| block.parameters.clone())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }
        pub(in crate::ssa) fn map_iteration_carried_arguments(
            &self,
            carried: &LinearBindings,
            span: Span,
        ) -> Result<Vec<EntityId>, LoweringError> {
            carried
                .slots
                .iter()
                .map(|slot| {
                    if slot.has_map_role() {
                        return self.map_current_private_slot(slot, span);
                    }
                    if let Some(symbol) = slot.symbol {
                        return match self.bindings.get(&symbol) {
                            Some(LoweredValue::Value(owner)) => Ok(EntityId::Value(*owner)),
                            _ => Err(error(LoweringErrorKind::MissingFact, span)),
                        };
                    }
                    if let [key] = slot.temporaries.as_slice() {
                        return self
                            .temporaries
                            .get(key)
                            .copied()
                            .map(EntityId::Value)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span));
                    }
                    // Only original non-entry borrow bindings or this qualified source's
                    // exact temporary owner can be present; other state is rejected at entry.
                    let loans = slot
                        .borrow_symbols
                        .iter()
                        .filter_map(|symbol| self.borrow_bindings.get(symbol))
                        .copied()
                        .collect::<Vec<_>>();
                    match loans.as_slice() {
                        [loan] => Ok(EntityId::Loan(*loan)),
                        _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
                    }
                })
                .collect()
        }

        pub(in crate::ssa) fn emit_map_iteration_cleanup(
            &mut self,
            exit: &IterationExitPlan,
            frame: &mut Frame,
            owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
            flow: bool,
            span: Span,
        ) -> Result<(), LoweringError> {
            for action in exit.actions() {
                self.emit_map_cleanup_action(action, exit.point(), frame, owner, flow, span)?;
            }
            Ok(())
        }

        pub(in crate::ssa) fn emit_map_cleanup_action(
            &mut self,
            action: &IterationCleanupAction,
            point: DropPoint,
            frame: &mut Frame,
            owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
            flow: bool,
            span: Span,
        ) -> Result<(), LoweringError> {
            let source_drop = owner
                .map(|(proof, _, _)| proof.at_checked(point, self.owned))
                .transpose()?
                .flatten();
            match action {
                IterationCleanupAction::EndBinding { statement, symbol }
                    if *statement == frame.statement =>
                {
                    let index = frame
                        .components
                        .iter()
                        .position(|(binding, _)| binding == symbol)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if index == 0 && frame.components[1].1.is_some() {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    let loan = frame.components[index]
                        .1
                        .take()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if self.borrow_bindings.remove(symbol) != Some(loan) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
                IterationCleanupAction::FinishProvider(statement)
                    if *statement == frame.statement =>
                {
                    if frame.finished
                        || frame.source.is_none()
                        || frame.components.iter().any(|(_, loan)| loan.is_some())
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    frame.finished = true;
                }
                IterationCleanupAction::EndSource(statement) if *statement == frame.statement => {
                    if !frame.finished {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    let loan = frame
                        .source
                        .take()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
                IterationCleanupAction::Drop(fact) if source_drop == Some(*fact) => {
                    let (_, source, value) =
                        owner.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if frame.source.is_some()
                        || !frame.finished
                        || source.current_owner(self) != Some(value)
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    source.take_owner(self, value, span)?;
                    self.append(
                        Operation::Drop { owner: value },
                        Vec::new(),
                        fact.value_origin(),
                    )?;
                }
                IterationCleanupAction::Drop(fact)
                    if !flow
                        && owner.is_some_and(|(_, source, _)| {
                            matches!(source, SourceIdentity::Temporary(_))
                        }) =>
                {
                    self.emit_temporary_map_local_drop(*fact, point, span)?;
                }
                IterationCleanupAction::Drop(fact) => {
                    if flow {
                        self.emit_flow_map_local_drop(*fact, span)?;
                    } else {
                        self.emit_iteration_cleanup_action(action, span)?;
                    }
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
            Ok(())
        }
    }

    pub(in crate::ssa) fn roles(
        parameters: &[EntityId],
        source: usize,
        cursor: usize,
        span: Span,
    ) -> Result<(LoanId, ValueId), LoweringError> {
        match (parameters.get(source), parameters.get(cursor)) {
            (Some(EntityId::Loan(source)), Some(EntityId::Value(cursor))) => Ok((*source, *cursor)),
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }
}
