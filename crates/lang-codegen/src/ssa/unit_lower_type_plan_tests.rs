use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{Operation, SsaTypeKind, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn reachable_body_only_boolean_type_is_planned_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun select(): Int = when {\n\
             true -> 1\n\
             else -> 2\n\
         }\n\
         fun preserve(own amount: Long): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int {\n\
             val done = p.preserve(1)\n\
             return p.select()\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("reachable body-only Boolean type lowers to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation preserves body-only scalar type planning");
    assert_text_eq!(render_program(&forward), render_program(&backward));
    assert_eq!(
        forward.modules[0].types,
        vec![
            SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
            SsaTypeKind::Integer {
                bits: 64,
                signed: true,
            },
            SsaTypeKind::Boolean,
            // SPEC-0454 routes the empty Unit-returning preserve helper through refine,
            // whose existing demand plan registers explicit Unit storage.
            SsaTypeKind::Unit,
        ],
        "ABI value signature types precede body/refine-only types"
    );
    let select = forward.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.select.d"))
        .expect("select function exists");
    assert_eq!(
        select
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .filter(|terminator| matches!(terminator.kind, TerminatorKind::Conditional { .. }))
            .count(),
        1
    );
}

#[test]
fn unreachable_body_types_do_not_pollute_the_ssa_type_table() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun dead(): Boolean = true\n\
         fun entry(): Int = 1",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("dead Boolean body does not affect reachable Int entry");
    assert_eq!(program.modules[0].functions.len(), 1);
    assert_eq!(
        program.modules[0].types,
        vec![
            SsaTypeKind::Integer {
                bits: 32,
                signed: true
            },
            // Refine requires its Boolean control type even for a literal-only entry.
            // The new String/Long sibling-local witness separately detects source pollution.
            SsaTypeKind::Boolean,
        ]
    );
}

#[test]
fn reachable_body_only_string_type_preserves_owner_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\nfun entry(): Unit { val text = \"body-only\" }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("reachable body-only String owner lowers to verified SSA");
    let module = &program.modules[0];
    assert_eq!(module.types, vec![SsaTypeKind::StringOwner]);
    assert_eq!(
        module.functions[0]
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
            .count(),
        1
    );
    assert_eq!(
        module.functions[0]
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1
    );
}

// Stage A only: legacy whole-file/shared-instance witnesses remain unchanged.
#[path = "unit_legacy_instance_witness_tests.rs"]
mod legacy_instance_witnesses;

mod registration_tests {
    //! CI-only mapper contract after the complete unit planner; Resource never bypasses admission.

    use std::collections::BTreeSet;

    use lang_frontend::{
        lexer::lex,
        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::check_compilation_unit_ownership,
        parser::{ParsedFile, parse_file},
        type_checking::{
            IntrinsicTypeConstructor, UnitCallTarget, check_compilation_unit_types,
            complete_compilation_unit_types, standard_environments,
        },
    };

    use crate::ssa::unit_lower::*;
    use crate::ssa::{
        model::{MapContainerKind, Module, Ownership, SsaTypeKind},
        unit_plan::plan_unit_instances,
    };
    use std::collections::BTreeMap;

    use crate::ssa::nullable_map_consume_lowering_tests::registration_r1_source;

    const ORDINARY: &str = r#"fun main(): Unit {
      val mapText: Map<Int, String>? = null
      var mapTextOuter = mutableMapOf<Int, Map<Int, String>>()
      val mapTextRemoved = mapTextOuter.remove(1)
      val mutableText: MutableMap<Int, String>? = null
      var mutableTextOuter = mutableMapOf<Int, MutableMap<Int, String>>()
      val mutableTextRemoved = mutableTextOuter.remove(1)
      val mutableKey: MutableMap<String, Int>? = null
      var mutableKeyOuter = mutableMapOf<Int, MutableMap<String, Int>>()
      val mutableKeyRemoved = mutableKeyOuter.remove(1)
    }
    "#;

    #[derive(Clone, Copy)]
    struct Case {
        local: &'static str,
        outer: &'static str,
        constructor: IntrinsicTypeConstructor,
        kind: MapContainerKind,
        key: BuiltinType,
        value: Option<BuiltinType>,
    }

    const CASES: &[Case] = &[
        Case {
            local: "mapText",
            outer: "mapTextOuter",
            constructor: IntrinsicTypeConstructor::Map,
            kind: MapContainerKind::Map,
            key: BuiltinType::Int,
            value: Some(BuiltinType::String),
        },
        Case {
            local: "mutableText",
            outer: "mutableTextOuter",
            constructor: IntrinsicTypeConstructor::MutableMap,
            kind: MapContainerKind::MutableMap,
            key: BuiltinType::Int,
            value: Some(BuiltinType::String),
        },
        Case {
            local: "mutableKey",
            outer: "mutableKeyOuter",
            constructor: IntrinsicTypeConstructor::MutableMap,
            kind: MapContainerKind::MutableMap,
            key: BuiltinType::String,
            value: Some(BuiltinType::Int),
        },
    ];

    const RESOURCE: Case = Case {
        local: "ordinary",
        outer: "outer",
        constructor: IntrinsicTypeConstructor::MutableMap,
        kind: MapContainerKind::MutableMap,
        key: BuiltinType::Int,
        value: None,
    };

    #[derive(Debug, PartialEq, Eq)]
    struct Snapshot {
        id: SsaTypeId,
        types: usize,
        nullable: BTreeMap<SsaTypeId, SsaTypeId>,
        results: BTreeMap<SsaTypeId, SsaTypeId>,
    }

    fn snapshot(module: &Module, id: SsaTypeId) -> Snapshot {
        Snapshot {
            id,
            types: module.types.len(),
            nullable: module.nullable_maps.clone(),
            results: module.map_results.clone(),
        }
    }

    fn builtin_identity(module: &Module, id: SsaTypeId, expected: BuiltinType) -> String {
        match expected {
            BuiltinType::Int => assert!(matches!(
                module.type_kind(id),
                Some(SsaTypeKind::Integer {
                    bits: 32,
                    signed: true
                })
            )),
            BuiltinType::String => {
                assert_eq!(module.type_kind(id), Some(&SsaTypeKind::StringOwner))
            }
            _ => panic!("fixture contains only Int/String builtins"),
        }
        format!("{expected:?}")
    }

    fn storage_identity(module: &Module, wrapper: SsaTypeId, case: Case) -> String {
        let inner = module
            .nullable_map_inner(wrapper)
            .expect("N wrapper exists");
        let (kind, key, value) = module
            .map_container(inner)
            .expect("exact MapContainer payload");
        assert_eq!(kind, case.kind);
        let key_name = builtin_identity(module, key, case.key);
        let value_name = match case.value {
            Some(builtin) => builtin_identity(module, value, builtin),
            None => {
                assert!(matches!(
                    module.type_kind(value),
                    Some(SsaTypeKind::HeapOwner { .. })
                ));
                assert!(
                    module.deinits.contains_key(&value),
                    "real Resource deinit is registered"
                );
                "Resource".to_owned()
            }
        };
        assert_eq!(module.type_ownership(inner), Some(Ownership::MoveOnly));
        assert_eq!(module.type_ownership(wrapper), Some(Ownership::MoveOnly));
        let [present, payload] = module
            .aggregate_fields(wrapper)
            .expect("presence aggregate")
        else {
            panic!("nullable Map has exactly two fields");
        };
        assert_eq!(module.type_kind(*present), Some(&SsaTypeKind::Boolean));
        assert_eq!(*payload, inner);
        assert_eq!(
            module
                .nullable_maps
                .values()
                .filter(|&&ty| ty == inner)
                .count(),
            1
        );
        assert_eq!(module.map_result_value(wrapper), None);
        assert!(
            module
                .map_results
                .keys()
                .all(|ty| !module.nullable_maps.contains_key(ty))
        );
        assert!(
            module
                .map_results
                .values()
                .all(|&ty| module.map_container(ty).is_none())
        );
        format!("{kind:?}<{key_name},{value_name}>?")
    }

    fn parsed(sources: &mut SourceMap, path: &str, text: &str) -> ParsedFile {
        let source = sources.add_source(path, text).expect("unique source");
        let lexed = lex(sources, source).expect("lex");
        assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
        let parsed = parse_file(sources, &lexed).expect("parse");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        parsed
    }

    fn observe(
        provider_text: &str,
        consumer_text: &str,
        cases: &[Case],
        ordinary_first: bool,
        reverse_sources: bool,
    ) -> Vec<String> {
        let mut sources = SourceMap::new();
        let provider = parsed(&mut sources, "p/provider.ko", provider_text);
        let consumer = parsed(&mut sources, "q/consumer.ko", consumer_text);
        let mut inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider.source_id(), &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer.source_id(), &consumer),
        ];
        if reverse_sources {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).expect("names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("validated names");
        let index = names.names().index();
        assert_eq!(index.source_units().len(), 2);
        let source_unit = |parsed: &ParsedFile, path: &str, package: &str| {
            let matches = index
                .source_units()
                .iter()
                .filter(|unit| {
                    unit.source_id() == parsed.source_id()
                        && unit.key().logical_path().expect("rooted source").as_str() == path
                        && index.packages().iter().any(|item| {
                            item.id() == unit.package()
                                && item.name().expect("named package").segments() == [package]
                        })
                })
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "unique source/package {path}");
            matches[0].id()
        };
        let p = source_unit(&provider, "p/provider.ko", "p");
        let q = source_unit(&consumer, "q/consumer.ko", "q");
        assert_ne!(p, q);
        let declaration = |source_unit, name: &str| {
            let matches = index
                .declarations()
                .iter()
                .filter(|declaration| {
                    declaration.source_unit() == source_unit
                        && declaration.name() == name
                        && declaration.kind() == SymbolKind::Function
                })
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "unique real function {name}");
            matches[0].id()
        };
        let main = declaration(p, "main");
        let entry = declaration(q, "entry");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let complete = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .expect("complete original unit type analysis");
        assert!(complete.is_same_analysis(&typed));
        assert!(std::ptr::eq(complete.types(), &typed));
        assert!(complete.types().diagnostics().is_empty());
        let typed = typed.validate().expect("validate same raw analysis");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .expect("ownership");
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let owned = owned.validate().expect("validated ownership");
        let raw = typed.types();
        let consumer_calls = raw
            .calls()
            .iter()
            .filter(|call| call.expression().source_unit() == q)
            .collect::<Vec<_>>();
        assert_eq!(consumer_calls.len(), 1);
        assert_eq!(
            consumer_calls[0].target(),
            UnitCallTarget::Declaration(main)
        );
        assert!(consumer_calls[0].receiver().is_none());
        assert!(consumer_calls[0].arguments().is_empty());
        assert_eq!(
            Some(consumer_calls[0].return_type()),
            raw.types().builtin(BuiltinType::Unit)
        );
        assert_eq!(raw.map_removes().len(), cases.len());

        let resolution = names
            .names()
            .source_units()
            .iter()
            .find(|source| source.source_unit() == p)
            .expect("real provider resolution")
            .resolution();
        let named = |name: &str| {
            let symbols = resolution
                .symbols()
                .iter()
                .filter(|symbol| {
                    symbol.name() == name
                        && symbol.kind() == SymbolKind::Variable
                        && !symbol.is_synthetic()
                        && symbol.span().source_id() == provider.source_id()
                })
                .collect::<Vec<_>>();
            assert_eq!(symbols.len(), 1, "unique real provider local {name}");
            let roots = raw
                .body_symbol_types()
                .keys()
                .copied()
                .chain(names.names().references().iter().filter_map(|reference| {
                    match reference.target() {
                        UnitReferenceTarget::Symbol(root) => Some(*root),
                        _ => None,
                    }
                }))
                .filter(|root| root.source_unit() == p && root.symbol() == symbols[0].id())
                .collect::<BTreeSet<_>>();
            assert_eq!(roots.len(), 1, "unique source-qualified symbol {name}");
            let root = *roots.first().expect("real root");
            (
                raw.symbol_type(root).expect("real symbol type"),
                symbols[0].span(),
            )
        };
        let resource = cases.iter().any(|case| case.value.is_none());
        let mut frontend_inputs = Vec::new();
        for &case in cases {
            let (nullable, local_span) = named(case.local);
            let Some(UnitTypeKind::Nullable(inner)) = raw.types().get(nullable) else {
                panic!("real ordinary provider local is nullable");
            };
            let Some(UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            }) = raw.types().get(*inner)
            else {
                panic!("real nullable inner is intrinsic");
            };
            assert_eq!(*constructor, case.constructor);
            let [key, value] = arguments.as_slice() else {
                panic!("exact two Map arguments")
            };
            assert_eq!(Some(*key), raw.types().builtin(case.key));
            if let Some(builtin) = case.value {
                assert_eq!(Some(*value), raw.types().builtin(builtin));
            } else {
                assert_eq!(raw.is_resource_type(*value), Some(true));
                assert_eq!(raw.is_resource_type(*inner), Some(true));
            }
            let call = format!("{}.remove(1)", case.outer);
            let removes = raw
                .map_removes()
                .iter()
                .filter(|remove| {
                    if remove.expression().source_unit() != p {
                        return false;
                    }
                    let span = provider
                        .ast()
                        .expressions()
                        .get(remove.expression().expression())
                        .expect("real remove")
                        .span();
                    sources.slice(span).expect("remove source") == call
                })
                .collect::<Vec<_>>();
            assert_eq!(removes.len(), 1, "unique real provider remove {call}");
            let remove = removes[0];
            let remove_span = provider
                .ast()
                .expressions()
                .get(remove.expression().expression())
                .expect("real remove")
                .span();
            assert_eq!(remove.receiver().source_unit(), p);
            assert_eq!(remove.result_type(), nullable);
            assert_eq!(remove.value_type(), *inner);
            assert_eq!(raw.expression_type(remove.expression()), Some(nullable));
            assert_eq!(
                Some(remove.key_type()),
                raw.types().builtin(BuiltinType::Int)
            );
            let (outer, _) = named(case.outer);
            assert_eq!(raw.expression_type(remove.receiver()), Some(outer));
            assert!(matches!(raw.types().get(outer),
                Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::MutableMap, arguments })
                    if arguments.as_slice() == [remove.key_type(), *inner]));
            frontend_inputs.push((case, nullable, *inner, local_span, remove_span));
        }
        assert_eq!(
            frontend_inputs
                .iter()
                .map(|input| input.1)
                .collect::<BTreeSet<_>>()
                .len(),
            cases.len()
        );

        let mut mapper_creations = 0;
        // The full production planner, including layout/deinit admission, precedes all mapper creation.
        let result = plan_unit_instances(&sources, &inputs, &names, &types, &typed, &owned, entry)
            .map(|plan| {
                assert!(plan.iter().any(|instance| instance.source_unit() == p
                    && instance.key() == &UnitFunctionInstanceKey::for_entry(main)));
                assert!(plan.iter().any(|instance| instance.source_unit() == q
                    && instance.key() == &UnitFunctionInstanceKey::for_entry(entry)));
                mapper_creations += 1;
                let mut program = Program::default();
                let module_id = program.add_module("registration");
                let module = program
                    .module_mut(module_id)
                    .expect("fresh module after planner");
                let mut mapper = type_lower::UnitTypeLowering::new(&names);
                let parsed_by_source =
                    crate::ssa::lowering_support::parsed_by_source_unit(&inputs, &names)
                        .expect("same qualified source objects");
                let deinits = plan
                    .iter()
                    .filter(|instance| instance.key().deinit_owner().is_some())
                    .collect::<Vec<_>>();
                assert_eq!(deinits.len(), usize::from(resource));
                for instance in deinits {
                    let owner = instance.key().deinit_owner().expect("real deinit owner");
                    let nominal = raw
                        .signatures()
                        .declaration(owner)
                        .and_then(|signature| signature.nominal())
                        .expect("Resource nominal");
                    let descriptor = nominal.deinit().expect("published Resource deinit");
                    assert_eq!(descriptor.owner(), owner);
                    assert_eq!(descriptor.item().source_unit(), p);
                    assert_eq!(instance.item(), descriptor.item().item());
                    assert_eq!(instance.source_unit(), p);
                    let declared = callable_abi::declare_deinit(
                        module,
                        &parsed_by_source,
                        &names,
                        raw,
                        &mut mapper,
                        instance.clone(),
                    )
                    .expect("planner-selected Resource deinit declaration");
                    let receiver = module
                        .function(declared.id)
                        .expect("hidden deinit")
                        .receiver()
                        .expect("shared Resource receiver")
                        .semantic_type();
                    assert_eq!(module.deinit(receiver), Some(declared.id));
                    assert_eq!(module.deinits.len(), 1);
                }
                assert!(module.nullable_maps.is_empty());
                assert!(module.map_results.is_empty());
                for &(_, nullable, inner, _, _) in &frontend_inputs {
                    assert!(!mapper.type_ids().contains_key(&nullable));
                    assert!(!mapper.type_ids().contains_key(&inner));
                }
                let mut wrappers = BTreeSet::new();
                let mut summaries = Vec::new();
                for (case, nullable, inner, local_span, remove_span) in frontend_inputs {
                    assert!(!mapper.type_ids().contains_key(&nullable));
                    assert!(!mapper.type_ids().contains_key(&inner));
                    let first = if ordinary_first {
                        mapper.intern(module, raw, nullable, local_span)
                    } else {
                        mapper.intern_map_result(module, raw, nullable, remove_span)
                    }
                    .expect("first real mapper registration");
                    let after_first = snapshot(module, first);
                    assert_eq!(mapper.type_ids().get(&nullable), Some(&first));
                    assert_eq!(
                        mapper.type_ids().get(&inner).copied(),
                        module.nullable_map_inner(first)
                    );
                    let semantic = storage_identity(module, first, case);
                    let second = if ordinary_first {
                        mapper.intern_map_result(module, raw, nullable, remove_span)
                    } else {
                        mapper.intern(module, raw, nullable, local_span)
                    }
                    .expect("second real mapper registration");
                    assert_eq!(
                        after_first,
                        snapshot(module, second),
                        "same mapper exact identity/N/R/type count"
                    );
                    assert_eq!(mapper.type_ids().get(&nullable), Some(&second));
                    assert_eq!(
                        mapper.type_ids().get(&inner).copied(),
                        module.nullable_map_inner(second)
                    );
                    assert_eq!(semantic, storage_identity(module, second, case));
                    assert!(
                        wrappers.insert(first),
                        "different kind or K/V must have different wrappers"
                    );
                    assert_eq!(module.nullable_maps.len(), wrappers.len());
                    summaries.push(format!(
                        "p/provider.ko:{}:{}..{}:{semantic}:{call}",
                        case.local,
                        local_span.start(),
                        local_span.end(),
                        call = sources.slice(remove_span).expect("source")
                    ));
                }
                summaries
            });
        let summaries = result.expect("complete planner succeeds before either mapper order");
        assert_eq!(mapper_creations, 1);
        summaries
    }

    #[test]
    fn nullable_map_registration_order_preserves_identity_and_resource_planner_gate() {
        let provider = format!("package p\n{ORDINARY}");
        let consumer = "package q\nfun entry(): Unit { p.main() }\n";
        let (path, single, resource_provider, resource_consumer) = registration_r1_source();
        assert_eq!(path, "r1_ordinary_source_before_remove.ko");
        assert_eq!(resource_provider, format!("package p\n{single}"));
        assert_eq!(
            resource_consumer,
            "package q\nfun entry(): Unit { p.main() }\n"
        );
        let mut ordinary_baseline = None;
        let mut resource_baseline = None;
        for ordinary_first in [true, false] {
            for reverse_sources in [false, true] {
                // Every (registration direction, input order) gets its own complete FE analysis.
                let ordinary = observe(&provider, consumer, CASES, ordinary_first, reverse_sources);
                let resource = observe(
                    resource_provider,
                    resource_consumer,
                    &[RESOURCE],
                    ordinary_first,
                    reverse_sources,
                );
                if let Some(baseline) = &ordinary_baseline {
                    assert_eq!(baseline, &ordinary);
                }
                if let Some(baseline) = &resource_baseline {
                    assert_eq!(baseline, &resource);
                }
                ordinary_baseline = Some(ordinary);
                resource_baseline = Some(resource);
            }
        }
    }
}
