//! 字段根沿真实 Value 对照检查；不把旧 single 裸字段缺口伪装成既有拒绝。
use super::*;
use lang_frontend::{
    diagnostic::DiagnosticDetail,
    name_resolution::{Namespace, ReferenceTarget, UnitReferenceTarget},
    type_checking::{CallableTarget, UnitCallTarget},
};

struct FieldCase {
    label: &'static str,
    text: String,
    ordinary: bool,
    bare: bool,
}

type FieldObservation = (Observation, Option<Vec<String>>);

macro_rules! stage {
    ($observation:ident, $name:literal, $expression:expr) => {
        match $expression {
            Ok(value) => value,
            Err(error) => {
                $observation.record($name, false, error);
                $observation.blocked("remaining stages: preceding internal failure");
                return ($observation, None);
            }
        }
    };
}

fn observation(case: &FieldCase, entry: &str) -> Observation {
    Observation {
        label: format!("{}/{entry}", case.label),
        checks: Vec::new(),
        details: Vec::new(),
    }
}

fn diagnostic_details(sources: &SourceMap, diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let labels = diagnostic
                .details()
                .iter()
                .filter_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => Some(sources.slice(label.span())),
                    DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
                })
                .collect::<Vec<_>>();
            format!(
                "{} primary={:?} labels={labels:?}; {diagnostic:?}",
                diagnostic.code(),
                sources.slice(diagnostic.primary_span())
            )
        })
        .collect()
}

fn selected_call(
    case: &FieldCase,
    sources: &SourceMap,
    file: &ParsedFile,
) -> Option<(ExpressionId, ExpressionId)> {
    let calls = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            Expression::Call {
                callee, arguments, ..
            } => {
                let callee = file.ast().expressions().get(*callee).ok()?;
                if case.ordinary {
                    return (sources.slice(callee.span()) == Ok("take"))
                        .then(|| arguments.first().map(|argument| (id, argument.value)))
                        .flatten();
                }
                match callee.payload() {
                    Expression::Member {
                        receiver,
                        name_span,
                        ..
                    } if sources.slice(*name_span) == Ok("consume") => Some((id, *receiver)),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    (calls.len() == 1).then(|| calls[0])
}

fn single(case: &FieldCase, sources: &SourceMap, file: &ParsedFile) -> FieldObservation {
    let mut observation = observation(case, "single");
    observation.record("parse", file.diagnostics().is_empty(), file.diagnostics());
    if !file.diagnostics().is_empty() {
        return (observation, None);
    }
    let (environment, types) = standard_environments();
    let names = stage!(
        observation,
        "names internal",
        resolve_names(sources, file, &environment)
    );
    observation.record("names", names.diagnostics().is_empty(), names.diagnostics());
    if !names.diagnostics().is_empty() {
        return (observation, None);
    }
    let typed = stage!(
        observation,
        "raw internal",
        check_types(sources, file, &names, &types)
    );
    observation.record(
        "raw diagnostics",
        typed.diagnostics().is_empty(),
        typed.diagnostics(),
    );
    let selected = selected_call(case, sources, file);
    observation.record("one real call", selected.is_some(), selected);
    let Some((call, operand)) = selected else {
        return (observation, None);
    };
    let field = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "source" && symbol.kind() == SymbolKind::Field)
        .map(|symbol| symbol.id());
    let read = typed.expression_type(operand);
    let storage = field.and_then(|field| typed.symbol_type(field));
    let operand_span = file
        .ast()
        .expressions()
        .get(operand)
        .ok()
        .map(|node| node.span());
    let actual_field = if case.bare {
        names
            .references()
            .iter()
            .find_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol)
                    if Some(reference.span()) == operand_span
                        && reference.namespace() == Namespace::Value =>
                {
                    Some(*symbol)
                }
                _ => None,
            })
    } else {
        typed
            .aggregate_projection(operand)
            .map(|projection| projection.field())
    };
    observation.record(
        "actual receiver Field identity",
        field.is_some() && actual_field == field,
        actual_field,
    );
    let int = typed.types().builtin(BuiltinType::Int);
    let map = |ty: Option<TypeId>, constructor| {
        matches!(ty.and_then(|ty| typed.types().get(ty)),
            Some(TypeKind::Intrinsic { constructor: actual, arguments })
                if *actual == constructor && arguments.len() == 2
                    && Some(arguments[0]) == int && Some(arguments[1]) == int)
    };
    observation.record(
        "real Field storage/read",
        field.is_some() && storage == read && map(read, IntrinsicTypeConstructor::MutableMap),
        (field, storage, read),
    );
    if case.ordinary {
        let target = typed
            .calls()
            .iter()
            .find(|descriptor| descriptor.expression() == call)
            .map(|descriptor| descriptor.target());
        let take = symbol_named(&names, "take");
        observation.record(
            "ordinary take identity and Unit",
            typed.expression_type(call) == typed.types().builtin(BuiltinType::Unit)
                && matches!(target, Some(CallableTarget::Source(symbol)) if Some(symbol) == take)
                && typed.map_consumes().is_empty(),
            (target, typed.expression_type(call), typed.map_consumes()),
        );
    } else {
        let descriptor = typed.map_consume(call);
        observation.record(
            "exact consume descriptor",
            typed.map_consumes().len() == 1
                && map(typed.expression_type(call), IntrinsicTypeConstructor::Map)
                && descriptor.is_some_and(|descriptor| {
                    descriptor.expression() == call
                        && descriptor.receiver() == operand
                        && Some(descriptor.receiver_type()) == read
                        && Some(descriptor.result_type()) == typed.expression_type(call)
                        && Some(descriptor.key_type()) == int
                        && Some(descriptor.value_type()) == int
                        && descriptor.source() == if case.bare { field.zip(storage) } else { None }
                }),
            descriptor,
        );
    }
    let complete = stage!(
        observation,
        "complete",
        complete_file_types(sources, file, &names, &types, &typed)
    );
    observation.record(
        "complete same analysis",
        complete.is_same_analysis(&typed),
        "original typed product",
    );
    let owned = stage!(
        observation,
        "ownership internal",
        check_ownership(sources, file, &names, complete.types())
    );
    observation.record(
        "ownership reached and complete",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    observation.record(
        "actual ownership diagnostics",
        true,
        diagnostic_details(sources, owned.diagnostics()),
    );
    if !owned.diagnostics().is_empty() {
        observation.record(
            "error cleanup is fail-closed",
            owned.drops().is_empty(),
            owned.drops(),
        );
    }
    let codes = owned
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();
    (observation, Some(codes))
}

fn unit(case: &FieldCase, sources: &SourceMap, file: &ParsedFile) -> FieldObservation {
    let mut observation = observation(case, "unit");
    observation.record("parse", file.diagnostics().is_empty(), file.diagnostics());
    if !file.diagnostics().is_empty() {
        return (observation, None);
    }
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::new(
        "root",
        "consume-field.ko",
        file.source_id(),
        file,
    )];
    let index = stage!(
        observation,
        "index internal",
        index_compilation_unit(sources, &inputs)
    );
    let names = stage!(
        observation,
        "names internal",
        resolve_compilation_unit_names(sources, &inputs, &index, &environment)
    );
    let names = match names.validate() {
        Ok(names) => names,
        Err(names) => {
            observation.record("names", false, names.diagnostics());
            return (observation, None);
        }
    };
    observation.record("names", true, names.names().diagnostics());
    let source = names.names().source_units()[0].source_unit();
    let local_names = names.names().source_units()[0].resolution();
    let typed = stage!(
        observation,
        "raw internal",
        check_compilation_unit_types(sources, &inputs, &names, &types)
    );
    observation.record(
        "raw diagnostics",
        typed.diagnostics().is_empty(),
        typed.diagnostics(),
    );
    let selected = selected_call(case, sources, file);
    observation.record("one real call", selected.is_some(), selected);
    let Some((call, operand)) = selected else {
        return (observation, None);
    };
    let call = UnitExpressionId::new(source, call);
    let operand = UnitExpressionId::new(source, operand);
    let local_field = local_names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "source" && symbol.kind() == SymbolKind::Field)
        .map(|symbol| symbol.id());
    let field = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.nominal())
        .flat_map(|nominal| nominal.fields())
        .map(|field| field.symbol())
        .find(|field| field.source_unit() == source && Some(field.symbol()) == local_field);
    let read = typed.expression_type(operand);
    let storage = field.and_then(|field| typed.symbol_type(field));
    let operand_span = file
        .ast()
        .expressions()
        .get(operand.expression())
        .ok()
        .map(|node| node.span());
    let actual_field = if case.bare {
        names.names().references().iter().find_map(|reference| {
            if reference.source_unit() != source || Some(reference.span()) != operand_span {
                return None;
            }
            match reference.target() {
                UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                UnitReferenceTarget::Declaration(declaration) => {
                    names.names().declaration_symbol(*declaration)
                }
                _ => None,
            }
        })
    } else {
        typed
            .aggregate_projection(operand)
            .map(|projection| projection.field())
    };
    observation.record(
        "actual qualified receiver Field identity",
        field.is_some() && actual_field == field,
        actual_field,
    );
    let int = typed.types().builtin(BuiltinType::Int);
    let map = |ty: Option<UnitTypeId>, constructor| {
        matches!(ty.and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                if *actual == constructor && arguments.len() == 2
                    && Some(arguments[0]) == int && Some(arguments[1]) == int)
    };
    observation.record(
        "real qualified Field storage/read",
        field.is_some() && storage == read && map(read, IntrinsicTypeConstructor::MutableMap),
        (field, storage, read),
    );
    if case.ordinary {
        let target = typed
            .calls()
            .iter()
            .find(|descriptor| descriptor.expression() == call)
            .map(|descriptor| descriptor.target());
        let take = symbol_named(local_names, "take");
        let identity = matches!(target,
            Some(UnitCallTarget::Declaration(declaration))
                if names.names().declaration_symbol(declaration).is_some_and(|symbol|
                    symbol.source_unit() == source && Some(symbol.symbol()) == take));
        observation.record(
            "ordinary take identity and Unit",
            typed.expression_type(call) == typed.types().builtin(BuiltinType::Unit)
                && identity
                && typed.map_consumes().is_empty(),
            (target, typed.expression_type(call), typed.map_consumes()),
        );
    } else {
        let descriptor = typed.map_consume(call);
        observation.record(
            "exact consume descriptor",
            typed.map_consumes().len() == 1
                && map(typed.expression_type(call), IntrinsicTypeConstructor::Map)
                && descriptor.is_some_and(|descriptor| {
                    descriptor.expression() == call
                        && descriptor.receiver() == operand
                        && Some(descriptor.receiver_type()) == read
                        && Some(descriptor.result_type()) == typed.expression_type(call)
                        && Some(descriptor.key_type()) == int
                        && Some(descriptor.value_type()) == int
                        && descriptor.source() == if case.bare { field.zip(storage) } else { None }
                }),
            descriptor,
        );
    }
    let complete = stage!(
        observation,
        "complete",
        complete_compilation_unit_types(sources, &inputs, &names, &types, &typed)
    );
    observation.record(
        "complete same analysis",
        complete.is_same_analysis(&typed),
        "original typed product",
    );
    let typed = match typed.validate() {
        Ok(typed) => typed,
        Err(typed) => {
            observation.record("basic type qualification", false, typed.diagnostics());
            return (observation, None);
        }
    };
    let owned = stage!(
        observation,
        "ownership internal",
        check_compilation_unit_ownership(sources, &inputs, &names, &types, &typed)
    );
    observation.record(
        "ownership reached and complete",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    observation.record(
        "actual ownership diagnostics",
        true,
        diagnostic_details(sources, owned.diagnostics()),
    );
    if !owned.diagnostics().is_empty() {
        // analyze() clears public facts on any error; this does not prove no intermediate move.
        observation.record(
            "error product is fail-closed",
            owned.drops().is_empty()
                && owned.receiver_facts().is_empty()
                && owned.value_deliveries().is_empty(),
            (
                owned.drops(),
                owned.receiver_facts(),
                owned.value_deliveries(),
            ),
        );
    }
    let codes = owned
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();
    (observation, Some(codes))
}

#[test]
fn map_consume_field_roots_preserve_old_value_rejection() {
    let cases = [
        ("bare-take", "", "source", true),
        ("this-take", "", "this.source", true),
        ("bare-consume", "", "source", false),
        ("this-consume", "", "this.source", false),
        ("own-this-take", "own ", "this.source", true),
        ("own-this-consume", "own ", "this.source", false),
    ].map(|(label, mode, operand, ordinary)| {
        let prefix = if ordinary {
            "fun take(own value: MutableMap<Int, Int>): Unit {}\n"
        } else {
            ""
        };
        let body = if ordinary {
            format!("take({operand})")
        } else {
            format!("val result = {operand}.consume()")
        };
        FieldCase {
            label,
            text: format!("{prefix}class Holder(val source: MutableMap<Int, Int>) {{\n  {mode}fun f(): Unit {{ {body} }}\n}}"),
            ordinary,
            bare: operand == "source",
        }
    });
    let mut observations = Vec::with_capacity(12);
    for case in &cases {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("consume-field.ko", &case.text)
            .expect("fixed source");
        // Parser harness invariant failures remain separate from an observed language rejection.
        let file = parser_test_assertions::parse_file_twice(&sources, source, case.label);
        observations.push(single(case, &sources, &file));
        observations.push(unit(case, &sources, &file));
    }
    for (consume, ordinary) in [(2, 0), (3, 1), (5, 4)] {
        for entry in 0..2 {
            let baseline = observations[ordinary * 2 + entry].1.clone();
            let baseline_qualified = observations[ordinary * 2 + entry].0.accepted();
            let (observation, actual) = &mut observations[consume * 2 + entry];
            let matches = match (&baseline, actual.as_ref()) {
                (Some(baseline), Some(actual)) if !baseline.is_empty() => actual == baseline,
                (Some(baseline), Some(actual))
                    if baseline.is_empty() && consume == 2 && entry == 0 =>
                {
                    actual == &["L0130"]
                }
                _ => false,
            };
            observation.record(
                "rejects with actual old Value priority",
                baseline_qualified
                    && matches
                    && actual.as_ref().is_some_and(|codes| !codes.is_empty()),
                (&baseline, actual.as_ref()),
            );
        }
    }
    assert_eq!(observations.len(), 12);
    assert!(
        observations
            .iter()
            .all(|(observation, _)| observation.accepted()),
        "all field-root phase observations:\n{observations:#?}"
    );
}
