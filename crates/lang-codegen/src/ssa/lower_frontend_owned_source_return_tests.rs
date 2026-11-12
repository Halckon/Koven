//! SPEC-0314: independently named ordinary-source baseline cells.
use super::{analyze, lower_scalar_file};
mod structure {
    //! Exact ordinary-source SSA lineage; only transparent block parameters are followed.
    use crate::ssa::model::{
        BinaryOperator, BlockId, Definition, Edge, EntityId, Function, Operation, Origin, Program,
        ScalarConstant, TerminatorKind,
    };
    fn edges(function: &Function) -> Vec<(BlockId, &Edge)> {
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
                    } => vec![when_true, when_false],
                    _ => Vec::new(),
                };
                edges.into_iter().map(move |edge| (block.id, edge))
            })
            .collect()
    }
    fn forwards(function: &Function, value: EntityId, expected: EntityId, _depth: usize) -> bool {
        fn check(
            function: &Function,
            value: EntityId,
            expected: EntityId,
            visited: &mut std::collections::BTreeSet<EntityId>,
            found: &mut bool,
        ) -> bool {
            if value == expected {
                *found = true;
                return true;
            }
            if !visited.insert(value) {
                return true;
            }
            let Some(ed) = function.entity(value) else {
                return false;
            };
            let Definition::BlockParameter { block, index } = ed.definition else {
                return false;
            };
            let incoming = edges(function)
                .into_iter()
                .filter(|(_, edge)| edge.target == block)
                .collect::<Vec<_>>();
            !incoming.is_empty()
                && incoming
                    .iter()
                    .all(|(_, edge)| check(function, edge.arguments[index], expected, visited, found))
        }
        let mut visited = std::collections::BTreeSet::new();
        let mut found = false;
        check(function, value, expected, &mut visited, &mut found) && found
    }
    pub(super) fn check(
        program: &Program,
        live: bool,
        seed_span: Option<lang_frontend::source::Span>,
        return_span: lang_frontend::source::Span,
        map: bool,
    ) {
        let functions = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| {
                f.blocks
                    .iter()
                    .any(|b| b.terminator.as_ref().unwrap().origin == Origin::Source(return_span))
            })
            .collect::<Vec<_>>();
        assert_eq!(functions.len(), 1);
        let f = functions[0];
        let seed = if let Some(span) = seed_span {
            let seeds = f
                .instructions
                .iter()
                .filter(|i| {
                    i.origin == Origin::Source(span)
                        && matches!(i.operation, Operation::ContainerConstruct { .. })
                })
                .collect::<Vec<_>>();
            assert_eq!(seeds.len(), 1, "the exact frontend declaration initializer");
            seeds[0].results[0]
        } else {
            // These ordinary-source fixtures declare own keys as the first parameter.
            let entry = &f.blocks[0];
            assert!(!entry.parameters.is_empty());
            let seed = entry.parameters[0];
            assert!(
                matches!(f.entity(seed).unwrap().definition, Definition::BlockParameter { block, index: 0 } if block == entry.id)
            );
            seed
        };
        let all_edges = edges(f);
        let entries = all_edges
            .iter()
            .filter_map(|(block, edge)| {
                edge.arguments
                    .iter()
                    .position(|arg| *arg == seed)
                    .map(|slot| (*block, *edge, slot))
            })
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        let (preheader, entry, owner_slot) = entries[0];
        let header = f.block(entry.target).unwrap();
        let lengths = f
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::ContainerLength { .. }))
            .collect::<Vec<_>>();
        let root_place = f.instructions.iter().find(|i| matches!(i.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == seed)).unwrap();
        let source_begin = f.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == root_place.results[0])).unwrap();
        let source = source_begin.results[0];
        let snapshots = lengths
            .iter()
            .filter(|i| matches!(i.operation, Operation::ContainerLength { owner } if owner == source))
            .collect::<Vec<_>>();
        assert_eq!(
            snapshots.len(),
            1,
            "one length snapshot for the actual initial source loan"
        );
        let snapshot = snapshots[0];
        assert_eq!(snapshot.block, preheader);
        let source_slot = entry
            .arguments
            .iter()
            .position(|arg| *arg == source)
            .unwrap();
        let length_slot = entry
            .arguments
            .iter()
            .position(|arg| *arg == snapshot.results[0])
            .unwrap();
        let zero = f
            .instructions
            .iter()
            .find(|i| {
                i.block == preheader
                    && matches!(i.operation, Operation::Constant(ScalarConstant::Integer(0)))
            })
            .unwrap()
            .results[0];
        let cursor_slot = entry.arguments.iter().position(|arg| *arg == zero).unwrap();
        let one = f
            .instructions
            .iter()
            .find(|i| {
                i.block == preheader
                    && i.id.index() > snapshot.id.index()
                    && matches!(i.operation, Operation::Constant(ScalarConstant::Integer(1)))
            })
            .unwrap()
            .results[0];
        let root_place = f.instructions.iter().find(|i| matches!(i.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == seed)).unwrap();
        let source_begin = f
            .instructions
            .iter()
            .find(|i| i.results.contains(&source))
            .unwrap();
        assert!(
            matches!(source_begin.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == root_place.results[0])
        );
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &header.terminator.as_ref().unwrap().kind
        else {
            panic!("provider guard")
        };
        let body = f.block(when_true.target).unwrap();
        let exhausted = f.block(when_false.target).unwrap();
        let body_slot = |slot| {
            when_true
                .arguments
                .iter()
                .position(|arg| *arg == header.parameters[slot])
                .unwrap()
        };
        let exhausted_slot = |slot| {
            when_false
                .arguments
                .iter()
                .position(|arg| *arg == header.parameters[slot])
                .unwrap()
        };
        let body_owner = body.parameters[body_slot(owner_slot)];
        let body_source = body.parameters[body_slot(source_slot)];
        let body_cursor = body.parameters[body_slot(cursor_slot)];
        let exhausted_owner = exhausted.parameters[exhausted_slot(owner_slot)];
        let exhausted_source = exhausted.parameters[exhausted_slot(source_slot)];
        let place = f
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::ContainerElementPlace { owner, .. } if owner == body_source))
            .unwrap();
        assert!(
            matches!(place.operation, Operation::ContainerElementPlace { owner, index } if owner == body_source && EntityId::Value(index) == body_cursor)
        );
        let element = f.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place: p, .. } if EntityId::Place(p) == place.results[0])).unwrap().results[0];
        let back = all_edges
            .iter()
            .filter(|(block, edge)| edge.target == header.id && *block != preheader)
            .collect::<Vec<_>>();
        assert_eq!(
            back.len(),
            1,
            "only Fallthrough transports; Return has no incoming"
        );
        for (block, edge) in back {
            for (slot, expected) in [(owner_slot, body_owner), (source_slot, body_source)] {
                assert!(forwards(f, edge.arguments[slot], expected, 0));
            }
            assert!(
                forwards(
                    f,
                    edge.arguments[length_slot],
                    header.parameters[length_slot],
                    0
                ),
                "same immutable length snapshot"
            );
            let advance = f
                .instructions
                .iter()
                .find(|i| i.results.contains(&edge.arguments[cursor_slot]))
                .unwrap();
            assert_eq!(advance.block, *block);
            assert!(
                matches!(advance.operation, Operation::Binary { operator: BinaryOperator::Add, left, right }
                if forwards(f, EntityId::Value(left), body_cursor, 0) && forwards(f, EntityId::Value(right), one, 0))
            );
            let ends = f
                .instructions
                .iter()
                .filter(|i| i.block == *block && matches!(i.operation, Operation::BorrowEnd { .. }))
                .collect::<Vec<_>>();
            assert!(!ends.iter().any(|i| matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), body_source, 0))), "source loan survives the sole backedge");
            let ends = ends.into_iter().filter(|i| matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), element, 0))).collect::<Vec<_>>();
            assert_eq!(
                ends.len(),
                1,
                "the actual element ends once on each iteration path"
            );
            let ids = &f.block(*block).unwrap().instructions;
            assert!(
                ids.iter().position(|id| *id == ends[0].id).unwrap()
                    < ids.iter().position(|id| *id == advance.id).unwrap()
            );
        }
        let returned = f
            .blocks
            .iter()
            .find(|b| b.terminator.as_ref().unwrap().origin == Origin::Source(return_span))
            .unwrap();
        assert!(matches!(
            returned.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { .. }
        ));
        assert!(
            !all_edges.iter().any(|(block, _)| *block == returned.id),
            "Return cannot reach Header or Exit"
        );
        let return_end = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), body_source, 0))).collect::<Vec<_>>();
        let element_end = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), element, 0))).collect::<Vec<_>>();
        let return_drop = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::Drop { owner } if forwards(f, EntityId::Value(owner), body_owner, 0))).collect::<Vec<_>>();
        assert_eq!(return_end.len(), 1);
        assert_eq!(element_end.len(), 1);
        assert_eq!(
            return_drop.len(),
            1,
            "the actual source owner, never a same-type decoy"
        );
        let index = |i: &crate::ssa::model::Instruction| {
            returned
                .instructions
                .iter()
                .position(|id| *id == i.id)
                .unwrap()
        };
        assert!(index(element_end[0]) < index(return_end[0]));
        assert!(index(return_end[0]) < index(return_drop[0]));
        if map {
            let put = f
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                .collect::<Vec<_>>();
            assert_eq!(put.len(), 1);
            let drops = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::Drop { owner } if forwards(f, EntityId::Value(owner), put[0].results[0], 0))).collect::<Vec<_>>();
            assert_eq!(drops.len(), 1, "Return consumes the latest MapPut result");
        }
        let source_end = f.instructions.iter().filter(|i| i.block == exhausted.id && matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == exhausted_source)).collect::<Vec<_>>();
        assert_eq!(source_end.len(), 1);
        let drops = f.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop { owner } if forwards(f, EntityId::Value(owner), exhausted_owner, 0))).collect::<Vec<_>>();
        assert_eq!(drops.len(), 1, "exact exhausted source owner consumed once");
        if live {
            let post = lengths.iter().find(|i| i.block == drops[0].block).unwrap();
            let Operation::ContainerLength { owner: post_loan } = post.operation else {
                unreachable!()
            };
            let begin = f
                .instructions
                .iter()
                .find(|i| i.results.contains(&post_loan))
                .unwrap();
            let Operation::BorrowBegin { place, .. } = begin.operation else {
                panic!("post-use source loan")
            };
            let root = f
                .instructions
                .iter()
                .find(|i| i.results.contains(&EntityId::Place(place)))
                .unwrap();
            assert!(
                matches!(root.operation, Operation::RootPlace { owner } if forwards(f, EntityId::Value(owner), exhausted_owner, 0))
            );
            assert_eq!(post.block, drops[0].block);
            let ids = &f.block(post.block).unwrap().instructions;
            assert!(
                ids.iter().position(|id| *id == post.id).unwrap()
                    < ids.iter().position(|id| *id == drops[0].id).unwrap()
            );
        } else {
            assert_eq!(drops[0].block, exhausted.id);
            let ids = &exhausted.instructions;
            assert!(
                ids.iter().position(|id| *id == source_end[0].id).unwrap()
                    < ids.iter().position(|id| *id == drops[0].id).unwrap()
            );
        }
    }
}
#[path = "lower_frontend_owned_source_unit_contract_tests.rs"]
pub(super) mod unit_contract;
use lang_frontend::type_checking::SequentialContainerKind;
use unit_contract::{Control, Expected};

fn source(factory: &str, ty: &str, own: bool, live: bool, map: bool) -> String {
    let parameter = if own {
        format!("own keys: {ty}<Int>")
    } else {
        String::new()
    };
    let seed = if own {
        String::new()
    } else {
        format!("val keys = {factory}(1, 2, 3);")
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
    format!(
        "fun work({parameter}): Unit {{ {seed} {before} for (key in keys) {{ {update} if (key == 2) {{ return }}; println(\"tail\") }}; {tail} {map_tail} }}"
    )
}

fn single(factory: &str, ty: &str, own: bool, live: bool, map: bool) {
    let text = source(factory, ty, own, live, map);
    original_text(&text, "work", ty, own, live, map);
}

fn single_text(text: &str, live: bool, map: bool) {
    original_text(text, "main", "Array", false, live, map);
}

fn original_text(text: &str, root: &str, ty: &str, own: bool, live: bool, map: bool) {
    let container = match ty {
        "Array" => SequentialContainerKind::Array,
        "List" => SequentialContainerKind::List,
        "MutableList" => SequentialContainerKind::MutableList,
        _ => panic!("original owned-source type"),
    };
    let (program, origins) = unit_contract::observe(
        text,
        root,
        &[Expected {
            function: root,
            live,
            container,
            own,
        }],
        Control::Return,
        map,
    );
    let [origin] = origins.as_slice() else {
        panic!("one original provider");
    };
    structure::check(&program, live, origin.seed, origin.control, map);
}
fn unit(factory: &str, ty: &str, own: bool, live: bool, constants: bool) {
    use crate::ssa::unit_lower_test_support::{declaration, parsed};
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        },
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let body = source(factory, ty, own, live, false);
    let argument = if own {
        format!("{factory}(1, 2, 3)")
    } else {
        String::new()
    };
    let text = format!("package p\n{body}\nfun main(): Unit {{ work({argument}) }}");
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source_id, &file)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let entry = declaration(&names, "p", "main");
    let result = if constants {
        let typed = result.validate_constants().unwrap();
        let owned =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let typed = result.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    let (program, entry) =
        result.unwrap_or_else(|error| panic!("unit constants={constants}, live={live}: {error:?}"));
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}

#[test]
fn owned_source_return_single_array_local_dead_no_map() {
    single("arrayOf", "Array", false, false, false);
}

#[test]
fn owned_source_return_single_array_local_dead_map() {
    single("arrayOf", "Array", false, false, true);
}

#[test]
fn owned_source_return_single_array_local_live_no_map() {
    single("arrayOf", "Array", false, true, false);
}

#[test]
fn owned_source_return_single_array_local_live_map() {
    single("arrayOf", "Array", false, true, true);
}

#[test]
fn owned_source_return_single_array_own_dead_no_map() {
    single("arrayOf", "Array", true, false, false);
}

#[test]
fn owned_source_return_single_array_own_dead_map() {
    single("arrayOf", "Array", true, false, true);
}

#[test]
fn owned_source_return_single_array_own_live_no_map() {
    single("arrayOf", "Array", true, true, false);
}

#[test]
fn owned_source_return_single_array_own_live_map() {
    single("arrayOf", "Array", true, true, true);
}

#[test]
fn owned_source_return_single_list_local_dead_no_map() {
    single("listOf", "List", false, false, false);
}

#[test]
fn owned_source_return_single_list_local_dead_map() {
    single("listOf", "List", false, false, true);
}

#[test]
fn owned_source_return_single_list_local_live_no_map() {
    single("listOf", "List", false, true, false);
}

#[test]
fn owned_source_return_single_list_local_live_map() {
    single("listOf", "List", false, true, true);
}

#[test]
fn owned_source_return_single_list_own_dead_no_map() {
    single("listOf", "List", true, false, false);
}

#[test]
fn owned_source_return_single_list_own_dead_map() {
    single("listOf", "List", true, false, true);
}

#[test]
fn owned_source_return_single_list_own_live_no_map() {
    single("listOf", "List", true, true, false);
}

#[test]
fn owned_source_return_single_list_own_live_map() {
    single("listOf", "List", true, true, true);
}

#[test]
fn owned_source_return_single_mutable_list_local_dead_no_map() {
    single("mutableListOf", "MutableList", false, false, false);
}

#[test]
fn owned_source_return_single_mutable_list_local_dead_map() {
    single("mutableListOf", "MutableList", false, false, true);
}

#[test]
fn owned_source_return_single_mutable_list_local_live_no_map() {
    single("mutableListOf", "MutableList", false, true, false);
}

#[test]
fn owned_source_return_single_mutable_list_local_live_map() {
    single("mutableListOf", "MutableList", false, true, true);
}

#[test]
fn owned_source_return_single_mutable_list_own_dead_no_map() {
    single("mutableListOf", "MutableList", true, false, false);
}

#[test]
fn owned_source_return_single_mutable_list_own_dead_map() {
    single("mutableListOf", "MutableList", true, false, true);
}

#[test]
fn owned_source_return_single_mutable_list_own_live_no_map() {
    single("mutableListOf", "MutableList", true, true, false);
}

#[test]
fn owned_source_return_single_mutable_list_own_live_map() {
    single("mutableListOf", "MutableList", true, true, true);
}

#[test]
fn owned_source_return_unit_array_local_dead_basic() {
    unit("arrayOf", "Array", false, false, false);
}

#[test]
fn owned_source_return_unit_array_local_dead_constant() {
    unit("arrayOf", "Array", false, false, true);
}

#[test]
fn owned_source_return_unit_array_local_live_basic() {
    unit("arrayOf", "Array", false, true, false);
}

#[test]
fn owned_source_return_unit_array_local_live_constant() {
    unit("arrayOf", "Array", false, true, true);
}

#[test]
fn owned_source_return_unit_array_own_dead_basic() {
    unit("arrayOf", "Array", true, false, false);
}

#[test]
fn owned_source_return_unit_array_own_dead_constant() {
    unit("arrayOf", "Array", true, false, true);
}

#[test]
fn owned_source_return_unit_array_own_live_basic() {
    unit("arrayOf", "Array", true, true, false);
}

#[test]
fn owned_source_return_unit_array_own_live_constant() {
    unit("arrayOf", "Array", true, true, true);
}

#[test]
fn owned_source_return_unit_list_local_dead_basic() {
    unit("listOf", "List", false, false, false);
}

#[test]
fn owned_source_return_unit_list_local_dead_constant() {
    unit("listOf", "List", false, false, true);
}

#[test]
fn owned_source_return_unit_list_local_live_basic() {
    unit("listOf", "List", false, true, false);
}

#[test]
fn owned_source_return_unit_list_local_live_constant() {
    unit("listOf", "List", false, true, true);
}

#[test]
fn owned_source_return_unit_list_own_dead_basic() {
    unit("listOf", "List", true, false, false);
}

#[test]
fn owned_source_return_unit_list_own_dead_constant() {
    unit("listOf", "List", true, false, true);
}

#[test]
fn owned_source_return_unit_list_own_live_basic() {
    unit("listOf", "List", true, true, false);
}

#[test]
fn owned_source_return_unit_list_own_live_constant() {
    unit("listOf", "List", true, true, true);
}

#[test]
fn owned_source_return_unit_mutable_list_local_dead_basic() {
    unit("mutableListOf", "MutableList", false, false, false);
}

#[test]
fn owned_source_return_unit_mutable_list_local_dead_constant() {
    unit("mutableListOf", "MutableList", false, false, true);
}

#[test]
fn owned_source_return_unit_mutable_list_local_live_basic() {
    unit("mutableListOf", "MutableList", false, true, false);
}

#[test]
fn owned_source_return_unit_mutable_list_local_live_constant() {
    unit("mutableListOf", "MutableList", false, true, true);
}

#[test]
fn owned_source_return_unit_mutable_list_own_dead_basic() {
    unit("mutableListOf", "MutableList", true, false, false);
}

#[test]
fn owned_source_return_unit_mutable_list_own_dead_constant() {
    unit("mutableListOf", "MutableList", true, false, true);
}

#[test]
fn owned_source_return_unit_mutable_list_own_live_basic() {
    unit("mutableListOf", "MutableList", true, true, false);
}

#[test]
fn owned_source_return_unit_mutable_list_own_live_constant() {
    unit("mutableListOf", "MutableList", true, true, true);
}

#[path = "lower_frontend_owned_source_return_native_tests.rs"]
pub(in crate::ssa) mod native;

mod boundaries {
    //! Ordinary-source boundaries kept separate from the core acceptance matrix.
    use super::{analyze, lower_scalar_file};
    use crate::test_support::diagnostics::diagnostic;

    fn observe(text: &str, supported: bool, keys_start: usize, expected_loops: usize) {
        let a = analyze(text);
        assert!(
            a.parsed.diagnostics().is_empty(),
            "{:?}",
            a.parsed.diagnostics()
        );
        assert!(
            a.names.diagnostics().is_empty(),
            "{:?}",
            a.names.diagnostics()
        );
        assert!(
            a.typed.diagnostics().is_empty(),
            "{:?}",
            a.typed.diagnostics()
        );
        assert!(
            a.owned.diagnostics().is_empty(),
            "{:?}",
            a.owned.diagnostics()
        );
        let result = lower_scalar_file(&a.sources, &a.parsed, &a.names, &a.typed, &a.owned);
        if supported {
            crate::llvm::render_verified_program(&result.unwrap()).unwrap();
        } else {
            let error = result
                .err()
                .expect("bounded source proof remains unsupported");
            let source = a.parsed.source_id();
            assert_eq!(a.sources.source_name(source), Ok("lowering.ko"));
            assert_eq!(a.sources.source_text(source), Ok(text));
            let expected_span = a.sources.span(source, keys_start, keys_start + 4).unwrap();
            assert_eq!(a.sources.slice(expected_span), Ok("keys"));
            assert_eq!(
                error,
                crate::ssa::LoweringError {
                    kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
                    span: Some(expected_span),
                },
                "the complete original Legacy boundary stays at its declaration"
            );
            diagnostic!("BOUNDARY {text}: {error:?}");
        }

        // Unit already accepts these complete original sources. Preserve the distinct
        // Legacy rejection above rather than treating this as consumer retirement.
        use crate::{
            native_tests::boxed_enum_tests::unit_source::UnitSource,
            ssa::model::{Operation, Origin},
        };
        use lang_frontend::{
            name_resolution::{SourceUnitKey, SymbolKind},
            parser::Statement,
        };

        let unit =
            UnitSource::analyze("lowering.ko", text).expect("strict unchanged return-boundary source");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        let [document] = unit.names().names().index().source_units() else {
            panic!("one original MemoryDocument");
        };
        let source = unit.parsed().source_id();
        assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(document.source_id(), source);
        assert_eq!(unit.sources().source_name(source), Ok("lowering.ko"));
        assert_eq!(unit.sources().source_text(source), Ok(text));
        assert_eq!(
            unit.basic_types().types().sequential_iterations().len(),
            expected_loops
        );
        assert_eq!(unit.ownership().iterations().len(), expected_loops);
        let declaration = unit.declaration("main", SymbolKind::Function);
        let main_span = unit.declaration_span(declaration);
        assert_eq!(main_span.source_id(), source);
        let mut statements = Vec::new();
        for plan in unit.ownership().iterations() {
            let descriptor = plan.descriptor();
            let statement = descriptor.statement();
            assert_eq!(statement.source_unit(), document.id());
            assert_eq!(descriptor.source().source_unit(), document.id());
            assert!(!statements.contains(&statement));
            statements.push(statement);
            assert_eq!(
                unit.types().sequential_iteration(statement),
                Some(descriptor)
            );
            let node = unit
                .parsed()
                .ast()
                .statements()
                .get(statement.statement())
                .unwrap();
            assert_eq!(node.span().source_id(), source);
            assert!(main_span.start() <= node.span().start() && node.span().end() <= main_span.end());
            let Statement::For { source, .. } = node.payload() else {
                panic!("a real original For, not a substitute provider");
            };
            assert_eq!(descriptor.source().expression(), *source);
        }
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("the complete original return boundary already lowers through Unit");
        let [module] = program.modules.as_slice() else {
            panic!("one original source module");
        };
        let roots = module
            .functions
            .iter()
            .filter(|function| function.origin == Origin::Source(main_span))
            .collect::<Vec<_>>();
        let [function] = roots.as_slice() else {
            panic!("exactly the original main root");
        };
        assert_eq!(function.id(), entry);
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                ))
                .count(),
            expected_loops
        );
        crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("the actual main and all original loop element accesses verify through LLVM");
    }

    #[test]
    fn owned_source_return_group_and_same_type_decoy() {
        for live in [false, true] {
            let text = super::source("arrayOf", "Array", false, live, true)
                .replace(
                    "val keys = arrayOf(1, 2, 3)",
                    "var keys = ((arrayOf(1, 2, 3)))",
                )
                .replace("if (key == 2)", "val decoy = arrayOf(7, 8); if (key == 2)")
                .replace("println(\"tail\")", "decoy.size; println(\"tail\")");
            super::original_text(&text, "work", "Array", false, live, true);
        }
    }

    #[test]
    fn owned_source_return_two_returns_remain_bounded() {
        observe(
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 1) { return }; if (key == 2) { return } }; keys.size }",
            false,
            23,
            1,
        );
    }
    #[test]
    fn owned_source_return_mixed_continue_remains_bounded() {
        observe(
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 1) { return }; if (key == 2) { continue } }; keys.size }",
            false,
            23,
            1,
        );
    }
    #[test]
    fn owned_source_return_replacement_seed_remains_bounded() {
        observe(
            "fun main(): Unit { var keys = arrayOf(1, 2); keys = arrayOf(2, 3); for (key in keys) { if (key == 2) { return } }; keys.size }",
            false,
            23,
            1,
        );
    }
    #[test]
    fn owned_source_return_alias_seed_remains_bounded() {
        observe(
            "fun main(): Unit { val original = arrayOf(1, 2); val keys = original; for (key in keys) { if (key == 2) { return } }; keys.size }",
            false,
            53,
            1,
        );
    }
    #[test]
    fn owned_source_return_resource_source_remains_bounded() {
        observe(
            "class Item(val id: Int) { deinit() {} }\nfun main(): Unit { val keys = arrayOf(Item(1), Item(2)); for (key in keys) { if (key.id == 2) { return } }; keys.size }",
            false,
            63,
            1,
        );
    }
    #[test]
    fn owned_source_return_fresh_inner_instance() {
        super::single_text(
            "fun main(): Unit { for (outer in arrayOf(1, 2)) { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { return } }; keys.size } }",
            true,
            false,
        );
    }
    #[test]
    fn owned_source_return_outer_owner_with_inner_loop() {
        observe(
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { for (other in arrayOf(3)) { other }; if (key == 2) { return } }; keys.size }",
            false,
            23,
            2,
        );
    }
    #[test]
    fn owned_source_return_same_source_nested() {
        observe(
            "fun main(): Unit { val keys = arrayOf(1, 2); for (outer in keys) { for (key in keys) { if (key == 2) { return } } }; keys.size }",
            false,
            23,
            2,
        );
    }
    #[test]
    fn owned_source_return_same_source_sequential() {
        observe(
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { return } }; for (key in keys) { if (key == 2) { return } }; keys.size }",
            false,
            23,
            2,
        );
    }

    #[test]
    fn owned_source_return_source_loan_rejects_body_moves_and_mutation() {
        for (factory, operation) in [
            ("arrayOf", "consume(keys)"),
            ("arrayOf", "keys[0] = 9"),
            ("mutableListOf", "keys.add(4)"),
            ("mutableListOf", "keys.removeLast()"),
        ] {
            let text = format!(
                "fun consume(own source: Array<Int>): Unit {{}}\nfun main(): Unit {{ var keys = {factory}(1, 2, 3); for (key in keys) {{ {operation}; if (key == 2) {{ return }} }} }}"
            );
            let a = analyze(&text);
            assert!(a.parsed.diagnostics().is_empty());
            assert!(a.names.diagnostics().is_empty());
            assert!(a.typed.diagnostics().is_empty());
            assert!(
                a.owned
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0135")
            );
        }
    }

    #[test]
    fn owned_source_return_fresh_inner_exact_source_structure() {
        for live in [false, true] {
            let after = if live { "keys.size" } else { "" };
            let text = format!(
                "fun main(): Unit {{ for (outer in arrayOf(1, 2)) {{ val keys = arrayOf(1, 2); for (key in keys) {{ if (key == 2) {{ return }} }}; {after} }} }}"
            );
            super::single_text(&text, live, false);
        }
    }
}
