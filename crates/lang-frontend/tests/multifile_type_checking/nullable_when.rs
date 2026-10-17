//! Ordinary source checks for the unit nullable-when flow gap (Phase 2 only).
//! Fixtures are the seven existing single-file cases in tests/type_checking.rs.

use std::panic::{AssertUnwindSafe, catch_unwind};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, SourceUnitId, SourceUnitInput, Symbol,
        SymbolId, SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{Expression, Item, NameMarker, ParsedFile, Statement, VariableKind},
    source::{SourceMap, Span},
    type_checking::{
        AggregateProjectionKind, BuiltinType, CompilationUnitTypes, FinalTypeAdmissionError,
        TypeKind, TypedFile, UnitAggregateProjectionKind, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

#[path = "../support/parser_test_assertions.rs"]
mod parser_test_assertions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Parameter,
    UnselectedBodyWrite,
    MixedAlternatives,
    RepeatedField,
    ConditionWrite,
    InoutCondition,
    FailedTrials,
}

impl Case {
    fn source(self) -> &'static str {
        match self {
            Self::Parameter => "fun extract(x: Int?): Int = when (x) { null -> 0; else -> x }",
            Self::UnselectedBodyWrite => {
                "fun extract(input: Int?, replacement: Int?): Int {
    var x = input
    return (when (x) {
        null -> { x = replacement if (true) { 0 } else { 0 } };
        else -> x
    })
}"
            }
            Self::MixedAlternatives => {
                "fun extract(x: Boolean?): Boolean = when (x) {
    null, true -> x;
    else -> false
}"
            }
            Self::RepeatedField => {
                "class Holder(val item: Int?)
fun extract(h: Holder): Int = when (h.item) { null -> 0; else -> h.item }"
            }
            Self::ConditionWrite => {
                "fun extract(input: Boolean?, replacement: Boolean?): Boolean {
    var x = input
    return (when (x) {
        null -> false;
        if (true) { x = replacement if (true) { true } else { false } } else { false } -> false;
        else -> x
    })
}"
            }
            Self::InoutCondition => {
                "fun mutate(inout input: Boolean?): Boolean = true
fun extract(input: Boolean?): Boolean {
    var x = input
    return (when (x) { mutate(&x) -> false; null -> false; else -> x })
}"
            }
            Self::FailedTrials => {
                "fun choose(callback: (Int?) -> Int): Int = 1
fun choose(callback: (Boolean?) -> Boolean): Boolean = true
fun use(): Unit {
    val selected = choose { when (it) { null -> it; else -> it } }
}"
            }
        }
    }

    fn positive(self) -> bool {
        matches!(self, Self::Parameter | Self::UnselectedBodyWrite)
    }

    fn builtin(self) -> BuiltinType {
        match self {
            Self::Parameter | Self::UnselectedBodyWrite | Self::RepeatedField => BuiltinType::Int,
            _ => BuiltinType::Boolean,
        }
    }

    fn local_var(self) -> bool {
        matches!(
            self,
            Self::UnselectedBodyWrite | Self::ConditionWrite | Self::InoutCondition
        )
    }
}

#[derive(Debug)]
struct Observation {
    label: String,
    stage: &'static str,
    checks: Vec<(&'static str, bool)>,
    details: Vec<String>,
}

impl Observation {
    fn check(&mut self, name: &'static str, passed: bool) {
        self.checks.push((name, passed));
    }

    fn passed(&self) -> bool {
        self.stage == "observed" && self.checks.iter().all(|(_, passed)| *passed)
    }
}

#[test]
fn nullable_when_remaining_domain_uses_only_stable_shared_proofs() {
    observe_matrix(
        &[
            Case::Parameter,
            Case::UnselectedBodyWrite,
            Case::MixedAlternatives,
            Case::RepeatedField,
        ],
        8,
    );
}

#[test]
fn nullable_when_mutation_and_failed_trials_do_not_restore_source_proofs() {
    observe_matrix(
        &[
            Case::ConditionWrite,
            Case::InoutCondition,
            Case::FailedTrials,
        ],
        6,
    );
}

fn observe_matrix(cases: &[Case], expected_count: usize) {
    let mut observations = Vec::with_capacity(cases.len() * 2);
    for &case in cases {
        for unit in [false, true] {
            let mut observation = Observation {
                label: format!("{case:?}/{}", if unit { "unit" } else { "single" }),
                stage: "parse; names/raw/complete not reached",
                checks: Vec::new(),
                details: Vec::new(),
            };
            // A parser harness assertion or compiler panic is an explicit failed
            // observation, never a reason to skip the other sources/entry point.
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
                    return Err("parser diagnostics; names/raw/complete not reached".to_owned());
                }
                if unit {
                    observe_unit(case, &sources, &file, &mut observation)
                } else {
                    observe_single(case, &sources, &file, &mut observation)
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
    // Print successful controls too, so the first red run retains every observation.
    eprintln!("nullable when observations:\n{observations:#?}");
    assert_eq!(observations.len(), expected_count);
    assert!(
        observations.iter().all(Observation::passed),
        "{observations:#?}"
    );
}

fn observe_single(
    case: Case,
    sources: &SourceMap,
    file: &ParsedFile,
    observation: &mut Observation,
) -> Result<(), String> {
    let (environment, types) = standard_environments();
    observation.stage = "names; raw/complete not reached";
    let names = resolve_names(sources, file, &environment)
        .map_err(|error| format!("names internal: {error:?}"))?;
    observation
        .details
        .push(format!("names {:?}", names.diagnostics()));
    if !names.diagnostics().is_empty() {
        return Err("name diagnostics; raw/complete not reached".to_owned());
    }
    observation.stage = "raw; complete not reached";
    let typed = check_types(sources, file, &names, &types)
        .map_err(|error| format!("raw internal: {error:?}"))?;
    observation
        .details
        .push(format!("raw {:?}", typed.diagnostics()));
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
    let storage = names
        .symbols()
        .iter()
        .map(|symbol| {
            let ty = typed.symbol_type(symbol.id());
            (symbol, ty, ty.and_then(|ty| typed.types().get(ty)))
        })
        .collect::<Vec<_>>();
    observation.details.push(format!(
        "expressions {expressions:#?}; declarations/storage {storage:#?}; references {:#?}; nonnull {:#?}; nullable whens {:#?}; calls {:#?}; projections {:#?}",
        names.references(), typed.non_null_uses(), typed.nullable_whens(), typed.calls(), typed.aggregate_projections()
    ));
    observation.stage = "complete";
    record_admission(
        case,
        typed.diagnostics(),
        complete_file_types(sources, file, &names, &types, &typed)
            .map(|complete| complete.is_same_analysis(&typed)),
        observation,
    );
    observation.stage = "fact inspection; complete recorded";
    if case == Case::FailedTrials {
        return inspect_single_trials(sources, file, &names, &typed, observation);
    }
    let (when, subject, target) = when_targets(file, case)?;
    let target_span = expression_span(file, target)?;
    record_diagnostics(case, typed.diagnostics(), target_span, observation);
    let facts = typed
        .non_null_uses()
        .iter()
        .filter(|fact| fact.expression() == target)
        .copied()
        .collect::<Vec<_>>();
    let source_symbol = single_reference(file, &names, subject);
    let target_symbol = single_reference(file, &names, target);
    let storage = source_symbol.and_then(|symbol| typed.symbol_type(symbol));
    let inner = typed.types().builtin(case.builtin());
    observation.details.push(format!(
        "when {when:?}; subject {subject:?}; target {target:?}/{target_span:?}; source binding {source_symbol:?}; target binding {target_symbol:?}; storage {storage:?}; target read {:?}; when result {:?}; target facts {facts:?}; query {:?}",
        typed.expression_type(target), typed.expression_type(when), typed.non_null_use(target)
    ));
    if case == Case::RepeatedField {
        inspect_single_field(file, &names, &typed, subject, target, observation);
    } else {
        let symbol =
            source_symbol.and_then(|id| names.symbols().iter().find(|symbol| symbol.id() == id));
        observation.check(
            "subject and body have the same real reference",
            source_symbol.is_some() && source_symbol == target_symbol,
        );
        observation.check(
            "source declaration identity and kind",
            symbol.is_some_and(|symbol| source_kind(file, symbol, case)),
        );
        observation.check("storage remains exact nullable", inner.is_some() && matches!(storage.and_then(|ty| typed.types().get(ty)), Some(TypeKind::Nullable(ty)) if Some(*ty) == inner));
        observation.check(
            "subject read retains declared storage",
            storage.is_some() && typed.expression_type(subject) == storage,
        );
    }
    if case.positive() {
        observation.check(
            "body read and when result are exact inner",
            inner.is_some()
                && typed.expression_type(target) == inner
                && typed.expression_type(when) == inner,
        );
        observation.check(
            "one exact nonnull use with real storage and identity",
            facts.len() == 1
                && facts.iter().all(|fact| {
                    Some(fact.symbol()) == source_symbol
                        && Some(fact.declared_type()) == storage
                        && Some(fact.narrowed_type()) == inner
                        && typed.non_null_use(target) == Some(*fact)
                }),
        );
        observation.check(
            "one real single nullable-when plan",
            typed.nullable_whens().len() == 1
                && typed.nullable_when(when).is_some_and(|plan| {
                    plan.expression() == when
                        && plan.subject() == subject
                        && plan.stable_symbol() == source_symbol
                        && Some(plan.subject_type()) == storage
                }),
        );
    } else {
        // Error recovery may replace the expression type; do not freeze it as Nullable.
        observation.check(
            "no invalid proof on the precise body read",
            facts.is_empty() && typed.non_null_use(target).is_none(),
        );
    }
    if matches!(case, Case::UnselectedBodyWrite | Case::ConditionWrite) {
        let (assignment, target, value) = assignment(file)?;
        observation.check(
            "assignment uses source storage and nullable RHS",
            source_symbol.is_some()
                && single_reference(file, &names, target) == source_symbol
                && typed.expression_type(value) == storage
                && typed.expression_type(assignment) == typed.types().builtin(BuiltinType::Unit),
        );
    }
    Ok(())
}

fn observe_unit(
    case: Case,
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
    let index = index_compilation_unit(sources, &inputs)
        .map_err(|error| format!("index internal: {error:?}"))?;
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &environment)
        .map_err(|error| format!("names internal: {error:?}"))?;
    observation
        .details
        .push(format!("names {:?}", names.diagnostics()));
    let names = names.validate().map_err(|names| {
        format!(
            "name diagnostics {:?}; raw/complete not reached",
            names.diagnostics()
        )
    })?;
    let local = names
        .names()
        .source_units()
        .iter()
        .find(|local| {
            names.names().index().source_units().iter().any(|unit| {
                unit.id() == local.source_unit() && unit.source_id() == file.source_id()
            })
        })
        .ok_or("actual source-unit identity missing")?;
    let source = local.source_unit();
    observation.stage = "raw; complete not reached";
    let typed = check_compilation_unit_types(sources, &inputs, &names, &types)
        .map_err(|error| format!("raw internal: {error:?}"))?;
    observation
        .details
        .push(format!("raw {:?}", typed.diagnostics()));
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
    observation.details.push(format!(
        "source {source:?}; expressions {expressions:#?}; local declarations {:#?}; body storage {:#?}; signatures {:#?}; references {:#?}; nonnull {:#?}; calls {:#?}; projections {:#?}; assignments {:#?}",
        local.resolution().symbols(), typed.body_symbol_types(), typed.signatures().declarations(), names.names().references(), typed.non_null_uses(), typed.calls(), typed.aggregate_projections(), typed.assignments()
    ));
    // There is no public unit nullable_when API. Its full plan/lowering contract
    // remains separate; do not synthesize a substitute plan or UnitSymbolId.
    observation.stage = "complete";
    record_admission(
        case,
        typed.diagnostics(),
        complete_compilation_unit_types(sources, &inputs, &names, &types, &typed)
            .map(|complete| complete.is_same_analysis(&typed)),
        observation,
    );
    observation.stage = "fact inspection; complete recorded";
    if case == Case::FailedTrials {
        return inspect_unit_trials(sources, file, source, &names, &typed, observation);
    }
    let (when, subject, target) = when_targets(file, case)?;
    let target_span = expression_span(file, target)?;
    record_diagnostics(case, typed.diagnostics(), target_span, observation);
    let source_symbol = unit_reference(file, &names, source, subject);
    let target_symbol = unit_reference(file, &names, source, target);
    let storage = source_symbol.and_then(|symbol| typed.symbol_type(symbol));
    let inner = typed.types().builtin(case.builtin());
    let when = UnitExpressionId::new(source, when);
    let subject = UnitExpressionId::new(source, subject);
    let target = UnitExpressionId::new(source, target);
    let facts = typed
        .non_null_uses()
        .iter()
        .filter(|fact| fact.expression() == target)
        .copied()
        .collect::<Vec<_>>();
    observation.details.push(format!(
        "when {when:?}; subject {subject:?}; target {target:?}/{target_span:?}; source binding {source_symbol:?}; target binding {target_symbol:?}; storage {storage:?}/{:?}; target read {:?}; when result {:?}; target facts {facts:?}; query {:?}",
        storage.and_then(|ty| typed.types().get(ty)), typed.expression_type(target), typed.expression_type(when), typed.non_null_use(target)
    ));
    if case == Case::RepeatedField {
        inspect_unit_field(
            local.resolution(),
            &typed,
            source,
            subject,
            target,
            observation,
        );
    } else {
        let symbol = source_symbol.and_then(|id| {
            local
                .resolution()
                .symbols()
                .iter()
                .find(|symbol| symbol.id() == id.symbol())
        });
        observation.check(
            "source-qualified real reference identity",
            source_symbol.is_some_and(|symbol| symbol.source_unit() == source)
                && source_symbol == target_symbol,
        );
        observation.check(
            "source declaration identity and kind",
            symbol.is_some_and(|symbol| source_kind(file, symbol, case)),
        );
        observation.check("storage remains exact nullable", inner.is_some() && matches!(storage.and_then(|ty| typed.types().get(ty)), Some(UnitTypeKind::Nullable(ty)) if Some(*ty) == inner));
        observation.check(
            "subject read retains declared storage",
            storage.is_some() && typed.expression_type(subject) == storage,
        );
    }
    if case.positive() {
        observation.check(
            "body read and when result are exact inner",
            inner.is_some()
                && typed.expression_type(target) == inner
                && typed.expression_type(when) == inner,
        );
        observation.check(
            "one exact source-qualified nonnull use",
            facts.len() == 1
                && facts.iter().all(|fact| {
                    fact.expression().source_unit() == source
                        && fact.symbol().source_unit() == source
                        && Some(fact.symbol()) == source_symbol
                        && Some(fact.declared_type()) == storage
                        && Some(fact.narrowed_type()) == inner
                        && typed.non_null_use(target) == Some(*fact)
                }),
        );
    } else {
        observation.check(
            "no invalid proof on the precise body read",
            facts.is_empty() && typed.non_null_use(target).is_none(),
        );
    }
    if matches!(case, Case::UnselectedBodyWrite | Case::ConditionWrite) {
        let (assignment, assignment_target, value) = assignment(file)?;
        let expression = UnitExpressionId::new(source, assignment);
        observation.check(
            "assignment preserves nullable source storage",
            source_symbol.is_some()
                && unit_reference(file, &names, source, assignment_target) == source_symbol
                && typed.expression_type(UnitExpressionId::new(source, value)) == storage
                && typed.expression_type(expression) == typed.types().builtin(BuiltinType::Unit)
                && typed.assignment(expression).is_some_and(|fact| {
                    fact.expression() == expression
                        && fact.target() == UnitExpressionId::new(source, assignment_target)
                        && fact.value() == UnitExpressionId::new(source, value)
                        && Some(fact.target_type()) == storage
                }),
        );
    }
    Ok(())
}

fn record_admission(
    case: Case,
    raw: &[Diagnostic],
    result: Result<bool, FinalTypeAdmissionError>,
    observation: &mut Observation,
) {
    observation.details.push(format!("complete {result:#?}"));
    match result {
        Ok(same_analysis) => observation.check(
            "complete accepted original analysis",
            case.positive() && same_analysis,
        ),
        Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics)) => {
            observation.check(
                "complete retains original language rejection",
                !case.positive()
                    && !raw.is_empty()
                    && raw
                        .iter()
                        .all(|diagnostic| final_diagnostics.contains(diagnostic))
                    && if case == Case::FailedTrials {
                        final_diagnostics.as_slice() == raw
                    } else {
                        final_diagnostics
                            .iter()
                            .all(|diagnostic| diagnostic.code().to_string() != "L0165")
                    },
            );
        }
        Err(FinalTypeAdmissionError::Internal(_)) => {
            observation.check("complete internal failure", false)
        }
    }
}

fn record_diagnostics(case: Case, raw: &[Diagnostic], target: Span, observation: &mut Observation) {
    if case.positive() {
        observation.check("raw accepts positive source", raw.is_empty());
    } else {
        observation.check(
            "raw retains L0084 at the precise body read",
            raw.len() == 1
                && raw[0].code().to_string() == "L0084"
                && raw[0].primary_span() == target,
        );
    }
}

fn expression_span(file: &ParsedFile, id: ExpressionId) -> Result<Span, String> {
    file.ast()
        .expressions()
        .get(id)
        .map(|node| node.span())
        .map_err(|error| format!("AST expression lookup: {error:?}"))
}

fn peel(file: &ParsedFile, mut id: ExpressionId) -> Result<ExpressionId, String> {
    loop {
        match file
            .ast()
            .expressions()
            .get(id)
            .map_err(|error| format!("AST group: {error:?}"))?
            .payload()
        {
            Expression::Group { expression } => id = *expression,
            _ => return Ok(id),
        }
    }
}

fn body_read(file: &ParsedFile, body: StatementId) -> Result<ExpressionId, String> {
    match file
        .ast()
        .statements()
        .get(body)
        .map_err(|error| format!("AST body: {error:?}"))?
        .payload()
    {
        Statement::Expression { expression } => peel(file, *expression),
        Statement::ControlBody { elements } if elements.len() == 1 => body_read(file, elements[0]),
        other => Err(format!(
            "fixture expected a single body read, got {other:?}"
        )),
    }
}

fn when_targets(
    file: &ParsedFile,
    case: Case,
) -> Result<(ExpressionId, ExpressionId, ExpressionId), String> {
    let whens = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            if let Expression::When {
                subject: Some(subject),
                entries,
                ..
            } = node.payload()
            {
                Some((id, *subject, entries))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [(when, subject, entries)] = whens.as_slice() else {
        return Err(format!(
            "fixture needs one real subjectful when, got {}",
            whens.len()
        ));
    };
    let entry = if case == Case::MixedAlternatives {
        entries
            .first()
            .filter(|entry| entry.conditions.len() == 2 && entry.else_span.is_none())
    } else {
        entries.iter().find(|entry| entry.else_span.is_some())
    };
    let entry = entry.ok_or("fixture target entry missing")?;
    Ok((*when, peel(file, *subject)?, body_read(file, entry.body)?))
}

fn single_reference(
    file: &ParsedFile,
    names: &NameResolution,
    expression: ExpressionId,
) -> Option<SymbolId> {
    let span = expression_span(file, expression).ok()?;
    names.references().iter().find_map(|reference| {
        if reference.span() == span && reference.namespace() == Namespace::Value {
            match reference.target() {
                ReferenceTarget::Symbol(symbol) => Some(*symbol),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn unit_reference(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    expression: ExpressionId,
) -> Option<UnitSymbolId> {
    let span = expression_span(file, expression).ok()?;
    names.names().references().iter().find_map(|reference| {
        if reference.source_unit() == source
            && reference.span() == span
            && reference.namespace() == Some(Namespace::Value)
        {
            match reference.target() {
                UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn source_kind(file: &ParsedFile, symbol: &Symbol, case: Case) -> bool {
    if case.local_var() {
        symbol.kind() == SymbolKind::Variable && file.ast().items().iter().any(|(_, node)| {
            matches!(node.payload(), Item::Variable { kind: VariableKind::Var, name: NameMarker::Present(span), .. } if *span == symbol.span())
        })
    } else {
        symbol.kind() == SymbolKind::ValueParameter && file.ast().items().iter().any(|(_, node)| {
            matches!(node.payload(), Item::Function { parameters, .. } if parameters.iter().any(|parameter| parameter.name == NameMarker::Present(symbol.span())))
        })
    }
}

fn assignment(file: &ParsedFile) -> Result<(ExpressionId, ExpressionId, ExpressionId), String> {
    let assignments = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            if let Expression::Assignment { target, value, .. } = node.payload() {
                Some((id, *target, *value))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    match assignments.as_slice() {
        [assignment] => Ok(*assignment),
        _ => Err(format!(
            "fixture expected one assignment, got {assignments:?}"
        )),
    }
}

fn inspect_single_field(
    file: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    subject: ExpressionId,
    target: ExpressionId,
    observation: &mut Observation,
) {
    let subject_fact = typed.aggregate_projection(subject);
    let target_fact = typed.aggregate_projection(target);
    observation.check("repeated field uses distinct AST reads and real Field identity", subject != target
        && subject_fact.zip(target_fact).is_some_and(|(subject_fact, target_fact)| {
            let field = target_fact.field();
            subject_fact.field() == field && subject_fact.kind() == AggregateProjectionKind::Field
                && target_fact.kind() == AggregateProjectionKind::Field
                && names.symbols().iter().any(|symbol| symbol.id() == field && symbol.kind() == SymbolKind::Field)
                && typed.symbol_type(field) == Some(target_fact.ty())
                && matches!(typed.types().get(target_fact.ty()), Some(TypeKind::Nullable(inner)) if Some(*inner) == typed.types().builtin(BuiltinType::Int))
                && single_reference(file, names, subject).is_none() && single_reference(file, names, target).is_none()
        }));
}

fn inspect_unit_field(
    names: &NameResolution,
    typed: &CompilationUnitTypes,
    source: SourceUnitId,
    subject: UnitExpressionId,
    target: UnitExpressionId,
    observation: &mut Observation,
) {
    observation.check("repeated field uses distinct reads and real source-qualified Field", subject != target
        && typed.aggregate_projection(subject).zip(typed.aggregate_projection(target)).is_some_and(|(subject_fact, target_fact)| {
            let field = target_fact.field();
            subject_fact.field() == field && field.source_unit() == source
                && subject_fact.kind() == UnitAggregateProjectionKind::Field && target_fact.kind() == UnitAggregateProjectionKind::Field
                && names.symbols().iter().any(|symbol| symbol.id() == field.symbol() && symbol.kind() == SymbolKind::Field)
                && typed.symbol_type(field) == Some(target_fact.ty())
                && matches!(typed.types().get(target_fact.ty()), Some(UnitTypeKind::Nullable(inner)) if Some(*inner) == typed.types().builtin(BuiltinType::Int))
        }));
}

fn trial_targets(
    sources: &SourceMap,
    file: &ParsedFile,
) -> Result<(ExpressionId, Span, Span), String> {
    let calls = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            let callee_span = expression_span(file, *callee).ok()?;
            (sources.slice(callee_span) == Ok("choose")).then_some((id, callee_span, arguments))
        })
        .collect::<Vec<_>>();
    let [(call, callee, arguments)] = calls.as_slice() else {
        return Err("fixture needs one choose call".to_owned());
    };
    let [argument] = arguments.as_slice() else {
        return Err("choose needs its one real lambda argument".to_owned());
    };
    let lambda = peel(file, argument.value)?;
    let node = file
        .ast()
        .expressions()
        .get(lambda)
        .map_err(|error| format!("AST lambda: {error:?}"))?;
    if !matches!(node.payload(), Expression::Lambda { .. }) {
        return Err("choose argument is not a lambda".to_owned());
    }
    Ok((*call, *callee, node.span()))
}

fn within(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn inspect_single_trials(
    sources: &SourceMap,
    file: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    observation: &mut Observation,
) -> Result<(), String> {
    let (call, callee, lambda) = trial_targets(sources, file)?;
    observation.details.push(format!(
        "failed trial call {call:?}; callee {callee:?}; lambda {lambda:?}"
    ));
    observation.check("two actual overload candidates", names.references().iter().any(|reference| {
        reference.span() == callee && reference.namespace() == Namespace::Value
            && matches!(reference.target(), ReferenceTarget::OverloadSet(candidates) if candidates.len() == 2)
    }));
    observation.check(
        "failed overload retains only the original exact L0123",
        typed.diagnostics().len() == 1
            && typed.diagnostics()[0].code().to_string() == "L0123"
            && typed.diagnostics()[0].primary_span() == callee,
    );
    observation.check(
        "failed choose has no selected call",
        typed.call(call).is_none(),
    );
    observation.check(
        "failed lambda has no selected nullable-when plan",
        typed.nullable_whens().iter().all(|plan| {
            expression_span(file, plan.expression()).is_ok_and(|span| !within(lambda, span))
        }),
    );
    observation.check(
        "failed lambda has no nonnull use",
        typed.non_null_uses().iter().all(|fact| {
            expression_span(file, fact.expression()).is_ok_and(|span| !within(lambda, span))
        }),
    );
    observation.check(
        "failed lambda has no selected call facts",
        typed.calls().iter().all(|fact| {
            expression_span(file, fact.expression()).is_ok_and(|span| !within(lambda, span))
        }),
    );
    Ok(())
}

fn inspect_unit_trials(
    sources: &SourceMap,
    file: &ParsedFile,
    source: SourceUnitId,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    observation: &mut Observation,
) -> Result<(), String> {
    let (call, callee, lambda) = trial_targets(sources, file)?;
    let call = UnitExpressionId::new(source, call);
    observation.details.push(format!("failed trial call {call:?}; callee {callee:?}; lambda {lambda:?}; unit nullable-when plan API absent"));
    observation.check("two actual source-qualified overload candidates", names.names().references().iter().any(|reference| {
        reference.source_unit() == source && reference.span() == callee && reference.namespace() == Some(Namespace::Value)
            && matches!(reference.target(), UnitReferenceTarget::OverloadSet(candidates) if candidates.len() == 2)
    }));
    observation.check(
        "failed overload retains only the original exact L0123",
        typed.diagnostics().len() == 1
            && typed.diagnostics()[0].code().to_string() == "L0123"
            && typed.diagnostics()[0].primary_span() == callee,
    );
    observation.check(
        "failed choose has no selected call",
        typed.call(call).is_none(),
    );
    observation.check(
        "failed lambda has no nonnull use",
        typed.non_null_uses().iter().all(|fact| {
            fact.expression().source_unit() != source
                || expression_span(file, fact.expression().expression())
                    .is_ok_and(|span| !within(lambda, span))
        }),
    );
    observation.check(
        "failed lambda has no selected call facts",
        typed.calls().iter().all(|fact| {
            fact.expression().source_unit() != source
                || expression_span(file, fact.expression().expression())
                    .is_ok_and(|span| !within(lambda, span))
        }),
    );
    Ok(())
}

#[path = "nullable_when_scope.rs"]
mod scope;
