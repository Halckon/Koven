//! Ordinary return-before-unsupported-storage observations; no presumed old lower verdict.

use lang_frontend::{
    ast::TypeRefId,
    name_resolution::{
        NameResolution, SourceUnitId, SourceUnitInput, SymbolKind, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::{
        Expression, FunctionBody, FunctionForm, Item, LiteralKind, NameMarker, Statement,
        VariableKind,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, TypeKind, TypedFile,
        UnitCallTarget, UnitTypeKind, UnitTypeRefId, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

use super::*;

const SINGLE_PATH: &str = "unreachable_nullable_map.ko";
const SINGLE_SOURCE: &str =
    "fun main(): Unit { return; val dead: MutableMap<Int, Int?>? = null }\n";
const PROVIDER_SOURCE: &str =
    "package p\nfun main(): Unit { return; val dead: MutableMap<Int, Int?>? = null }\n";
const CONSUMER_SOURCE: &str = "package q\nfun entry(): Unit { p.main() }\n";

#[derive(Clone, Copy, Debug)]
struct DeadSyntax {
    name: Span,
    type_ref: TypeRefId,
    main_return_type: TypeRefId,
}

fn dead_syntax(sources: &SourceMap, parsed: &ParsedFile) -> Option<DeadSyntax> {
    let [root] = parsed.roots() else {
        return None;
    };
    let Item::Function {
        name: NameMarker::Present(main_name),
        extension_receiver: None,
        type_parameters,
        parameters,
        form:
            FunctionForm::Explicit {
                type_ref: main_return_type,
                body: FunctionBody::Block(body),
                ..
            },
        ..
    } = parsed.ast().items().get(*root).ok()?.payload()
    else {
        return None;
    };
    if sources.slice(*main_name).ok()? != "main"
        || !type_parameters.is_empty()
        || !parameters.is_empty()
    {
        return None;
    }
    let Statement::Block { elements } = parsed.ast().statements().get(*body).ok()?.payload() else {
        return None;
    };
    let [returned, local] = elements.as_slice() else {
        return None;
    };
    let Statement::Expression { expression } =
        parsed.ast().statements().get(*returned).ok()?.payload()
    else {
        return None;
    };
    if !matches!(
        parsed.ast().expressions().get(*expression).ok()?.payload(),
        Expression::Return { value: None, .. }
    ) {
        return None;
    }
    let Statement::LocalVariable { declaration } =
        parsed.ast().statements().get(*local).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::Val,
        name: NameMarker::Present(name),
        type_ref: Some(type_ref),
        initializer,
        ..
    } = parsed.ast().items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    if sources.slice(*name).ok()? != "dead"
        || !matches!(
            parsed.ast().expressions().get(*initializer).ok()?.payload(),
            Expression::Literal(LiteralKind::Null)
        )
    {
        return None;
    }
    Some(DeadSyntax {
        name: *name,
        type_ref: *type_ref,
        main_return_type: *main_return_type,
    })
}

fn inspect_single(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    trace: &mut Trace,
) {
    let syntax = dead_syntax(sources, parsed);
    trace.check(
        "exact main Block: valueless Return then only dead typed null declaration",
        syntax.is_some(),
        syntax,
    );
    let Some(syntax) = syntax else {
        return;
    };
    let roots: Vec<_> = names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == "dead"
                && symbol.kind() == SymbolKind::Variable
                && !symbol.is_synthetic()
                && symbol.span() == syntax.name
        })
        .collect();
    let storage = match roots.as_slice() {
        [root] => typed.symbol_type(root.id()),
        _ => None,
    };
    let int = typed.types().builtin(BuiltinType::Int);
    let nullable_int = int.and_then(|int| typed.types().find(&TypeKind::Nullable(int)));
    let map = int.zip(nullable_int).and_then(|(key, value)| {
        typed.types().find(&TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::MutableMap,
            arguments: vec![key, value],
        })
    });
    let nullable_map = map.and_then(|map| typed.types().find(&TypeKind::Nullable(map)));
    let declared = typed.type_ref_type(syntax.type_ref);
    trace.check(
        "actual dead declaration and storage are Nullable<MutableMap<Int, Nullable<Int>>>",
        nullable_map.is_some()
            && roots.len() == 1
            && storage == nullable_map
            && declared == nullable_map
            && typed.type_ref_type(syntax.main_return_type)
                == typed.types().builtin(BuiltinType::Unit),
        (roots, storage, declared, nullable_map, typed.types()),
    );
    let counts = [
        typed.map_constructions().len(),
        typed.map_sizes().len(),
        typed.map_contains_calls().len(),
        typed.map_gets().len(),
        typed.map_puts().len(),
        typed.map_removes().len(),
        typed.map_with_values().len(),
        typed.map_consumes().len(),
        typed.map_require_values().len(),
        typed.non_null_assertions().len(),
    ];
    trace.check(
        "no Map operation or assertion descriptors",
        counts == [0; 10],
        counts,
    );
    trace.check(
        "declared dead storage is concretely pure memory",
        storage.is_some_and(|ty| typed.is_resource_type(ty) == Some(false)),
        storage.and_then(|ty| typed.is_resource_type(ty)),
    );
}

fn inspect_unit(
    sources: &SourceMap,
    file: (SourceUnitId, &ParsedFile),
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    trace: &mut Trace,
) {
    let syntax = dead_syntax(sources, file.1);
    trace.check(
        "exact provider main Block: valueless Return then only dead typed null declaration",
        syntax.is_some(),
        syntax,
    );
    let Some(syntax) = syntax else {
        return;
    };
    let roots: Vec<_> = names
        .names()
        .source_units()
        .iter()
        .filter(|source| source.source_unit() == file.0)
        .flat_map(|source| source.resolution().symbols())
        .filter(|symbol| {
            symbol.name() == "dead"
                && symbol.kind() == SymbolKind::Variable
                && !symbol.is_synthetic()
                && symbol.span() == syntax.name
        })
        .collect();
    let root = match roots.as_slice() {
        [root] => typed
            .body_symbol_types()
            .keys()
            .copied()
            .find(|symbol| symbol.source_unit() == file.0 && symbol.symbol() == root.id()),
        _ => None,
    };
    let storage = root.and_then(|root| typed.symbol_type(root));
    let int = typed.types().builtin(BuiltinType::Int);
    let nullable_int = int.and_then(|int| typed.types().find(&UnitTypeKind::Nullable(int)));
    let map = int.zip(nullable_int).and_then(|(key, value)| {
        typed.types().find(&UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::MutableMap,
            arguments: vec![key, value],
        })
    });
    let nullable_map = map.and_then(|map| typed.types().find(&UnitTypeKind::Nullable(map)));
    // These public IDs qualify actual AST nodes; no private SymbolId or typed fact is fabricated.
    let declared = typed.type_ref_type(UnitTypeRefId::new(file.0, syntax.type_ref));
    let main_return = typed.type_ref_type(UnitTypeRefId::new(file.0, syntax.main_return_type));
    trace.check("actual qualified dead declaration and storage are Nullable<MutableMap<Int, Nullable<Int>>>",
        nullable_map.is_some() && root.is_some() && storage == nullable_map && declared == nullable_map
            && main_return == typed.types().builtin(BuiltinType::Unit),
        (root, storage, declared, nullable_map, main_return, typed.types()));
    let counts = [
        typed.map_constructions().len(),
        typed.map_sizes().len(),
        typed.map_contains_calls().len(),
        typed.map_gets().len(),
        typed.map_puts().len(),
        typed.map_removes().len(),
        typed.map_with_values().len(),
        typed.map_consumes().len(),
        typed.map_require_values().len(),
        typed.non_null_assertions().len(),
    ];
    trace.check(
        "no unit Map operation or assertion descriptors",
        counts == [0; 10],
        counts,
    );
    trace.check(
        "qualified declared dead storage is concretely pure memory",
        storage.is_some_and(|ty| typed.is_resource_type(ty) == Some(false)),
        storage.and_then(|ty| typed.is_resource_type(ty)),
    );
}

fn observe_single(trace: &mut Trace) -> Outcome<LowerObservation> {
    let mut sources = SourceMap::new();
    let parsed = parsed(&mut sources, SINGLE_PATH, SINGLE_SOURCE, trace)?;
    let (environment, types) = standard_environments();
    let names = step(
        "names",
        resolve_names(&sources, &parsed, &environment),
        trace,
    )?;
    diagnostics("names diagnostics", &sources, names.diagnostics(), trace)?;
    let entries: Vec<_> = names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == "main"
                && symbol.kind() == SymbolKind::Function
                && !symbol.is_synthetic()
                && symbol.span().source_id() == parsed.source_id()
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(Failure {
            stage: "unique real single main".to_owned(),
            detail: format!("candidates={entries:#?}; symbols={:#?}", names.symbols()),
        });
    };
    trace.stages.push(format!(
        "selected real main={entry:#?}; {}",
        origin(&sources, entry.span())
    ));
    let typed = step(
        "raw types",
        check_types(&sources, &parsed, &names, &types),
        trace,
    )?;
    let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
    inspect_single(&sources, &parsed, &names, &typed, trace);
    let complete = step(
        "complete types",
        complete_file_types(&sources, &parsed, &names, &types, &typed)
            .map_err(|error| admission_error(&sources, error)),
        trace,
    )?;
    raw?;
    trace.check(
        "same complete analysis and original facts",
        complete.is_same_analysis(&typed) && std::ptr::eq(complete.types(), &typed),
        (
            complete.is_same_analysis(&typed),
            std::ptr::eq(complete.types(), &typed),
        ),
    );
    diagnostics(
        "complete diagnostics",
        &sources,
        complete.types().diagnostics(),
        trace,
    )?;
    let owned = step(
        "ownership",
        check_ownership(&sources, &parsed, &names, complete.types()),
        trace,
    )?;
    let clean = diagnostics(
        "ownership diagnostics",
        &sources,
        owned.diagnostics(),
        trace,
    );
    trace.stages.push(format!(
        "ownership deferred={:#?}; drops={:#?}; assertion plans={:#?}",
        owned.deferred(),
        owned.drops(),
        owned.non_null_assertions()
    ));
    let complete_ownership = gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    trace.check(
        "no assertion ownership plans in the return/dead declaration source",
        owned.non_null_assertions().is_empty(),
        owned.non_null_assertions(),
    );
    clean?;
    complete_ownership?;
    trace
        .stages
        .push("attempt lower SSA with real main".to_owned());
    Ok(LowerObservation {
        entry: selected_entry(&sources, entry.name(), entry.span()),
        outcome: finish_lower(
            &sources,
            crate::ssa::lower_scalar_file_with_entry(
                &sources,
                &parsed,
                &names,
                complete.types(),
                &owned,
                entry.id(),
            ),
            trace,
        ),
    })
}

fn observe_unit(trace: &mut Trace) -> Outcome<LowerObservation> {
    let mut sources = SourceMap::new();
    let provider = parsed(&mut sources, "p/provider.ko", PROVIDER_SOURCE, trace);
    let consumer = parsed(&mut sources, "q/consumer.ko", CONSUMER_SOURCE, trace);
    let (provider, consumer) = (provider?, consumer?);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider.source_id(), &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer.source_id(), &consumer),
    ];
    let (environment, types) = standard_environments();
    let index = step("index", index_compilation_unit(&sources, &inputs), trace)?;
    let names = step(
        "names",
        resolve_compilation_unit_names(&sources, &inputs, &index, &environment),
        trace,
    )?;
    diagnostics("names diagnostics", &sources, names.diagnostics(), trace)?;
    let names = step(
        "qualified names",
        names
            .validate()
            .map_err(|value| diagnostic_text(&sources, value.diagnostics())),
        trace,
    )?;
    let index = names.names().index();
    let provider_units: Vec<_> = index
        .source_units()
        .iter()
        .filter(|unit| {
            unit.source_id() == provider.source_id()
                && unit.key().logical_path().as_str() == "p/provider.ko"
        })
        .collect();
    let consumer_units: Vec<_> = index
        .source_units()
        .iter()
        .filter(|unit| {
            unit.source_id() == consumer.source_id()
                && unit.key().logical_path().as_str() == "q/consumer.ko"
        })
        .collect();
    let ([provider_unit], [consumer_unit]) = (provider_units.as_slice(), consumer_units.as_slice())
    else {
        return Err(Failure {
            stage: "unique real provider and consumer".to_owned(),
            detail: format!("provider={provider_units:#?}; consumer={consumer_units:#?}"),
        });
    };
    gate(
        "two distinct source-qualified packages",
        index.source_units().len() == 2
            && provider_unit.id() != consumer_unit.id()
            && index.packages().iter().any(|package| {
                package.id() == provider_unit.package() && package.name().segments() == ["p"]
            })
            && index.packages().iter().any(|package| {
                package.id() == consumer_unit.package() && package.name().segments() == ["q"]
            }),
        (index.source_units(), index.packages()),
    )?;
    let entries: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == consumer_unit.id()
                && declaration.package() == consumer_unit.package()
                && declaration.name_span().source_id() == consumer.source_id()
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(Failure {
            stage: "unique real q.entry".to_owned(),
            detail: format!(
                "candidates={entries:#?}; declarations={:#?}",
                index.declarations()
            ),
        });
    };
    trace.stages.push(format!(
        "selected real q.entry={entry:#?}; sources={:#?}",
        index.source_units()
    ));
    let typed = step(
        "raw types",
        check_compilation_unit_types(&sources, &inputs, &names, &types),
        trace,
    )?;
    let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
    let mains: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == provider_unit.id()
                && declaration.package() == provider_unit.package()
                && declaration.name_span().source_id() == provider.source_id()
        })
        .collect();
    let consumer_calls: Vec<_> = typed
        .calls()
        .iter()
        .filter(|call| call.expression().source_unit() == consumer_unit.id())
        .collect();
    trace.check(
        "real q.entry consumer call targets actual p.main",
        matches!((mains.as_slice(), consumer_calls.as_slice()), ([main], [call])
            if call.target() == UnitCallTarget::Declaration(main.id())
                && Some(call.return_type()) == typed.types().builtin(BuiltinType::Unit)
                && call.receiver().is_none() && call.arguments().is_empty()),
        (mains, consumer_calls),
    );
    inspect_unit(
        &sources,
        (provider_unit.id(), &provider),
        &names,
        &typed,
        trace,
    );
    let complete = step(
        "complete types",
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| admission_error(&sources, error)),
        trace,
    )?;
    raw?;
    trace.check(
        "same complete analysis and original facts",
        complete.is_same_analysis(&typed) && std::ptr::eq(complete.types(), &typed),
        (
            complete.is_same_analysis(&typed),
            std::ptr::eq(complete.types(), &typed),
        ),
    );
    diagnostics(
        "complete diagnostics",
        &sources,
        complete.types().diagnostics(),
        trace,
    )?;
    let typed = step(
        "qualified types",
        typed
            .validate()
            .map_err(|value| diagnostic_text(&sources, value.diagnostics())),
        trace,
    )?;
    let owned = step(
        "ownership",
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed),
        trace,
    )?;
    let clean = diagnostics(
        "ownership diagnostics",
        &sources,
        owned.diagnostics(),
        trace,
    );
    trace.stages.push(format!(
        "ownership deferred={:#?}; drops={:#?}; assertion plans={:#?}",
        owned.deferred(),
        owned.drops(),
        owned.non_null_assertions()
    ));
    let complete_ownership = gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    trace.check(
        "no assertion ownership plans in the return/dead declaration source",
        owned.non_null_assertions().is_empty(),
        owned.non_null_assertions(),
    );
    clean?;
    complete_ownership?;
    let owned = step(
        "qualified ownership",
        owned.validate().map_err(|value| {
            format!(
                "{}; deferred={:#?}",
                diagnostic_text(&sources, value.diagnostics()),
                value.deferred()
            )
        }),
        trace,
    )?;
    trace
        .stages
        .push("attempt lower SSA with real q.entry".to_owned());
    Ok(LowerObservation {
        entry: selected_entry(&sources, entry.name(), entry.name_span()),
        outcome: finish_lower(
            &sources,
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                entry.id(),
            ),
            trace,
        ),
    })
}

#[test]
fn unreachable_nullable_map_storage_collect_before_baseline() {
    let mut observations = Vec::with_capacity(2);
    for unit in [false, true] {
        let mut trace = Trace::default();
        let outcome = if unit {
            observe_unit(&mut trace)
        } else {
            observe_single(&mut trace)
        };
        observations.push(Observation {
            label: if unit { "dead/unit" } else { "dead/single" }.to_owned(),
            trace,
            outcome,
        });
    }
    for observation in &observations {
        println!(
            "UNREACHABLE NULLABLE MAP {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        println!("input failures={:?}", observation.trace.input_failures);
        print_outcome(&observation.outcome);
    }
    assert_eq!(observations.len(), 2);
    assert!(
        observations.iter().all(|observation| {
            observation.outcome.as_ref().is_ok_and(|observed| {
                let (name, path) = if observation.label == "dead/unit" {
                    ("entry", "q/consumer.ko")
                } else {
                    ("main", SINGLE_PATH)
                };
                observed.entry.name == name
                    && observed
                        .entry
                        .path
                        .as_ref()
                        .is_ok_and(|actual| actual.as_str() == path)
                    && !observed.entry.span.is_empty()
            }) && observation.trace.input_failures.is_empty()
        }),
        "both ordinary inputs must pass FE/complete/ownership/input and actually attempt lower"
    );
    println!(
        "COLLECTION ONLY: two actual lower outcomes recorded; no old success or rejection \
        verdict is assumed. Freeze the precise old table before the first-local planning change."
    );
}
