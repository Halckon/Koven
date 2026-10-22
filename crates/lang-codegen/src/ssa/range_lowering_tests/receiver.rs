//! Trusted source receivers use the same checked descriptor ABI as free producers.
use super::*;

const EXTENSIONS: &str = "borrow fun <T> List<T>.prefix(count:Int):View<T> from this=rangeView(this,0,count)\nborrow fun <T> View<T>.prefix(count:Int):View<T> from this=rangeView(this,0,count)";

#[test]
fn range_receiver_source_single_and_unit_keep_root_until_last_child() {
    let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun consume(own source:List<String>):Unit{}\nfun main():Unit{val source=listOf(\"first\",\"second\");borrow val parent=source.prefix(2);borrow val child=parent.prefix(1);read(child);consume(source)}";
    for program in [
        single(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_entry(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
        )
        .0,
    ] {
        let module = &program.modules[0];
        assert!(
            module
                .functions
                .iter()
                .filter(|f| f.carrier_return.is_some())
                .all(|f| f.receiver.is_none() && f.carrier_return == Some(0))
        );
        assert_eq!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.instructions)
                .filter(|i| matches!(i.operation, Operation::RangeCall { .. }))
                .count(),
            2
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_receiver_source_native_single_and_reversed_unit_clean_exactly_once() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    for (declaration, element, values, read, drops) in [
        (
            "",
            "String",
            "\"first\".clone(),\"second\".clone()",
            "println(item)",
            "",
        ),
        (
            "class Item(val text:String){}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "second\nfirst\n",
        ),
    ] {
        let consumer = format!(
            "{declaration}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun consume(own source:List<{element}>):Unit{{println(\"consume\")}}\nfun main():Unit{{val source=listOf({values});borrow val parent=(source).prefix(2);borrow val child=(parent).prefix(1);read(child);println(\"after\");consume(source);println(\"done\")}}"
        );
        for (program, entry) in [
            single_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
            unit_with_provider_order(
                &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
                EXTENSIONS,
                false,
            ),
            unit_with_provider_order(
                &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
                EXTENSIONS,
                true,
            ),
        ] {
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
            assert_success(
                &run,
                format!("first\nafter\nconsume\n{drops}done\n").as_bytes(),
            );
        }
    }
}

#[test]
fn range_receiver_source_count_control_flow_ends_receiver_before_early_return() {
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun scan(stop:Boolean):Unit{val source=listOf(Item(\"first\"),Item(\"second\"));read(source.prefix(if(stop){return}else{1}))}\nfun main():Unit{scan(false);scan(true)}";
    for (program, entry) in [
        single_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
            &llvm,
            &[1, 0, 2, 4, 3, 5],
        );
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"first\nsecond\nfirst\nsecond\nfirst\n",
        );
    }
}

#[test]
fn range_receiver_source_this_size_and_group_size_preserve_actual_view_extent() {
    let trusted = "borrow fun <T> List<T>.all():View<T> from this=rangeView(this,0,this.size)\nborrow fun <T> View<T>.all():View<T> from this=rangeView(this,0,(this).size)";
    let consumer = "fun read(view:View<String>):Unit{if(view.size!=2){error(\"size\")}}\nfun main():Unit{val source=listOf(\"first\",\"second\");borrow val parent=source.all();borrow val child=parent.all();read(child)}";
    for (program, entry) in [
        single_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.all\n{consumer}"),
            trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"");
    }
}

#[test]
fn range_receiver_source_temporary_chain_cleans_the_root_once() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun source():List<Item>{println(\"source\");return listOf(Item(\"first\"),Item(\"second\"))}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{read(source().prefix(2).prefix(1));println(\"done\")}";
    for (program, entry) in [
        single_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
        assert_success(&run, b"source\nfirst\nsecond\nfirst\ndone\n");
    }
}

#[test]
fn range_receiver_source_forwarded_nonzero_view_preserves_absolute_offsets() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    let trusted = format!(
        "{EXTENSIONS}\nborrow fun <T> List<T>.window():View<T> from this=rangeView(this,1,2)\nborrow fun <T> View<T>.forward(count:Int):View<T> from this=this.prefix(count)"
    );
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{val root=listOf(Item(\"first\"),Item(\"second\"));borrow val parent=root.window();borrow val child=parent.forward(1);read(child);println(\"done\")}";
    for (program, entry) in [
        single_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!(
                "package app\nimport koven.algorithms.window\nimport koven.algorithms.forward\n{consumer}"
            ),
            &trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
        assert_success(&run, b"second\ndone\nsecond\nfirst\n");
    }
}

#[test]
fn range_receiver_source_count_break_retains_existing_loop_cfg_frontier() {
    let body = "for(item in source){read(source.prefix(if(stop){break}else{1}))}";
    assert_cfg_frontier(body, "source", UnitCfgFrontier::CountBreak);
}

#[test]
fn range_receiver_source_borrow_parameter_rebinds_across_nonreturn_count_branch() {
    let trusted = EXTENSIONS;
    let consumer = "fun read(view:View<String>):Unit{println(\"read\")}\nfun scan(source:List<String>,flag:Boolean):Unit{read((source).prefix(if(flag){1}else{0}))}\nfun scan_view(source:View<String>,flag:Boolean):Unit{read((source).prefix(if(flag){1}else{0}))}\nfun main():Unit{val root=listOf(\"first\");scan(root,true);scan(root,false);borrow val parent=root.prefix(1);scan_view(parent,true);scan_view(parent,false)}";
    for (program, entry) in [
        single_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"read\nread\nread\nread\n");
    }
}

#[test]
fn range_receiver_source_retains_existing_binding_cfg_frontier() {
    assert_cfg_frontier(
        "borrow val part=source.prefix(if(stop){return}else{1});read(part)",
        "return",
        UnitCfgFrontier::Reject("return"),
    );
}

enum UnitCfgFrontier<'a> {
    Reject(&'a str),
    CountBreak,
}

fn assert_cfg_frontier(body: &str, fragment: &str, unit_frontier: UnitCfgFrontier<'_>) {
    use lang_frontend::{
        name_resolution::resolve_names, ownership_checking::check_ownership,
        type_checking::check_types,
    };
    let mut sources = SourceMap::new();
    let text = format!(
        "package app\n{EXTENSIONS}\nfun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}\nfun scan(stop:Boolean):Unit{{val source=listOf(\"first\");{body}}}\nfun main():Unit{{scan(false);scan(true)}}"
    );
    let (source, parsed) =
        crate::ssa::unit_lower_test_support::parsed(&mut sources, "frontier.ko", &text);
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let entry = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "main")
        .unwrap()
        .id();
    let error = crate::ssa::lower_frontend::orchestrate::lower_scalar_file_with_entry(
        &sources, &parsed, &names, &typed, &owned, entry,
    )
    .err()
    .expect("the existing CFG frontier must reject source before emission");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), fragment);
    {
        let inputs = [SourceUnitInput::new(
            "app",
            "app/frontier.ko",
            source,
            &parsed,
        )];
        let (names, typed, owned) =
            crate::ssa::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let lowered = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            crate::ssa::unit_lower_test_support::declaration(&names, "app", "main"),
        );
        match unit_frontier {
            UnitCfgFrontier::Reject(unit_fragment) => {
                let error = lowered
                    .err()
                    .expect("the existing CFG frontier must reject source before emission");
                assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
                assert_eq!(sources.slice(error.span.unwrap()).unwrap(), unit_fragment);
            }
            UnitCfgFrontier::CountBreak => {
                let (program, entry) =
                    lowered.expect("real loop-target cleanup admits this Unit source");
                let [source_unit] = names.names().index().source_units() else {
                    panic!("one original source unit")
                };
                assert_eq!(source_unit.source_id(), source);
                assert_count_break_products(&sources, &parsed, source_unit.id(), owned.ownership());
                assert_count_break_structure(&sources, &program);
                verify_program(&program).unwrap();
                crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            }
        }
    }
}

// Observe the original public cleanup products, not a reconstructed liveness set.
fn assert_count_break_products(
    sources: &SourceMap,
    parsed: &lang_frontend::parser::ParsedFile,
    source_unit: lang_frontend::name_resolution::SourceUnitId,
    owned: &lang_frontend::ownership_checking::CompilationUnitOwnership,
) {
    use lang_frontend::ownership_checking::{
        UnitDropTarget, UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        UnitLoanTarget, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    };
    use lang_frontend::type_checking::UnitCallReceiverOrigin;

    let text = |expression: lang_frontend::type_checking::UnitExpressionId| {
        assert_eq!(expression.source_unit(), source_unit);
        sources
            .slice(
                parsed
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .unwrap()
                    .span(),
            )
            .unwrap()
    };
    let plans = owned
        .iterations()
        .iter()
        .filter(|plan| text(plan.descriptor().source()) == "source")
        .collect::<Vec<_>>();
    let [plan] = plans.as_slice() else {
        panic!("one original scan provider")
    };
    let Some(UnitLoanTarget::Place(place)) = plan.source() else {
        panic!("original named List source")
    };
    assert!(place.is_root());
    let statement = plan.descriptor().statement();
    let exits = plan
        .exits()
        .iter()
        .filter(|exit| matches!(exit.kind(), Exit::Break(_) | Exit::Exhaustion))
        .collect::<Vec<_>>();
    assert_eq!(exits.len(), 2);
    for exit in exits {
        let actions = exit.actions();
        assert_eq!(owned.iteration_cleanup_at(exit.point()), Some(actions));
        let position = |action| actions.iter().position(|actual| *actual == action).unwrap();
        let finish = position(Action::FinishProvider(statement));
        let end_source = position(Action::EndSource(statement));
        let drops = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                Action::Drop(fact) if fact.target() == UnitDropTarget::Named(place.root()) => {
                    Some((index, fact))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(drop_index, fact)] = drops.as_slice() else {
            panic!("one source drop on each real exit, including break")
        };
        assert!(finish < end_source && end_source < *drop_index);
        assert_eq!(fact.point(), exit.point());
        assert_eq!(
            owned.drops().iter().filter(|other| *other == *fact).count(),
            1
        );
        let receivers = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                Action::EndReceiverLoan(fact) => Some((index, fact)),
                _ => None,
            })
            .collect::<Vec<_>>();
        match exit.kind() {
            Exit::Break(jump) => {
                assert_eq!(text(jump), "break");
                let [(receiver_index, fact)] = receivers.as_slice() else {
                    panic!("cancel the exact evaluated receiver before its provider")
                };
                assert_eq!(text(fact.call()), "source.prefix(if(stop){break}else{1})");
                let UnitCallReceiverOrigin::Expression(receiver) = fact.source() else {
                    panic!("source receiver expression")
                };
                assert_eq!(text(receiver), "source");
                assert_eq!(fact.kind(), UnitReceiverOwnershipKind::SharedLoan);
                assert!(matches!(
                    fact.target(),
                    UnitReceiverOwnershipTarget::Place(actual) if actual == place
                ));
                let end_element = position(Action::EndElement(statement));
                assert!(*receiver_index < end_element && end_element < finish);
            }
            Exit::Exhaustion => assert!(receivers.is_empty()),
            _ => unreachable!(),
        }
    }
}

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

fn assert_count_break_structure(sources: &SourceMap, program: &Program) {
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
    assert!(origin_is(
        &receiver_begin.origin,
        "source.prefix(if(stop){break}else{1})"
    ));
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
    assert!(is_from(
        EntityId::Value(*condition),
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
    let count = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [arguments[1]]
    });
    assert_eq!(
        count.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    let provider = program.modules[callee.module().index()]
        .function(*callee)
        .unwrap();
    assert!(origin_is(
        &provider.origin,
        "borrow fun <T> List<T>.prefix(count:Int):View<T> from this=rangeView(this,0,count)"
    ));
    assert_eq!(provider.carrier_return, Some(0));
    assert!(provider.receiver.is_none());
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
    let metadata_end = end(call_block, EntityId::Loan(*metadata));
    let receiver_end = end(call_block, receiver);
    let element_end = end(call_block, element);
    let order = [
        range_call.id,
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
    let step = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [EntityId::Value(right)]
    });
    assert_eq!(
        step.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
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
    let TerminatorKind::Branch(break_exit) = &break_block.terminator.as_ref().unwrap().kind else {
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

#[test]
fn range_receiver_source_temporary_for_cleans_the_root_after_elements() {
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun source():List<Item>{println(\"source\");return listOf(Item(\"first\"),Item(\"second\"))}\nfun main():Unit{for(item in source().prefix(2).prefix(1)){println(item.text)};println(\"done\")}";
    for (program, entry) in [
        single_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
            &llvm,
            &[1, 0, 2],
        );
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"source\nfirst\nsecond\nfirst\ndone\n",
        );
    }
}

#[test]
fn real_std_receiver_take_preserves_nonzero_view_offsets_and_top_level_compatibility() {
    let trusted = "fun window(source:List<String>):View<String> from source=rangeView(source,1,2)";
    let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{val root=listOf(\"first\".clone(),\"second\".clone());borrow val parent=window(root);borrow val child=parent.take(2147483647);read(child);read(take(child,1));read(child.take(0))}";
    for (program, entry) in [
        single_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!(
                "package app\nimport koven.algorithms.take\nimport koven.algorithms.window\n{consumer}"
            ),
            trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
            &llvm,
            &[1, 0, 2],
        );
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"second\nsecond\n");
    }
}

#[test]
fn range_receiver_named_root_short_borrow_cleans_after_consumer_and_return() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    for (declaration, element, values, read, drops) in [
        (
            "",
            "String",
            "\"first\".clone(),\"second\".clone()",
            "println(item)",
            "",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "second\nfirst\n",
        ),
    ] {
        for operation in ["cut(root,1)", "root.prefix(1)"] {
            for reader in [
                "println(\"read\")".to_owned(),
                format!("for(item in view){{{read}}};println(\"read\")"),
            ] {
                let provider = format!(
                    "{EXTENSIONS}\nfun <T> cut(source:List<T>,count:Int):View<T> from source=rangeView(source,0,count)"
                );
                let consumer = format!(
                    "{declaration}\nfun read(view:View<{element}>,number:Int):Unit{{{reader}}}\nfun scan(stop:Boolean):Unit{{val root=listOf({values});read({operation},if(stop){{return}}else{{0}});println(\"after\")}}\nfun main():Unit{{scan(false);scan(true);println(\"done\")}}"
                );
                for (program, entry) in [
                    single_with_entry(&format!("{provider}\n{consumer}")),
                    unit_with_provider_order(
                        &format!(
                            "package app\nimport koven.algorithms.prefix\nimport koven.algorithms.cut\n{consumer}"
                        ),
                        &provider,
                        true,
                    ),
                ] {
                    let llvm =
                        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                    let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2, 4, 3, 5]);
                    let prefix = if reader.starts_with("for") {
                        "first\n"
                    } else {
                        ""
                    };
                    assert_success(
                        &run,
                        format!("{prefix}read\nafter\n{drops}{drops}done\n").as_bytes(),
                    );
                }
            }
        }
    }
}
