//! Bounded single-survivor trees require actual producer and fold evidence.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    type_checking::UnitCallTarget,
};

#[test]
fn ordinary_bounded_continuation_trees_require_actual_receipt() {
    for (shape, body, count, flag_count, routes) in [
        (
            "root-hole",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { return c2(source) } }; return c3(source)",
            4,
            3,
            vec!["TTT", "TTF", "TF", "F"],
        ),
        (
            "depth-two-hole",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } } else { return c2(source) }; return c3(source)",
            4,
            3,
            vec!["TTT", "TTF", "F", "TF"],
        ),
        (
            "right-chain-hole",
            "if (f0) { return c0(source) } else { if (f1) { return c1(source) } else { if (f2) { return c2(source) } } }; return c3(source)",
            4,
            3,
            vec!["T", "FT", "FFT", "FFF"],
        ),
        (
            "then-subtree-hole",
            "if (f0) { if (f1) { return c0(source) } else { if (f2) { return c1(source) } } } else { return c2(source) }; return c3(source)",
            4,
            3,
            vec!["TT", "TFT", "F", "TFF"],
        ),
        (
            "mixed-deep-hole",
            "if (f0) { if (f1) { if (f2) { return c0(source) } } else { return c1(source) } } else { if (f3) { return c2(source) } else { return c3(source) } }; return c4(source)",
            5,
            4,
            vec!["TTT", "TF", "FT", "FF", "TTF"],
        ),
    ] {
        let callees = (0..count)
            .map(|i| format!("fun c{i}(source: String): borrow String from source = source\n"))
            .collect::<String>();
        let params = (0..flag_count)
            .map(|i| format!("f{i}: Boolean"))
            .collect::<Vec<_>>()
            .join(", ");
        let args = (0..flag_count)
            .map(|i| format!("f{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}\nfun consume(item: String): Unit {{}}\nfun run({params}): Unit {{val owner = \"kept\"; borrow val item = wrap(owner, {args}); consume(item)}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "nested-neighbor.ko", &text);
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "nested-neighbor.ko", source, &parsed),
            ),
            ("memory", SourceUnitInput::memory_document(source, &parsed)),
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
            assert_eq!(
                on.ownership().owner_observations(),
                off.ownership().owner_observations()
            );
            assert!(off.ownership().loan_action_observations().is_empty());
            let raw = on.ownership().borrow_forward_transports();
            assert_eq!(raw.len(), count, "{shape}/{layout}");
            let mut calls = Vec::new();
            for transport in raw {
                let call = transport.returned_expression();
                assert!(!calls.contains(&call));
                calls.push(call);
                assert_eq!(transport.source_loan().call(), call);
                assert_eq!(
                    transport.source_loan().target(),
                    raw[0].source_loan().target()
                );
                assert_eq!(transport.handoff_point(), UnitDropPoint::CallReturn(call));
                assert!(matches!(
                    typed.types().call(call).unwrap().target(),
                    UnitCallTarget::Declaration(_)
                ));
                assert_eq!(
                    on.ownership()
                        .loans()
                        .iter()
                        .filter(|loan| loan.call() == call)
                        .collect::<Vec<_>>(),
                    vec![transport.source_loan()]
                );
            }
            let handoffs = on
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|catalog| catalog.actions())
                .filter_map(|action| match action {
                    UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff.transaction()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(handoffs, raw.iter().collect::<Vec<_>>());
            assert!(
                on.ownership()
                    .loan_action_observations()
                    .iter()
                    .all(|catalog| catalog.nested_forward_regions().is_empty())
            );
            let declarations = names.names().index().declarations();
            let wrapper = declarations.iter().find(|d| d.name() == "wrap").unwrap();
            let entry = declarations
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let marker = typed
                .types()
                .signatures()
                .declaration(wrapper.id())
                .unwrap()
                .callable()
                .unwrap()
                .borrow_return()
                .unwrap()
                .marker_span();
            assert_eq!(sources.slice(marker).unwrap(), "borrow");
            let [catalog] = on.ownership().loan_action_observations() else {
                panic!("one catalog")
            };
            let [region] = catalog.bounded_continuation_regions() else {
                panic!("one bounded region")
            };
            let expected = region.paths();
            assert_eq!(expected.len(), count);
            let mut prefixes = Vec::new();
            for route in &routes {
                for depth in 0..route.len() {
                    if !prefixes.contains(&&route[..depth]) {
                        prefixes.push(&route[..depth]);
                    }
                }
            }
            assert_eq!(prefixes.len(), flag_count);
            let mut previous = None;
            for (setting, owned) in [("on", &on), ("off", &off)] {
                let ((lowered, verification), receipts) =
                    crate::ssa::refine::single_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let (program, _) = lowered.unwrap_or_else(|error| {
                    panic!("{shape}/{layout}/{setting}: actual continuation lowering {error:?}")
                });
                assert_eq!(verification.len(), 1);
                verify_program(&program).unwrap();
                let module = &program.modules[0];
                let function = module
                    .functions
                    .iter()
                    .find(|f| f.name.contains(".wrap."))
                    .unwrap();
                assert_eq!(function.blocks.len(), count + flag_count);
                assert_eq!(function.instructions.len(), count + flag_count);
                let entry_block = &function.blocks[0];
                assert_eq!(entry_block.parameters.len(), flag_count + 1);
                let entry_loans = entry_block
                    .parameters
                    .iter()
                    .map(|entity| {
                        let EntityId::Loan(loan) = entity else {
                            panic!("loan ABI")
                        };
                        *loan
                    })
                    .collect::<Vec<_>>();
                let mapped = |edge: &Edge, loan| {
                    edge.arguments
                        .iter()
                        .position(|arg| *arg == EntityId::Loan(loan))
                        .map(|index| {
                            let EntityId::Loan(mapped) =
                                function.block(edge.target).unwrap().parameters[index]
                            else {
                                panic!("loan transport")
                            };
                            mapped
                        })
                };
                let mut observed = Vec::new();
                for (leaf, route) in routes.iter().enumerate() {
                    let mut block = entry_block.id;
                    let mut source = entry_loans[0];
                    let mut flags = entry_loans[1..]
                        .iter()
                        .copied()
                        .map(Some)
                        .collect::<Vec<_>>();
                    let mut edges = Vec::new();
                    for (depth, arm) in route.bytes().enumerate() {
                        let split_index =
                            prefixes.iter().position(|p| *p == &route[..depth]).unwrap();
                        let flag = flags[split_index].unwrap();
                        let TerminatorKind::Conditional {
                            condition,
                            when_true,
                            when_false,
                        } = &function
                            .block(block)
                            .unwrap()
                            .terminator
                            .as_ref()
                            .unwrap()
                            .kind
                        else {
                            panic!("actual split")
                        };
                        let read = function
                            .instructions
                            .iter()
                            .find(|i| i.results == [EntityId::Value(*condition)])
                            .unwrap();
                        assert_eq!(
                            read.operation,
                            Operation::Read {
                                source: PlaceAccess::Loan(flag)
                            }
                        );
                        if setting == "on" {
                            let [receipt] = receipts.as_slice() else {
                                panic!("one actual receipt")
                            };
                            let split = &receipt.splits[split_index];
                            assert_eq!(split.split, expected[leaf].edges()[depth].split());
                            assert_eq!(split.condition, expected[leaf].edges()[depth].condition());
                            assert_eq!(
                                (split.block, split.flag, split.read, split.value),
                                (block, flag, read.id, *condition)
                            );
                            assert_eq!(split.edges, [when_true.clone(), when_false.clone()]);
                        }
                        assert_eq!(expected[leaf].edges()[depth].then_arm(), arm == b'T');
                        let edge = if arm == b'T' { when_true } else { when_false };
                        source = mapped(edge, source).unwrap();
                        flags = flags
                            .into_iter()
                            .map(|flag| flag.and_then(|flag| mapped(edge, flag)))
                            .collect();
                        block = edge.target;
                        edges.push(edge.clone());
                    }
                    let TerminatorKind::BorrowReturn { loan: result } = function
                        .block(block)
                        .unwrap()
                        .terminator
                        .as_ref()
                        .unwrap()
                        .kind
                    else {
                        panic!("terminal leaf")
                    };
                    let call = function
                        .instructions
                        .iter()
                        .find(|i| i.results == [EntityId::Loan(result)])
                        .unwrap();
                    let callee = module
                        .functions
                        .iter()
                        .find(|f| f.name.contains(&format!(".c{leaf}.")))
                        .unwrap()
                        .id();
                    assert_eq!(
                        call.operation,
                        Operation::BorrowCall {
                            callee,
                            arguments: vec![EntityId::Loan(source)],
                            source
                        }
                    );
                    observed.push((block, call.id, source, result, callee, edges));
                }
                let output = (
                    crate::ssa::render_program(&program),
                    crate::llvm::render_verified_program(&program).unwrap(),
                );
                if setting == "off" {
                    assert!(receipts.is_empty());
                    assert_eq!(previous.as_ref(), Some(&output));
                    continue;
                }
                previous = Some(output);
                let [receipt] = receipts.as_slice() else {
                    panic!("one dynamic receipt")
                };
                assert_eq!(receipt.callable, region.callable());
                assert_eq!(
                    (receipt.caller, receipt.entry, receipt.source),
                    (function.id(), entry_block.id, entry_loans[0])
                );
                assert_eq!(receipt.splits.len(), flag_count);
                assert_eq!(receipt.paths.len(), count);
                let continuation = region.continuation().unwrap();
                assert_eq!(
                    receipt.continuation_goto_count(),
                    continuation.entries().len()
                );
                assert_eq!(
                    receipt.continuation_fold_count(),
                    continuation.entries().len()
                );
                assert!(receipt.folded_continuation_chain());
                assert!(receipt.tail_mapping_matches(observed.last().unwrap().0));
                for ((path, expected), actual) in receipt.paths.iter().zip(expected).zip(observed) {
                    assert_eq!(path.control, expected.control());
                    assert_eq!(path.value, expected.value());
                    assert_eq!(path.ids, vec![expected.source_begin(), expected.handoff()]);
                    assert_eq!(
                        (
                            path.block,
                            path.instruction,
                            path.source,
                            path.result,
                            path.callee,
                            &path.edges
                        ),
                        (actual.0, actual.1, actual.2, actual.3, actual.4, &actual.5)
                    );
                }
            }
        }
    }
}

/// Enumerate every eligible implicit-false leaf in every bounded full tree, independently
/// of the producer event planner. Old fixed cases retain their existing receipt path.
#[test]
fn ordinary_all_bounded_single_hole_shapes_preserve_observer_off() {
    #[derive(Clone)]
    enum Tree {
        Leaf,
        Split(Box<Tree>, Box<Tree>),
    }
    fn trees(depth: usize) -> Vec<Tree> {
        let mut out = vec![Tree::Leaf];
        if depth > 0 {
            let children = trees(depth - 1);
            for left in &children {
                for right in &children {
                    out.push(Tree::Split(Box::new(left.clone()), Box::new(right.clone())));
                }
            }
        }
        out
    }
    fn holes(tree: &Tree, route: &str, out: &mut Vec<String>) {
        if let Tree::Split(left, right) = tree {
            holes(left, &format!("{route}T"), out);
            let right_route = format!("{route}F");
            if matches!(right.as_ref(), Tree::Leaf) {
                out.push(right_route);
            } else {
                holes(right, &right_route, out);
            }
        }
    }
    fn source(
        tree: &Tree,
        route: &str,
        hole: &str,
        flags: &mut usize,
        returns: &mut usize,
    ) -> String {
        match tree {
            Tree::Leaf => {
                let i = *returns;
                *returns += 1;
                format!("return c{i}(source)")
            }
            Tree::Split(left, right) => {
                let flag = *flags;
                *flags += 1;
                let left = source(left, &format!("{route}T"), hole, flags, returns);
                let right_route = format!("{route}F");
                if right_route == hole {
                    format!("if (f{flag}) {{ {left} }}")
                } else {
                    let right = source(right, &right_route, hole, flags, returns);
                    format!("if (f{flag}) {{ {left} }} else {{ {right} }}")
                }
            }
        }
    }
    let mut cases = 0;
    for tree in trees(3) {
        let mut candidates = Vec::new();
        holes(&tree, "", &mut candidates);
        for hole in candidates {
            cases += 1;
            let (mut flags, mut returns) = (0, 0);
            let body = source(&tree, "", &hole, &mut flags, &mut returns);
            assert_eq!(returns, flags);
            let body = format!("{body}; return c{returns}(source)");
            let callees = (0..=returns)
                .map(|i| format!("fun c{i}(source: String): borrow String from source = source\n"))
                .collect::<String>();
            let params = (0..flags)
                .map(|i| format!("f{i}: Boolean"))
                .collect::<Vec<_>>()
                .join(", ");
            let args = (0..flags)
                .map(|i| format!("f{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let text = format!(
                "{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}\nfun consume(item: String): Unit {{}}\nfun run({params}): Unit {{val owner = \"kept\"; borrow val item = wrap(owner, {args}); consume(item)}}"
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "all-hole-trees.ko", &text);
            let inputs = [SourceUnitInput::memory_document(source, &parsed)];
            let (environment, types) = standard_environments();
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
            let mut previous = None;
            for (setting, owned) in [("on", &on), ("off", &off)] {
                let (lowered, verification) =
                    crate::ssa::unit_lower::observe_final_verifier(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, owned, entry,
                        )
                    });
                let (program, _) = lowered.unwrap_or_else(|error| {
                    panic!("case {cases}/{hole}/{setting}: {error:?}\n{body}")
                });
                assert_eq!(verification.len(), 1);
                let wrapper = program.modules[0]
                    .functions
                    .iter()
                    .find(|f| f.name.contains(".wrap."))
                    .unwrap();
                assert_eq!(wrapper.blocks.len(), returns + 1 + flags);
                assert_eq!(wrapper.instructions.len(), returns + 1 + flags);
                let output = (
                    crate::ssa::render_program(&program),
                    crate::llvm::render_verified_program(&program).unwrap(),
                );
                if setting == "off" {
                    assert_eq!(previous.as_ref(), Some(&output));
                }
                previous = Some(output);
            }
        }
    }
    assert_eq!(cases, 65);
}
