//! Legal neighboring programs do not acquire single-continuation receipt authority.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    type_checking::UnitCallTarget,
};

#[test]
fn ordinary_single_continuation_neighbors_stay_unsupported() {
    let right = "if (f0) { return c0(source) } else { if (f1) { return c1(source) } else { if (f2) { return c2(source) } } }; return c3(source)";
    for (shape, body, count, flag_count) in [
        ("two-holes-extra-statement", "if (f0) { if (f1) { return c0(source) } } else { if (f2) { return c1(source) } }; f0; return c2(source)".to_owned(),3,3),
        ("extra-statement", right.replace("if (f2)","f2; if (f2)"),4,3),
        ("repeated-flag", right.replace("if (f2)","if (f1)"),4,3),
        ("wrong-flag-order",right.replace("if (f1)","if (SWAP)").replace("if (f2)","if (f1)").replace("if (SWAP)","if (f2)"),4,3),
        ("depth-four",right.replace("if (f2) { return c2(source) }","if (f2) { return c2(source) } else { if (f3) { return c4(source) } }"),5,4),
        ("explicit-empty-else",right.replace("if (f2) { return c2(source) }","if (f2) { return c2(source) } else {}"),4,3),
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
                    .all(|catalog| catalog.nested_forward_regions().is_empty()
                        && catalog.bounded_forward_regions().is_empty() && catalog.bounded_continuation_regions().is_empty())
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
            for (setting, owned) in [("on", &on), ("off", &off)] {
                let ((lowered, verification), receipts) =
                    crate::ssa::refine::single_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let error = lowered.err().unwrap_or_else(|| {
                    panic!("{shape}/{layout}/{setting}: unsupported neighboring tree admitted")
                });
                assert_eq!(
                    error.kind,
                    crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
                );
                assert_eq!(error.span, Some(marker), "{shape}/{layout}/{setting}");
                assert!(verification.is_empty());
                assert!(receipts.is_empty());
            }
        }
    }
}

/// Every bounded multi-origin tree retains actual multi-predecessor joins and one shared tail.
/// The independent single-survivor protocol must never publish duplicate authority.
#[test]
fn ordinary_multiple_hole_trees_require_shared_tail_contract() {
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
            let next = format!("{route}F");
            if matches!(right.as_ref(), Tree::Leaf) {
                out.push(next);
            } else {
                holes(right, &next, out);
            }
        }
    }
    fn source(
        tree: &Tree,
        route: &str,
        holes: &[String],
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
                let left = source(left, &format!("{route}T"), holes, flags, returns);
                let next = format!("{route}F");
                if holes.contains(&next) {
                    format!("if (f{flag}) {{ {left} }}")
                } else {
                    let right = source(right, &next, holes, flags, returns);
                    format!("if (f{flag}) {{ {left} }} else {{ {right} }}")
                }
            }
        }
    }
    fn live_joins(
        tree: &Tree,
        prefix: &str,
        origins: &[String],
        out: &mut Vec<(String, [Vec<usize>; 2])>,
    ) {
        if let Tree::Split(left, right) = tree {
            live_joins(left, &format!("{prefix}T"), origins, out);
            live_joins(right, &format!("{prefix}F"), origins, out);
            let sides = ['T', 'F'].map(|side| {
                let route = format!("{prefix}{side}");
                origins
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.starts_with(&route))
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>()
            });
            if sides.iter().any(|s| !s.is_empty()) {
                out.push((prefix.to_owned(), sides));
            }
        }
    }
    fn routes(
        tree: &Tree,
        prefix: &str,
        selected: &[String],
        tail: usize,
        ordinal: &mut usize,
        prefixes: &mut Vec<String>,
        out: &mut Vec<(String, usize)>,
    ) {
        match tree {
            Tree::Leaf => {
                let index = if selected.iter().any(|p| p == prefix) {
                    tail
                } else {
                    let i = *ordinal;
                    *ordinal += 1;
                    i
                };
                out.push((prefix.to_owned(), index));
            }
            Tree::Split(left, right) => {
                prefixes.push(prefix.to_owned());
                routes(
                    left,
                    &format!("{prefix}T"),
                    selected,
                    tail,
                    ordinal,
                    prefixes,
                    out,
                );
                routes(
                    right,
                    &format!("{prefix}F"),
                    selected,
                    tail,
                    ordinal,
                    prefixes,
                    out,
                );
            }
        }
    }
    let mut cases = 0;
    let mut saw_multi_predecessor_fold = false;
    for tree in trees(3) {
        let mut candidates = Vec::new();
        holes(&tree, "", &mut candidates);
        for mask in 0usize..(1 << candidates.len()) {
            if mask.count_ones() < 2 {
                continue;
            }
            let selected = candidates
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, route)| route.clone())
                .collect::<Vec<_>>();
            cases += 1;
            let (mut flags, mut returns) = (0, 0);
            let body = source(&tree, "", &selected, &mut flags, &mut returns);
            assert_eq!(returns + selected.len(), flags + 1);
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
            let mut prefixes = Vec::new();
            let mut route_specs = Vec::new();
            routes(
                &tree,
                "",
                &selected,
                returns,
                &mut 0,
                &mut prefixes,
                &mut route_specs,
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "multi-hole-trees.ko", &text);
            for (layout, input) in [
                (
                    "rooted",
                    SourceUnitInput::new("root", "multi-hole-trees.ko", source, &parsed),
                ),
                ("memory", SourceUnitInput::memory_document(source, &parsed)),
            ] {
                let inputs = [input];
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
                let regions = on
                    .ownership()
                    .loan_action_observations()
                    .iter()
                    .flat_map(|catalog| {
                        catalog
                            .bounded_multi_continuation_regions()
                            .iter()
                            .map(move |region| (catalog, region))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    regions.len(),
                    1,
                    "case {cases}/{layout}: complete multi-origin certificate"
                );
                let (catalog, region) = regions[0];
                assert!(catalog.owns_bounded_multi_continuation_region(region));
                assert_eq!(
                    region.coverage(),
                    lang_frontend::ownership_checking::UnitLoanActionCoverage::Complete
                );
                assert_eq!(region.split_count(), flags);
                assert_eq!(region.terminal_paths().len(), returns);
                assert_eq!(region.origins().len(), selected.len());
                for (origin, route) in region.origins().iter().zip(&selected) {
                    assert_eq!(
                        origin
                            .edges()
                            .iter()
                            .map(|edge| if edge.then_arm() { 'T' } else { 'F' })
                            .collect::<String>(),
                        *route
                    );
                }
                let mut expected_joins = Vec::new();
                live_joins(&tree, "", &selected, &mut expected_joins);
                assert_eq!(region.joins().len(), expected_joins.len());
                for (join, (prefix, incoming)) in region.joins().iter().zip(&expected_joins) {
                    assert_eq!(join.incoming(), incoming);
                    assert!(incoming[0].iter().all(|i| !incoming[1].contains(i)));
                    let union = incoming.iter().flatten().copied().collect::<Vec<_>>();
                    assert_eq!(join.origins(), union);
                    for index in &union {
                        let edge = &region.origins()[*index].edges()[prefix.len()];
                        assert_eq!(join.split(), edge.split());
                        assert_eq!(join.condition(), edge.condition());
                    }
                }
                let raw = on.ownership().borrow_forward_transports();
                assert_eq!(raw.len(), returns + 1, "case {cases}");
                assert!(raw.len() < flags + 1, "transactions are not control paths");
                let tail = raw.last().unwrap();
                let tail_call = tail.returned_expression();
                let shared = region.tail().expect("one physical shared-tail certificate");
                assert_eq!(shared.value(), tail_call);
                assert_eq!(shared.callable(), region.callable());
                assert_eq!(shared.source_parameter(), region.source_parameter());
                assert!(
                    matches!(catalog.action(&shared.source_begin()), Some(UnitLoanAction::Begin(loan)) if loan == tail.source_loan())
                );
                assert!(
                    matches!(catalog.action(&shared.handoff()), Some(UnitLoanAction::BorrowForwardHandoff(handoff)) if handoff.transaction() == tail)
                );
                let mut ids = Vec::new();
                for (path, transaction) in
                    region.terminal_paths().iter().zip(raw.iter().take(returns))
                {
                    assert_eq!(path.value(), transaction.returned_expression());
                    assert!(
                        matches!(catalog.action(&path.source_begin()), Some(UnitLoanAction::Begin(loan)) if loan == transaction.source_loan())
                    );
                    assert!(
                        matches!(catalog.action(&path.handoff()), Some(UnitLoanAction::BorrowForwardHandoff(handoff)) if handoff.transaction() == transaction)
                    );
                    for id in [path.source_begin(), path.handoff()] {
                        assert!(!ids.contains(&id));
                        ids.push(id);
                    }
                }
                for id in [shared.source_begin(), shared.handoff()] {
                    assert!(!ids.contains(&id));
                    ids.push(id);
                }
                assert_eq!(ids.len(), 2 * (returns + 1));

                let expected_tail = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == format!("c{returns}"))
                    .unwrap()
                    .id();
                assert_eq!(
                    typed.types().call(tail_call).unwrap().target(),
                    UnitCallTarget::Declaration(expected_tail),
                    "case {cases}: identify the physical shared tail independently"
                );
                let mut begins = 0;
                let mut handoffs = 0;
                for catalog in on.ownership().loan_action_observations() {
                    assert!(catalog.bounded_forward_regions().is_empty());
                    assert!(catalog.bounded_continuation_regions().is_empty());
                    assert!(catalog.nested_forward_regions().is_empty());
                    for action in catalog.actions() {
                        match action {
                            UnitLoanAction::Begin(loan) if loan.call() == tail_call => {
                                assert_eq!(loan, tail.source_loan());
                                begins += 1;
                            }
                            UnitLoanAction::BorrowForwardHandoff(handoff)
                                if handoff.transaction().returned_expression() == tail_call =>
                            {
                                assert_eq!(handoff.transaction(), tail);
                                handoffs += 1;
                            }
                            _ => {}
                        }
                    }
                }
                assert_eq!(
                    (begins, handoffs),
                    (1, 1),
                    "case {cases}: physical tail pair"
                );
                let declarations = names.names().index().declarations();
                let entry = declarations
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap()
                    .id();
                let mut previous = None;
                for (setting, owned) in [("on", &on), ("off", &off)] {
                    let ((((lowered, verification), receipts), bounded_receipts), multi_receipts) =
                        crate::ssa::refine::multi_receipt::observe(|| {
                            crate::ssa::refine::bounded_receipt::observe(|| {
                                crate::ssa::refine::single_receipt::observe(|| {
                                    crate::ssa::unit_lower::observe_final_verifier(|| {
                                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                            &sources, &inputs, &names, &types, &typed, owned, entry,
                                        )
                                    })
                                })
                            })
                        });
                    let (program, _) = lowered.unwrap_or_else(|error| {
                        panic!("case {cases}/{layout}/{setting}: {error:?}")
                    });
                    assert_eq!(verification.len(), 1);
                    assert!(receipts.is_empty());
                    assert!(bounded_receipts.is_empty());
                    let function = program.modules[0]
                        .functions
                        .iter()
                        .find(|f| f.name.contains(".wrap."))
                        .unwrap();
                    assert_eq!(function.instructions.len(), flags + returns + 1);
                    let entry_block = &function.blocks[0];
                    assert_eq!(entry_block.parameters.len(), flags + 1);
                    let entry_loans = entry_block
                        .parameters
                        .iter()
                        .map(|e| match e {
                            EntityId::Loan(id) => *id,
                            _ => panic!("loan ABI"),
                        })
                        .collect::<Vec<_>>();
                    let mapped = |edge: &Edge, id| {
                        let index = edge
                            .arguments
                            .iter()
                            .position(|e| *e == EntityId::Loan(id))?;
                        match function.block(edge.target)?.parameters.get(index)? {
                            EntityId::Loan(id) => Some(*id),
                            _ => None,
                        }
                    };
                    let mut actual_origins = Vec::new();
                    let mut tail_identity = None;
                    for (route, callee_index) in &route_specs {
                        let mut block = entry_block.id;
                        let mut source = entry_loans[0];
                        let mut flag_loans = entry_loans[1..]
                            .iter()
                            .copied()
                            .map(Some)
                            .collect::<Vec<_>>();
                        let mut edges = Vec::new();
                        for (depth, side) in route.bytes().enumerate() {
                            let index = prefixes.iter().position(|p| p == &route[..depth]).unwrap();
                            let data = function.block(block).unwrap();
                            let TerminatorKind::Conditional {
                                condition,
                                when_true,
                                when_false,
                            } = &data.terminator.as_ref().unwrap().kind
                            else {
                                panic!("real split")
                            };
                            assert_eq!(data.instructions.len(), 1);
                            let read = function.instruction(data.instructions[0]).unwrap();
                            assert_eq!(read.results, [EntityId::Value(*condition)]);
                            assert_eq!(
                                read.operation,
                                Operation::Read {
                                    source: PlaceAccess::Loan(flag_loans[index].unwrap())
                                }
                            );
                            let edge = if side == b'T' { when_true } else { when_false };
                            source = mapped(edge, source).unwrap();
                            for flag in &mut flag_loans {
                                *flag = flag.and_then(|id| mapped(edge, id));
                            }
                            block = edge.target;
                            edges.push(edge.clone());
                        }
                        let mut visited = std::collections::BTreeSet::new();
                        while let TerminatorKind::Branch(edge) = &function
                            .block(block)
                            .unwrap()
                            .terminator
                            .as_ref()
                            .unwrap()
                            .kind
                        {
                            assert!(visited.insert(block));
                            assert!(visited.len() <= flags);
                            assert!(function.block(block).unwrap().instructions.is_empty());
                            source = mapped(edge, source).unwrap();
                            block = edge.target;
                            edges.push(edge.clone());
                        }
                        let data = function.block(block).unwrap();
                        assert_eq!(data.parameters, [EntityId::Loan(source)]);
                        assert_eq!(data.instructions.len(), 1);
                        let call = function.instruction(data.instructions[0]).unwrap();
                        let callee = program.modules[0]
                            .functions
                            .iter()
                            .find(|f| f.name.contains(&format!(".c{callee_index}.")))
                            .unwrap()
                            .id();
                        assert_eq!(call.block, block);
                        assert_eq!(
                            call.operation,
                            Operation::BorrowCall {
                                callee,
                                arguments: vec![EntityId::Loan(source)],
                                source
                            }
                        );
                        let [EntityId::Loan(result)] = call.results.as_slice() else {
                            panic!("borrow result")
                        };
                        assert_eq!(
                            data.terminator.as_ref().unwrap().kind,
                            TerminatorKind::BorrowReturn { loan: *result }
                        );
                        if selected.contains(route) {
                            let actual = (block, call.id, source, *result, callee);
                            if let Some(previous) = tail_identity {
                                assert_eq!(previous, actual);
                            } else {
                                tail_identity = Some(actual);
                            }
                            actual_origins.push(edges);
                        }
                    }
                    let mut predecessors = std::collections::BTreeMap::new();
                    let mut actual_gotos = Vec::new();
                    for block in &function.blocks {
                        if let TerminatorKind::Branch(edge) =
                            &block.terminator.as_ref().unwrap().kind
                        {
                            *predecessors.entry(edge.target).or_insert(0usize) += 1;
                            actual_gotos.push((block.id, edge.clone()));
                        }
                    }
                    assert!(!actual_gotos.is_empty());
                    assert!(
                        actual_gotos
                            .iter()
                            .all(|(_, edge)| predecessors[&edge.target] >= 2)
                    );
                    let output = (
                        crate::ssa::render_program(&program),
                        crate::llvm::render_verified_program(&program).unwrap(),
                    );
                    if setting == "off" {
                        assert!(multi_receipts.is_empty());
                        assert_eq!(previous.as_ref(), Some(&output));
                        continue;
                    }
                    previous = Some(output);
                    let [receipt] = multi_receipts.as_slice() else {
                        panic!("one multi receipt")
                    };
                    saw_multi_predecessor_fold |= receipt.folded_multi_predecessor_successor();
                    assert_eq!(receipt.callable, region.callable());
                    assert_eq!(receipt.caller, function.id());
                    assert_eq!(receipt.entry, entry_block.id);
                    assert_eq!(receipt.source, entry_loans[0]);
                    assert_eq!(receipt.paths.len(), returns + 1);
                    assert_eq!(receipt.splits.len(), flags);
                    assert_eq!(receipt.origins, actual_origins);
                    assert_eq!(receipt.continuation_edges.len(), actual_gotos.len());
                    assert!(
                        receipt
                            .continuation_edges
                            .iter()
                            .all(|e| actual_gotos.contains(e))
                    );
                    let original_gotos = region
                        .joins()
                        .iter()
                        .map(|j| j.incoming().iter().filter(|s| !s.is_empty()).count())
                        .sum::<usize>();
                    assert_eq!(receipt.continuation_goto_count(), original_gotos);
                    assert_eq!(
                        receipt.continuation_fold_count() + actual_gotos.len(),
                        original_gotos
                    );
                    let tail = receipt.paths.last().unwrap();
                    let actual = tail_identity.unwrap();
                    assert_eq!(
                        (
                            tail.block,
                            tail.instruction,
                            tail.source,
                            tail.result,
                            tail.callee
                        ),
                        actual
                    );
                    assert!(tail.edges.is_empty());
                    assert!(receipt.tail_mapping_matches(actual.0));
                    assert_eq!(tail.ids, vec![shared.source_begin(), shared.handoff()]);
                    for (path, expected) in
                        receipt.paths[..returns].iter().zip(region.terminal_paths())
                    {
                        assert_eq!(path.control, expected.control());
                        assert_eq!(path.value, expected.value());
                        assert_eq!(path.ids, vec![expected.source_begin(), expected.handoff()]);
                    }
                }
            }
        }
    }
    assert_eq!(cases, 92);
    assert!(
        saw_multi_predecessor_fold,
        "actual multi-predecessor join absorbs a single-predecessor successor"
    );
}
