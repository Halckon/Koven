//! Fixed three-leaf actual producer receipt and the adjacent unsupported four-leaf frontier.
use super::*;
use lang_frontend::ownership_checking::{
    UnitBorrowNestedForwardTopology, check_compilation_unit_ownership, legacy_ownership_eq,
    with_loan_observation,
};
fn nested_three_leaf_source() -> &'static str {
    "fun left(source: String): borrow String from source = source\nfun middle(source: String): borrow String from source = source\nfun right(source: String): borrow String from source = source\nfun wrap(source: String, outer: Boolean, inner: Boolean): borrow String from source { if (outer) { return left(source) } else { if (inner) { return middle(source) } else { return right(source) } } }\nfun consume(item: String): Unit {}\nfun run(outer: Boolean, inner: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, outer, inner); consume(item) }"
}

#[test]
fn ordinary_mirror_three_leaf_requires_actual_receipt() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "three-leaf.ko", nested_three_leaf_source());
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "three-leaf.ko", source, &parsed),
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
        assert!(off.ownership().loan_action_observations().is_empty());
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(on.ownership().borrow_forward_transports().len(), 3);
        let region = catalog.nested_forward_regions().iter().find(|r| {
            r.coverage() == UnitLoanActionCoverage::Complete
                && catalog.owns_nested_forward_region(r)
        });
        let expected = region.map(|r| r.paths().to_vec()).unwrap_or_default();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            let ((lowered, verification), receipts) =
                crate::ssa::refine::nested_receipt::observe(|| {
                    crate::ssa::unit_lower::observe_final_verifier(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, owned, entry,
                        )
                    })
                });
            println!(
                "{layout}/{setting}: lowering={:?}, verifier={}, receipts={}",
                lowered.as_ref().map(|_| ()),
                verification.len(),
                receipts.len()
            );
            completed.push((
                layout,
                setting,
                lowered,
                verification,
                receipts,
                expected.clone(),
                region.map(|r| r.callable()),
            ));
        }
    }
    let mut previous = None;
    for (layout, setting, lowered, verification, receipts, expected, callable) in completed {
        let (program, _) = lowered
            .unwrap_or_else(|e| panic!("{layout}/{setting}: real three-leaf lowering {e:?}"));
        assert_eq!(verification.len(), 1);
        verify_program(&program).unwrap();
        let module = &program.modules[0];
        let wrapper = module
            .functions
            .iter()
            .find(|f| f.name.contains(".wrap."))
            .unwrap();
        let callees = [".left.", ".middle.", ".right."].map(|name| {
            module
                .functions
                .iter()
                .find(|f| f.name.contains(name))
                .unwrap()
                .id()
        });
        assert_ne!(callees[0], callees[1]);
        assert_ne!(callees[1], callees[2]);
        assert_ne!(callees[0], callees[2]);
        assert_eq!(
            wrapper.blocks.len(),
            5,
            "two splits and three terminal leaves only"
        );
        assert_eq!(
            wrapper.instructions.len(),
            5,
            "two reads and three borrowed calls only"
        );
        let [
            EntityId::Loan(source),
            EntityId::Loan(outer),
            EntityId::Loan(inner),
        ] = wrapper.blocks[0].parameters.as_slice()
        else {
            panic!("borrowed entry ABI")
        };
        let branch = |block| {
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &wrapper
                .block(block)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind
            else {
                panic!("actual split")
            };
            (*condition, [when_true.clone(), when_false.clone()])
        };
        let mapped = |edge: &Edge, loan: LoanId| {
            let index = edge
                .arguments
                .iter()
                .position(|arg| *arg == EntityId::Loan(loan))
                .unwrap();
            let EntityId::Loan(mapped) = wrapper.block(edge.target).unwrap().parameters[index]
            else {
                panic!("borrowed edge")
            };
            mapped
        };
        let (outer_value, outer_edges) = branch(wrapper.blocks[0].id);
        let inner_block = outer_edges[1].target;
        let inner_source = mapped(&outer_edges[1], *source);
        let inner_flag = mapped(&outer_edges[1], *inner);
        let (inner_value, inner_edges) = branch(inner_block);
        let read = |value, flag| {
            let instruction = wrapper
                .instructions
                .iter()
                .find(|i| i.results == [EntityId::Value(value)])
                .unwrap();
            assert_eq!(
                instruction.operation,
                Operation::Read {
                    source: PlaceAccess::Loan(flag)
                }
            );
            instruction.id
        };
        let reads = [read(outer_value, *outer), read(inner_value, inner_flag)];
        let routes = [
            vec![outer_edges[0].clone()],
            vec![outer_edges[1].clone(), inner_edges[0].clone()],
            vec![outer_edges[1].clone(), inner_edges[1].clone()],
        ];
        let mut actual = Vec::new();
        for (i, route) in routes.iter().enumerate() {
            let edge = route.last().unwrap();
            let path_source = mapped(edge, if i > 0 { inner_source } else { *source });
            let block = wrapper.block(edge.target).unwrap();
            let TerminatorKind::BorrowReturn { loan: result } =
                block.terminator.as_ref().unwrap().kind
            else {
                panic!("terminal leaf")
            };
            let call = wrapper
                .instructions
                .iter()
                .find(|i| i.results == [EntityId::Loan(result)])
                .unwrap();
            assert_eq!(
                call.operation,
                Operation::BorrowCall {
                    callee: callees[i],
                    arguments: vec![EntityId::Loan(path_source)],
                    source: path_source
                }
            );
            actual.push((block.id, call.id, path_source, result));
        }
        assert!(!wrapper.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::BorrowBegin { .. } | Operation::BorrowEnd { .. } | Operation::Drop { .. }
        )));
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
            panic!("one independently produced nested receipt")
        };
        assert_eq!(Some(receipt.callable), callable);
        assert_eq!(expected.len(), 3);
        assert_eq!(
            receipt.topology,
            UnitBorrowNestedForwardTopology::NestedElse
        );
        assert_eq!(receipt.caller, wrapper.id());
        assert_eq!(receipt.entry, wrapper.blocks[0].id);
        assert_eq!(receipt.source, *source);
        assert_eq!(receipt.splits.len(), 2);
        assert_eq!(receipt.paths.len(), 3);
        for (i, split) in receipt.splits.iter().enumerate() {
            let edge = &expected[1].edges()[i];
            assert_eq!(split.split, edge.split());
            assert_eq!(split.condition, edge.condition());
            assert_eq!(
                split.block,
                if i == 0 {
                    wrapper.blocks[0].id
                } else {
                    inner_block
                }
            );
            assert_eq!(split.flag, if i == 0 { *outer } else { inner_flag });
            assert_eq!(split.read, reads[i]);
            assert_eq!(split.value, if i == 0 { outer_value } else { inner_value });
            assert_eq!(
                split.edges,
                if i == 0 {
                    outer_edges.clone()
                } else {
                    inner_edges.clone()
                }
            );
        }
        for (i, path) in receipt.paths.iter().enumerate() {
            assert_eq!(path.control, expected[i].control());
            assert_eq!(path.value, expected[i].value());
            assert_eq!(
                path.ids,
                vec![expected[i].source_begin(), expected[i].handoff()]
            );
            assert_eq!(
                (path.block, path.instruction, path.source, path.result),
                actual[i]
            );
            assert_eq!(path.callee, callees[i]);
            assert_eq!(path.edges, routes[i]);
        }
    }
}
