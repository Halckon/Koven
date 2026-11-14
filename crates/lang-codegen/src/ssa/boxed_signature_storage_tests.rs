//! Signature-only storage mapping without construction demand.
use super::{analyze, lower_scalar_file, render_program, render_verified_program};
use crate::test_support::diagnostics::{assert_diagnostic, assert_text_eq};

#[test]
fn boxed_enum_signature_only_storage_accepts_box_or_enum_first() {
    for (first, second) in [("Box<Expr>", "Expr"), ("Expr", "Box<Expr>")] {
        let source = format!(
            "enum class Expr {{ Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }}\n\
             fun first(value: {first}): Unit {{}}\n\
             fun second(value: {second}): Unit {{}}"
        );
        let analysis = analyze(&source);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        // Without any construction demand, the first parameter is the mapper's actual entry root.
        assert!(analysis.typed.constructions().is_empty());
        let lower = || {
            lower_scalar_file(
                &analysis.sources,
                &analysis.parsed,
                &analysis.names,
                &analysis.typed,
                &analysis.owned,
            )
            .expect("either recursive storage demand root must lower")
        };
        let program = lower();
        let rendered = render_program(&program);
        assert_diagnostic!(rendered.contains("heap_owner"), "{rendered}");
        assert_diagnostic!(rendered.contains("tagged_union"), "{rendered}");
        assert_text_eq!(rendered, render_program(&lower()));
        render_verified_program(&program).expect("signature-only recursive Box/enum LLVM verifies");
    }
}

#[test]
fn boxed_enum_mapper_preserves_nonrecursive_value_wrappers() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    for wrapper in [
        "Box<Wrapped>",
        "Box<Wrapped>?",
        "Rc<Box<Wrapped>>",
        "Array<Box<Wrapped>>",
    ] {
        let text = format!(
            "value class Wrapped(val value: Int)\nfun inspect(input: {wrapper}): Unit {{}}"
        );
        let analysis = UnitSource::analyze("lowering.ko", &text)
            .unwrap_or_else(|error| panic!("{wrapper}: {error:?}"));
        analysis.assert_frontend_clean();
        assert!(analysis.ownership().deferred().is_empty());
        assert!(analysis.types().constructions().is_empty());
        let [source] = analysis.names().names().index().source_units() else {
            panic!("one original memory document");
        };
        assert_eq!(source.source_id(), analysis.parsed().source_id());
        assert_eq!(
            source.key(),
            &lang_frontend::name_resolution::SourceUnitKey::MemoryDocument
        );
        assert_eq!(
            analysis.sources().source_name(source.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(
            analysis.sources().source_text(source.source_id()),
            Ok(text.as_str())
        );
        let declaration = analysis.declaration("inspect", SymbolKind::Function);
        let span = analysis.declaration_span(declaration);
        let original_declaration = format!("fun inspect(input: {wrapper}): Unit {{}}");
        assert_eq!(
            analysis.sources().slice(span),
            Ok(original_declaration.as_str())
        );
        // The original parameter is the storage root; no construction or native wrapper is added.
        let (program, root) = analysis
            .lower_entry(declaration)
            .unwrap_or_else(|error| panic!("{wrapper}: {error:?}"));
        let function = program
            .module(root.module())
            .and_then(|module| module.function(root))
            .expect("inspect belongs to its own lowered Program");
        assert_eq!(function.origin, crate::ssa::model::Origin::Source(span));
        assert!(!function.blocks.is_empty(), "inspect has its original body");
        assert_eq!(function.blocks[0].parameters.len(), 1);
        crate::ssa::verify::verify_program(&program)
            .expect("existing wrapped Box payload must verify through SSA");
        render_verified_program(&program)
            .expect("existing wrapped Box payload must stay fully defined");
    }
}

#[test]
fn boxed_enum_signature_only_unit_roots_accept_box_or_enum_first() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use crate::ssa::model::{EntityType, LoanKind, Origin};
    use lang_frontend::name_resolution::SymbolKind;

    for (first, second) in [("Box<Expr>", "Expr"), ("Expr", "Box<Expr>")] {
        let source = format!(
            "enum class Expr {{ Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }}\n\
             fun first(value: {first}): Unit {{}}\n\
             fun second(value: {second}): Unit {{}}"
        );
        // Independent Unit roots supplement, not replace, the shared-mapper legacy observation.
        let unit = UnitSource::analyze("lowering.ko", &source)
            .unwrap_or_else(|error| panic!("{first}/{second}: strict Unit admission: {error:?}"));
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        assert!(unit.types().constructions().is_empty());
        let [document] = unit.names().names().index().source_units() else {
            panic!("one original memory document");
        };
        assert_eq!(document.source_id(), unit.parsed().source_id());
        assert_eq!(
            document.key(),
            &lang_frontend::name_resolution::SourceUnitKey::MemoryDocument
        );
        assert_eq!(
            unit.sources().source_name(document.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(
            unit.sources().source_text(document.source_id()),
            Ok(source.as_str())
        );
        for (name, parameter_type) in [("first", first), ("second", second)] {
            let declaration = unit.declaration(name, SymbolKind::Function);
            let span = unit.declaration_span(declaration);
            let original_declaration = format!("fun {name}(value: {parameter_type}): Unit {{}}");
            assert_eq!(
                unit.sources().slice(span),
                Ok(original_declaration.as_str())
            );
            let lower_root = || {
                let (program, root) = unit.lower_entry(declaration).unwrap_or_else(|error| {
                    panic!("{first}/{second}: {name}: Unit lowering: {error:?}")
                });
                let function = program
                    .module(root.module())
                    .and_then(|module| module.function(root))
                    .expect("original root belongs to its own lowered Program");
                assert_eq!(function.origin, Origin::Source(span));
                assert!(!function.blocks.is_empty(), "{name} has its original body");
                assert_eq!(function.blocks[0].parameters.len(), 1);
                assert!(
                    matches!(
                        function
                            .entity(function.blocks[0].parameters[0])
                            .map(|parameter| parameter.ty),
                        Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            ..
                        })
                    ),
                    "{first}/{second}: {name} retains its original Borrow parameter"
                );
                crate::ssa::verify::verify_program(&program).unwrap_or_else(|error| {
                    panic!("{first}/{second}: {name}: full Unit SSA verification: {error:?}")
                });
                let rendered = render_program(&program);
                assert_diagnostic!(rendered.contains("heap_owner"), "{name}: {rendered}");
                assert_diagnostic!(rendered.contains("tagged_union"), "{name}: {rendered}");
                render_verified_program(&program).unwrap_or_else(|error| {
                    panic!("{first}/{second}: {name}: full Unit LLVM verification: {error:?}")
                });
                rendered
            };
            assert_eq!(
                lower_root(),
                lower_root(),
                "{first}/{second}: {name}: repeated Unit root lowering is deterministic"
            );
        }
    }
}

pub(crate) mod owner_consumer_test_support {
    #[cfg(test)]
    pub(in crate::ssa) mod checkpoint {
        //! Exact call sites: before MapSize/NullableTake, before CallReturn cleanup, or If retirement.
        use crate::ssa::lower_frontend::orchestrate::nullable_map_guard_support::*;
        use crate::ssa::lower_frontend::orchestrate::unwrap_modified;
        use crate::ssa::lower_frontend::*;
        use crate::ssa::lowering_support::callable_instances::SourceToken;
        use crate::ssa::model::{Definition, InstructionId};
        use lang_frontend::ownership_checking::DropTarget;
        use std::collections::BTreeSet;

        #[derive(Clone, Copy)]
        pub(in crate::ssa) struct Point {
            pub(in crate::ssa) expression: ExpressionId,
            pub(in crate::ssa) span: Span,
            pub(in crate::ssa) root: SymbolId,
            pub(in crate::ssa) storage: TypeId,
            pub(in crate::ssa) narrowed: TypeId,
            pub(in crate::ssa) nullable: SsaTypeId,
            pub(in crate::ssa) control_span: Span,
            pub(in crate::ssa) header: BlockId,
            pub(in crate::ssa) header_owner: ValueId,
            pub(in crate::ssa) null_block: BlockId,
            pub(in crate::ssa) body: BlockId,
            pub(in crate::ssa) proof: NonNullProof,
            pub(in crate::ssa) instruction: InstructionId,
        }

        #[derive(Clone)]
        pub(in crate::ssa) struct Checkpoint {
            pub(in crate::ssa) function: Function,
            pub(in crate::ssa) point: Point,
            pub(in crate::ssa) bindings: BTreeMap<SymbolId, LoweredValue>,
            pub(in crate::ssa) type_ids: BTreeMap<TypeId, SsaTypeId>,
            pub(in crate::ssa) nullable_maps: BTreeMap<SsaTypeId, SsaTypeId>,
            pub(in crate::ssa) function_ids: BTreeMap<FunctionInstanceKey, FunctionId>,
            pub(in crate::ssa) source: Option<SourceToken>,
        }

        impl Fixture {
            pub(in crate::ssa) fn read_checkpoint(&self, cleanup: bool) -> Checkpoint {
                let [descriptor] = self.typed.map_sizes() else {
                    panic!("one frozen B read")
                };
                self.checkpoint(
                    descriptor.expression(),
                    descriptor.receiver(),
                    false,
                    cleanup,
                )
            }

            pub(in crate::ssa) fn consume_checkpoint(&self) -> Checkpoint {
                let [descriptor] = self.typed.map_consumes() else {
                    panic!("one frozen C consume")
                };
                self.checkpoint(descriptor.expression(), descriptor.receiver(), true, false)
            }

            fn checkpoint(
                &self,
                expression: ExpressionId,
                receiver: ExpressionId,
                consume: bool,
                cleanup: bool,
            ) -> Checkpoint {
                let span = self.span(expression);
                let use_fact = self.typed.non_null_use(receiver).unwrap();
                let root = use_fact.symbol();
                assert_eq!(root, self.reference(receiver));
                assert!(self.owned.formed_owned_locals().contains(&root));
                let storage = use_fact.declared_type();
                let narrowed = use_fact.narrowed_type();
                assert_eq!(self.typed.symbol_type(root), Some(storage));
                let nullable = self.type_ids[&storage];
                let inner = self.type_ids[&narrowed];
                let controls = self
                    .parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, node)| {
                        let Expression::If {
                            condition,
                            then_branch,
                            else_branch,
                            ..
                        } = node.payload()
                        else {
                            return None;
                        };
                        let comparison = self.typed.null_comparison(self.ungroup(*condition))?;
                        if comparison.symbol() != root {
                            return None;
                        }
                        let body = if comparison.non_null_when_true() {
                            Some(*then_branch)
                        } else {
                            *else_branch
                        }?;
                        let body_span = self.parsed.ast().statements().get(body).unwrap().span();
                        contains(body_span, span).then_some((id, node.span(), body, comparison))
                    })
                    .collect::<Vec<_>>();
                let [(control, control_span, statement, comparison)] = controls.as_slice() else {
                    panic!("one exact source If introducing this read/consume")
                };
                assert_eq!(comparison.nullable_type(), storage);
                let functions = self.product.program.modules[0]
                    .functions
                    .iter()
                    .filter(|function| contains(function.origin.span(), span))
                    .collect::<Vec<_>>();
                let [baseline] = functions.as_slice() else {
                    panic!("one actual source callable")
                };
                let baseline = *baseline;
                let births = baseline
                    .blocks
                    .iter()
                    .filter(|block| {
                        block.terminator.as_ref().is_some_and(|term| {
                            term.origin == Origin::Source(*control_span)
                                && matches!(term.kind, TerminatorKind::NullableBranch { .. })
                        })
                    })
                    .collect::<Vec<_>>();
                let [header] = births.as_slice() else {
                    panic!("one exact If branch birth")
                };
                assert_eq!(
                    header.id,
                    baseline.entry_block().unwrap(),
                    "no preceding CFG in bounded callable"
                );
                let TerminatorKind::NullableBranch {
                    owner,
                    view,
                    when_null,
                    when_non_null,
                } = &header.terminator.as_ref().unwrap().kind
                else {
                    unreachable!()
                };
                let before_if = self.prelude_bindings(baseline, *control);
                assert_eq!(
                    before_if,
                    BTreeMap::from([(root, LoweredValue::Value(*owner))])
                );
                assert_eq!(when_null.arguments, vec![EntityId::Value(*owner)]);
                assert_eq!(when_non_null.arguments, when_null.arguments);
                let body = baseline.block(when_non_null.target).unwrap();
                let [EntityId::Value(body_owner), EntityId::Loan(body_view)] =
                    body.parameters.as_slice()
                else {
                    panic!("exact owner column and branch-born view")
                };
                assert_eq!(*body_view, *view);
                assert_eq!(
                    baseline
                        .entity(EntityId::Value(*body_owner))
                        .unwrap()
                        .definition,
                    Definition::BlockParameter {
                        block: body.id,
                        index: 0
                    }
                );
                assert_eq!(
                    baseline.entity(EntityId::Loan(*view)).unwrap().definition,
                    Definition::BlockParameter {
                        block: body.id,
                        index: 1
                    }
                );
                assert_eq!(
                    baseline.entity(EntityId::Loan(*view)).unwrap().ty,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: inner
                    }
                );
                let null = baseline.block(when_null.target).unwrap();
                assert_eq!(null.parameters.len(), 1);
                assert_eq!(
                    baseline.entity(null.parameters[0]).unwrap().ty,
                    EntityType::Value(nullable)
                );
                assert_eq!(
                    self.product.program.modules[0].nullable_maps.get(&nullable),
                    Some(&inner)
                );
                let instruction = if consume {
                    one_instruction(baseline, self.span(receiver), |operation| {
                        matches!(operation, Operation::NullableTake { owner, proof }
                            if *owner == *body_owner && *proof == *view)
                    })
                } else {
                    one_instruction(
                        baseline,
                        span,
                        |operation| matches!(operation, Operation::MapSize { owner } if *owner == EntityId::Loan(*view)),
                    )
                };
                assert_eq!(instruction.block, body.id);
                let target_index = body
                    .instructions
                    .iter()
                    .position(|&id| id == instruction.id)
                    .unwrap();
                assert_eq!(
                    target_index, 0,
                    "first body use has no pending evaluation prefix"
                );
                let mut bindings = BTreeMap::from([(root, LoweredValue::Value(*body_owner))]);
                let stop = if cleanup {
                    let end_id = body.instructions[target_index + 1];
                    let end = baseline.instruction(end_id).unwrap();
                    assert_eq!(end.operation, Operation::BorrowEnd { loan: *view });
                    if self.id == "B3" {
                        assert_eq!(end.origin, Origin::Source(*control_span));
                        // Single lowering keeps the actual Copyable local until merge_exits.
                        let Statement::ControlBody { elements } = self
                            .parsed
                            .ast()
                            .statements()
                            .get(*statement)
                            .unwrap()
                            .payload()
                        else {
                            panic!("B body")
                        };
                        let [local] = elements.as_slice() else {
                            panic!("one real count declaration")
                        };
                        let Statement::LocalVariable { declaration } = self
                            .parsed
                            .ast()
                            .statements()
                            .get(*local)
                            .unwrap()
                            .payload()
                        else {
                            panic!("real count local")
                        };
                        let (count, initializer) = self.declaration(*declaration);
                        assert_eq!(initializer, expression);
                        assert_eq!(
                            self.typed
                                .copyability(self.typed.symbol_type(count).unwrap()),
                            Some(Copyability::Copyable)
                        );
                        bindings.insert(count, LoweredValue::Value(result(instruction)));
                    } else {
                        let facts = self
                            .owned
                            .drops()
                            .iter()
                            .filter(|fact| {
                                fact.point() == DropPoint::CallReturn(expression)
                                    && fact.target() == DropTarget::Named(root)
                            })
                            .collect::<Vec<_>>();
                        let [fact] = facts.as_slice() else {
                            panic!("one real read CallReturn NamedDrop")
                        };
                        assert_eq!(end.origin, Origin::Source(fact.value_origin()));
                        let drop = baseline
                            .instruction(body.instructions[target_index + 2])
                            .unwrap();
                        assert_eq!(drop.operation, Operation::Drop { owner: *body_owner });
                        assert_eq!(drop.origin, Origin::Source(fact.value_origin()));
                    }
                    target_index + 1
                } else {
                    target_index
                };
                let point = Point {
                    expression,
                    span,
                    root,
                    storage,
                    narrowed,
                    nullable,
                    control_span: *control_span,
                    header: header.id,
                    header_owner: *owner,
                    null_block: null.id,
                    body: body.id,
                    proof: NonNullProof::introduced(
                        *view,
                        *body_owner,
                        inner,
                        non_null_proof::ProofScope::If(*control),
                    ),
                    instruction: instruction.id,
                };
                let mut function = baseline.clone();
                // Follow lower_nullable_if's actual then/else order. Arena ordinals are not a clock.
                // The header is already emitted; a false-polarity null arm has already lowered.
                for block in &mut function.blocks {
                    if block.id == header.id {
                        continue;
                    }
                    if block.id == body.id {
                        block.instructions.truncate(stop);
                    } else if block.id != null.id || comparison.non_null_when_true() {
                        block.instructions.clear();
                    }
                    // All merge/return terminators were installed after the selected checkpoint.
                    block.terminator = None;
                }
                assert_prefix_state(&function, point);
                Checkpoint {
                    function,
                    point,
                    bindings,
                    type_ids: self.type_ids.clone(),
                    nullable_maps: self.product.program.modules[0].nullable_maps.clone(),
                    function_ids: self.product.function_ids.clone(),
                    source: Some(self.plans[&baseline.id].source),
                }
            }

            pub(in crate::ssa) fn reference(&self, expression: ExpressionId) -> SymbolId {
                let expression = self.ungroup(expression);
                assert!(matches!(
                    self.parsed
                        .ast()
                        .expressions()
                        .get(expression)
                        .unwrap()
                        .payload(),
                    Expression::Name
                ));
                self.references[&span_key(self.span(expression))]
            }

            pub(in crate::ssa) fn ungroup(&self, mut expression: ExpressionId) -> ExpressionId {
                while let Expression::Group { expression: inner } = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .unwrap()
                    .payload()
                {
                    expression = *inner;
                }
                expression
            }

            pub(in crate::ssa) fn declaration(
                &self,
                declaration: ItemId,
            ) -> (SymbolId, ExpressionId) {
                let (item, _) = unwrap_modified(&self.parsed, declaration).unwrap();
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } = item
                else {
                    panic!("ordinary named local")
                };
                let symbols = self
                    .names
                    .symbols()
                    .iter()
                    .filter(|symbol| symbol.span() == name && symbol.kind() == SymbolKind::Variable)
                    .collect::<Vec<_>>();
                assert_eq!(symbols.len(), 1);
                (symbols[0].id(), initializer)
            }

            pub(in crate::ssa) fn prelude_bindings(
                &self,
                function: &Function,
                control: ExpressionId,
            ) -> BTreeMap<SymbolId, LoweredValue> {
                let plan = &self.plans[&function.id];
                let Statement::Block { elements } = self
                    .parsed
                    .ast()
                    .statements()
                    .get(plan.body)
                    .unwrap()
                    .payload()
                else {
                    panic!("ordinary function block")
                };
                let mut bindings = plan.entry_bindings.clone();
                let mut found = false;
                for &statement in elements {
                    let node = self.parsed.ast().statements().get(statement).unwrap();
                    match node.payload() {
                        Statement::Expression { expression } if *expression == control => {
                            found = true;
                            break;
                        }
                        Statement::LocalVariable { declaration } => {
                            let (symbol, initializer) = self.declaration(*declaration);
                            let declared = self.typed.symbol_type(symbol).unwrap();
                            assert_eq!(
                                self.typed.copyability(declared),
                                Some(Copyability::MoveOnly)
                            );
                            assert!(self.owned.formed_owned_locals().contains(&symbol));
                            let mut owner = if self.typed.map_construction(initializer).is_some() {
                                let instruction = one_instruction(
                                    function,
                                    self.span(initializer),
                                    |operation| matches!(operation, Operation::MapConstruct { .. }),
                                );
                                result(instruction)
                            } else {
                                // C2 source <- seed and C3 source <- input use the actual old binding.
                                let LoweredValue::Value(value) =
                                    bindings[&self.reference(initializer)]
                                else {
                                    panic!("existing owned initializer")
                                };
                                value
                            };
                            // binding.rs retires every old alias before optional NullableWrap.
                            bindings.retain(|_, binding| *binding != LoweredValue::Value(owner));
                            if self.typed.expression_type(initializer) != Some(declared) {
                                let instruction = one_instruction(
                                    function,
                                    node.span(),
                                    |operation| {
                                        matches!(operation, Operation::NullableWrap { owner: actual, nullable }
                                        if *actual == owner && *nullable == self.type_ids[&declared])
                                    },
                                );
                                owner = result(instruction);
                            }
                            assert_eq!(
                                function.entity(EntityId::Value(owner)).unwrap().ty,
                                EntityType::Value(self.type_ids[&declared])
                            );
                            bindings.insert(symbol, LoweredValue::Value(owner));
                        }
                        Statement::Expression { expression } => {
                            // The only pre-If calls are the two frozen C2 MapPut statements.
                            // Both rebind seed to the actual returned owner, not the construction ID.
                            let descriptor = self
                                .typed
                                .map_put(*expression)
                                .expect("bounded prelude MapPut");
                            let symbol = self.reference(descriptor.receiver());
                            let LoweredValue::Value(owner) = bindings[&symbol] else {
                                panic!("put owner")
                            };
                            let instruction = one_instruction(
                                function,
                                self.span(*expression),
                                |operation| matches!(operation, Operation::MapPut { owner: actual, .. } if *actual == owner),
                            );
                            bindings.insert(symbol, LoweredValue::Value(result(instruction)));
                        }
                        _ => panic!("checkpoint cannot reconstruct an extended prelude"),
                    }
                    // These frozen prelude points cannot discard one of the retained named owners.
                    for fact in self
                        .owned
                        .drops()
                        .iter()
                        .filter(|fact| fact.point() == DropPoint::AfterStatement(statement))
                    {
                        if let DropTarget::Named(symbol) = fact.target() {
                            bindings.remove(&symbol);
                        }
                    }
                }
                assert!(
                    found,
                    "target If is a top-level statement, not a pending call operand"
                );
                self.assert_closed_header(function, &plan.entry_bindings, &bindings);
                bindings
            }

            fn assert_closed_header(
                &self,
                function: &Function,
                entry: &BTreeMap<SymbolId, LoweredValue>,
                bindings: &BTreeMap<SymbolId, LoweredValue>,
            ) {
                let owners = |bindings: &BTreeMap<SymbolId, LoweredValue>| {
                    bindings
                        .values()
                        .map(|binding| {
                            let LoweredValue::Value(owner) = binding else {
                                panic!("the frozen prelude has only owned bindings")
                            };
                            *owner
                        })
                        .collect::<BTreeSet<_>>()
                };
                let mut live = owners(entry);
                let mut places = BTreeMap::new();
                let mut loans = BTreeMap::new();
                let header = function.block(function.entry_block().unwrap()).unwrap();
                let string =
                    self.type_ids[&self.typed.types().builtin(BuiltinType::String).unwrap()];
                // This is the actual header instruction order, never arena ordinal order.
                for &id in &header.instructions {
                    let instruction = function.instruction(id).unwrap();
                    assert_eq!(instruction.block, header.id);
                    match instruction.operation {
                        Operation::MapConstruct { .. } | Operation::StringLiteral { .. } => {
                            assert!(live.insert(result(instruction)), "one real owner birth");
                        }
                        Operation::RootPlace { owner } => {
                            assert!(live.contains(&owner), "place follows its live owner birth");
                            let [EntityId::Place(place)] = instruction.results.as_slice() else {
                                panic!("one real clone receiver place")
                            };
                            assert!(places.insert(*place, owner).is_none());
                        }
                        Operation::BorrowBegin {
                            place,
                            kind: LoanKind::Shared,
                        } => {
                            let owner = places[&place];
                            assert!(live.contains(&owner));
                            assert_eq!(
                                function.entity(EntityId::Value(owner)).unwrap().ty,
                                EntityType::Value(string)
                            );
                            let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
                                panic!("one actual String clone loan")
                            };
                            assert_eq!(
                                function.entity(EntityId::Loan(*loan)).unwrap().ty,
                                EntityType::Loan {
                                    kind: LoanKind::Shared,
                                    target: string
                                }
                            );
                            assert!(
                                loans.insert(*loan, owner).is_none(),
                                "loan birth follows its place"
                            );
                        }
                        Operation::StringClone { source } => {
                            let owner = loans[&source];
                            assert!(
                                live.contains(&owner),
                                "clone reads a presently live loan target"
                            );
                            assert_eq!(
                                function
                                    .entity(EntityId::Value(result(instruction)))
                                    .unwrap()
                                    .ty,
                                EntityType::Value(string)
                            );
                            assert!(live.insert(result(instruction)));
                        }
                        Operation::BorrowEnd { loan } => {
                            assert!(
                                loans.remove(&loan).is_some(),
                                "End follows birth exactly once"
                            );
                            assert!(instruction.results.is_empty());
                        }
                        Operation::Drop { owner } => {
                            retire_owner(&mut live, &loans, owner);
                            assert!(instruction.results.is_empty());
                        }
                        Operation::MapPut { owner, key, value } => {
                            assert_eq!(self.id, "C2", "only the frozen String prelude has puts");
                            for string_owner in [key, value] {
                                assert_eq!(
                                    function.entity(EntityId::Value(string_owner)).unwrap().ty,
                                    EntityType::Value(string)
                                );
                            }
                            for moved in [owner, key, value] {
                                retire_owner(&mut live, &loans, moved);
                            }
                            assert!(
                                live.insert(result(instruction)),
                                "put publishes its real replacement owner"
                            );
                        }
                        Operation::NullableWrap { owner, .. } => {
                            retire_owner(&mut live, &loans, owner);
                            assert!(live.insert(result(instruction)));
                        }
                        _ => panic!(
                            "unaccounted operation in fixed six-source header: {:?}",
                            instruction.operation
                        ),
                    }
                }
                assert!(
                    loans.is_empty(),
                    "no created clone loan survives into the If"
                );
                assert_eq!(
                    live,
                    owners(bindings),
                    "every literal/clone temporary was delivered or dropped"
                );
            }
        }

        fn one_instruction(
            function: &Function,
            span: Span,
            predicate: impl Fn(&Operation) -> bool,
        ) -> &crate::ssa::model::Instruction {
            let matches = function
                .instructions
                .iter()
                .filter(|instruction| {
                    instruction.origin == Origin::Source(span) && predicate(&instruction.operation)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "one instruction with exact source and operation"
            );
            matches[0]
        }

        fn result(instruction: &crate::ssa::model::Instruction) -> ValueId {
            let [EntityId::Value(value)] = instruction.results.as_slice() else {
                panic!("one actual result")
            };
            *value
        }

        fn retire_owner(
            live: &mut BTreeSet<ValueId>,
            loans: &BTreeMap<LoanId, ValueId>,
            owner: ValueId,
        ) {
            assert!(
                loans.values().all(|&target| target != owner),
                "owner delivery follows all loan ends"
            );
            assert!(
                live.remove(&owner),
                "consume exactly one already-produced owner"
            );
        }

        fn contains(outer: Span, inner: Span) -> bool {
            outer.source_id() == inner.source_id()
                && outer.start() <= inner.start()
                && inner.end() <= outer.end()
        }

        fn assert_prefix_state(function: &Function, point: Point) {
            for block in &function.blocks {
                if block.id == point.header {
                    continue;
                }
                assert!(block.terminator.is_none());
                if block.id == point.body {
                    assert!(block.instructions.len() <= 1);
                    for &id in &block.instructions {
                        assert_eq!(
                            function.instruction(id).unwrap().operation,
                            Operation::MapSize {
                                owner: EntityId::Loan(point.proof.loan)
                            }
                        );
                    }
                } else if block.id == point.null_block {
                    let [EntityId::Value(owner)] = block.parameters.as_slice() else {
                        panic!("one actual null-arm owner")
                    };
                    assert!(block.instructions.len() <= 1);
                    for &id in &block.instructions {
                        assert_eq!(
                            function.instruction(id).unwrap().operation,
                            Operation::Drop { owner: *owner }
                        );
                    }
                } else {
                    assert!(
                        block.instructions.is_empty(),
                        "all future block instruction references removed"
                    );
                }
            }
        }
    }
}
