//! SPEC-0296: inspect unmodified ordinary-source SSA, never fabricate IR.
use super::{
    borrow_memory_test_support::single as memory_unit,
    borrow_storage_contract_tests::unit,
    model::{EntityId, Operation},
};

#[test]
fn owned_null_cleanup_presence_precedes_unique_asap_drop() {
    for payload in ["String", "Text"] {
        let value = if payload == "String" {
            "\"text\".clone()"
        } else {
            "Text(\"text\".clone())"
        };
        let source = format!(
            "value class Text(val text: String)\nfun entry() {{ var m = mutableMapOf<Int, {payload}>(); m.put(1, {value}); val removed = m.remove(1); val present = ((removed)) != null; if (present) {{ println(\"present\") }} }}"
        );
        for (label, program) in [("memory", memory_unit(&source)), ("rooted", unit(&source))] {
            let program = program.unwrap_or_else(|error| panic!("{label}/{payload}: {error:?}"));
            super::verify::verify_program(&program).unwrap();
            let function = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains("entry"))
                .unwrap();
            let removed = function
                .instructions
                .iter()
                .find(|instruction| matches!(instruction.operation, Operation::MapRemove { .. }))
                .unwrap();
            let [EntityId::Value(_), EntityId::Value(owner)] = removed.results.as_slice() else {
                panic!("owned result")
            };
            let reads = function
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, instruction)| match instruction.operation {
                    Operation::NullableIsNull { owner: nullable }
                        if payload == "String" && nullable == *owner =>
                    {
                        Some(index)
                    }
                    Operation::AggregateProject {
                        aggregate,
                        field: 0,
                    } if payload == "Text" && aggregate == *owner => Some(index),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let drops = function.instructions.iter().enumerate().filter_map(|(index, instruction)| matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == *owner).then_some(index)).collect::<Vec<_>>();
            assert_eq!(reads.len(), 1, "{label}/{payload}");
            assert_eq!(drops.len(), 1, "{label}/{payload}");
            assert!(
                reads[0] < drops[0],
                "presence read must precede original ASAP cleanup"
            );
            assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Copy { source } if source == *owner)));
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}

#[test]
fn temporary_null_cleanup_presence_precedes_unique_result_drop() {
    for (payload, value) in [
        ("String", "\"text\".clone()"),
        ("Text", "Text(\"text\".clone())"),
        ("Resource", "Resource()"),
    ] {
        for condition in ["m.remove(1) != null", "null == ((m.remove(1)))"] {
            let source = format!(
                "value class Text(val text: String)\nclass Resource {{ deinit() {{ println(\"drop\") }} }}\nfun entry() {{ var m = mutableMapOf<Int, {payload}>(); m.put(1, {value}); val present = {condition}; if (present) {{ println(\"present\") }} }}"
            );
            for (label, program) in [("memory", memory_unit(&source)), ("rooted", unit(&source))] {
                let program =
                    program.unwrap_or_else(|error| panic!("{label}/{payload}: {error:?}"));
                super::verify::verify_program(&program).unwrap();
                let function = program.modules[0]
                    .functions
                    .iter()
                    .find(|function| function.name.contains("entry"))
                    .unwrap();
                let removed = function
                    .instructions
                    .iter()
                    .find(|instruction| {
                        matches!(instruction.operation, Operation::MapRemove { .. })
                    })
                    .unwrap();
                let [EntityId::Value(_), EntityId::Value(owner)] = removed.results.as_slice()
                else {
                    panic!("updated map and owned nullable result")
                };
                let reads = function
                    .instructions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, instruction)| match instruction.operation {
                        Operation::AggregateProject {
                            aggregate,
                            field: 0,
                        } if aggregate == *owner => Some(index),
                        Operation::NullableIsNull { owner: nullable } if nullable == *owner => {
                            Some(index)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let drops = function.instructions.iter().enumerate().filter_map(|(index, instruction)| matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == *owner).then_some(index)).collect::<Vec<_>>();
                assert_eq!(reads.len(), 1, "{label}/{payload}/{condition}");
                assert_eq!(drops.len(), 1, "{label}/{payload}/{condition}");
                assert!(reads[0] < drops[0]);
                assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Copy { source } if source == *owner)));
                crate::llvm::render_verified_program(&program).unwrap();
            }
        }
    }
}

#[test]
fn single_nullable_factory_planning_boundary_precedes_any_comparison() {
    let source = "class Resource(val text: String) { deinit() { println(this.text) } }\nfun make(): Resource? = Resource(\"plain nullable drop\")\nfun entry() { val value = make() }";
    // The former UnsupportedNode was a legacy single-file planner boundary, not a
    // language prohibition. Preserve the complete source through both Unit adapters.
    use super::model::{Origin, SsaTypeKind, TerminatorKind};
    use lang_frontend::{
        analysis::analyze_basic_unit_ownership,
        name_resolution::{
            SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::owned_compilation_unit_view,
        source::SourceMap,
        type_checking::{
            check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
        },
    };

    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let (source_id, parsed) =
            super::unit_lower_test_support::parsed(&mut sources, "factory.ko", source);
        let input = if rooted {
            SourceUnitInput::new("nullable-factory-root", "factory.ko", source_id, &parsed)
        } else {
            SourceUnitInput::memory_document(source_id, &parsed)
        };
        let inputs = [input];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        assert!(raw.diagnostics().is_empty());
        // Basic validation alone is not strict final type admission.
        complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw).unwrap();
        let (typed, owned) =
            analyze_basic_unit_ownership(&sources, &inputs, &names, &environment, raw)
                .unwrap()
                .into_result()
                .unwrap();
        assert!(owned.diagnostics().is_empty());
        let owned = owned.validate().unwrap();
        let declarations = names.names().index().declarations();
        let declaration = |name| {
            declarations
                .iter()
                .find(|d| d.name() == name && d.kind() == SymbolKind::Function)
                .unwrap()
        };
        let entry = declaration("entry");
        let make = declaration("make");
        let make_span = parsed.ast().items().get(make.root()).unwrap().span();
        assert_eq!(
            sources.slice(make_span).unwrap(),
            "fun make(): Resource? = Resource(\"plain nullable drop\")"
        );
        let entry_span = parsed.ast().items().get(entry.root()).unwrap().span();
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
                .unwrap();
        let (program, entry_id) =
            super::unit_lower::lower_owned_unit_with_entry(&unit, entry.id()).unwrap();
        super::verify::verify_program(&program).unwrap();
        let module = program.module(entry_id.module()).unwrap();
        let entry = module.function(entry_id).unwrap();
        assert_eq!(entry.origin, Origin::Source(entry_span));
        let factories = module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(make_span))
            .collect::<Vec<_>>();
        let [factory] = factories.as_slice() else {
            panic!("one original factory")
        };
        let allocations = factory
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. }))
            .collect::<Vec<_>>();
        let [allocation] = allocations.as_slice() else {
            panic!("one Resource allocation")
        };
        let Operation::HeapAllocate {
            owner: resource_type,
            ..
        } = allocation.operation
        else {
            unreachable!()
        };
        assert!(matches!(
            module.type_kind(resource_type),
            Some(SsaTypeKind::HeapOwner { .. })
        ));
        let [EntityId::Value(resource)] = allocation.results.as_slice() else {
            panic!("Resource owner")
        };
        let wraps = factory.instructions.iter().filter(|i| matches!(i.operation, Operation::NullableWrap { owner, .. } if owner == *resource)).collect::<Vec<_>>();
        let [wrap] = wraps.as_slice() else {
            panic!("allocated Resource transferred once to nullable")
        };
        for origin in [&allocation.origin, &wrap.origin] {
            assert_eq!(
                sources.slice(origin.span()).unwrap(),
                "Resource(\"plain nullable drop\")"
            );
        }
        assert_eq!(allocation.block, wrap.block);
        let factory_block = factory.block(allocation.block).unwrap();
        let position = |id| {
            factory_block
                .instructions
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap()
        };
        assert!(position(allocation.id) < position(wrap.id));
        let Operation::NullableWrap { nullable, .. } = wrap.operation else {
            unreachable!()
        };
        assert_eq!(
            module.type_kind(nullable),
            Some(&SsaTypeKind::NullableHandle {
                inner: resource_type
            })
        );
        let [EntityId::Value(returned)] = wrap.results.as_slice() else {
            panic!("nullable owner")
        };
        assert_eq!(factory.return_types, vec![nullable]);
        let returns = factory
            .blocks
            .iter()
            .filter_map(|b| b.terminator.as_ref())
            .filter_map(|t| match &t.kind {
                TerminatorKind::Return { values } => Some(values),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(returns, vec![&vec![*returned]]);
        assert!(matches!(&factory_block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values == &vec![*returned]));
        let calls = entry.instructions.iter().enumerate().filter(|(_, i)| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == factory.id)).collect::<Vec<_>>();
        let [(call_index, call)] = calls.as_slice() else {
            panic!("one call to actual factory")
        };
        let Operation::DirectCall {
            receiver,
            arguments,
            ..
        } = &call.operation
        else {
            unreachable!()
        };
        assert!(receiver.is_none() && arguments.is_empty());
        assert_eq!(sources.slice(call.origin.span()).unwrap(), "make()");
        let [EntityId::Value(value)] = call.results.as_slice() else {
            panic!("factory result owner")
        };
        assert_eq!(
            entry
                .entity(EntityId::Value(*value))
                .unwrap()
                .ty
                .semantic_type(),
            nullable
        );
        let drops = entry
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::Drop { owner } if owner == *value))
            .collect::<Vec<_>>();
        let [(drop_index, drop)] = drops.as_slice() else {
            panic!("exactly one factory-result Drop")
        };
        assert!(call_index < drop_index);
        assert_eq!(call.block, drop.block);
        let entry_block = entry.block(call.block).unwrap();
        let position = |id| {
            entry_block
                .instructions
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap()
        };
        assert!(position(call.id) < position(drop.id));
        assert!(
            !entry
                .instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::Copy { source } if source == *value))
        );
        assert!(!entry.instructions.iter().any(
            |i| matches!(i.operation, Operation::NullableIsNull { owner } if owner == *value)
        ));
        let deinit_id = module
            .deinits
            .get(&resource_type)
            .expect("original Resource destructor registered");
        let deinit = module.function(*deinit_id).unwrap();
        assert_eq!(
            sources.slice(deinit.origin.span()).unwrap(),
            "deinit() { println(this.text) }"
        );
        // Rendering proves the normal verified LLVM path, not runtime deinit execution.
        crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    }
}
