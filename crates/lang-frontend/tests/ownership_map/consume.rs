//! SPEC-0291：普通源码先取得最终类型资格，再观察 ownership 与真实清理事实。

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    name_resolution::{
        NameResolution, SourceUnitId, SourceUnitInput, SymbolId, SymbolKind, UnitSymbolId,
        index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropTarget, NullableWhenExtractionKind, OwnershipCheckedFile,
        UnitDropTarget, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
        UnitValueDeliveryKind, check_compilation_unit_ownership, check_ownership,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, TypeId, TypeKind, TypedFile,
        UnitExpressionId, UnitTypeId, UnitTypeKind, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[path = "../support/parser_test_assertions.rs"]
mod parser_test_assertions;

mod field_roots;

#[derive(Clone, Copy)]
enum Expected {
    Bound,
    Discarded,
    Nullable { when: bool, ordinary: bool },
    Rejected(&'static str),
}

struct Case {
    label: &'static str,
    text: &'static str,
    key: BuiltinType,
    value: BuiltinType,
    expected: Expected,
    calls: usize,
}

impl Case {
    fn ordinary(&self) -> bool {
        matches!(self.expected, Expected::Nullable { ordinary: true, .. })
    }
}

#[derive(Debug)]
struct Observation {
    label: String,
    checks: Vec<(&'static str, bool)>,
    details: Vec<String>,
}

impl Observation {
    fn new(case: &Case, entry: &str) -> Self {
        Self {
            label: format!("{}/{entry}", case.label),
            checks: Vec::new(),
            details: Vec::new(),
        }
    }

    fn record(&mut self, stage: &'static str, passed: bool, detail: impl std::fmt::Debug) {
        self.checks.push((stage, passed));
        self.details.push(format!("{stage}: {detail:?}"));
    }

    fn blocked(&mut self, stages: &'static str) {
        self.record("not reached", false, stages);
    }

    fn accepted(&self) -> bool {
        !self.label.is_empty() && self.checks.iter().all(|(_, passed)| *passed)
    }
}

fn assert_matrix(cases: &[Case], expected_observations: usize) {
    let mut observations = Vec::with_capacity(expected_observations);
    for case in cases {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("consume.ko", case.text)
            .expect("fixed source");
        let file = parser_test_assertions::parse_file_twice(&sources, source, case.label);
        observations.push(observe_single(case, &sources, &file));
        observations.push(observe_unit(case, &sources, &file));
    }
    assert_eq!(observations.len(), expected_observations);
    assert!(
        observations.iter().all(Observation::accepted),
        "all phase observations:\n{observations:#?}"
    );
}

fn selected_calls(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
) -> Vec<(ExpressionId, ExpressionId)> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            let callee_node = file.ast().expressions().get(*callee).ok()?;
            if case.ordinary() {
                return (sources.slice(callee_node.span()) == Ok("take"))
                    .then(|| arguments.first().map(|argument| (id, argument.value)))
                    .flatten();
            }
            match callee_node.payload() {
                Expression::Member {
                    receiver,
                    name_span,
                    ..
                } if sources.slice(*name_span) == Ok("consume") => Some((id, *receiver)),
                _ => None,
            }
        })
        .collect()
}

fn symbol_named(names: &NameResolution, name: &str) -> Option<SymbolId> {
    names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .map(|symbol| symbol.id())
}

fn unit_symbol_named(
    typed: &CompilationUnitTypes,
    names: &NameResolution,
    source: SourceUnitId,
    name: &str,
) -> Option<UnitSymbolId> {
    let symbol = symbol_named(names, name)?;
    typed
        .body_symbol_types()
        .keys()
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol)
        .copied()
}

fn check_source_kind(case: &Case, names: &NameResolution, observation: &mut Observation) {
    let expected = if case.label == "for-binding" {
        Some(SymbolKind::ForBinding)
    } else if !matches!(case.expected, Expected::Rejected(_)) {
        Some(SymbolKind::Variable)
    } else {
        None
    };
    if let Some(expected) = expected {
        let actual = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "source")
            .map(|symbol| symbol.kind());
        observation.record(
            "source declaration kind",
            actual == Some(expected),
            (actual, expected),
        );
    }
    if case.label == "lexical-val-inside-loop" {
        let actual = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "n")
            .map(|symbol| symbol.kind());
        observation.record(
            "loop binding is distinct",
            actual == Some(SymbolKind::ForBinding),
            actual,
        );
    }
}

fn single_map(
    typed: &TypedFile,
    ty: Option<TypeId>,
    constructor: IntrinsicTypeConstructor,
    case: &Case,
) -> bool {
    matches!(ty.and_then(|ty| typed.types().get(ty)), Some(TypeKind::Intrinsic { constructor: actual, arguments })
        if *actual == constructor && arguments.len() == 2
            && Some(arguments[0]) == typed.types().builtin(case.key)
            && Some(arguments[1]) == typed.types().builtin(case.value))
}

fn unit_map(
    typed: &CompilationUnitTypes,
    ty: Option<UnitTypeId>,
    constructor: IntrinsicTypeConstructor,
    case: &Case,
) -> bool {
    matches!(ty.and_then(|ty| typed.types().get(ty)), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
        if *actual == constructor && arguments.len() == 2
            && Some(arguments[0]) == typed.types().builtin(case.key)
            && Some(arguments[1]) == typed.types().builtin(case.value))
}

fn observe_single(case: &Case, sources: &SourceMap, file: &ParsedFile) -> Observation {
    let mut observation = Observation::new(case, "single");
    observation.record("parse", file.diagnostics().is_empty(), file.diagnostics());
    if !file.diagnostics().is_empty() {
        observation.blocked("names/raw/complete/ownership: parser diagnostics");
        return observation;
    }
    let (environment, types) = standard_environments();
    let names = match resolve_names(sources, file, &environment) {
        Ok(names) => names,
        Err(error) => {
            observation.record("names internal", false, error);
            observation.blocked("raw/complete/ownership");
            return observation;
        }
    };
    observation.record("names", names.diagnostics().is_empty(), names.diagnostics());
    if !names.diagnostics().is_empty() {
        observation.blocked("raw/complete/ownership: name diagnostics");
        return observation;
    }
    check_source_kind(case, &names, &mut observation);
    let typed = match check_types(sources, file, &names, &types) {
        Ok(typed) => typed,
        Err(error) => {
            observation.record("raw internal", false, error);
            observation.blocked("complete/ownership");
            return observation;
        }
    };
    observation.record(
        "raw diagnostics",
        typed.diagnostics().is_empty(),
        typed.diagnostics(),
    );
    let calls = selected_calls(case, sources, file);
    observation.record("call identities", calls.len() == case.calls, &calls);
    for &(call, operand) in &calls {
        let actual = typed.expression_type(call);
        let valid = if case.ordinary() {
            actual == typed.types().builtin(BuiltinType::Unit)
        } else {
            single_map(&typed, actual, IntrinsicTypeConstructor::Map, case)
        };
        observation.record(
            "raw call type",
            valid,
            (call, actual.and_then(|ty| typed.types().get(ty))),
        );
        if !matches!(case.expected, Expected::Rejected(_)) {
            let root = symbol_named(&names, "source");
            let storage = root.and_then(|symbol| typed.symbol_type(symbol));
            let nullable = matches!(case.expected, Expected::Nullable { .. });
            let inner = match storage.and_then(|ty| typed.types().get(ty)) {
                Some(TypeKind::Nullable(inner)) if nullable => Some(*inner),
                _ if !nullable => storage,
                _ => None,
            };
            let read = typed.expression_type(operand);
            observation.record(
                "source storage/read identity",
                root.is_some()
                    && (!nullable || storage != read)
                    && single_map(&typed, inner, IntrinsicTypeConstructor::MutableMap, case)
                    && single_map(&typed, read, IntrinsicTypeConstructor::MutableMap, case),
                (
                    root,
                    operand,
                    storage,
                    storage.and_then(|ty| typed.types().get(ty)),
                    read,
                    read.and_then(|ty| typed.types().get(ty)),
                ),
            );
        }
        if case.ordinary() {
            let target = typed
                .calls()
                .iter()
                .find(|descriptor| descriptor.expression() == call)
                .map(|descriptor| descriptor.target());
            let take = symbol_named(&names, "take");
            observation.record("ordinary take target", matches!(target, Some(lang_frontend::type_checking::CallableTarget::Source(symbol)) if Some(symbol) == take), target);
        }
    }
    let complete = match complete_file_types(sources, file, &names, &types, &typed) {
        Ok(complete) => {
            observation.record(
                "complete same analysis",
                complete.is_same_analysis(&typed),
                "qualified original typed product",
            );
            complete
        }
        Err(error) => {
            observation.record("complete", false, error);
            observation.blocked("ownership: final admission failed");
            return observation;
        }
    };
    match check_ownership(sources, file, &names, complete.types()) {
        Ok(owned) => observe_single_ownership(case, &names, &calls, &owned, &mut observation),
        Err(error) => observation.record("ownership internal", false, error),
    }
    observation
}

fn observe_unit(case: &Case, sources: &SourceMap, file: &ParsedFile) -> Observation {
    let mut observation = Observation::new(case, "unit");
    observation.record("parse", file.diagnostics().is_empty(), file.diagnostics());
    if !file.diagnostics().is_empty() {
        observation.blocked("names/raw/complete/ownership: parser diagnostics");
        return observation;
    }
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::new(
        "root",
        "consume.ko",
        file.source_id(),
        file,
    )];
    let index = match index_compilation_unit(sources, &inputs) {
        Ok(index) => index,
        Err(error) => {
            observation.record("index internal", false, error);
            observation.blocked("names/raw/complete/ownership");
            return observation;
        }
    };
    let names = match resolve_compilation_unit_names(sources, &inputs, &index, &environment) {
        Ok(names) => match names.validate() {
            Ok(names) => names,
            Err(names) => {
                observation.record("names", false, names.diagnostics());
                observation.blocked("raw/complete/ownership: name qualification failed");
                return observation;
            }
        },
        Err(error) => {
            observation.record("names internal", false, error);
            observation.blocked("raw/complete/ownership");
            return observation;
        }
    };
    observation.record("names", true, names.names().diagnostics());
    let source = names.names().source_units()[0].source_unit();
    let local_names = names.names().source_units()[0].resolution();
    check_source_kind(case, local_names, &mut observation);
    let typed = match check_compilation_unit_types(sources, &inputs, &names, &types) {
        Ok(typed) => typed,
        Err(error) => {
            observation.record("raw internal", false, error);
            observation.blocked("complete/ownership");
            return observation;
        }
    };
    observation.record(
        "raw diagnostics",
        typed.diagnostics().is_empty(),
        typed.diagnostics(),
    );
    let calls = selected_calls(case, sources, file);
    observation.record("call identities", calls.len() == case.calls, &calls);
    for &(call, operand) in &calls {
        let call = UnitExpressionId::new(source, call);
        let actual = typed.expression_type(call);
        let valid = if case.ordinary() {
            actual == typed.types().builtin(BuiltinType::Unit)
        } else {
            unit_map(&typed, actual, IntrinsicTypeConstructor::Map, case)
        };
        observation.record(
            "raw call type",
            valid,
            (call, actual.and_then(|ty| typed.types().get(ty))),
        );
        if !matches!(case.expected, Expected::Rejected(_)) {
            let root = unit_symbol_named(&typed, local_names, source, "source");
            let storage = root.and_then(|symbol| typed.symbol_type(symbol));
            let nullable = matches!(case.expected, Expected::Nullable { .. });
            let inner = match storage.and_then(|ty| typed.types().get(ty)) {
                Some(UnitTypeKind::Nullable(inner)) if nullable => Some(*inner),
                _ if !nullable => storage,
                _ => None,
            };
            let read = typed.expression_type(UnitExpressionId::new(source, operand));
            observation.record(
                "source storage/read identity",
                root.is_some()
                    && (!nullable || storage != read)
                    && unit_map(&typed, inner, IntrinsicTypeConstructor::MutableMap, case)
                    && unit_map(&typed, read, IntrinsicTypeConstructor::MutableMap, case),
                (
                    root,
                    operand,
                    storage,
                    storage.and_then(|ty| typed.types().get(ty)),
                    read,
                    read.and_then(|ty| typed.types().get(ty)),
                ),
            );
        }
        if case.ordinary() {
            let target = typed
                .calls()
                .iter()
                .find(|descriptor| descriptor.expression() == call)
                .map(|descriptor| descriptor.target());
            let take = symbol_named(local_names, "take");
            let selected = match target {
                Some(lang_frontend::type_checking::UnitCallTarget::Declaration(declaration)) => {
                    names
                        .names()
                        .declaration_symbol(declaration)
                        .is_some_and(|symbol| {
                            symbol.source_unit() == source && Some(symbol.symbol()) == take
                        })
                }
                _ => false,
            };
            observation.record("ordinary take target", selected, target);
        }
    }
    match complete_compilation_unit_types(sources, &inputs, &names, &types, &typed) {
        Ok(complete) => observation.record(
            "complete same analysis",
            complete.is_same_analysis(&typed),
            "qualified original typed product",
        ),
        Err(error) => {
            observation.record("complete", false, error);
            observation.blocked("ownership: final admission failed");
            return observation;
        }
    }
    let root = unit_symbol_named(&typed, local_names, source, "source");
    let result = unit_symbol_named(&typed, local_names, source, "result");
    let typed = match typed.validate() {
        Ok(typed) => typed,
        Err(typed) => {
            observation.record("basic type qualification", false, typed.diagnostics());
            observation.blocked("ownership: basic qualification failed");
            return observation;
        }
    };
    match check_compilation_unit_ownership(sources, &inputs, &names, &types, &typed) {
        Ok(owned) => {
            observe_unit_ownership(case, source, root, result, &calls, &owned, &mut observation)
        }
        Err(error) => observation.record("ownership internal", false, error),
    }
    observation
}

fn diagnostics_match(expected: Expected, diagnostics: &[Diagnostic]) -> bool {
    match expected {
        Expected::Rejected(code) => {
            diagnostics.len() == 1 && diagnostics[0].code().to_string() == code
        }
        _ => diagnostics.is_empty(),
    }
}

fn observe_single_ownership(
    case: &Case,
    names: &NameResolution,
    calls: &[(ExpressionId, ExpressionId)],
    owned: &OwnershipCheckedFile,
    observation: &mut Observation,
) {
    observation.record(
        "ownership diagnostics",
        diagnostics_match(case.expected, owned.diagnostics()),
        owned.diagnostics(),
    );
    observation.record(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    if matches!(case.expected, Expected::Rejected(_)) {
        observation.record(
            "error cleanup is fail-closed",
            owned.drops().is_empty(),
            owned.drops(),
        );
        return;
    }
    let root = symbol_named(names, "source");
    if matches!(case.expected, Expected::Bound | Expected::Discarded) {
        let source_drops = owned
            .drops()
            .iter()
            .filter(
                |drop| matches!(drop.target(), DropTarget::Named(symbol) if Some(symbol) == root),
            )
            .count();
        observation.record(
            "moved source has no cleanup",
            root.is_some() && source_drops == 0,
            (root, source_drops),
        );
        let cleanup = match case.expected {
            Expected::Bound => {
                let result = symbol_named(names, "result");
                result.is_some() && owned.drops().iter().filter(|drop| matches!(drop.target(), DropTarget::Named(symbol) if Some(symbol) == result)).count() == 1
            }
            Expected::Discarded => calls.first().is_some_and(|(call, _)| {
                owned
                    .drops()
                    .iter()
                    .filter(|drop| drop.target() == DropTarget::Temporary(*call))
                    .count()
                    == 1
            }),
            _ => false,
        };
        observation.record("result has one cleanup", cleanup, owned.drops());
    }
    if let Expected::Nullable { when, .. } = case.expected {
        let extractions = owned
            .nullable_whens()
            .iter()
            .flat_map(|plan| plan.extractions())
            .collect::<Vec<_>>();
        let consuming = extractions
            .iter()
            .filter(|fact| {
                fact.kind() == NullableWhenExtractionKind::Consume
                    && calls
                        .iter()
                        .any(|(_, operand)| fact.expression() == *operand)
            })
            .count();
        let source_edges = owned.nullable_whens().iter().all(|plan| {
            plan.extractions().iter().all(|fact| {
                fact.alternative().is_none()
                    && plan.branches().iter().any(|branch| {
                        branch.index() == fact.entry()
                            && branch.view().is_some_and(|view| {
                                view.subject() == plan.subject()
                                    && view.stable_symbol() == root
                                    && view.place().is_some_and(|place| {
                                        Some(place.root()) == root && place.is_root()
                                    })
                            })
                    })
            })
        });
        observation.record(
            "original nullable extraction facts",
            if when {
                owned.nullable_whens().len() == 1 && consuming == 1 && source_edges
            } else {
                owned.nullable_whens().is_empty()
            },
            (root, owned.nullable_whens(), owned.drops()),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn observe_unit_ownership(
    case: &Case,
    source: SourceUnitId,
    root: Option<UnitSymbolId>,
    result: Option<UnitSymbolId>,
    calls: &[(ExpressionId, ExpressionId)],
    owned: &CompilationUnitOwnership,
    observation: &mut Observation,
) {
    observation.record(
        "ownership diagnostics",
        diagnostics_match(case.expected, owned.diagnostics()),
        owned.diagnostics(),
    );
    observation.record(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    if matches!(case.expected, Expected::Rejected(_)) {
        // analyze() clears all deliveries on any error, including an earlier valid consume.
        // Empty public facts prove fail-closed, not that an invalid path never committed a move.
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
        return;
    }
    for &(call, operand) in calls {
        let call = UnitExpressionId::new(source, call);
        if case.ordinary() {
            let operand = UnitExpressionId::new(source, operand);
            let deliveries = owned
                .value_deliveries()
                .iter()
                .filter(|fact| fact.call() == call && fact.argument() == operand)
                .collect::<Vec<_>>();
            observation.record(
                "ordinary owned delivery",
                deliveries.len() == 1
                    && deliveries.iter().all(|fact| {
                        fact.kind() == UnitValueDeliveryKind::Move
                            && fact
                                .place()
                                .is_some_and(|place| Some(place.root()) == root && place.is_root())
                    }),
                deliveries,
            );
        } else {
            let fact = owned.receiver_fact(call);
            observation.record("consume moves its actual root", root.is_some() && fact.is_some_and(|fact| fact.kind() == UnitReceiverOwnershipKind::Move && matches!(fact.target(), UnitReceiverOwnershipTarget::Place(place) if Some(place.root()) == root && place.is_root())), fact);
        }
    }
    if matches!(case.expected, Expected::Bound | Expected::Discarded) {
        let source_drops = owned.drops().iter().filter(|drop| matches!(drop.target(), UnitDropTarget::Named(symbol) if Some(symbol) == root)).count();
        observation.record(
            "moved source has no cleanup",
            root.is_some() && source_drops == 0,
            (root, source_drops),
        );
        let cleanup = match case.expected {
            Expected::Bound => result.is_some() && owned.drops().iter().filter(|drop| matches!(drop.target(), UnitDropTarget::Named(symbol) if Some(symbol) == result)).count() == 1,
            Expected::Discarded => calls.first().is_some_and(|(call, _)| owned.drops().iter().filter(|drop| drop.target() == UnitDropTarget::Temporary(UnitExpressionId::new(source, *call))).count() == 1),
            _ => false,
        };
        observation.record("result has one cleanup", cleanup, owned.drops());
    } else {
        // Nullable declaration storage is not the refined read type; branch cleanup stays separate.
        observation.details.push(format!(
            "nullable branch cleanup (unmodified): {:?}",
            owned.drops()
        ));
    }
}

#[test]
fn map_consume_complete_local_roots_transfer_one_cleanup() {
    use BuiltinType::{Int, String as Str};
    let cases = [
        (
            "grouped-val-bound",
            r#"fun run(): Unit {
  val source = mutableMapOf<Int, Int>()
  val result = (source).consume()
}"#,
            Int,
            Expected::Bound,
            1,
        ),
        (
            "populated-var-bound",
            r#"fun run(): Unit {
  var source = mutableMapOf<String, String>()
  source.put("key", "value")
  val result = source.consume()
}"#,
            Str,
            Expected::Bound,
            1,
        ),
        (
            "discarded-result",
            r#"fun run(): Unit {
  val source = mutableMapOf<Int, Int>()
  source.consume()
}"#,
            Int,
            Expected::Discarded,
            1,
        ),
        (
            "lexical-val-inside-loop",
            r#"fun run(): Unit {
  for (n in listOf(1)) {
    val source = mutableMapOf<Int, Int>()
    val result = source.consume()
  }
}"#,
            Int,
            Expected::Bound,
            1,
        ),
        (
            "ended-loan",
            r#"fun run(): Unit {
  var source = mutableMapOf<String, String>()
  source.put("key", "value");
  { borrow val item = source.requireValue("key"); println(item) }
  val result = source.consume()
}"#,
            Str,
            Expected::Bound,
            1,
        ),
        (
            "nullable-local-if",
            r#"fun run(own input: MutableMap<Int, Int>?): Unit {
  val source: MutableMap<Int, Int>? = input
  if (source != null) { val result = source.consume() }
}"#,
            Int,
            Expected::Nullable {
                when: false,
                ordinary: false,
            },
            1,
        ),
        (
            "nullable-local-when",
            r#"fun run(own input: MutableMap<Int, Int>?): Unit {
  var source: MutableMap<Int, Int>? = input
  val ignored = when (source) {
    null -> {}
    else -> { val result = source.consume() }
  }
}"#,
            Int,
            Expected::Nullable {
                when: true,
                ordinary: false,
            },
            1,
        ),
        (
            "nullable-if-ordinary-own-control",
            r#"fun take(own value: MutableMap<Int, Int>): Unit {}
fun run(own input: MutableMap<Int, Int>?): Unit {
  val source: MutableMap<Int, Int>? = input
  if (source != null) { take(source) }
}"#,
            Int,
            Expected::Nullable {
                when: false,
                ordinary: true,
            },
            1,
        ),
        (
            "nullable-when-ordinary-own-control",
            r#"fun take(own value: MutableMap<Int, Int>): Unit {}
fun run(own input: MutableMap<Int, Int>?): Unit {
  var source: MutableMap<Int, Int>? = input
  val ignored = when (source) {
    null -> {}
    else -> { take(source) }
  }
}"#,
            Int,
            Expected::Nullable {
                when: true,
                ordinary: true,
            },
            1,
        ),
    ]
    .map(|(label, text, element, expected, calls)| Case {
        label,
        text,
        key: element,
        value: element,
        expected,
        calls,
    });
    assert_matrix(&cases, 18);
}

#[test]
fn map_consume_complete_rejects_receiver_move_and_live_loan_boundaries() {
    // SPEC-0291 e94b938f chooses L0130 for the extra root eligibility failures.
    // Existing subtree/access diagnostics still take priority; no new span is frozen here.
    use BuiltinType::{Int, String as Str};
    let cases = [
        ("temporary", r#"fun run(): Unit { val result = mutableMapOf<Int, Int>().consume() }"#, Int, Expected::Rejected("L0130"), 1),
        ("global-root", r#"val source = mutableMapOf<Int, Int>()
fun run(): Unit {
  val result = source.consume()
  val count = source.size
}"#, Int, Expected::Rejected("L0130"), 1),
        ("own-parameter", r#"fun run(own source: MutableMap<Int, Int>): Unit {
  val result = source.consume()
  val count = source.size
}"#, Int, Expected::Rejected("L0130"), 1),
        ("borrow-parameter", r#"fun run(source: MutableMap<Int, Int>): Unit { val result = source.consume() }"#, Int, Expected::Rejected("L0133"), 1),
        ("inout-parameter", r#"fun run(inout source: MutableMap<Int, Int>): Unit { val result = source.consume() }"#, Int, Expected::Rejected("L0133"), 1),
        ("explicit-borrow-local", r#"fun run(input: MutableMap<Int, Int>): Unit {
  borrow val source = input
  val result = source.consume()
}"#, Int, Expected::Rejected("L0133"), 1),
        ("stable-field", r#"class Holder(val source: MutableMap<Int, Int>)
fun run(): Unit {
  val holder = Holder(mutableMapOf<Int, Int>())
  val result = holder.source.consume()
}"#, Int, Expected::Rejected("L0132"), 1),
        ("stable-element", r#"fun run(): Unit {
  val maps = listOf(mutableMapOf<Int, Int>())
  val result = maps[0].consume()
}"#, Int, Expected::Rejected("L0136"), 1),
        ("read-after-move", r#"fun run(): Unit {
  val source = mutableMapOf<Int, Int>()
  val result = source.consume()
  val count = source.size
}"#, Int, Expected::Rejected("L0131"), 1),
        ("consume-twice", r#"fun run(): Unit {
  val source = mutableMapOf<Int, Int>()
  val first = source.consume()
  val second = source.consume()
}"#, Int, Expected::Rejected("L0131"), 2),
        ("active-loan", r#"fun run(): Unit {
  var source = mutableMapOf<String, String>()
  source.put("key", "value")
  borrow val item = source.requireValue("key")
  val result = source.consume()
  println(item)
}"#, Str, Expected::Rejected("L0135"), 1),
        ("for-binding", r#"fun run(): Unit {
  val maps = listOf(mutableMapOf<Int, Int>())
  for (source in maps) {
    val result = source.consume()
    break
  }
}"#, Int, Expected::Rejected("L0133"), 1),
        ("temporary-preserves-child-error", r#"fun identity(own item: MutableMap<Int, Int>): MutableMap<Int, Int> { return item }
fun run(): Unit {
  val source = mutableMapOf<Int, Int>()
  val moved = source
  val result = identity(source).consume()
}"#, Int, Expected::Rejected("L0131"), 1),
    ].map(|(label, text, element, expected, calls)| Case { label, text, key: element, value: element, expected, calls });
    assert_matrix(&cases, 26);
}
