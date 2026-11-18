//! SPEC-0318: real-source last-use baseline; no synthesized ownership facts.
use crate::ssa::map_unit_consumer_test_support::assert_source;
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

use crate::ssa::lower_frontend_tests::constant_lowering_tests::map_operand_unit_identity as unit_identity;

pub(in crate::ssa) use crate::ssa::unit_constant_tests::control_completion::map_nullable_operand_unit as nullable_operand;
pub(in crate::ssa) use crate::ssa::unit_constant_tests::receiver_two_phase_control::map_same_guard_unit as same_guard;

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko");

fn source(kind: &str, live: bool) -> String {
    let text = match kind {
        "remove" => ORIGINAL.to_owned(),
        "contains" => ORIGINAL.replace("cache.remove(item) != null", "cache.contains(item)"),
        "item" => ORIGINAL.replace(
            "if (cache.remove(item) != null) { println(\"removed operand\"); item } else { 0 }",
            "item",
        ),
        "independent-nullable" => {
            ORIGINAL.replace("cache.remove(item) != null", "nullableValue(item) != null")
        }
        "no-map" => ORIGINAL
            .replace("    var cache = mutableMapOf<Int, String>()\n", "")
            .replace("        cache.put(item, \"head\".clone())\n", "")
            .replace("        cache.put(9, \"tail\".clone())\n", "")
            .replace("    cache.contains(9)\n", "")
            .replace("cache.remove(item) != null", "nullableValue(item) != null"),
        _ => unreachable!(),
    };
    let text = if live {
        text
    } else {
        text.replace("    cache.contains(9)\n", "")
    };
    if kind == "independent-nullable" || kind == "no-map" {
        format!(
            "fun nullableValue(item: Int): String? {{ return if (item == 1) {{ \"value\".clone() }} else {{ null }} }}\n{text}"
        )
    } else {
        text
    }
}

fn assert_unit(text: &str, constants: bool, supported: bool) {
    unit(
        text,
        constants,
        supported,
        "\nfun main(): Unit { updatingOperand(2) }",
    );
}

fn unit(text: &str, constants: bool, supported: bool, entry: &str) {
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
    let expected = text.matches("return if (cache.").count();
    let text = format!("package p\n{text}{entry}");
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
        diagnostic!(
            "UNIT FACTS constants=true: drops={:#?}",
            owned.ownership().drops()
        );
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let typed = result.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        diagnostic!(
            "UNIT FACTS constants=false: drops={:#?}",
            owned.ownership().drops()
        );
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    diagnostic!(
        "RESULT unit constants={constants}: {:?}",
        result.as_ref().map(|_| "VERIFIED")
    );
    match result {
        Ok((program, entry)) => {
            assert!(supported);
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            assert_eq!(
                super::operand_structure::assert_source(&program, &file),
                expected
            );
        }
        Err(error) => {
            assert_diagnostic!(!supported, "{error:?}");
            assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
            // The factory's String? value-if now lowers; the iteration operand's
            // temporary nullable comparison remains the actual rejected operation.
            let span = error.span.unwrap();
            let boundary = "nullableValue(item) != null";
            assert_eq!(text.matches(boundary).count(), 1);
            let start = text.find(boundary).unwrap();
            assert_eq!((span.start(), span.end()), (start, start + boundary.len()));
            assert_eq!(sources.slice(span).unwrap(), boundary);
        }
    }
}

#[test]
fn map_operand_last_use_single_baseline() {
    for live in [false, true] {
        for kind in ["remove", "contains", "item", "independent-nullable"] {
            nullable_operand::check(
                &source(kind, live),
                &format!("{kind}-live={live}"),
                kind != "independent-nullable",
            );
        }
    }
}

#[test]
fn map_operand_last_use_unit_basic_baseline() {
    for live in [false, true] {
        for kind in [
            "remove",
            "contains",
            "item",
            "independent-nullable",
            "no-map",
        ] {
            diagnostic!("CASE unit-basic {kind}-live={live}");
            assert_unit(
                &source(kind, live),
                false,
                // SPEC-0394: the `String.clone()` puts move `updatingOperand` into the
                // refinement layer, which lowers the temporary nullable comparison; without
                // a Map ("no-map") the per-family boundary remains.
                kind != "no-map",
            );
        }
    }
}

#[test]
fn map_operand_last_use_unit_constant_baseline() {
    for live in [false, true] {
        for kind in [
            "remove",
            "contains",
            "item",
            "independent-nullable",
            "no-map",
        ] {
            diagnostic!("CASE unit-constant {kind}-live={live}");
            assert_unit(
                &source(kind, live),
                true,
                // SPEC-0394: the `String.clone()` puts move `updatingOperand` into the
                // refinement layer, which lowers the temporary nullable comparison; without
                // a Map ("no-map") the per-family boundary remains.
                kind != "no-map",
            );
        }
    }
}

#[test]
fn map_operand_last_use_no_map_nullable_control() {
    nullable_operand::check(&source("no-map", false), "no-map-nullable", false);
}

#[test]
fn map_operand_last_use_remove_live_red() {
    unit_identity::check(ORIGINAL, "updatingOperand", 1);
}
#[test]
fn map_operand_last_use_remove_dead_red() {
    unit_identity::check(&source("remove", false), "updatingOperand", 1);
}
#[test]
fn map_operand_last_use_contains_live_red() {
    unit_identity::check(&source("contains", true), "updatingOperand", 1);
}
#[test]
fn map_operand_last_use_contains_dead_red() {
    unit_identity::check(&source("contains", false), "updatingOperand", 1);
}

#[test]
fn map_operand_last_use_same_guard_outside_return_is_rejected() {
    let text = ORIGINAL.replace(
        "return if (cache.remove(item) != null) { println(\"removed operand\"); item } else { 0 }",
        "cache.contains(item); return item",
    );
    same_guard::check(&text);
}

#[test]
fn map_operand_last_use_checked_operand_abort_path_is_rejected() {
    let text = ORIGINAL.replace("cache.remove(item)", "cache.remove(item + 100)");
    assert_source(&text, &["updatingOperand"]);
}

#[test]
fn map_operand_last_use_extended_provider_and_result_shapes_are_rejected() {
    for text in [
        ORIGINAL.replace("arrayOf(1, 2, 3)", "listOf(1, 2, 3)"),
        ORIGINAL.replace(
            "    for (item in arrayOf(1, 2, 3))",
            "    val keys = arrayOf(1, 2, 3)\n    for (item in keys)",
        ),
        ORIGINAL
            .replace(
                "updatingOperand(stop: Int): Int",
                "updatingOperand(stop: Int): Boolean",
            )
            .replace("; item } else { 0 }", "; true } else { false }")
            .replace("return 0", "return false"),
    ] {
        assert_source(&text, &["updatingOperand"]);
    }
}

#[test]
fn map_operand_last_use_two_maps_and_borrowed_array_are_rejected() {
    let two_maps = ORIGINAL
        .replace(
            "    for (item",
            "    var spare = mutableMapOf<Int, String>()\n    for (item",
        )
        .replace(
            "        cache.put(item",
            "        spare.put(item, \"spare\".clone())\n        cache.put(item",
        )
        .replace(
            "        cache.put(9",
            "        spare.put(9, \"spare-tail\".clone())\n        cache.put(9",
        )
        .replace(
            "    cache.contains(9)",
            "    spare.contains(9)\n    cache.contains(9)",
        );
    let borrowed = ORIGINAL
        .replace("stop: Int", "stop: Int, keys: Array<Int>")
        .replace("arrayOf(1, 2, 3)", "keys");
    for text in [two_maps, borrowed] {
        assert_source(&text, &["updatingOperand"]);
    }
}

#[test]
fn map_operand_last_use_cardinality_fixture_ssa() {
    let text = include_str!("../../tests/fixtures/map-return-last-use-0318/cardinality.ko");
    unit_identity::check(text, "main", 4);
    unit(text, false, true, "");
    unit(text, true, true, "");
}

pub(in crate::ssa) mod source_matrix {
    //! Frozen ordinary sources shared by SSA and unmodified object/link/run.
    use std::collections::BTreeSet;

    use crate::{
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::model::{FunctionId, Operation, Program, TerminatorKind},
    };
    use lang_frontend::{
        name_resolution::{SymbolKind, UnitReferenceTarget, UnitSymbolId},
        ownership_checking::{UnitDropPoint, UnitDropTarget},
        parser::{Expression, Item, NameMarker},
    };

    fn check(source: &str) -> (UnitSource, Program) {
        let analysis = UnitSource::analyze("lowering.ko", source).unwrap();
        analysis.assert_frontend_clean();
        assert_eq!(
            analysis
                .sources()
                .source_text(analysis.parsed().source_id())
                .unwrap(),
            source,
            "the original fixture is the entire MemoryDocument input"
        );
        let main = analysis.declaration("main", SymbolKind::Function);
        let (program, entry) = analysis.lower_entry(main).unwrap();
        assert_eq!(
            super::super::structure::source_function(&program, analysis.declaration_span(main))
                .1
                .id,
            entry,
            "the executable entry is the original main, not a synthetic wrapper"
        );
        assert_all_source_functions(&analysis, &program, entry);
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let results = returned_result_symbols(&analysis);
        for fact in analysis.ownership().drops() {
            if let (UnitDropPoint::ControlTransfer(_), UnitDropTarget::Named(symbol)) =
                (fact.point(), fact.target())
            {
                assert!(
                    !results.contains(&symbol),
                    "the owned return result must be excluded from transfer cleanup"
                );
            }
        }
        for function in program.modules.iter().flat_map(|module| &module.functions) {
            for block in &function.blocks {
                if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind
                {
                    for value in values {
                        assert!(
                        !function.instructions.iter().any(|instruction| {
                            matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
                        }),
                        "returned value has a single consuming Return"
                    );
                    }
                }
            }
        }
        (analysis, program)
    }

    // Main-rooted planning must retain every function the old whole-file entry checked.
    // Bind each AST root through this analysis's declaration identity to one SSA body.
    fn assert_all_source_functions(analysis: &UnitSource, program: &Program, entry: FunctionId) {
        let parsed = analysis.parsed();
        let index = analysis.names().names().index();
        let [source] = index.source_units() else {
            panic!("one original memory document");
        };
        let mut functions = BTreeSet::new();
        for root in parsed.roots() {
            let node = parsed.ast().items().get(*root).unwrap();
            let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = node.payload()
            else {
                continue;
            };
            let declarations = index
                .declarations()
                .iter()
                .filter(|declaration| {
                    declaration.root() == *root
                        && declaration.source_unit() == source.id()
                        && declaration.kind() == SymbolKind::Function
                })
                .collect::<Vec<_>>();
            let [declaration] = declarations.as_slice() else {
                panic!("one actual declaration for source function root {root:?}");
            };
            assert_eq!(declaration.name_span(), *name);
            assert_eq!(analysis.declaration_span(declaration.id()), node.span());
            let (_, function) = super::super::structure::source_function(program, node.span());
            assert!(
                functions.insert(function.id),
                "distinct source function roots"
            );
        }
        assert!(functions.contains(&entry));

        // Resource bodies are reached through the real typed deinit descriptor and
        // storage drop registration, not through guessed generated function names.
        let mut deinits = 0;
        for signature in analysis.types().signatures().declarations() {
            let Some(deinit) = signature.nominal().and_then(|nominal| nominal.deinit()) else {
                continue;
            };
            deinits += 1;
            assert_eq!(deinit.owner(), signature.declaration());
            assert_eq!(deinit.item().source_unit(), source.id());
            assert_eq!(deinit.body().source_unit(), source.id());
            let node = parsed.ast().items().get(deinit.item().item()).unwrap();
            assert!(matches!(node.payload(), Item::Deinit { body, .. }
            if *body == deinit.body().statement()));
            let (module, function) = super::super::structure::source_function(program, node.span());
            assert!(module.deinits.values().any(|id| *id == function.id));
            assert!(
                functions.insert(function.id),
                "distinct source deinit bodies"
            );
        }
        assert_eq!(
            deinits,
            parsed
                .ast()
                .items()
                .iter()
                .filter(|(_, node)| { matches!(node.payload(), Item::Deinit { .. }) })
                .count()
        );
    }

    // Keep the old all-Named(result) exclusion, but take qualified identities from
    // published type facts and bind them to actual declarations and return operands.
    fn returned_result_symbols(analysis: &UnitSource) -> BTreeSet<UnitSymbolId> {
        let parsed = analysis.parsed();
        let names = analysis.names().names();
        let [source] = names.source_units() else {
            panic!("one original source's names");
        };
        let return_names = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| {
                let Expression::Return {
                    value: Some(value), ..
                } = node.payload()
                else {
                    return None;
                };
                let mut value = *value;
                loop {
                    let operand = parsed.ast().expressions().get(value).unwrap();
                    match operand.payload() {
                        Expression::Group { expression } => value = *expression,
                        Expression::Name => return Some(operand.span()),
                        _ => return None,
                    }
                }
            })
            .collect::<Vec<_>>();
        let mut results = BTreeSet::new();
        for symbol in source
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == "result")
        {
            assert_eq!(symbol.kind(), SymbolKind::Variable);
            assert_eq!(symbol.span().source_id(), parsed.source_id());
            assert_eq!(
            parsed
                .ast()
                .items()
                .iter()
                .filter(|(_, node)| {
                    matches!(node.payload(), Item::Variable { name: NameMarker::Present(span), .. }
                if *span == symbol.span())
                })
                .count(),
            1,
            "one real result binding declaration"
        );
            let qualified = analysis
                .types()
                .body_symbol_types()
                .keys()
                .copied()
                .filter(|id| id.source_unit() == source.source_unit() && id.symbol() == symbol.id())
                .collect::<Vec<_>>();
            let [qualified] = qualified.as_slice() else {
                panic!("one published qualified identity for the actual result binding");
            };
            assert!(names.references().iter().any(|reference| {
            reference.source_unit() == source.source_unit()
                && return_names.contains(&reference.span())
                && matches!(reference.target(), UnitReferenceTarget::Symbol(id) if id == qualified)
        }), "the result identity is the actual direct or grouped Return operand");
            assert!(results.insert(*qualified));
        }
        results
    }

    #[test]
    fn single_map_return_cardinality_matrix() {
        check(include_str!(
            "../../tests/fixtures/map-conditional-return-0313/live-dead.ko"
        ));
    }
    #[test]
    fn single_map_return_distinct_owner_matrix() {
        let (analysis, program) = check(include_str!(
            "../../tests/fixtures/map-conditional-return-0313/distinct-owners.ko"
        ));
        super::super::structure::assert_distinct_return_owners(
            &program,
            analysis.declaration_span(analysis.declaration("separateOwners", SymbolKind::Function)),
        );
    }
    #[test]
    fn single_map_return_provider_matrix() {
        check(include_str!(
            "../../tests/fixtures/map-conditional-return-0313/borrowed-temporary-providers.ko"
        ));
    }
    #[test]
    fn single_map_return_operand_cleanup_matrix() {
        let source =
            include_str!("../../tests/fixtures/map-conditional-return-0313/operand-cleanup.ko");
        let (analysis, program) = check(source);
        super::super::structure::assert_operand_before_body_cleanup(
            &program,
            source,
            analysis.declaration_span(analysis.declaration("cleanup", SymbolKind::Function)),
            analysis.declaration_span(analysis.declaration("operand", SymbolKind::Function)),
        );
    }

    #[test]
    fn single_map_return_owned_results_matrix() {
        check(include_str!(
            "../../tests/fixtures/map-conditional-return-0313/owned-results.ko"
        ));
    }

    #[test]
    fn owned_source_return_operand_cleanup_structure() {
        let source =
            include_str!("../../tests/fixtures/owned-source-return-0314/operand-cleanup.ko");
        let (analysis, program) = check(source);
        super::super::structure::assert_owned_operand_before_body_cleanup(
            &program,
            source,
            analysis.declaration_span(analysis.declaration("cleanup", SymbolKind::Function)),
            analysis.declaration_span(analysis.declaration("operand", SymbolKind::Function)),
        );
    }

    #[test]
    fn owned_source_return_distinct_maps_structure() {
        let (analysis, program) = check(include_str!(
            "../../tests/fixtures/owned-source-return-0314/distinct-owners.ko"
        ));
        super::super::structure::assert_distinct_return_owners(
            &program,
            analysis.declaration_span(analysis.declaration("separateOwners", SymbolKind::Function)),
        );
    }

    #[test]
    fn owned_source_return_independent_results_structure() {
        check(include_str!(
            "../../tests/fixtures/owned-source-return-0314/owned-results.ko"
        ));
    }
}

pub(in crate::ssa) mod operand_structure {
    //! Independent assertions over original ordinary-source SSA, never edited IR.
    use super::super::structure::{
        before, branch, definition, edges, forwards, only, reaches, slot, transport,
    };
    use crate::ssa::model::{
        BlockId, EntityId, Function, LoanKind, Operation, Origin, Program, TerminatorKind,
    };
    use lang_frontend::{
        parser::{Expression, ParsedFile},
        source::Span,
    };

    fn all_paths_return(
        function: &Function,
        start: BlockId,
        target: BlockId,
        path: &mut Vec<BlockId>,
    ) -> bool {
        if start == target {
            return true;
        }
        if path.contains(&start) {
            return false;
        }
        path.push(start);
        let successors = function
            .block(start)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
            .targets();
        let valid = !successors.is_empty()
            && successors
                .into_iter()
                .all(|next| all_paths_return(function, next, target, path));
        path.pop();
        valid
    }

    // The parsed Return operand provides an exact location; the chosen intrinsic instruction
    // and actual terminator must independently carry those same source origins.
    pub(in crate::ssa) fn assert_source(program: &Program, parsed: &ParsedFile) -> usize {
        let mut checked = 0;
        for (_, node) in parsed.ast().expressions().iter() {
            let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            else {
                continue;
            };
            let Expression::If { condition, .. } =
                parsed.ast().expressions().get(*value).unwrap().payload()
            else {
                continue;
            };
            let condition = parsed.ast().expressions().get(*condition).unwrap();
            let call = if let Expression::Binary { left, .. } = condition.payload() {
                parsed.ast().expressions().get(*left).unwrap()
            } else {
                condition
            };
            let functions = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| {
                    f.instructions.iter().any(|i| {
                        i.origin == Origin::Source(call.span())
                            && matches!(
                                i.operation,
                                Operation::MapRemove { .. } | Operation::MapContains { .. }
                            )
                    })
                })
                .collect::<Vec<_>>();
            if functions.is_empty() {
                continue;
            }
            assert_eq!(functions.len(), 1);
            check(functions[0], node.span(), call.span());
            checked += 1;
        }
        checked
    }

    fn check(function: &Function, return_span: Span, call_span: Span) {
        let seed = only(function, |i| {
            matches!(i.operation, Operation::MapConstruct { .. })
        });
        let length = only(function, |i| {
            matches!(i.operation, Operation::ContainerLength { .. })
        });
        let Operation::ContainerLength { owner: source_loan } = length.operation else {
            unreachable!()
        };
        let source_borrow = definition(function, source_loan);
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = source_borrow.operation
        else {
            panic!("source shared loan")
        };
        let root = definition(function, EntityId::Place(place));
        let Operation::RootPlace {
            owner: source_owner,
        } = root.operation
        else {
            panic!("exact hidden owner")
        };
        let source_owner = EntityId::Value(source_owner);
        assert_ne!(seed.results[0], source_owner);
        let source = definition(function, source_owner);
        let preheader = function.block(length.block).unwrap();
        before(function, source, root);
        before(function, root, source_borrow);
        before(function, source_borrow, length);
        let enter = branch(preheader);
        let header = function.block(enter.target).unwrap();
        let seed_inputs = enter
            .arguments
            .iter()
            .copied()
            .filter(|value| forwards(function, *value, seed.results[0]))
            .collect::<Vec<_>>();
        assert_eq!(
            seed_inputs.len(),
            1,
            "constructor guards may forward the exact Map seed before provider entry"
        );
        let map_slot = slot(&enter.arguments, seed_inputs[0]);
        let source_slot = slot(&enter.arguments, source_owner);
        let loan_slot = slot(&enter.arguments, source_loan);
        let length_slot = slot(&enter.arguments, length.results[0]);
        let [header_map, header_source, header_loan, header_length] =
            [map_slot, source_slot, loan_slot, length_slot].map(|s| header.parameters[s]);
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &header.terminator.as_ref().unwrap().kind
        else {
            panic!("provider header")
        };
        let body = function.block(when_true.target).unwrap();
        let exhausted = function.block(when_false.target).unwrap();
        let body_map = transport(function, when_true, header_map);
        let body_source = transport(function, when_true, header_source);
        let body_loan = transport(function, when_true, header_loan);
        let exhausted_map = transport(function, when_false, header_map);
        let exhausted_source = transport(function, when_false, header_source);
        let exhausted_loan = transport(function, when_false, header_loan);
        let all_edges = edges(function);
        for target in [body.id, exhausted.id] {
            assert_eq!(
                all_edges
                    .iter()
                    .filter(|(_, edge)| edge.target == target)
                    .map(|(from, _)| *from)
                    .collect::<Vec<_>>(),
                [header.id]
            );
        }
        let updates = function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
            .collect::<Vec<_>>();
        assert_eq!(updates.len(), 2);
        let head = *updates
            .iter()
            .find(|i| i.origin.span().end() <= return_span.start())
            .unwrap();
        let tail = *updates
            .iter()
            .find(|i| i.origin.span().start() >= return_span.end())
            .unwrap();
        assert!(
            matches!(head.operation, Operation::MapPut { owner, .. } if forwards(function, EntityId::Value(owner), body_map))
        );
        assert!(
            matches!(tail.operation, Operation::MapPut { owner, .. } if forwards(function, EntityId::Value(owner), head.results[0]))
        );
        let call = only(function, |i| {
            i.origin == Origin::Source(call_span)
                && matches!(
                    i.operation,
                    Operation::MapRemove { .. } | Operation::MapContains { .. }
                )
        });
        let (current, nullable) = match call.operation {
            Operation::MapRemove { owner, .. } => {
                assert!(forwards(function, EntityId::Value(owner), head.results[0]));
                assert_eq!(call.results.len(), 2);
                (call.results[0], Some(call.results[1]))
            }
            Operation::MapContains { owner, .. } => {
                assert!(forwards(function, owner, head.results[0]));
                (owner, None)
            }
            _ => unreachable!(),
        };
        assert_ne!(current, source_owner);
        let map_drop = only(
            function,
            |i| matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == current),
        );
        before(function, call, map_drop);
        if let Some(nullable) = nullable {
            assert_ne!(nullable, current);
            assert_ne!(nullable, source_owner);
            let presence = only(function, |i| {
                matches!(i.operation,
            Operation::AggregateProject { aggregate, field: 0 } | Operation::NullableIsNull { owner: aggregate } if EntityId::Value(aggregate) == nullable)
            });
            let nullable_drop = only(
                function,
                |i| matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == nullable),
            );
            before(function, map_drop, presence);
            before(function, presence, nullable_drop);
            assert_eq!(
                function
                    .instructions
                    .iter()
                    .filter(|i| i.operation.entities().contains(&nullable))
                    .count(),
                2,
                "presence read and one independent nullable drop"
            );
            assert_eq!(
                function
                    .instructions
                    .iter()
                    .filter(|i| i.operation.entities().contains(&current))
                    .count(),
                1,
                "updated Map result0 is consumed only by its drop"
            );
            assert!(
                all_edges
                    .iter()
                    .all(|(_, edge)| !edge.arguments.contains(&current)
                        && !edge.arguments.contains(&nullable))
            );
        }
        let returned = function
            .blocks
            .iter()
            .filter(|block| {
                block.terminator.as_ref().unwrap().origin == Origin::Source(return_span)
            })
            .collect::<Vec<_>>();
        assert_eq!(returned.len(), 1);
        let returned = returned[0];
        let TerminatorKind::Return { values } = &returned.terminator.as_ref().unwrap().kind else {
            panic!("actual Return origin")
        };
        assert_eq!(values.len(), 1);
        assert!(
            !values.iter().any(|value| current == EntityId::Value(*value)
                || Some(EntityId::Value(*value)) == nullable
                || source_owner == EntityId::Value(*value))
        );
        assert!(all_paths_return(
            function,
            map_drop.block,
            returned.id,
            &mut Vec::new()
        ));
        assert!(all_edges.iter().all(|(from, _)| *from != returned.id));
        let TerminatorKind::Conditional {
            when_true: selected,
            when_false: fallthrough,
            ..
        } = &function
            .block(head.block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            panic!("outer Return selection")
        };
        assert!(all_paths_return(
            function,
            selected.target,
            returned.id,
            &mut Vec::new()
        ));
        assert!(all_paths_return(
            function,
            selected.target,
            map_drop.block,
            &mut Vec::new()
        ));
        assert!(!reaches(function, selected.target, tail.block, header.id));
        assert!(!reaches(
            function,
            fallthrough.target,
            map_drop.block,
            header.id
        ));
        assert!(!reaches(
            function,
            fallthrough.target,
            returned.id,
            header.id
        ));
        let incoming = all_edges
            .iter()
            .filter(|(_, edge)| edge.target == header.id)
            .collect::<Vec<_>>();
        assert_eq!(incoming.len(), 2, "only Entry and Fallthrough");
        let (back_block, back) = **incoming
            .iter()
            .find(|(block, _)| *block != preheader.id)
            .unwrap();
        for (slot, expected) in [
            (map_slot, tail.results[0]),
            (source_slot, body_source),
            (loan_slot, body_loan),
            (length_slot, header_length),
        ] {
            assert!(forwards(function, back.arguments[slot], expected));
        }
        assert!(!forwards(
            function,
            back.arguments[map_slot],
            head.results[0]
        ));
        let element = only(
            function,
            |i| matches!(i.operation, Operation::ContainerElementPlace { owner, .. } if owner == body_loan),
        );
        let element_borrow = only(
            function,
            |i| matches!(i.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(place) == element.results[0]),
        );
        let element_end = only(function, |i| {
            i.block == returned.id
                && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), element_borrow.results[0]))
        });
        let back_element_end = only(function, |i| {
            i.block == back_block
                && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), element_borrow.results[0]))
        });
        before(function, tail, back_element_end);
        let source_end = only(function, |i| {
            i.block == returned.id
                && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), body_loan))
        });
        let source_drop = only(function, |i| {
            i.block == returned.id
                && matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), body_source))
        });
        before(function, element_end, source_end);
        before(function, source_end, source_drop);
        let exhausted_end = only(function, |i| {
            i.block == exhausted.id
                && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), exhausted_loan))
        });
        let exhausted_drop = only(function, |i| {
            i.block == exhausted.id
                && matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), exhausted_source))
        });
        before(function, exhausted_end, exhausted_drop);
        let normal_map_drop = only(
            function,
            |i| matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), exhausted_map)),
        );
        before(function, exhausted_drop, normal_map_drop);
        let normal_queries = function
            .instructions
            .iter()
            .filter(|i| {
                i.origin.span().start() >= return_span.end()
                    && matches!(i.operation, Operation::MapContains { .. })
            })
            .collect::<Vec<_>>();
        assert!(normal_queries.len() <= 1);
        if let Some(query) = normal_queries.first() {
            assert!(
                matches!(query.operation, Operation::MapContains { owner, .. } if forwards(function, owner, exhausted_map))
            );
            before(function, exhausted_drop, query);
            before(function, query, normal_map_drop);
        }
        let map_type = function.entity(seed.results[0]).unwrap().ty;
        assert_eq!(function.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop { owner } if function.entity(EntityId::Value(owner)).unwrap().ty == map_type)).count(), 2, "both enumerated drops proved by exact owner above");
    }
}
