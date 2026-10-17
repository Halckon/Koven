//! Ordinary-source final-admission controls for rejected contextual lambda trials.
//! No ownership/codegen, injected facts, or private production test hooks.
use super::*;
use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{Namespace, ReferenceTarget, UnitReferenceTarget},
    parser::Expression,
    source::Span,
    type_checking::{
        FinalTypeAdmissionError, complete_compilation_unit_types, complete_file_types,
    },
};
use std::panic::{AssertUnwindSafe, catch_unwind};

const FAILED_WHEN: &str = "fun choose(callback: (Int?) -> Int): Int = 1
fun choose(callback: (Boolean?) -> Boolean): Boolean = true
fun use(): Unit {
    val selected = choose { when (it) { null -> it; else -> it } }
}";

const SELECTED_LAMBDA: &str = "fun resolve(callback: (Int) -> Int): Int = 1
fun resolve(callback: (String) -> String): String = \"text\"
fun intResult(input: Int): Int = input
fun use(): Unit {
    val selected = resolve { intResult(it) }
}";

const FAILED_WITH_ARGUMENT: &str = "fun choose(tag: Int, callback: (Int?) -> Int): Int = 1
fun choose(tag: Int, callback: (Boolean?) -> Boolean): Boolean = true
fun use(): Unit {
    val selected = choose(if (true) { val n = 1; val bad = n.fooBar(); 1 } else { 1 }, { when (it) { null -> it; else -> it } })
}";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    OriginalW7,
    SelectedLambda,
    OrdinaryArgument,
    IndependentRoot,
}
impl Case {
    fn text(self) -> String {
        match self {
            Self::OriginalW7 => FAILED_WHEN.to_owned(),
            Self::SelectedLambda => SELECTED_LAMBDA.to_owned(),
            Self::OrdinaryArgument => FAILED_WITH_ARGUMENT.to_owned(),
            Self::IndependentRoot => {
                format!("{FAILED_WHEN}\nfun separate(): Int {{ val n = 1; return n.fooBar() }}")
            }
        }
    }
    fn selected(self) -> bool {
        self == Self::SelectedLambda
    }
    fn callee(self) -> &'static str {
        if self.selected() { "resolve" } else { "choose" }
    }
}

#[derive(Debug)]
struct Observation {
    case: Case,
    mode: &'static str,
    stage: &'static str,
    checks: Vec<(&'static str, bool)>,
    details: Vec<String>,
}
impl Observation {
    fn check(&mut self, label: &'static str, passed: bool) {
        self.checks.push((label, passed));
    }
    fn passed(&self) -> bool {
        self.stage == "observed" && self.checks.iter().all(|(_, passed)| *passed)
    }
}

#[test]
fn failed_contextual_lambda_recovery_preserves_call_and_live_roots() {
    let mut observations = Vec::new();
    for case in [
        Case::OriginalW7,
        Case::SelectedLambda,
        Case::OrdinaryArgument,
        Case::IndependentRoot,
    ] {
        for unit in [false, true] {
            let mut observation = Observation {
                case,
                mode: if unit { "unit" } else { "single" },
                stage: "parse; later phases not reached",
                checks: Vec::new(),
                details: Vec::new(),
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                let text = case.text();
                let mut sources = SourceMap::new();
                let (source, file) = parsed(&mut sources, "failed-trial.ko", &text);
                let targets = targets(&sources, &file, case)?;
                observation.details.push(format!("targets {targets:?}"));
                if unit {
                    observe_unit(case, &sources, source, &file, &targets, &mut observation)
                } else {
                    observe_single(case, &sources, &file, &targets, &mut observation)
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
    // Preserve all eight observations, including controls, before asserting the matrix.
    for observation in &observations {
        eprintln!(
            "{:?}/{}: {observation:#?}",
            observation.case, observation.mode
        );
    }
    assert_eq!(observations.len(), 8);
    assert!(
        observations.iter().all(Observation::passed),
        "{observations:#?}"
    );
}

#[derive(Debug)]
struct Targets {
    call: ExpressionId,
    callee: Span,
    lambda: ExpressionId,
    lambda_span: Span,
    ordinary_argument: Option<Span>,
    final_member: Option<Span>,
}

fn targets(sources: &SourceMap, file: &ParsedFile, case: Case) -> Result<Targets, String> {
    let mut calls = Vec::new();
    for (id, node) in file.ast().expressions().iter() {
        if let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        {
            let callee = file
                .ast()
                .expressions()
                .get(*callee)
                .map_err(|error| format!("callee AST: {error:?}"))?
                .span();
            if sources.slice(callee) == Ok(case.callee()) {
                calls.push((id, callee, arguments));
            }
        }
    }
    let [(call, callee, arguments)] = calls.as_slice() else {
        return Err(format!(
            "expected one outer {} call, got {}",
            case.callee(),
            calls.len()
        ));
    };
    let argument = arguments
        .last()
        .ok_or("missing contextual lambda argument")?;
    let mut lambda = argument.value;
    loop {
        let node = file
            .ast()
            .expressions()
            .get(lambda)
            .map_err(|error| format!("lambda AST: {error:?}"))?;
        match node.payload() {
            Expression::Group { expression } => lambda = *expression,
            Expression::Lambda { .. } => break,
            _ => return Err("last argument must be a real lambda literal".to_owned()),
        }
    }
    let lambda_span = file
        .ast()
        .expressions()
        .get(lambda)
        .map_err(|error| format!("lambda span: {error:?}"))?
        .span();
    let ordinary_argument = if case == Case::OrdinaryArgument {
        if arguments.len() != 2 {
            return Err("ordinary-argument control requires two real arguments".to_owned());
        }
        Some(
            file.ast()
                .expressions()
                .get(arguments[0].value)
                .map_err(|error| format!("ordinary argument AST: {error:?}"))?
                .span(),
        )
    } else {
        None
    };
    let members = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            if let Expression::Member { name_span, .. } = node.payload()
                && sources.slice(*name_span) == Ok("fooBar")
            {
                Some(*name_span)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let final_member = match case {
        Case::OriginalW7 | Case::SelectedLambda if members.is_empty() => None,
        Case::OrdinaryArgument | Case::IndependentRoot if members.len() == 1 => Some(members[0]),
        _ => return Err(format!("unexpected final-only member roots: {members:?}")),
    };
    if let Some(argument) = ordinary_argument
        && !final_member.is_some_and(|member| within(argument, member))
    {
        return Err("fooBar must be inside this call's ordinary argument".to_owned());
    }
    Ok(Targets {
        call: *call,
        callee: *callee,
        lambda,
        lambda_span,
        ordinary_argument,
        final_member,
    })
}

fn within(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
fn inside_lambda(file: &ParsedFile, targets: &Targets, expression: ExpressionId) -> bool {
    file.ast()
        .expressions()
        .get(expression)
        .is_ok_and(|node| within(targets.lambda_span, node.span()))
}

fn record_admission(
    case: Case,
    sources: &SourceMap,
    targets: &Targets,
    raw: &[Diagnostic],
    result: Result<bool, FinalTypeAdmissionError>,
    observation: &mut Observation,
) {
    observation
        .details
        .push(format!("raw {raw:#?}; complete {result:#?}"));
    if case == Case::OrdinaryArgument {
        observation.check(
            "final-only error belongs to this call ordinary argument",
            targets
                .ordinary_argument
                .zip(targets.final_member)
                .is_some_and(|(argument, member)| within(argument, member)),
        );
    }
    observation.check(
        "raw diagnostic is exactly the intended source result",
        if case.selected() {
            raw.is_empty()
        } else {
            raw.len() == 1
                && raw[0].code().to_string() == "L0123"
                && raw[0].primary_span() == targets.callee
                && sources.slice(targets.callee) == Ok("choose")
        },
    );
    match result {
        Ok(same_analysis) => observation.check(
            "only selected lambda completes original analysis",
            case.selected() && same_analysis,
        ),
        Err(FinalTypeAdmissionError::Internal(_)) => {
            observation.check("no internal admission failure", false)
        }
        Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics)) => {
            if case == Case::OriginalW7 {
                observation.check(
                    "original W7 preserves the entire sole L0123 without additions",
                    final_diagnostics.as_slice() == raw,
                );
            } else if case.selected() {
                observation.check("selected lambda must complete", false);
            } else {
                observation.check(
                    "ordinary or independent final root survives beside original L0123",
                    raw.len() == 1
                        && final_diagnostics.len() == 2
                        && final_diagnostics[0] == raw[0]
                        && final_diagnostics[1].code().to_string() == "L0080"
                        && Some(final_diagnostics[1].primary_span()) == targets.final_member
                        && sources.slice(final_diagnostics[1].primary_span()) == Ok("fooBar"),
                );
                observation.check(
                    "no fabricated L0165 recovery diagnostic",
                    final_diagnostics
                        .iter()
                        .all(|diagnostic| diagnostic.code().to_string() != "L0165"),
                );
            }
        }
    }
}

fn observe_single(
    case: Case,
    sources: &SourceMap,
    file: &ParsedFile,
    targets: &Targets,
    observation: &mut Observation,
) -> Result<(), String> {
    let (environment, types) = standard_environments();
    observation.stage = "single names";
    let names =
        resolve_names(sources, file, &environment).map_err(|error| format!("names: {error:?}"))?;
    observation
        .details
        .push(format!("names {:?}", names.diagnostics()));
    if !names.diagnostics().is_empty() {
        return Err("unexpected name diagnostics".to_owned());
    }
    observation.stage = "single raw";
    let typed =
        check_types(sources, file, &names, &types).map_err(|error| format!("raw: {error:?}"))?;
    let raw = typed.diagnostics().to_vec();
    let before = file
        .ast()
        .expressions()
        .iter()
        .map(|(id, _)| typed.expression_type(id))
        .collect::<Vec<_>>();
    observation.check("real two-candidate overload reference", names.references().iter().any(|reference| {
        reference.span() == targets.callee && reference.namespace() == Namespace::Value
            && matches!(reference.target(), ReferenceTarget::OverloadSet(candidates) if candidates.len() == 2)
    }));
    observation.details.push(format!(
        "expression types {before:?}; calls {:?}; nonnull {:?}; when plans {:?}",
        typed.calls(),
        typed.non_null_uses(),
        typed.nullable_whens()
    ));
    if case.selected() {
        let inner = expression(sources, file, "intResult(it)");
        let read = expression(sources, file, "it");
        let int = typed.types().builtin(BuiltinType::Int);
        observation.check(
            "selected outer and nested calls retain actual Int facts",
            int.is_some()
                && typed.calls().len() == 2
                && typed
                    .call(targets.call)
                    .is_some_and(|call| Some(call.return_type()) == int)
                && typed
                    .call(inner)
                    .is_some_and(|call| Some(call.return_type()) == int)
                && typed.expression_type(read) == int
                && matches!(
                    typed
                        .expression_type(targets.lambda)
                        .and_then(|ty| typed.types().get(ty)),
                    Some(TypeKind::Function { .. })
                ),
        );
    } else {
        observation.check(
            "failed call has Error and no selected descriptor",
            typed.call(targets.call).is_none()
                && matches!(
                    typed
                        .expression_type(targets.call)
                        .and_then(|ty| typed.types().get(ty)),
                    Some(TypeKind::Error)
                ),
        );
        observation.check(
            "failed contextual lambda remains uncommitted Error",
            matches!(
                typed
                    .expression_type(targets.lambda)
                    .and_then(|ty| typed.types().get(ty)),
                Some(TypeKind::Error)
            ),
        );
        observation.check(
            "failed lambda has no selected call, nonnull, or when plan",
            typed
                .calls()
                .iter()
                .all(|fact| !inside_lambda(file, targets, fact.expression()))
                && typed
                    .non_null_uses()
                    .iter()
                    .all(|fact| !inside_lambda(file, targets, fact.expression()))
                && typed
                    .nullable_whens()
                    .iter()
                    .all(|fact| !inside_lambda(file, targets, fact.expression())),
        );
    }
    observation.stage = "single complete";
    let result = complete_file_types(sources, file, &names, &types, &typed)
        .map(|complete| complete.is_same_analysis(&typed));
    record_admission(case, sources, targets, &raw, result, observation);
    observation.check(
        "completion leaves raw diagnostics and expression facts unchanged",
        typed.diagnostics() == raw
            && file
                .ast()
                .expressions()
                .iter()
                .map(|(id, _)| typed.expression_type(id))
                .collect::<Vec<_>>()
                == before,
    );
    Ok(())
}

fn observe_unit(
    case: Case,
    sources: &SourceMap,
    source: SourceId,
    file: &ParsedFile,
    targets: &Targets,
    observation: &mut Observation,
) -> Result<(), String> {
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::new(
        "root",
        "failed-trial.ko",
        source,
        file,
    )];
    observation.stage = "unit names";
    let index = index_compilation_unit(sources, &inputs)
        .map_err(|error| format!("unit index: {error:?}"))?;
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &environment)
        .map_err(|error| format!("unit names: {error:?}"))?;
    observation
        .details
        .push(format!("names {:?}", names.diagnostics()));
    let names = names
        .validate()
        .map_err(|error| format!("unit name diagnostics: {error:?}"))?;
    let unit = names.names().index().source_units()[0].id();
    observation.stage = "unit raw";
    let typed = check_compilation_unit_types(sources, &inputs, &names, &types)
        .map_err(|error| format!("unit raw: {error:?}"))?;
    let raw = typed.diagnostics().to_vec();
    let id = |local| UnitExpressionId::new(unit, local);
    let before = file
        .ast()
        .expressions()
        .iter()
        .map(|(local, _)| typed.expression_type(id(local)))
        .collect::<Vec<_>>();
    observation.check("real source-qualified two-candidate overload reference", names.names().references().iter().any(|reference| {
        reference.source_unit() == unit && reference.span() == targets.callee
            && reference.namespace() == Some(Namespace::Value)
            && matches!(reference.target(), UnitReferenceTarget::OverloadSet(candidates) if candidates.len() == 2)
    }));
    observation.details.push(format!(
        "source {unit:?}; expression types {before:?}; calls {:?}; nonnull {:?}",
        typed.calls(),
        typed.non_null_uses()
    ));
    if case.selected() {
        let inner = id(expression(sources, file, "intResult(it)"));
        let read = id(expression(sources, file, "it"));
        let int = typed.types().builtin(BuiltinType::Int);
        observation.check(
            "selected unit outer and nested calls retain actual Int facts",
            int.is_some()
                && typed.calls().len() == 2
                && typed
                    .call(id(targets.call))
                    .is_some_and(|call| Some(call.return_type()) == int)
                && typed
                    .call(inner)
                    .is_some_and(|call| Some(call.return_type()) == int)
                && typed.expression_type(read) == int
                && matches!(
                    typed
                        .expression_type(id(targets.lambda))
                        .and_then(|ty| typed.types().get(ty)),
                    Some(UnitTypeKind::Function { .. })
                ),
        );
    } else {
        observation.check(
            "failed unit call has Error and no selected descriptor",
            typed.call(id(targets.call)).is_none()
                && matches!(
                    typed
                        .expression_type(id(targets.call))
                        .and_then(|ty| typed.types().get(ty)),
                    Some(UnitTypeKind::Error)
                ),
        );
        observation.check(
            "failed contextual unit lambda remains without a type fact",
            typed.expression_type(id(targets.lambda)).is_none(),
        );
        observation.check(
            "failed unit lambda has no selected call or nonnull use",
            typed.calls().iter().all(|fact| {
                fact.expression().source_unit() != unit
                    || !inside_lambda(file, targets, fact.expression().expression())
            }) && typed.non_null_uses().iter().all(|fact| {
                fact.expression().source_unit() != unit
                    || !inside_lambda(file, targets, fact.expression().expression())
            }),
        );
    }
    observation.stage = "unit complete";
    let result = complete_compilation_unit_types(sources, &inputs, &names, &types, &typed)
        .map(|complete| complete.is_same_analysis(&typed));
    record_admission(case, sources, targets, &raw, result, observation);
    observation.check(
        "unit completion leaves raw diagnostics and expression facts unchanged",
        typed.diagnostics() == raw
            && file
                .ast()
                .expressions()
                .iter()
                .map(|(local, _)| typed.expression_type(id(local)))
                .collect::<Vec<_>>()
                == before,
    );
    Ok(())
}
