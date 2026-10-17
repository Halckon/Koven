//! Source-only guards for conditional writes and callable-creation flow scope.
use super::*;

#[derive(Clone, Copy, Debug)]
enum ScopeCase {
    ShortCircuitWrite,
    LoopWrite,
    LambdaDoesNotPublishProof,
    LambdaPreservesIncomingProof,
}

impl ScopeCase {
    fn source(self) -> &'static str {
        match self {
            Self::ShortCircuitWrite => {
                "fun mutate(inout value: Boolean?): Boolean = true
fun extract(input: Boolean?, flag: Boolean): Boolean {
    var x = input
    return (when (x) {
        null -> false;
        flag && mutate(&x) -> false;
        else -> x
    })
}"
            }
            Self::LoopWrite => {
                "fun extract(input: Boolean?, replacement: Boolean?, flag: Boolean): Boolean {
    var x = input
    return (when (x) {
        null -> false;
        if (flag) {
            while (flag) { x = replacement }
            true
        } else { false } -> false;
        else -> x
    })
}"
            }
            Self::LambdaDoesNotPublishProof => {
                "fun extract(x: Int?): Int {
    val callback: () -> Int = {
        when (x) { null -> return 0; else -> x }
    }
    return x
}"
            }
            Self::LambdaPreservesIncomingProof => {
                "fun extract(x: Int?): Int {
    if (x == null) { return 0 }
    val callback: () -> Int = { x }
    return x
}"
            }
        }
    }

    fn preserves(self) -> bool {
        matches!(self, Self::LambdaPreservesIncomingProof)
    }
}

#[test]
fn conditional_writes_and_lambda_creation_preserve_source_proof_boundaries() {
    let mut observations = Vec::with_capacity(8);
    for case in [
        ScopeCase::ShortCircuitWrite,
        ScopeCase::LoopWrite,
        ScopeCase::LambdaDoesNotPublishProof,
        ScopeCase::LambdaPreservesIncomingProof,
    ] {
        for unit in [false, true] {
            let mut observation = Observation {
                label: format!("{case:?}/{}", if unit { "unit" } else { "single" }),
                stage: "parse; names/raw/complete not reached",
                checks: Vec::new(),
                details: Vec::new(),
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut sources = SourceMap::new();
                let source = sources
                    .add_source("nullable_when.ko", case.source())
                    .map_err(|error| format!("source internal: {error:?}"))?;
                let file =
                    parser_test_assertions::parse_file_twice(&sources, source, &observation.label);
                observation
                    .details
                    .push(format!("parse {:?}", file.diagnostics()));
                if !file.diagnostics().is_empty() {
                    return Err("parse diagnostics; names/raw/complete not reached".to_owned());
                }
                if matches!(case, ScopeCase::ShortCircuitWrite | ScopeCase::LoopWrite) {
                    // Reuse the existing negative Boolean local-var contract, not its source.
                    // It checks the actual when subject/else reference, storage and target proof.
                    if unit {
                        observe_unit(Case::InoutCondition, &sources, &file, &mut observation)
                    } else {
                        observe_single(Case::InoutCondition, &sources, &file, &mut observation)
                    }
                } else if unit {
                    observe_lambda_unit(case, &sources, &file, &mut observation)
                } else {
                    observe_lambda_single(case, &sources, &file, &mut observation)
                }
            }));
            match result {
                Ok(Ok(())) => observation.stage = "observed",
                Ok(Err(error)) => observation.details.push(error),
                Err(payload) => {
                    let message = payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("non-string panic payload");
                    observation
                        .details
                        .push(format!("harness/compiler panic: {message}"));
                }
            }
            observations.push(observation);
        }
    }
    eprintln!("scope boundary observations:\n{observations:#?}");
    assert_eq!(observations.len(), 8);
    assert!(
        observations.iter().all(Observation::passed),
        "{observations:#?}"
    );
}

#[derive(Debug)]
struct LambdaTargets {
    lambda: ExpressionId,
    inner: ExpressionId,
    outer: ExpressionId,
}

fn lambda_targets(
    sources: &SourceMap,
    file: &ParsedFile,
    case: ScopeCase,
) -> Result<LambdaTargets, String> {
    let lambdas = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
        })
        .collect::<Vec<_>>();
    let [(lambda, span)] = lambdas.as_slice() else {
        return Err("fixture needs one actual lambda".to_owned());
    };
    let inner = if case.preserves() {
        let reads = file
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                (within(*span, node.span())
                    && matches!(node.payload(), Expression::Name)
                    && sources.slice(node.span()) == Ok("x"))
                .then_some(id)
            })
            .collect::<Vec<_>>();
        let [read] = reads.as_slice() else {
            return Err("fixture needs one lambda-body x read".to_owned());
        };
        *read
    } else {
        let (_, _, target) = when_targets(file, Case::Parameter)?;
        if !within(*span, expression_span(file, target)?) {
            return Err("when target is outside lambda".to_owned());
        }
        target
    };
    let outer_reads = file
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
            let value = peel(file, *value).ok()?;
            let value_span = expression_span(file, value).ok()?;
            (!within(*span, node.span()) && sources.slice(value_span) == Ok("x")).then_some(value)
        })
        .collect::<Vec<_>>();
    let [outer] = outer_reads.as_slice() else {
        return Err("fixture needs one outer return x".to_owned());
    };
    Ok(LambdaTargets {
        lambda: *lambda,
        inner,
        outer: *outer,
    })
}

fn observe_lambda_single(
    case: ScopeCase,
    sources: &SourceMap,
    file: &ParsedFile,
    observation: &mut Observation,
) -> Result<(), String> {
    let (environment, types) = standard_environments();
    observation.stage = "names; raw/complete not reached";
    let names =
        resolve_names(sources, file, &environment).map_err(|e| format!("names internal: {e:?}"))?;
    observation
        .details
        .push(format!("names {:?}", names.diagnostics()));
    if !names.diagnostics().is_empty() {
        return Err("names diagnostics; raw/complete not reached".to_owned());
    }
    observation.stage = "raw; complete not reached";
    let typed =
        check_types(sources, file, &names, &types).map_err(|e| format!("raw internal: {e:?}"))?;
    let expressions = file
        .ast()
        .expressions()
        .iter()
        .map(|(id, node)| {
            let ty = typed.expression_type(id);
            (
                id,
                node.span(),
                sources.slice(node.span()),
                ty,
                ty.and_then(|ty| typed.types().get(ty)),
            )
        })
        .collect::<Vec<_>>();
    observation.details.push(format!("raw {:?}; expressions {expressions:#?}; symbols {:?}; references {:?}; nonnull {:?}; calls {:?}",
        typed.diagnostics(), names.symbols(), names.references(), typed.non_null_uses(), typed.calls()));
    observation.stage = "complete";
    record_admission(
        if case.preserves() {
            Case::Parameter
        } else {
            Case::RepeatedField
        },
        typed.diagnostics(),
        complete_file_types(sources, file, &names, &types, &typed)
            .map(|complete| complete.is_same_analysis(&typed)),
        observation,
    );
    observation.stage = "fact inspection; complete recorded";
    let targets = lambda_targets(sources, file, case)?;
    let symbol = single_reference(file, &names, targets.inner);
    let storage = symbol.and_then(|symbol| typed.symbol_type(symbol));
    let inner_type = typed.types().builtin(BuiltinType::Int);
    observation.details.push(format!("targets {targets:?}; storage {storage:?}/{:?}; inner read {:?}; outer read {:?}; lambda type {:?}",
        storage.and_then(|ty| typed.types().get(ty)), typed.expression_type(targets.inner), typed.expression_type(targets.outer), typed.expression_type(targets.lambda)));
    observation.check(
        "both reads use one real value parameter",
        symbol.is_some()
            && single_reference(file, &names, targets.outer) == symbol
            && names.symbols().iter().any(|candidate| {
                Some(candidate.id()) == symbol && source_kind(file, candidate, Case::Parameter)
            }),
    );
    observation.check("declared storage remains Nullable(Int)", inner_type.is_some()
        && matches!(storage.and_then(|ty| typed.types().get(ty)), Some(TypeKind::Nullable(inner)) if Some(*inner) == inner_type));
    for (label, expression) in [
        ("inner read has its own valid proof", targets.inner),
        ("outer proof equals creation-point proof", targets.outer),
    ] {
        let facts = typed
            .non_null_uses()
            .iter()
            .filter(|fact| fact.expression() == expression)
            .collect::<Vec<_>>();
        let expected = expression == targets.inner || case.preserves();
        observation.check(
            label,
            if expected {
                typed.expression_type(expression) == inner_type
                    && facts.len() == 1
                    && facts.iter().all(|fact| {
                        Some(fact.symbol()) == symbol
                            && Some(fact.declared_type()) == storage
                            && Some(fact.narrowed_type()) == inner_type
                            && typed.non_null_use(expression) == Some(**fact)
                    })
            } else {
                facts.is_empty() && typed.non_null_use(expression).is_none()
            },
        );
    }
    record_diagnostics(
        if case.preserves() {
            Case::Parameter
        } else {
            Case::RepeatedField
        },
        typed.diagnostics(),
        expression_span(file, targets.outer)?,
        observation,
    );
    observation.check(
        "lambda is created and never called",
        typed.calls().is_empty()
            && file
                .ast()
                .expressions()
                .iter()
                .all(|(_, node)| !matches!(node.payload(), Expression::Call { .. })),
    );
    Ok(())
}

fn observe_lambda_unit(
    case: ScopeCase,
    sources: &SourceMap,
    file: &ParsedFile,
    observation: &mut Observation,
) -> Result<(), String> {
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable_when.ko",
        file.source_id(),
        file,
    )];
    observation.stage = "index/names; raw/complete not reached";
    let index =
        index_compilation_unit(sources, &inputs).map_err(|e| format!("index internal: {e:?}"))?;
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &environment)
        .map_err(|e| format!("names internal: {e:?}"))?;
    observation
        .details
        .push(format!("names {:?}", names.diagnostics()));
    let names = names
        .validate()
        .map_err(|names| format!("names {:?}; raw/complete not reached", names.diagnostics()))?;
    let local = names
        .names()
        .source_units()
        .iter()
        .find(|local| {
            names.names().index().source_units().iter().any(|unit| {
                unit.id() == local.source_unit() && unit.source_id() == file.source_id()
            })
        })
        .ok_or("real source unit missing")?;
    let source = local.source_unit();
    observation.stage = "raw; complete not reached";
    let typed = check_compilation_unit_types(sources, &inputs, &names, &types)
        .map_err(|e| format!("raw internal: {e:?}"))?;
    let expressions = file
        .ast()
        .expressions()
        .iter()
        .map(|(id, node)| {
            let expression = UnitExpressionId::new(source, id);
            let ty = typed.expression_type(expression);
            (
                expression,
                node.span(),
                sources.slice(node.span()),
                ty,
                ty.and_then(|ty| typed.types().get(ty)),
            )
        })
        .collect::<Vec<_>>();
    observation.details.push(format!("raw {:?}; source {source:?}; expressions {expressions:#?}; symbols {:?}; storage {:?}; references {:?}; nonnull {:?}; calls {:?}",
        typed.diagnostics(), local.resolution().symbols(), typed.body_symbol_types(), names.names().references(), typed.non_null_uses(), typed.calls()));
    observation.stage = "complete";
    record_admission(
        if case.preserves() {
            Case::Parameter
        } else {
            Case::RepeatedField
        },
        typed.diagnostics(),
        complete_compilation_unit_types(sources, &inputs, &names, &types, &typed)
            .map(|complete| complete.is_same_analysis(&typed)),
        observation,
    );
    observation.stage = "fact inspection; complete recorded";
    let targets = lambda_targets(sources, file, case)?;
    let symbol = unit_reference(file, &names, source, targets.inner);
    let storage = symbol.and_then(|symbol| typed.symbol_type(symbol));
    let inner_type = typed.types().builtin(BuiltinType::Int);
    let inner = UnitExpressionId::new(source, targets.inner);
    let outer = UnitExpressionId::new(source, targets.outer);
    observation.details.push(format!("targets {targets:?}; storage {storage:?}/{:?}; inner read {:?}; outer read {:?}; lambda type {:?}",
        storage.and_then(|ty| typed.types().get(ty)), typed.expression_type(inner), typed.expression_type(outer), typed.expression_type(UnitExpressionId::new(source, targets.lambda))));
    observation.check(
        "both reads use one real source-qualified parameter",
        symbol.is_some_and(|symbol| symbol.source_unit() == source)
            && unit_reference(file, &names, source, targets.outer) == symbol
            && local.resolution().symbols().iter().any(|candidate| {
                symbol.is_some_and(|symbol| symbol.symbol() == candidate.id())
                    && source_kind(file, candidate, Case::Parameter)
            }),
    );
    observation.check("declared storage remains Nullable(Int)", inner_type.is_some()
        && matches!(storage.and_then(|ty| typed.types().get(ty)), Some(UnitTypeKind::Nullable(inner)) if Some(*inner) == inner_type));
    for (label, expression) in [
        ("inner read has its own valid proof", inner),
        ("outer proof equals creation-point proof", outer),
    ] {
        let facts = typed
            .non_null_uses()
            .iter()
            .filter(|fact| fact.expression() == expression)
            .collect::<Vec<_>>();
        let expected = expression == inner || case.preserves();
        observation.check(
            label,
            if expected {
                typed.expression_type(expression) == inner_type
                    && facts.len() == 1
                    && facts.iter().all(|fact| {
                        fact.expression().source_unit() == source
                            && fact.symbol().source_unit() == source
                            && Some(fact.symbol()) == symbol
                            && Some(fact.declared_type()) == storage
                            && Some(fact.narrowed_type()) == inner_type
                            && typed.non_null_use(expression) == Some(**fact)
                    })
            } else {
                facts.is_empty() && typed.non_null_use(expression).is_none()
            },
        );
    }
    record_diagnostics(
        if case.preserves() {
            Case::Parameter
        } else {
            Case::RepeatedField
        },
        typed.diagnostics(),
        expression_span(file, targets.outer)?,
        observation,
    );
    observation.check(
        "lambda is created and never called",
        typed.calls().is_empty()
            && file
                .ast()
                .expressions()
                .iter()
                .all(|(_, node)| !matches!(node.payload(), Expression::Call { .. })),
    );
    Ok(())
}
