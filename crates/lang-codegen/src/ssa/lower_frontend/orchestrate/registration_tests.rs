//! CI-only mapper contract: real FE identities, no descriptor pre-scan or SSA/native claim.

use std::collections::BTreeSet;

use lang_frontend::{
    lexer::lex,
    name_resolution::{SymbolKind, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    type_checking::{
        IntrinsicTypeConstructor, TypeKind, check_types, complete_file_types, standard_environments,
    },
};

use super::*;
use crate::ssa::model::{MapContainerKind, Module, Ownership, SsaTypeId, SsaTypeKind};

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
        BuiltinType::String => assert_eq!(module.type_kind(id), Some(&SsaTypeKind::StringOwner)),
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

fn observe(text: &str, path: &str, cases: &[Case], ordinary_first: bool) -> Vec<String> {
    let mut sources = SourceMap::new();
    let source = sources.add_source(path, text).expect("unique source");
    let lexed = lex(&sources, source).expect("lex");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).expect("parse");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let complete = complete_file_types(&sources, &parsed, &names, &types, &typed)
        .expect("complete original type analysis");
    assert!(complete.is_same_analysis(&typed));
    assert!(std::ptr::eq(complete.types(), &typed));
    assert!(complete.types().diagnostics().is_empty());
    let owned = check_ownership(&sources, &parsed, &names, complete.types()).expect("ownership");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    validate_inputs(&sources, &parsed, &names, &typed, &owned).expect("ordinary input gate");

    let declarations = collect_functions(&parsed, &names, &typed).expect("real declarations");
    let templates = declarations
        .iter()
        .map(|declaration| FunctionTemplate {
            symbol: declaration.symbol,
            type_parameters: declaration.callable.type_parameters().to_vec(),
            span: declaration.span,
        })
        .collect::<Vec<_>>();
    let mut program = Program::default();
    let module_id = program.add_module("registration");
    let module = program.module_mut(module_id).expect("fresh module");
    let mut mapper = NominalTypeMapper::new(&typed).expect("fresh mapper");
    let deinits = resource_deinit::declare(module, &mut mapper, &parsed, &names, &typed)
        .expect("full deinit declaration");
    let deinit_spans = deinits.iter().map(|plan| plan.span).collect::<Vec<_>>();
    let instances = plan_instances(&parsed, &typed, &owned, &templates, &deinit_spans)
        .expect("full instance planning");
    assert!(instances.instances().iter().any(|instance| {
        names
            .symbols()
            .iter()
            .any(|symbol| symbol.id() == instance.key.symbol() && symbol.name() == "main")
    }));
    let resource = cases.iter().any(|case| case.value.is_none());
    assert_eq!(deinits.len(), usize::from(resource));
    assert_eq!(module.deinits.len(), deinits.len());
    for plan in &deinits {
        let function = module
            .function(plan.id)
            .expect("declared real deinit function");
        let Some(EntityType::Loan {
            kind: crate::ssa::model::LoanKind::Shared,
            target,
        }) = function.receiver()
        else {
            panic!("deinit has a Borrow receiver")
        };
        assert_eq!(module.deinits.get(&target), Some(&plan.id));
        assert_eq!(
            typed.is_resource_type(plan.descriptor.receiver_type()),
            Some(true)
        );
    }

    // No map-result descriptor pre-scan. Assert preparation did not register any target.
    assert!(module.nullable_maps.is_empty());
    assert!(module.map_results.is_empty());
    assert_eq!(typed.map_removes().len(), cases.len());
    let mut frontend_inputs = Vec::new();
    for &case in cases {
        let named = |name: &str| {
            let matches = names
                .symbols()
                .iter()
                .filter(|symbol| {
                    symbol.name() == name
                        && symbol.kind() == SymbolKind::Variable
                        && !symbol.is_synthetic()
                        && symbol.span().source_id() == source
                })
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "unique real local {name}");
            (
                typed.symbol_type(matches[0].id()).expect("symbol type"),
                matches[0].span(),
            )
        };
        let (nullable, local_span) = named(case.local);
        let Some(TypeKind::Nullable(inner)) = typed.types().get(nullable) else {
            panic!("real ordinary local is nullable");
        };
        let Some(TypeKind::Intrinsic {
            constructor,
            arguments,
        }) = typed.types().get(*inner)
        else {
            panic!("real nullable inner is intrinsic");
        };
        assert_eq!(*constructor, case.constructor);
        let [key, value] = arguments.as_slice() else {
            panic!("exact two Map arguments")
        };
        assert_eq!(Some(*key), typed.types().builtin(case.key));
        if let Some(builtin) = case.value {
            assert_eq!(Some(*value), typed.types().builtin(builtin));
        } else {
            assert_eq!(
                Some(*value),
                deinits.first().map(|plan| plan.descriptor.receiver_type())
            );
        }
        let call = format!("{}.remove(1)", case.outer);
        let removes = typed
            .map_removes()
            .iter()
            .filter(|remove| {
                let span = parsed
                    .ast()
                    .expressions()
                    .get(remove.expression())
                    .expect("real remove")
                    .span();
                sources.slice(span).expect("remove source") == call
            })
            .collect::<Vec<_>>();
        assert_eq!(removes.len(), 1, "unique real remove {call}");
        let remove = removes[0];
        let remove_span = parsed
            .ast()
            .expressions()
            .get(remove.expression())
            .expect("remove")
            .span();
        assert_eq!(remove.result_type(), nullable);
        assert_eq!(remove.value_type(), *inner);
        assert_eq!(typed.expression_type(remove.expression()), Some(nullable));
        assert_eq!(
            Some(remove.key_type()),
            typed.types().builtin(BuiltinType::Int)
        );
        let (outer, _) = named(case.outer);
        assert_eq!(typed.expression_type(remove.receiver()), Some(outer));
        assert!(matches!(typed.types().get(outer),
            Some(TypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::MutableMap, arguments })
                if arguments.as_slice() == [remove.key_type(), *inner]));
        assert!(
            !mapper.contains_type(nullable),
            "preparation must not warm nullable cache"
        );
        assert!(
            !mapper.contains_type(*inner),
            "preparation must not warm Map cache"
        );
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
    let mut wrappers = BTreeSet::new();
    let mut cache_pairs = Vec::new();
    let mut summaries = Vec::new();
    for (case, nullable, inner, local_span, remove_span) in frontend_inputs {
        assert!(!mapper.contains_type(nullable));
        assert!(!mapper.contains_type(inner));
        let first = if ordinary_first {
            mapper.intern(module, &names, &typed, nullable, local_span)
        } else {
            mapper.intern_map_result(module, &names, &typed, nullable, remove_span)
        }
        .expect("first real mapper registration");
        let after_first = snapshot(module, first);
        cache_pairs.push((nullable, first));
        cache_pairs.push((
            inner,
            module.nullable_map_inner(first).expect("exact inner"),
        ));
        let semantic = storage_identity(module, first, case);
        let second = if ordinary_first {
            mapper.intern_map_result(module, &names, &typed, nullable, remove_span)
        } else {
            mapper.intern(module, &names, &typed, nullable, local_span)
        }
        .expect("second real mapper registration");
        assert_eq!(
            after_first,
            snapshot(module, second),
            "same mapper exact identity/N/R/type count"
        );
        assert_eq!(semantic, storage_identity(module, second, case));
        assert!(
            wrappers.insert(first),
            "different kind or K/V must have different wrappers"
        );
        assert_eq!(module.nullable_maps.len(), wrappers.len());
        summaries.push(format!(
            "{path}:{}:{}..{}:{semantic}:{call}",
            case.local,
            local_span.start(),
            local_span.end(),
            call = sources.slice(remove_span).expect("source")
        ));
    }
    let (mapped, _, _) = mapper.into_parts();
    for (frontend, storage) in cache_pairs {
        assert_eq!(
            mapped.get(&frontend),
            Some(&storage),
            "exact frontend cache identity"
        );
    }
    summaries
}

#[test]
fn nullable_map_registration_order_preserves_identity_and_resource_deinit() {
    // Each observation owns a fresh FE analysis, Program and mapper. Never compare bare IDs across them.
    assert_eq!(
        observe(ORDINARY, "ordinary.ko", CASES, true),
        observe(ORDINARY, "ordinary.ko", CASES, false)
    );
    let (path, single, provider, consumer) = registration_r1_source();
    assert_eq!(path, "r1_ordinary_source_before_remove.ko");
    assert_eq!(provider, format!("package p\n{single}"));
    assert_eq!(consumer, "package q\nfun entry(): Unit { p.main() }\n");
    assert_eq!(
        observe(single, path, &[RESOURCE], true),
        observe(single, path, &[RESOURCE], false)
    );
}
