//! Ordinary source observations isolating source and returned-owner boundaries.
use crate::ssa::map_unit_consumer_test_support::{assert_outer_resource, assert_return_controls};
use crate::test_support::diagnostics::diagnostic;

// The pre-existing all-success whole retains its original Unit oracle.
fn observe_owned_source_unit(name: &str, text: &str) {
    use crate::{
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::model::{Operation, Origin, TerminatorKind},
    };
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration("work", SymbolKind::Function);
    let (program, entry) = analysis
        .lower_entry(declaration)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    assert_eq!(
        program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap()
            .origin,
        Origin::Source(analysis.declaration_span(declaration))
    );
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    for function in program.modules.iter().flat_map(|module| &module.functions) {
        for block in &function.blocks {
            if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind {
                for value in values {
                    assert!(
                        !function.instructions.iter().any(|instruction| {
                            matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
                        }),
                        "returned owner is never also dropped"
                    );
                }
            }
        }
    }
    diagnostic!("CONTROL {name}: VERIFIED_LLVM");
}

#[test]
fn ordinary_source_controls_owned_source_return() {
    for own in [false, true] {
        for live in [false, true] {
            for map in [false, true] {
                let parameter = if own { "own keys: Array<Int>" } else { "" };
                let source = if own {
                    ""
                } else {
                    "val keys = arrayOf(1, 2, 3);"
                };
                let before = if map {
                    "var cache = mutableMapOf<Int, String>();"
                } else {
                    ""
                };
                let update = if map {
                    "cache.put(key, \"value\".clone());"
                } else {
                    ""
                };
                let tail = if live { "keys.size;" } else { "" };
                let map_tail = if map { "cache.contains(1);" } else { "" };
                let text = format!(
                    "fun work({parameter}): Unit {{ {source} {before} for (key in keys) {{ {update} if (key == 2) {{ return }}; println(\"tail\") }}; {tail} {map_tail} }}"
                );
                observe_owned_source_unit(
                    &format!("owned-source-own={own}-live={live}-map={map}"),
                    &text,
                );
            }
        }
    }
}

#[test]
fn ordinary_source_controls_owned_return_operand() {
    for map in [false, true] {
        for grouped in [false, true] {
            for kind in ["string", "value", "removed", "map"] {
                if (kind == "removed" || kind == "map") && !map {
                    continue;
                }
                let (ty, binding, operand, fallback) = match kind {
                    "string" => (
                        "String",
                        "val result = \"result\".clone();",
                        "result",
                        "\"fallback\".clone()",
                    ),
                    "value" => (
                        "Text",
                        "val result = Text(\"result\".clone());",
                        "result",
                        "Text(\"fallback\".clone())",
                    ),
                    "removed" => ("String?", "", "cache.remove(1)", "null"),
                    "map" => ("MutableMap<Int, String>", "", "cache", "cache"),
                    _ => unreachable!(),
                };
                let before = if map {
                    "var cache = mutableMapOf<Int, String>();"
                } else {
                    ""
                };
                let update = if map {
                    "cache.put(key, \"value\".clone());"
                } else {
                    ""
                };
                let operand = if grouped {
                    format!("({operand})")
                } else {
                    operand.to_owned()
                };
                let text = format!(
                    "value class Text(val word: String)\nfun work(): {ty} {{ {before} for (key in arrayOf(1, 2, 3)) {{ {update} {binding} if (key == 2) {{ return {operand} }}; println(\"tail\") }}; return {fallback} }}"
                );
                assert_return_controls(
                    &format!("operand-{kind}-map={map}-grouped={grouped}"),
                    &text,
                    &["work"],
                );
            }
        }
    }
}

#[test]
fn ordinary_source_controls_outer_resource_and_updating_operand() {
    let outer =
        include_str!("../../tests/fixtures/map-conditional-return-0313/outer-resource-boundary.ko");
    assert_outer_resource("outer-resource-map", outer);
    let without_map = outer
        .replace("    var cache = mutableMapOf<Int, String>()\n", "")
        .replace("        cache.put(1, \"head\".clone())\n", "")
        .replace("        cache.put(9, \"tail\".clone())\n", "")
        .replace(
            "    if (cache.contains(9)) { println(\"tail kept\") }\n",
            "",
        );
    assert_outer_resource("outer-resource-no-map", &without_map);
    assert_return_controls(
        "updating-return-operand",
        include_str!(
            "../../tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko"
        ),
        &["updatingOperand"],
    );
}

#[test]
fn ordinary_source_controls_return_shapes_remain_bounded() {
    for exit in [
        "return",
        "if (key == 1) { return }; if (key == 2) { return }",
        "if (key == 1) { return }; if (key == 2) { break }",
        "if (key == 1) { return }; if (key == 2) { continue }",
    ] {
        let text = format!(
            "fun work(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) {{ cache.put(key, \"head\".clone()); {exit} }}; cache.contains(1) }}"
        );
        assert_return_controls("unsupported-return-shape", &text, &["work"]);
    }
}

// This FE-only observer keeps the original single source and ownership phase.
// Its diagnostics remain a contains-code contract, not a unique-diagnostic oracle.
fn active_source_unit_ownership(
    text: &str,
) -> lang_frontend::ownership_checking::CompilationUnitOwnership {
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::check_compilation_unit_ownership,
        source::SourceMap,
        type_checking::{
            check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
        },
    };

    let mut sources = SourceMap::new();
    let (source, parsed) =
        crate::ssa::unit_lower_test_support::parsed(&mut sources, "lowering.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    {
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        assert!(complete.is_same_analysis(&raw));
        assert!(std::ptr::eq(complete.types(), &raw));
        assert!(raw.is_compatible_with(&sources, &inputs, &names, &types));
    }
    let typed = raw.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    owned
}

#[test]
fn ordinary_source_controls_active_source_and_element_diagnostics() {
    for (text, code) in [
        (
            "fun work(own keys: Array<Int>): Array<Int> { for (key in keys) { if (key == 1) { return keys } }; return keys }",
            "L0135",
        ),
        (
            "fun work(keys: Array<String>): String { for (key in keys) { if (key == \"stop\") { return key } }; return \"fallback\".clone() }",
            "L0133",
        ),
        (
            "fun work(): Unit { var keys = mutableListOf(1, 2); for (key in keys) { if (key == 1) { keys.add(2); return } } }",
            "L0135",
        ),
    ] {
        let owned = active_source_unit_ownership(text);
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == code),
            "{:?}",
            owned.diagnostics()
        );
    }
}

pub(in crate::ssa) mod range_receiver_count_break {
    //! Exact source-backed SSA roles and CFG cleanup for the count-break regression.
    use crate::ssa::model::*;

    use lang_frontend::source::SourceMap;

    fn count_break_edges(function: &Function) -> Vec<(BlockId, &Edge)> {
        function
            .blocks
            .iter()
            .flat_map(|block| {
                let edges = match &block.terminator.as_ref().unwrap().kind {
                    TerminatorKind::Branch(edge) => vec![edge],
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => {
                        vec![when_true, when_false]
                    }
                    TerminatorKind::Return { .. } => Vec::new(),
                    _ => panic!("this original scan has only ordinary loop/if/return edges"),
                };
                edges.into_iter().map(move |edge| (block.id, edge))
            })
            .collect()
    }

    // Follow each actual incoming phi column backwards. Cycles are visited once;
    // every external leaf must be the exact expected seed, never a same-typed owner.
    // This is directed transport, not an undirected alias equivalence relation.
    fn count_break_origins(
        function: &Function,
        value: EntityId,
    ) -> std::collections::BTreeSet<EntityId> {
        let edges = count_break_edges(function);
        let mut pending = vec![value];
        let mut visited = std::collections::BTreeSet::new();
        let mut origins = std::collections::BTreeSet::new();
        let mut dependencies = std::collections::BTreeMap::new();
        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }
            match function.entity(value).unwrap().definition {
                Definition::BlockParameter { block, index } if block != function.blocks[0].id => {
                    let incoming = edges
                        .iter()
                        .filter(|(_, edge)| edge.target == block)
                        .collect::<Vec<_>>();
                    assert!(!incoming.is_empty(), "no unanchored phi column");
                    let inputs = incoming
                        .iter()
                        .map(|(_, edge)| edge.arguments[index])
                        .collect::<Vec<_>>();
                    pending.extend(inputs.iter().copied());
                    dependencies.insert(value, inputs);
                }
                Definition::InstructionResult { instruction, .. } => {
                    if let Operation::Copy { source } =
                        function.instruction(instruction).unwrap().operation
                    {
                        let source = EntityId::Value(source);
                        pending.push(source);
                        dependencies.insert(value, vec![source]);
                    } else {
                        origins.insert(value);
                    }
                }
                Definition::BlockParameter { .. } => {
                    origins.insert(value);
                }
            }
        }
        // Ground every reached transport node, not merely the queried union. A
        // good root plus a separate rootless incoming SCC must still fail closed.
        let mut grounded = origins.clone();
        loop {
            let before = grounded.len();
            for (value, inputs) in &dependencies {
                if inputs.iter().any(|input| grounded.contains(input)) {
                    grounded.insert(*value);
                }
            }
            if grounded.len() == before {
                break;
            }
        }
        assert_eq!(
            grounded, visited,
            "every incoming transport must reach a real leaf"
        );
        origins
    }

    fn count_break_instruction(
        function: &Function,
        select: impl Fn(&Instruction) -> bool,
    ) -> &Instruction {
        let found = function
            .instructions
            .iter()
            .filter(|instruction| select(instruction))
            .collect::<Vec<_>>();
        let [instruction] = found.as_slice() else {
            panic!("one exact source instruction")
        };
        instruction
    }

    pub(in crate::ssa) fn assert_count_break_structure(
        sources: &SourceMap,
        program: &Program,
        receiver_begin_span: lang_frontend::source::Span,
    ) {
        let scans = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| {
                sources
                    .slice(function.origin.span())
                    .unwrap()
                    .starts_with("fun scan(")
            })
            .collect::<Vec<_>>();
        let [function] = scans.as_slice() else {
            panic!("one original scan")
        };
        let function = *function;
        let origin_is = |origin: &Origin, text| sources.slice(origin.span()) == Ok(text);
        let seed = count_break_instruction(function, |instruction| {
            matches!(instruction.operation, Operation::ContainerConstruct { .. })
                && origin_is(&instruction.origin, "listOf(\"first\")")
        })
        .results[0];
        let is_from = |value, expected| count_break_origins(function, value) == [expected].into();
        let length = count_break_instruction(function, |instruction| {
            matches!(instruction.operation, Operation::ContainerLength { .. })
        });
        let Operation::ContainerLength {
            owner: provider_loan,
        } = length.operation
        else {
            unreachable!()
        };
        let loan_from_owner = |instruction: &Instruction, owner| {
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = instruction.operation
            else {
                return false;
            };
            let Definition::InstructionResult { instruction, .. } =
                function.entity(EntityId::Place(place)).unwrap().definition
            else {
                return false;
            };
            matches!(
                function.instruction(instruction).unwrap().operation,
                Operation::RootPlace { owner: actual } if is_from(EntityId::Value(actual), owner)
            )
        };
        let provider_begin = count_break_instruction(function, |instruction| {
            instruction.results.as_slice() == [provider_loan]
        });
        assert!(loan_from_owner(provider_begin, seed));
        let receiver_begin = count_break_instruction(function, |instruction| {
            instruction.id != provider_begin.id && loan_from_owner(instruction, seed)
        });
        // Named receiver BorrowBegin uses its checked loan's receiver-expression span,
        // not the full call span used by RootPlace and the eventual RangeCall.
        assert_eq!(receiver_begin.origin, Origin::Source(receiver_begin_span));
        let receiver = receiver_begin.results[0];
        let element_place = count_break_instruction(function, |instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
            )
        });
        let Operation::ContainerElementPlace {
            owner,
            index: element_index,
        } = element_place.operation
        else {
            unreachable!()
        };
        assert!(is_from(owner, provider_loan));
        let element = count_break_instruction(function, |instruction| {
            matches!(
                instruction.operation,
                Operation::BorrowBegin { place, kind: LoanKind::Shared }
                    if EntityId::Place(place) == element_place.results[0]
            )
        })
        .results[0];
        let branches = function
            .blocks
            .iter()
            .filter(|block| {
                origin_is(
                    &block.terminator.as_ref().unwrap().origin,
                    "if(stop){break}else{1}",
                )
            })
            .collect::<Vec<_>>();
        let [branch] = branches.as_slice() else {
            panic!("one original stop branch")
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &branch.terminator.as_ref().unwrap().kind
        else {
            panic!("stop controls the real branch")
        };
        assert_eq!(function.blocks[0].parameters.len(), 1);
        let module = &program.modules[function.id.module().index()];
        let condition_read = count_break_instruction(function, |instruction| {
            instruction.results.as_slice() == [EntityId::Value(*condition)]
        });
        let Operation::Read {
            source: PlaceAccess::Loan(stop),
        } = condition_read.operation
        else {
            panic!("the condition reads the original borrowed stop parameter")
        };
        assert_eq!(condition_read.block, branch.id);
        assert!(origin_is(&condition_read.origin, "stop"));
        let EntityType::Value(boolean) = function.entity(EntityId::Value(*condition)).unwrap().ty
        else {
            panic!("the condition is a Boolean value, not its borrowed storage")
        };
        assert_eq!(module.type_kind(boolean), Some(&SsaTypeKind::Boolean));
        let stop_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: boolean,
        };
        assert_eq!(function.entity(EntityId::Loan(stop)).unwrap().ty, stop_type);
        assert_eq!(
            function
                .entity(function.blocks[0].parameters[0])
                .unwrap()
                .ty,
            stop_type
        );
        assert!(is_from(
            EntityId::Loan(stop),
            function.blocks[0].parameters[0]
        ));
        assert_eq!(
            receiver_begin.block, branch.id,
            "receiver is evaluated before count"
        );
        let break_block = function.block(when_true.target).unwrap();
        let call_block = function.block(when_false.target).unwrap();
        let range_call = count_break_instruction(function, |instruction| {
            matches!(instruction.operation, Operation::RangeCall { .. })
        });
        assert_eq!(
            range_call.block, call_block.id,
            "only stop=false invokes prefix"
        );
        assert!(origin_is(
            &range_call.origin,
            "source.prefix(if(stop){break}else{1})"
        ));
        let Operation::RangeCall {
            callee,
            arguments,
            source,
        } = &range_call.operation
        else {
            unreachable!()
        };
        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[0], EntityId::Loan(*source));
        assert!(is_from(EntityId::Loan(*source), receiver));
        let count_loan = arguments[1];
        let count = count_break_instruction(function, |instruction| {
            matches!(
                instruction.operation,
                Operation::Constant(ScalarConstant::Integer(1))
            ) && origin_is(&instruction.origin, "1")
        });
        let count_begin = count_break_instruction(function, |instruction| {
            instruction.results.as_slice() == [count_loan]
        });
        assert_eq!(count.block, call_block.id);
        assert_eq!(count_begin.block, call_block.id);
        assert!(loan_from_owner(count_begin, count.results[0]));
        let EntityType::Value(integer) = function.entity(count.results[0]).unwrap().ty else {
            panic!("the original literal is an Int value")
        };
        assert_eq!(
            module.type_kind(integer),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
        let count_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: integer,
        };
        assert_eq!(function.entity(count_loan).unwrap().ty, count_type);
        let provider = program.modules[callee.module().index()]
            .function(*callee)
            .unwrap();
        assert!(origin_is(
            &provider.origin,
            "borrow fun <T> List<T>.prefix(count:Int):View<T> from this=rangeView(this,0,count)"
        ));
        assert_eq!(provider.carrier_return, Some(0));
        assert!(provider.receiver.is_none());
        assert_eq!(provider.blocks[0].parameters.len(), 2);
        assert_eq!(
            provider
                .entity(provider.blocks[0].parameters[1])
                .unwrap()
                .ty,
            count_type
        );
        let read = count_break_instruction(function, |instruction| {
            matches!(instruction.operation, Operation::DirectCall { .. })
        });
        assert_eq!(read.block, call_block.id, "only stop=false invokes read");
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &read.operation
        else {
            panic!("ordinary read consumer")
        };
        let consumer = program.modules[callee.module().index()]
            .function(*callee)
            .unwrap();
        assert!(origin_is(
            &consumer.origin,
            "fun read(view:View<String>):Unit{for(item in view){println(item)}}"
        ));
        let [EntityId::Loan(metadata)] = arguments.as_slice() else {
            panic!("one view metadata loan")
        };
        let metadata_begin = count_break_instruction(function, |instruction| {
            instruction.results.as_slice() == [EntityId::Loan(*metadata)]
        });
        assert!(loan_from_owner(metadata_begin, range_call.results[0]));
        let position = |block: &Block, id| {
            block
                .instructions
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap()
        };
        let end = |block: &Block, seed| {
            count_break_instruction(function, |instruction| {
                instruction.block == block.id
                    && matches!(instruction.operation,
                        Operation::BorrowEnd { loan } if is_from(EntityId::Loan(loan), seed))
            })
        };
        let break_receiver = end(break_block, receiver);
        let break_element = end(break_block, element);
        let break_source = end(break_block, provider_loan);
        assert!(position(break_block, break_receiver.id) < position(break_block, break_element.id));
        assert!(position(break_block, break_element.id) < position(break_block, break_source.id));
        assert_eq!(
            break_block
                .instructions
                .iter()
                .filter(|id| {
                    matches!(
                        function.instruction(**id).unwrap().operation,
                        Operation::BorrowEnd { .. }
                    )
                })
                .count(),
            3
        );
        let range_end = count_break_instruction(function, |instruction| {
            matches!(instruction.operation, Operation::RangeEnd { view, source }
                if EntityId::Value(view) == range_call.results[0]
                    && EntityId::Loan(source) == range_call.results[1])
        });
        assert_eq!(range_end.block, call_block.id);
        let count_end = end(call_block, count_loan);
        let metadata_end = end(call_block, EntityId::Loan(*metadata));
        let receiver_end = end(call_block, receiver);
        let element_end = end(call_block, element);
        let order = [
            count.id,
            count_begin.id,
            range_call.id,
            count_end.id,
            read.id,
            metadata_end.id,
            range_end.id,
            receiver_end.id,
            element_end.id,
        ];
        assert!(
            order
                .windows(2)
                .all(|pair| { position(call_block, pair[0]) < position(call_block, pair[1]) })
        );
        let TerminatorKind::Branch(backedge) = &call_block.terminator.as_ref().unwrap().kind else {
            panic!("normal call resumes the same loop")
        };
        let header = function.block(backedge.target).unwrap();
        let TerminatorKind::Conditional {
            condition: has_next,
            when_true: body,
            when_false: exhausted,
        } = &header.terminator.as_ref().unwrap().kind
        else {
            panic!("provider guard")
        };
        assert_eq!(body.target, branch.id);
        let entry = &function.blocks[0];
        let TerminatorKind::Branch(first) = &entry.terminator.as_ref().unwrap().kind else {
            panic!("the original entry reaches the provider guard")
        };
        assert_eq!(first.target, header.id);
        assert_eq!(provider_begin.block, entry.id);
        assert_eq!(length.block, entry.id);
        let guard = count_break_instruction(function, |instruction| {
            instruction.results.as_slice() == [EntityId::Value(*has_next)]
        });
        let Operation::Compare {
            operator: ComparisonOperator::LessThan,
            left: cursor,
            right,
        } = guard.operation
        else {
            panic!("the real provider guard compares cursor against this source length")
        };
        assert_eq!(guard.block, header.id);
        assert!(is_from(EntityId::Value(right), length.results[0]));
        let incoming = |edge: &Edge, parameter| {
            let Definition::BlockParameter { block, index } =
                function.entity(parameter).unwrap().definition
            else {
                panic!("the exact edge carries this provider parameter")
            };
            assert_eq!(block, edge.target);
            edge.arguments[index]
        };
        let initial = incoming(first, EntityId::Value(cursor));
        let initial = count_break_instruction(function, |instruction| {
            instruction.results.as_slice() == [initial]
        });
        assert_eq!(
            initial.operation,
            Operation::Constant(ScalarConstant::Integer(0))
        );
        assert_eq!(
            incoming(body, EntityId::Value(element_index)),
            EntityId::Value(cursor)
        );
        let next = incoming(backedge, EntityId::Value(cursor));
        let next = count_break_instruction(function, |instruction| {
            instruction.results.as_slice() == [next]
        });
        let Operation::Binary {
            operator: BinaryOperator::Add,
            left,
            right,
        } = next.operation
        else {
            panic!("the successful iteration advances exactly this cursor")
        };
        assert_eq!(next.block, call_block.id);
        let body_cursor = incoming(when_false, EntityId::Value(left));
        assert_eq!(incoming(body, body_cursor), EntityId::Value(cursor));
        // The step may reach the add through the loop header's parameters (refinement layer).
        for origin in count_break_origins(function, EntityId::Value(right)) {
            let step = count_break_instruction(function, |instruction| {
                instruction.results.as_slice() == [origin]
            });
            assert_eq!(
                step.operation,
                Operation::Constant(ScalarConstant::Integer(1))
            );
        }
        let exhausted = function.block(exhausted.target).unwrap();
        let exhaustion_end = end(exhausted, provider_loan);
        let drops = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            2,
            "one source-owner drop per mutually exclusive exit"
        );
        for (block, source_end) in [(break_block, break_source), (exhausted, exhaustion_end)] {
            let drop = count_break_instruction(function, |instruction| {
                instruction.block == block.id
                    && matches!(instruction.operation,
                        Operation::Drop { owner } if is_from(EntityId::Value(owner), seed))
            });
            assert!(position(block, source_end.id) < position(block, drop.id));
            let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
                panic!("cleanup reaches the common exit")
            };
            assert!(
                edge.arguments
                    .iter()
                    .all(|argument| !is_from(*argument, seed)),
                "dead owner is not transported after cleanup"
            );
        }
        let TerminatorKind::Branch(break_exit) = &break_block.terminator.as_ref().unwrap().kind
        else {
            unreachable!()
        };
        let TerminatorKind::Branch(exhaustion_exit) = &exhausted.terminator.as_ref().unwrap().kind
        else {
            unreachable!()
        };
        assert_eq!(break_exit.target, exhaustion_exit.target);
        assert_ne!(
            break_exit.target, header.id,
            "stop=true exits rather than continuing"
        );
        let exit = function.block(break_exit.target).unwrap();
        assert!(exit.instructions.is_empty());
        assert!(matches!(
            &exit.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.is_empty()
        ));
    }
}
