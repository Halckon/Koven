//! Dynamic bounded terminal trees require real backend lowering before receipt acceptance.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    type_checking::UnitCallTarget,
};

#[test]
fn ordinary_dynamic_terminal_trees_require_actual_receipt() {
    for (shape, body, count, flag_count, routes) in [
        (
            "depth-three-chain",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { return c2(source) } } else { return c3(source) }",
            4,
            3,
            vec!["TTT", "TTF", "TF", "F"],
        ),
        (
            "mixed-five-leaf",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { return c2(source) } } else { if (f3) { return c3(source) } else { return c4(source) } }",
            5,
            4,
            vec!["TTT", "TTF", "TF", "FT", "FF"],
        ),
        (
            "full-eight-leaf",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { if (f3) { return c2(source) } else { return c3(source) } } } else { if (f4) { if (f5) { return c4(source) } else { return c5(source) } } else { if (f6) { return c6(source) } else { return c7(source) } } }",
            8,
            7,
            vec!["TTT", "TTF", "TFT", "TFF", "FTT", "FTF", "FFT", "FFF"],
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
            "{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}\nfun consume(item: String): Unit {{}}\nfun run({params}): Unit {{ val owner = \"kept\"; borrow val item = wrap(owner, {args}); consume(item) }}"
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
            let [region] = catalog.bounded_forward_regions() else {
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
                    crate::ssa::refine::bounded_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let (program, _) = lowered.unwrap_or_else(|error| {
                    panic!("{shape}/{layout}/{setting}: actual bounded tree lowering {error:?}")
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
