//! Direct sealed sibling loops share an external source, never a fresh result lease.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
use std::collections::BTreeSet;

fn edges(function: &Function) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![(block.id, edge)],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![(block.id, when_true), (block.id, when_false)],
            TerminatorKind::Return { values } => {
                assert!(values.is_empty());
                Vec::new()
            }
            other => panic!("unexpected sibling terminator {other:?}"),
        })
        .collect()
}

fn actual_abi_roles(
    function: &Function,
    edges: &[(BlockId, &Edge)],
) -> std::collections::BTreeMap<EntityId, usize> {
    let mut roles = function
        .block(function.entry_block().unwrap())
        .unwrap()
        .parameters
        .iter()
        .copied()
        .enumerate()
        .map(|(index, entity)| (entity, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    loop {
        let old = roles.len();
        for (_, edge) in edges {
            for (argument, parameter) in edge
                .arguments
                .iter()
                .zip(&function.block(edge.target).unwrap().parameters)
            {
                if let Some(role) = roles.get(argument).copied()
                    && let Some(previous) = roles.insert(*parameter, role)
                {
                    assert_eq!(previous, role);
                }
            }
        }
        if old == roles.len() {
            break;
        }
    }
    for block in &function.blocks {
        assert_eq!(
            block
                .parameters
                .iter()
                .map(|entity| roles[entity])
                .collect::<BTreeSet<_>>()
                .len(),
            block.parameters.len()
        );
    }
    for (from, edge) in edges {
        for (argument, parameter) in edge
            .arguments
            .iter()
            .zip(&function.block(edge.target).unwrap().parameters)
        {
            let role = roles[parameter];
            let local = function
                .block(*from)
                .unwrap()
                .parameters
                .iter()
                .filter(|entity| roles[entity] == role)
                .collect::<Vec<_>>();
            assert_eq!(local, vec![argument]);
        }
    }
    roles
}

#[test]
fn ordinary_no_carry_siblings_have_independent_real_body_leases() {
    let mut cases = Vec::new();
    for counts in [vec![1, 3], vec![1, 3, 9], vec![1, 2, 3, 4, 9]] {
        for same_flag in [false, true] {
            for reversed in [false, true] {
                let flags = (0..counts.len())
                    .map(|index| if same_flag { 0 } else { index % 2 })
                    .collect::<Vec<_>>();
                let mut abi = if same_flag {
                    vec![None, Some(0)]
                } else {
                    vec![None, Some(0), Some(1)]
                };
                if reversed {
                    abi.swap(0, 1);
                }
                cases.push((counts.clone(), flags, abi));
            }
        }
    }
    cases.push((vec![1, 3, 9], vec![0, 0, 1], vec![Some(1), None, Some(0)]));
    cases.push((
        vec![1, 2, 3, 4, 9],
        vec![2, 0, 1, 2, 0],
        vec![Some(1), Some(0), None, Some(2)],
    ));
    // Unused Borrow Boolean parameters retain ABI identity but leave no loop obligation.
    for abi in [
        vec![Some(9), None, Some(0)],
        vec![None, Some(9), Some(0)],
        vec![None, Some(0), Some(9)],
        vec![Some(9), Some(0), Some(8), None],
    ] {
        cases.push((vec![1], vec![0], abi));
    }
    cases.push((
        vec![1, 3, 9],
        vec![0, 1, 0],
        vec![Some(9), Some(1), None, Some(0), Some(8)],
    ));
    cases.push((
        vec![1, 2, 3, 4, 9],
        vec![2, 0, 1, 2, 0],
        vec![Some(1), Some(9), Some(0), None, Some(8), Some(2)],
    ));
    let mut cases = cases
        .into_iter()
        .map(|(counts, flags, abi)| (counts, flags, abi, BTreeSet::new()))
        .collect::<Vec<_>>();
    for reversed in [false, true] {
        for own in [
            BTreeSet::from([0]),
            BTreeSet::from([1]),
            BTreeSet::from([0, 1]),
        ] {
            let mut abi = vec![None, Some(0), Some(1), Some(9)];
            if reversed {
                abi.reverse();
            }
            cases.push((vec![1, 3, 9], vec![0, 1, 0], abi, own));
        }
    }
    cases.push((vec![1], vec![0], vec![Some(0), None], BTreeSet::from([0])));
    cases.push((
        vec![1, 2, 3, 4, 17],
        vec![2, 0, 1, 2, 0],
        vec![Some(1), Some(9), Some(0), None, Some(8), Some(2)],
        BTreeSet::from([0, 2, 9]),
    ));
    for (counts, flag_order, abi_order, own_flags) in cases {
        let loops = counts
            .iter()
            .enumerate()
            .map(|(index, count)| {
                let calls = (0..*count)
                    .map(|_| format!("consume{index}(item{index})"))
                    .collect::<Vec<_>>()
                    .join(";");
                format!(
                    "while(flag{}){{borrow val item{index}=view(source);{calls}}}",
                    flag_order[index]
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let declarations = (0..counts.len())
            .map(|index| format!("fun consume{index}(item:String):Unit {{}}\n"))
            .collect::<String>();
        let parameters = abi_order
            .iter()
            .map(|flag| match flag {
                None => "source:String".to_owned(),
                Some(index) if own_flags.contains(index) => format!("own flag{index}:Boolean"),
                Some(index) => format!("flag{index}:Boolean"),
            })
            .collect::<Vec<_>>()
            .join(",");
        let source_parameter = abi_order.iter().position(Option::is_none).unwrap();
        let flag_parameters = flag_order
            .iter()
            .map(|flag| {
                abi_order
                    .iter()
                    .position(|entry| *entry == Some(*flag))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let text = format!(
            "fun view(source:String):borrow String from source=source\n{declarations}fun run({parameters}):Unit {{{loops}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "siblings.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "siblings.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, on) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let mut baseline = None;
            for owned in [&off, &on] {
                let facts = owned.ownership().no_carry_loops();
                assert_eq!(facts.len(), counts.len());
                for (index, fact) in facts.iter().enumerate() {
                    assert!(owned.ownership().owns_no_carry_loop(fact));
                    assert_eq!(fact.consumers().len(), counts[index]);
                    assert_eq!(fact.source(), facts[0].source());
                    assert_eq!(fact.callable(), facts[0].callable());
                    for previous in &facts[..index] {
                        assert_ne!(fact.binding(), previous.binding());
                        assert_ne!(fact.initializer(), previous.initializer());
                        assert_ne!(fact.body(), previous.body());
                        assert_ne!(fact.end(), previous.end());
                    }
                }
                let ((lowered, verified), receipts) =
                    crate::ssa::refine::fresh_loop_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let (program, _) = lowered.expect("sealed independent sibling lowering");
                assert_eq!(verified.len(), 1);
                assert_eq!(receipts.len(), 1, "distinct flag composed receipt missing");
                verify_program(&program).unwrap();
                let run = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.name.contains(".run."))
                    .unwrap();
                {
                    let receipt = receipts[0].clone();
                    receipt
                        .validate(run)
                        .expect("independent composed graph verification");
                    assert_eq!(receipt.loops.len(), facts.len());
                    assert_eq!(receipt.recorded_fold_count(), facts.len());
                    assert!(receipt.sequential_exit_links());
                    for (recorded, fact) in receipt.loops.iter().zip(facts) {
                        assert!(recorded.fact.same_identity(fact));
                        assert_eq!(&recorded.fact, fact);
                        let index = facts.iter().position(|f| f.same_identity(fact)).unwrap();
                        assert_eq!(
                            recorded.instructions.len(),
                            fact.consumers().len() + 3
                                - usize::from(own_flags.contains(&flag_order[index]))
                        );
                    }
                }
                let actual_edges = edges(run);
                let roles = actual_abi_roles(run, &actual_edges);
                let actual_entry = run.block(run.entry_block().unwrap()).unwrap();
                assert_eq!(actual_entry.parameters.len(), abi_order.len());
                for (index, entity) in actual_entry.parameters.iter().enumerate() {
                    let own = abi_order[index].is_some_and(|flag| own_flags.contains(&flag));
                    assert_eq!(matches!(entity, EntityId::Value(_)), own);
                    assert_eq!(matches!(entity, EntityId::Loan(_)), !own);
                }
                for block in &run.blocks {
                    for entity in &block.parameters {
                        let role = roles[entity];
                        let own = abi_order[role].is_some_and(|flag| own_flags.contains(&flag));
                        assert_eq!(matches!(entity, EntityId::Value(_)), own);
                        assert_eq!(
                            run.entity(*entity).unwrap().ty,
                            run.entity(actual_entry.parameters[role]).unwrap().ty
                        );
                    }
                }

                let unused = (0..abi_order.len())
                    .filter(|index| *index != source_parameter && !flag_parameters.contains(index))
                    .collect::<BTreeSet<_>>();
                for block in &run.blocks {
                    if block.id != actual_entry.id {
                        assert!(
                            block
                                .parameters
                                .iter()
                                .all(|entity| !unused.contains(&roles[entity]))
                        );
                    }
                }
                for (_, edge) in &actual_edges {
                    assert!(
                        edge.arguments
                            .iter()
                            .all(|entity| !unused.contains(&roles[entity]))
                    );
                }
                for instruction in &run.instructions {
                    let loan = match instruction.operation {
                        Operation::Read {
                            source: PlaceAccess::Loan(loan),
                        }
                        | Operation::BorrowEnd { loan } => Some(loan),
                        _ => None,
                    };
                    if let Some(role) = loan.and_then(|loan| roles.get(&EntityId::Loan(loan))) {
                        assert!(!unused.contains(role));
                    }
                }
                // Derive source transport from the actual ABI and every actual incoming edge.
                let source_seed =
                    run.block(run.entry_block().unwrap()).unwrap().parameters[source_parameter];
                let mut source_entities = BTreeSet::from([source_seed]);
                loop {
                    let old = source_entities.len();
                    for (_, edge) in &actual_edges {
                        for (argument, parameter) in edge
                            .arguments
                            .iter()
                            .zip(&run.block(edge.target).unwrap().parameters)
                        {
                            if source_entities.contains(argument) {
                                source_entities.insert(*parameter);
                            }
                        }
                    }
                    if old == source_entities.len() {
                        break;
                    }
                }
                for (from, edge) in &actual_edges {
                    for (argument, parameter) in edge
                        .arguments
                        .iter()
                        .zip(&run.block(edge.target).unwrap().parameters)
                    {
                        if source_entities.contains(parameter) {
                            assert!(source_entities.contains(argument));
                            assert!(run.block(*from).unwrap().parameters.contains(argument));
                        }
                    }
                }
                let mut fresh = BTreeSet::new();
                for (index, fact) in facts.iter().enumerate() {
                    let span = parsed
                        .ast()
                        .expressions()
                        .get(fact.initializer().expression())
                        .unwrap()
                        .span();
                    let acquired = run
                        .instructions
                        .iter()
                        .find(|i| {
                            i.origin.span() == span
                                && matches!(i.operation, Operation::BorrowCall { .. })
                        })
                        .unwrap();
                    let Operation::BorrowCall {
                        source, arguments, ..
                    } = &acquired.operation
                    else {
                        unreachable!()
                    };
                    assert!(source_entities.contains(&EntityId::Loan(*source)));
                    assert_eq!(arguments.as_slice(), &[EntityId::Loan(*source)]);
                    let [EntityId::Loan(result)] = acquired.results.as_slice() else {
                        panic!("fresh loan")
                    };
                    assert!(fresh.insert(*result));
                    let body = run.block(acquired.block).unwrap();
                    assert!(body.parameters.contains(&EntityId::Loan(*source)));
                    assert_eq!(
                        body.parameters
                            .iter()
                            .filter(|entity| source_entities.contains(entity))
                            .count(),
                        1
                    );
                    let sequence = body
                        .instructions
                        .iter()
                        .map(|id| &run.instructions[id.index()])
                        .collect::<Vec<_>>();
                    assert_eq!(sequence.len(), counts[index] + 2);
                    assert_eq!(sequence[0].id, acquired.id);
                    for (position, call) in sequence[1..=counts[index]].iter().enumerate() {
                        let Operation::DirectCall {
                            callee,
                            receiver,
                            arguments,
                        } = &call.operation
                        else {
                            panic!("consumer")
                        };
                        assert!(receiver.is_none());
                        assert_eq!(arguments.as_slice(), &[EntityId::Loan(*result)]);
                        let actual_callee = program
                            .modules
                            .iter()
                            .flat_map(|m| &m.functions)
                            .find(|f| f.id == *callee)
                            .unwrap();
                        assert!(actual_callee.name.contains(&format!(".consume{index}.")));
                        let call_span = parsed
                            .ast()
                            .expressions()
                            .get(fact.consumers()[position].1.expression())
                            .unwrap()
                            .span();
                        assert_eq!(call.origin.span(), call_span);
                    }
                    assert!(
                        matches!(sequence.last().unwrap().operation, Operation::BorrowEnd { loan } if loan == *result)
                    );
                    let TerminatorKind::Branch(backedge) = &body.terminator.as_ref().unwrap().kind
                    else {
                        panic!("backedge")
                    };
                    assert!(!backedge.arguments.contains(&EntityId::Loan(*result)));
                    let header = run.block(backedge.target).unwrap();
                    let TerminatorKind::Conditional { when_true, .. } =
                        &header.terminator.as_ref().unwrap().kind
                    else {
                        panic!("header")
                    };
                    assert_eq!(when_true.target, body.id);
                    let expected_roles = std::iter::once(source_parameter)
                        .chain(flag_parameters[index..].iter().copied())
                        .collect::<BTreeSet<_>>();
                    for block in [header, body] {
                        assert_eq!(
                            block
                                .parameters
                                .iter()
                                .map(|entity| roles[entity])
                                .collect::<BTreeSet<_>>(),
                            expected_roles
                        );
                        assert_eq!(block.parameters.len(), expected_roles.len());
                    }
                    let TerminatorKind::Conditional {
                        condition,
                        when_false,
                        ..
                    } = &header.terminator.as_ref().unwrap().kind
                    else {
                        unreachable!()
                    };
                    if own_flags.contains(&flag_order[index]) {
                        assert!(
                            header.instructions.is_empty(),
                            "Own Name must not manufacture Read"
                        );
                        assert!(header.parameters.contains(&EntityId::Value(*condition)));
                        assert_eq!(roles[&EntityId::Value(*condition)], flag_parameters[index]);
                        assert_ne!(
                            EntityId::Value(*condition),
                            actual_entry.parameters[flag_parameters[index]]
                        );
                    } else {
                        let read = run
                            .instructions
                            .iter()
                            .find(|instruction| {
                                instruction.block == header.id
                                    && matches!(instruction.operation, Operation::Read { .. })
                            })
                            .unwrap();
                        let Operation::Read {
                            source: PlaceAccess::Loan(flag),
                        } = read.operation
                        else {
                            panic!("actual Boolean loan read")
                        };
                        assert!(header.parameters.contains(&EntityId::Loan(flag)));
                        assert_eq!(roles[&EntityId::Loan(flag)], flag_parameters[index]);
                        assert_eq!(read.results.as_slice(), &[EntityId::Value(*condition)]);
                    }
                    let after = if index + 1 == facts.len() {
                        BTreeSet::new()
                    } else {
                        std::iter::once(source_parameter)
                            .chain(flag_parameters[index + 1..].iter().copied())
                            .collect::<BTreeSet<_>>()
                    };
                    let exit = run.block(when_false.target).unwrap();
                    assert_eq!(
                        exit.parameters
                            .iter()
                            .map(|entity| roles[entity])
                            .collect::<BTreeSet<_>>(),
                        after
                    );
                    assert_eq!(exit.parameters.len(), after.len());
                }
                assert_eq!(
                    run.instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                        .count(),
                    counts.len()
                );
                assert_eq!(
                    run.instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
                        .count(),
                    counts.len()
                );
                assert!(run.instructions.iter().all(|i| !matches!(i.operation, Operation::BorrowEnd { loan } if source_entities.contains(&EntityId::Loan(loan)))));
                for (_, edge) in &actual_edges {
                    assert!(edge.arguments.iter().all(
                        |argument| !matches!(argument, EntityId::Loan(loan) if fresh.contains(loan))
                    ));
                }
                let rendered = crate::llvm::render_verified_program(&program).unwrap();
                if let Some(previous) = baseline.as_ref() {
                    assert_eq!(&rendered, previous);
                }
                baseline = Some(rendered);
            }
        }
    }
}

#[test]
fn ordinary_no_carry_peer_permission_does_not_cross_proof_boundaries() {
    for (label, parameters, body, fact_count) in [
        (
            "different-root",
            "source:String,other:String,flag:Boolean",
            "while(flag){borrow val item=view(source);consume(item)};while(flag){borrow val peer=view(other);consume(peer)}",
            2,
        ),
        (
            "unsealed-peer",
            "source:String,flag:Boolean",
            "while(flag){borrow val item=view(source);consume(item)};while(flag){consume(source)}",
            1,
        ),
    ] {
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run({parameters}):Unit {{{body}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "peer-boundary.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "peer-boundary.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, on) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            for owned in [&off, &on] {
                assert_eq!(
                    owned.ownership().no_carry_loops().len(),
                    fact_count,
                    "{label}"
                );
                let ((result, verified), receipts) =
                    crate::ssa::refine::fresh_loop_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let error = result
                    .err()
                    .unwrap_or_else(|| panic!("{label} must not inherit peer permission"));
                assert_eq!(
                    error.kind,
                    crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode,
                    "{label}"
                );
                let rejected = parsed
                    .ast()
                    .statements()
                    .iter()
                    .filter_map(|(_, node)| {
                        matches!(
                            node.payload(),
                            lang_frontend::parser::Statement::While { .. }
                        )
                        .then_some(node.span())
                    })
                    .max_by_key(|span| span.start())
                    .unwrap();
                assert_eq!(error.span, Some(rejected), "{label}");
                assert!(verified.is_empty());
                assert!(receipts.is_empty());
            }
        }
    }
}

#[test]
fn ordinary_no_carry_unsealed_frame_boundaries_remain_frontend_errors() {
    use lang_frontend::name_resolution::{index_compilation_unit, resolve_compilation_unit_names};
    use lang_frontend::type_checking::check_compilation_unit_types;
    for (label, body) in [
        (
            "ordinary-binding",
            "borrow val outer=view(source);consume(outer);while(flag){borrow val item=view(source);consume(item)}",
        ),
        (
            "nested",
            "while(flag){while(flag){borrow val item=view(source);consume(item)}}",
        ),
        (
            "branch",
            "if(flag){while(flag){borrow val item=view(source);consume(item)}}",
        ),
    ] {
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(source:String,flag:Boolean):Unit {{{body}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "unsealed-frame.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "unsealed-frame.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let on = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert!(legacy_ownership_eq(&on, &off), "{label}");
            for owned in [&on, &off] {
                assert!(owned.no_carry_loops().is_empty(), "{label}");
                assert!(
                    owned.borrow_results().bindings().is_empty(),
                    "atomic failed facts"
                );
                let failures = owned
                    .diagnostics()
                    .iter()
                    .filter(|d| d.code().to_string() == "L0164")
                    .collect::<Vec<_>>();
                assert_eq!(failures.len(), 1, "{label}: {:?}", owned.diagnostics());
                let start = text.rfind("borrow val item").unwrap();
                assert_eq!(failures[0].primary_span().start(), start);
                assert_eq!(failures[0].primary_span().end(), start + "borrow".len());
            }
        }
    }
}
