//! Compare field moves against the real Unit Value-delivery order.
use super::*;
use lang_frontend::{
    diagnostic::{DiagnosticCodeCatalog, DiagnosticDetail, Severity},
    name_resolution::UnitReferenceTarget,
    type_checking::{ParameterMode, UnitAggregateProjectionReceiver, UnitCallTarget},
};

struct FieldCase {
    label: &'static str,
    text: String,
    ordinary: bool,
    bare: bool,
}

impl FieldCase {
    fn exact_this_consume(&self) -> Option<(ParameterMode, usize)> {
        match self.label {
            "this-consume" => Some((ParameterMode::Borrow, 85)),
            "own-this-consume" => Some((ParameterMode::Value, 89)),
            _ => None,
        }
    }
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

fn unit(
    case: &FieldCase,
    sources: &SourceMap,
    file: &ParsedFile,
    rooted: bool,
) -> FieldObservation {
    let mut observation = observation(case, if rooted { "unit" } else { "memory" });
    let exact = case.exact_this_consume();
    observation.record("parse", file.diagnostics().is_empty(), file.diagnostics());
    if !file.diagnostics().is_empty() {
        return (observation, None);
    }
    let (environment, types) = standard_environments();
    let inputs = [if rooted {
        SourceUnitInput::new("root", "consume-field.ko", file.source_id(), file)
    } else {
        SourceUnitInput::memory_document(file.source_id(), file)
    }];
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
    if exact.is_some() {
        let key = names.names().index().source_units()[0].key();
        assert!(match key {
            SourceUnitKey::MemoryDocument => !rooted,
            SourceUnitKey::Rooted { root, logical_path } =>
                rooted && root.as_str() == "root" && logical_path.as_str() == "consume-field.ko",
        });
    }
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
    if case.bare {
        let member = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.nominal())
            .flat_map(|nominal| nominal.members())
            .find(|member| member.name() == "f");
        let actual = member.and_then(|member| member.receiver());
        observation.record(
            "bare field is read inside the real default Borrow receiver",
            actual.is_some_and(|receiver| {
                receiver.mode() == ParameterMode::Borrow && receiver.marker_span().is_none()
            }),
            actual,
        );
    }
    let explicit_receiver = match case.label {
        "this-take" => Some(ParameterMode::Borrow),
        "own-this-take" => Some(ParameterMode::Value),
        _ => exact.map(|(mode, _)| mode),
    };
    if let Some(expected_mode) = explicit_receiver {
        let projection = typed
            .aggregate_projection(operand)
            .expect("real explicit projection");
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            panic!("the field root is the explicit this expression")
        };
        assert_eq!(receiver.source_unit(), source);
        let actual = file.ast().expressions().get(receiver.expression()).unwrap();
        assert!(matches!(actual.payload(), Expression::This));
        assert_eq!(sources.slice(actual.span()), Ok("this"));
        assert_eq!(projection.expression(), operand);
        assert_eq!(Some(projection.field()), field);
        let nominals = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.nominal());
        let member = nominals
            .flat_map(|nominal| nominal.members())
            .find(|member| member.name() == "f")
            .expect("original Holder.f");
        let signature = member.receiver().expect("declared method receiver");
        assert_eq!(signature.mode(), expected_mode);
        match expected_mode {
            ParameterMode::Value => assert_eq!(
                sources.slice(signature.marker_span().expect("original own marker")),
                Ok("own")
            ),
            ParameterMode::Borrow => assert!(signature.marker_span().is_none()),
            ParameterMode::Inout => unreachable!("only the original Borrow and Value receivers"),
        }
        assert_eq!(typed.expression_type(receiver), Some(signature.ty()));
        if case.ordinary {
            let target = typed
                .call(call)
                .expect("selected own argument call")
                .target();
            let UnitCallTarget::Declaration(target) = target else {
                panic!("real original top-level take");
            };
            let selected = typed
                .signatures()
                .declaration(target)
                .unwrap()
                .callable()
                .unwrap();
            let [parameter] = selected.parameters() else {
                panic!("one real parameter");
            };
            assert_eq!(parameter.mode(), ParameterMode::Value);
            assert_eq!(Some(parameter.ty()), read);
        }
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
    let exact_move = match case.label {
        "bare-take" => Some((123, 68, true)),
        "bare-consume" => Some((80, 17, false)),
        "this-take" => Some((128, 68, true)),
        "own-this-take" => Some((132, 68, true)),
        _ => exact.map(|(_, start)| (start, 17, false)),
    };
    if let Some((primary, declaration, selected_parameter)) = exact_move {
        let source = file.source_id();
        let slice = |start, end, expected| {
            let span = sources.span(source, start, end).unwrap();
            assert_eq!(sources.slice(span), Ok(expected));
            span
        };
        let catalog = DiagnosticCodeCatalog::try_new(&["L0132"]).unwrap();
        let mut expected = Diagnostic::new(
            sources,
            Severity::Error,
            catalog.resolve("L0132").unwrap(),
            "cannot move a non-Copyable component out of its owner",
            slice(primary, primary + "source".len(), "source"),
        )
        .unwrap();
        expected
            .add_label(
                sources,
                slice(declaration, declaration + "source".len(), "source"),
                "non-Copyable component declared here",
            )
            .unwrap();
        if selected_parameter {
            expected
                .add_label(
                    sources,
                    slice(9, 40, "own value: MutableMap<Int, Int>"),
                    "selected parameter declared here",
                )
                .unwrap();
        }
        assert_eq!(owned.diagnostics(), std::slice::from_ref(&expected));
        assert!(owned.loans().is_empty());
        assert!(owned.direct_call_loan_ends().is_empty());
        assert!(owned.borrow_results().bindings().is_empty());
        assert!(owned.borrow_results().call_uses().is_empty());
        assert!(owned.construction_plans().is_empty());
        assert!(owned.clone().validate().is_err());
    }
    let codes = owned
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();
    (observation, Some(codes))
}

fn field_case(label: &'static str, mode: &str, operand: &str, ordinary: bool) -> FieldCase {
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
        text: format!(
            "{prefix}class Holder(val source: MutableMap<Int, Int>) {{\n  {mode}fun f(): Unit {{ {body} }}\n}}"
        ),
        ordinary,
        bare: operand == "source",
    }
}

#[test]
fn map_consume_field_roots_match_unit_value_rejection() {
    let cases = [
        ("this-take", "", "this.source", true),
        ("this-consume", "", "this.source", false),
        ("own-this-take", "own ", "this.source", true),
        ("own-this-consume", "own ", "this.source", false),
    ]
    .map(|(label, mode, operand, ordinary)| field_case(label, mode, operand, ordinary));
    let mut observations = Vec::with_capacity(8);
    for case in &cases {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("consume-field.ko", &case.text)
            .expect("fixed source");
        // Parser harness invariant failures remain separate from an observed language rejection.
        let file = parser_test_assertions::parse_file_twice(&sources, source, case.label);
        observations.push(unit(case, &sources, &file, false));
        observations.push(unit(case, &sources, &file, true));
    }
    for (consume, ordinary) in [(1, 0), (3, 2)] {
        for entry in 0..2 {
            let baseline = observations[ordinary * 2 + entry].1.clone();
            let baseline_qualified = observations[ordinary * 2 + entry].0.accepted();
            let (observation, actual) = &mut observations[consume * 2 + entry];
            let matches = match (&baseline, actual.as_ref()) {
                (Some(baseline), Some(actual)) if !baseline.is_empty() => actual == baseline,
                _ => false,
            };
            observation.record(
                "same rejection code as the actual Unit Value diagnostic",
                baseline_qualified
                    && matches
                    && actual.as_ref().is_some_and(|codes| !codes.is_empty()),
                (&baseline, actual.as_ref()),
            );
        }
    }
    assert_eq!(observations.len(), 8);
    assert!(
        observations
            .iter()
            .all(|(observation, _)| observation.accepted()),
        "all field-root phase observations:\n{observations:#?}"
    );
}

// Guide v0.46 §8: bare field syntax does not authorize moving a non-Copyable field.
// This deliberately replaces old Single success for bare take and L0130 for bare
// consume, not the explicit-this diagnostics preserved by the matrix above.
#[test]
fn bare_noncopyable_map_field_move_is_rejected_on_both_unit_inputs() {
    for (label, ordinary) in [("bare-take", true), ("bare-consume", false)] {
        let case = field_case(label, "", "source", ordinary);
        let mut sources = SourceMap::new();
        let source = sources.add_source("consume-field.ko", &case.text).unwrap();
        assert_eq!(sources.source_text(source), Ok(case.text.as_str()));
        let file = parser_test_assertions::parse_file_twice(&sources, source, case.label);
        for rooted in [false, true] {
            let (observation, diagnostics) = unit(&case, &sources, &file, rooted);
            assert!(observation.accepted(), "{observation:#?}");
            assert_eq!(
                diagnostics.as_deref(),
                Some(["L0132".to_string()].as_slice()),
                "the real non-Copyable field move must be refused before root eligibility",
            );
        }
    }
}

// Unit includes the selected Value parameter label; consume retains its own exact diagnostic.
#[test]
fn original_explicit_map_field_take_keeps_exact_unit_diagnostic() {
    for (label, mode) in [("this-take", ""), ("own-this-take", "own ")] {
        let case = field_case(label, mode, "this.source", true);
        let mut sources = SourceMap::new();
        let source = sources.add_source("consume-field.ko", &case.text).unwrap();
        assert_eq!(sources.source_text(source), Ok(case.text.as_str()));
        let file = parser_test_assertions::parse_file_twice(&sources, source, case.label);
        for rooted in [false, true] {
            let (observation, diagnostics) = unit(&case, &sources, &file, rooted);
            assert!(observation.accepted(), "{observation:#?}");
            assert_eq!(
                diagnostics.as_deref(),
                Some(["L0132".to_string()].as_slice())
            );
        }
    }
}
