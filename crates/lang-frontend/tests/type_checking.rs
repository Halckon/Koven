//! SPEC-0019 基础类型、局部推导和返回契约集成测试。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameEnvironment, NameResolution, resolve_names},
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        AggregateProjectionReceiver, BuiltinType, CallReceiverOrigin, Capability, DeferredReason,
        IntrinsicCallable, IntrinsicTypeConstructor, ParameterMode, TypeCheckingError,
        TypeEnvironment, TypeKind, TypedFile, check_types,
    },
};
use std::{collections::BTreeSet, fs, path::Path};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

const BUILTINS: [BuiltinType; 16] = [
    BuiltinType::Byte,
    BuiltinType::Short,
    BuiltinType::Int,
    BuiltinType::Long,
    BuiltinType::UByte,
    BuiltinType::UShort,
    BuiltinType::UInt,
    BuiltinType::ULong,
    BuiltinType::Float,
    BuiltinType::Double,
    BuiltinType::Boolean,
    BuiltinType::Char,
    BuiltinType::String,
    BuiltinType::Unit,
    BuiltinType::Nothing,
    BuiltinType::Any,
];

fn parse(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("types.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "type checking source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (sources, parsed)
}

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let mut declarations = Vec::new();
    for builtin in BUILTINS {
        declarations.push((
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        ));
    }
    let capabilities = [
        (
            names.declare_type("Copyable").expect("Copyable"),
            Capability::Copyable,
        ),
        (
            names.declare_type("Transferable").expect("Transferable"),
            Capability::Transferable,
        ),
    ];
    let containers = [
        (
            names.declare_type("Box").expect("Box"),
            IntrinsicTypeConstructor::Box,
        ),
        (
            names.declare_type("Rc").expect("Rc"),
            IntrinsicTypeConstructor::Rc,
        ),
        (
            names.declare_type("Array").expect("Array"),
            IntrinsicTypeConstructor::Array,
        ),
        (
            names.declare_type("List").expect("List"),
            IntrinsicTypeConstructor::List,
        ),
        (
            names.declare_type("MutableList").expect("MutableList"),
            IntrinsicTypeConstructor::MutableList,
        ),
    ];
    let constructors = [
        (
            names.declare_function("arrayOf").expect("arrayOf"),
            IntrinsicCallable::ArrayOf,
        ),
        (
            names.declare_function("listOf").expect("listOf"),
            IntrinsicCallable::ListOf,
        ),
        (
            names
                .declare_function("mutableListOf")
                .expect("mutableListOf"),
            IntrinsicCallable::MutableListOf,
        ),
    ];
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in declarations {
        types.bind_builtin(symbol, builtin).expect("binding");
    }
    for (symbol, capability) in capabilities {
        types
            .bind_capability(symbol, capability)
            .expect("capability binding");
    }
    for (symbol, container) in containers {
        types
            .bind_intrinsic(symbol, container)
            .expect("container binding");
    }
    for (symbol, constructor) in constructors {
        types
            .bind_intrinsic_callable(symbol, constructor)
            .expect("constructor binding");
    }
    (names, types)
}

fn checked(text: &str) -> (SourceMap, ParsedFile, NameResolution, TypedFile) {
    let (sources, parsed) = parse(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    (sources, parsed, resolution, typed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn diagnostic_spans<'a>(sources: &'a SourceMap, diagnostic: &Diagnostic) -> (&'a str, &'a str) {
    let primary = sources
        .slice(diagnostic.primary_span())
        .expect("primary text");
    let label = diagnostic
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("diagnostic label");
    (primary, label)
}

fn literal_type(
    sources: &SourceMap,
    parsed: &ParsedFile,
    typed: &TypedFile,
    text: &str,
) -> BuiltinType {
    let (id, _) = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()) == Ok(text))
        .expect("literal");
    let ty = typed.expression_type(id).expect("expression type");
    match typed.types().get(ty).expect("type") {
        TypeKind::Builtin(builtin) => *builtin,
        other => panic!("unexpected type {other:?}"),
    }
}

#[test]
fn numeric_defaults_suffixes_and_contextual_integer_types_are_stable() {
    let text = "fun values(): Unit {\n\
                val byte: Byte = 1\n\
                val default = 2\n\
                val wide = 2147483648\n\
                val unsigned = 3u\n\
                val unsignedWide = 4294967296u\n\
                val long = 4L\n\
                val unsignedLong = 5uL\n\
                val float = 6f\n\
                val double = 7.0\n\
                }";
    let (sources, parsed, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    for (literal, expected) in [
        ("1", BuiltinType::Byte),
        ("2", BuiltinType::Int),
        ("2147483648", BuiltinType::Long),
        ("3u", BuiltinType::UInt),
        ("4294967296u", BuiltinType::ULong),
        ("4L", BuiltinType::Long),
        ("5uL", BuiltinType::ULong),
        ("6f", BuiltinType::Float),
        ("7.0", BuiltinType::Double),
    ] {
        assert_eq!(literal_type(&sources, &parsed, &typed, literal), expected);
    }
}

#[test]
fn nominal_box_and_rc_values_adapt_to_their_nullable_types() {
    let text = "class Node {}\n\
                value class Token(val item: Int)\n\
                fun accepted(): Unit {\n\
                    val node: Node? = Node()\n\
                    val boxed: Box<Token>? = Box(Token(1))\n\
                    val owner: Rc<Int>? = Rc(2)\n\
                }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.constructions().len(), 4);
}

#[test]
fn enum_case_types_preserve_root_payloads_and_are_rejected_outside_type_tests() {
    let text = "enum class Shape {\n\
                    Circle(radius: Int), Point;\n\
                    fun isCircle(): Boolean = this is Circle\n\
                }\n\
                fun invalid(input: Shape.Circle): Unit {}";
    let (sources, parsed, names, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0114"]);
    assert_eq!(typed.enum_cases().len(), 2);
    let circle = &typed.enum_cases()[0];
    assert_eq!(circle.id(), names.enum_cases()[0].id());
    assert_eq!(circle.payloads().len(), 1);
    assert!(matches!(
        typed.types().get(circle.root_type()),
        Some(TypeKind::Nominal { .. })
    ));
    let case_type = parsed
        .ast()
        .type_refs()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("Circle"))
        .find_map(|(id, _)| {
            let ty = typed.type_ref_type(id)?;
            matches!(typed.types().get(ty), Some(TypeKind::EnumCase { .. })).then_some(ty)
        })
        .expect("type-test case type");
    assert!(matches!(
        typed.types().get(case_type),
        Some(TypeKind::EnumCase { case, .. }) if *case == circle.id()
    ));
    assert_eq!(
        diagnostic_spans(&sources, &typed.diagnostics()[0]),
        ("Shape.Circle", "Shape")
    );
}

#[test]
fn type_tests_drive_enum_payload_and_nullable_smart_casts() {
    let text = "enum class Shape { Circle(radius: Int), Point }\n\
                fun radius(shape: Shape): Int {\n\
                    if (shape is Shape.Circle) { return shape.radius }\n\
                    return 0\n\
                }\n\
                fun increment(input: Int?): Int {\n\
                    if (input != null) { return input + 1 }\n\
                    return 0\n\
                }";
    let (sources, parsed, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.aggregate_projections().len(), 1);
    let [non_null_use] = typed.non_null_uses() else {
        panic!("expected exactly one proven non-null use");
    };
    let expression = parsed
        .ast()
        .expressions()
        .get(non_null_use.expression())
        .expect("non-null expression");
    assert_eq!(sources.slice(expression.span()), Ok("input"));
    assert_eq!(
        typed.types().get(non_null_use.declared_type()),
        Some(&TypeKind::Nullable(non_null_use.narrowed_type()))
    );
    assert_eq!(
        typed.expression_type(non_null_use.expression()),
        Some(non_null_use.narrowed_type())
    );
    assert_eq!(
        typed.non_null_use(non_null_use.expression()),
        Some(*non_null_use)
    );
    let comparison = typed
        .null_comparison(
            parsed
                .ast()
                .expressions()
                .iter()
                .find(|(_, node)| sources.slice(node.span()) == Ok("input != null"))
                .expect("null comparison")
                .0,
        )
        .expect("typed null comparison fact");
    assert!(comparison.non_null_when_true());
    assert_eq!(comparison.symbol(), non_null_use.symbol());
    assert_eq!(comparison.nullable_type(), non_null_use.declared_type());
}

#[test]
fn assignment_and_capture_invalidate_mutable_smart_casts_and_invalid_tests_are_precise() {
    let text = "enum class Shape { Circle(radius: Int), Point }\n\
                interface Marker\n\
                fun assigned(initial: Shape, replacement: Shape): Int {\n\
                    var current = initial\n\
                    if (current is Shape.Circle) {\n\
                        current = replacement\n\
                        return current.radius\n\
                    }\n\
                    return 0\n\
                }\n\
                fun captured(initial: Shape): Int {\n\
                    var current = initial\n\
                    val callback = { current }\n\
                    if (current is Shape.Circle) { return current.radius }\n\
                    return 0\n\
                }\n\
                fun invalid(shape: Shape): Boolean = shape is Marker";
    let (sources, _, _, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0113", "L0113", "L0106"]);
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
            .collect::<Vec<_>>(),
        ["radius", "radius", "is"]
    );
}

#[test]
fn exhaustive_when_covers_enum_boolean_nullable_and_short_circuit_facts() {
    let text = "enum class Shape {
                    Circle(radius: Int), Point;
                    fun ownRadius(): Int = when (this) {
                        is Circle -> radius
                        is Point -> 0
                    }
                }
                fun external(shape: Shape): Int = when (shape) {
                    is Shape.Circle -> shape.radius
                    Shape.Point -> 0
                }
                fun positive(shape: Shape): Boolean =
                    shape is Shape.Circle && shape.radius > 0
                fun local(shape: Shape): Boolean {
                    val current = shape
                    return current is Shape.Circle && current.radius > 0
                }
                fun disjunction(shape: Shape): Boolean =
                    shape !is Shape.Circle || shape.radius > 0
                fun afterExit(shape: Shape): Int {
                    if (shape !is Shape.Circle) { return 0 }
                    return shape.radius
                }
                fun nullable(shape: Shape?): Int = when (shape) {
                    null -> 0
                    is Shape.Circle -> shape.radius
                    is Shape.Point -> 0
                }
                fun boolean(flag: Boolean): Int = when (flag) {
                    true -> 1
                    false -> 0
                }
                fun preserveRoot(shape: Shape): Shape {
                    if (shape is Shape.Circle) { return shape }
                    return shape
                }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn generic_enum_case_tests_substitute_root_arguments_for_payloads() {
    let text = "enum class Maybe<T> {
                    Some(item: T), None;
                    fun valueOr(fallback: T): T = when (this) {
                        is Some -> item
                        is None -> fallback
                    }
                }
                fun external(input: Maybe<Int>): Int = when (input) {
                    is Maybe.Some<Int> -> input.item
                    is Maybe.None<Int> -> 0
                }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn bare_enum_cases_publish_zero_operand_construction_facts() {
    let text = "enum class Flag { On, Off }
                enum class Maybe<T> { Some(item: T), None }
                fun flag(): Flag = Flag.On
                fun none(): Maybe<Int> = Maybe.None";
    let (_, _, names, typed) = checked(text);

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.constructions().len(), 2);
    assert_eq!(typed.constructions()[0].arguments(), []);
    assert_eq!(typed.constructions()[1].arguments(), []);
    assert_eq!(
        typed.constructions()[0].target(),
        lang_frontend::type_checking::ConstructionTarget::EnumCase(names.enum_cases()[0].id())
    );
    assert_eq!(
        typed.constructions()[1].instance().type_arguments().len(),
        1
    );
    assert!(typed.calls().is_empty());
}

#[test]
fn generic_bare_enum_case_requires_an_independent_expected_root() {
    let text = "enum class Maybe<T> { Some(item: T), None }
                fun invalid(): Unit { val result = Maybe.None }";
    let (sources, _, _, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0144"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("constructor span"),
        "None"
    );
    assert!(typed.constructions().is_empty());
}

#[test]
fn nominal_and_payload_constructions_publish_instantiated_value_mappings() {
    let text = "class Resource {}
                class Holder<T>(val item: T)
                value class Pair<T>(val first: T, val second: T)
                enum class Maybe<T> { Some(item: T), None }
                fun resource(): Resource = Resource()
                fun holder(): Holder<Int> = Holder(1)
                fun pair(): Pair<Int> = Pair(second = 2, first = 1)
                fun some(): Maybe<Int> = Maybe.Some(1)";
    let (sources, parsed) = parse(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    let repeated = check_types(&sources, &parsed, &resolution, &types).expect("repeated types");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.constructions(), repeated.constructions());
    assert_eq!(typed.constructions().len(), 4);
    assert!(typed.calls().is_empty());
    assert_eq!(typed.constructions()[0].arguments(), []);
    assert_eq!(
        typed.constructions()[1].instance().type_arguments().len(),
        1
    );
    let pair = &typed.constructions()[2];
    assert_eq!(pair.arguments()[0].parameter_name(), "first");
    assert_eq!(pair.arguments()[0].evaluation_index(), 1);
    assert_eq!(pair.arguments()[1].parameter_name(), "second");
    assert_eq!(pair.arguments()[1].evaluation_index(), 0);
    assert!(matches!(
        typed.constructions()[3].target(),
        lang_frontend::type_checking::ConstructionTarget::EnumCase(_)
    ));
    for construction in typed.constructions() {
        assert_eq!(
            typed.expression_type(construction.expression()),
            Some(construction.result_type())
        );
    }
}

#[test]
fn expected_result_and_intrinsic_box_complete_construction_instances() {
    let text = "class Marker<T> {}
                value class Point(val x: Int)
                fun marker(): Marker<Int> = Marker()
                fun boxed(): Box<Point> = Box(Point(1))
                fun explicit(): Box<Point> = Box<Point>(Point(2))";
    let (_, _, _, typed) = checked(text);

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.constructions().len(), 5);
    assert!(matches!(
        typed.constructions()[2].target(),
        lang_frontend::type_checking::ConstructionTarget::IntrinsicBox
    ));
    assert!(typed.calls().is_empty());
}

#[test]
fn intrinsic_rc_publishes_construction_share_and_payload_borrow_facts() {
    use lang_frontend::type_checking::{
        ConstructionTarget, Copyability, ParameterMode, RcOperationKind,
    };

    let text = "value class Point(val x: Int)
                fun shared(): Int {
                    val first = Rc(Point(1))
                    val second = first.share()
                    val explicit = Rc<Point>(Point(2))
                    val copied = first.value.x
                    return second.value.x
                }";
    let (_, _, _, typed) = checked(text);

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let rc_constructions = typed
        .constructions()
        .iter()
        .filter(|descriptor| descriptor.target() == ConstructionTarget::IntrinsicRc)
        .collect::<Vec<_>>();
    assert_eq!(rc_constructions.len(), 2);
    assert!(rc_constructions.iter().all(|descriptor| {
        descriptor.arguments().len() == 1
            && descriptor.arguments()[0].parameter_name() == "value"
            && descriptor.arguments()[0].mode() == ParameterMode::Value
            && typed.copyability(descriptor.result_type()) == Some(Copyability::MoveOnly)
    }));
    assert_eq!(typed.rc_operations().len(), 3);
    assert_eq!(typed.rc_operations()[0].kind(), RcOperationKind::Share);
    assert_eq!(typed.rc_operations()[0].result_mode(), ParameterMode::Value);
    assert!(typed.rc_operations()[1..].iter().all(|descriptor| {
        descriptor.kind() == RcOperationKind::Value
            && descriptor.result_mode() == ParameterMode::Borrow
    }));
}

#[test]
fn rc_shapes_reuse_existing_call_diagnostics_without_partial_facts() {
    let text = "fun missing(): Unit { val result = Rc() }
                fun extra(): Unit { val result = Rc<Int>(1, 2) }
                fun badShare(): Unit {
                    val owner = Rc(1)
                    val shared = owner.share(2)
                }";
    let (_, _, _, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0121", "L0121", "L0121"]);
    assert_eq!(
        typed
            .constructions()
            .iter()
            .filter(|descriptor| matches!(
                descriptor.target(),
                lang_frontend::type_checking::ConstructionTarget::IntrinsicRc
            ))
            .count(),
        1
    );
    assert!(typed.rc_operations().is_empty());
}

#[test]
fn invalid_and_underconstrained_constructions_fail_without_typed_facts() {
    let text = "interface Contract
                enum class Choice { One }
                class Marker<T> {}
                fun invalidTargets(): Unit {
                    val contract = Contract()
                    val choice = Choice()
                }
                fun missing(): Unit { val result = Marker() }";
    let (sources, _, _, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0143", "L0143", "L0144"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("span"),
        "Contract"
    );
    assert!(typed.constructions().is_empty());
}

#[test]
fn uninstantiated_outer_type_parameters_do_not_complete_constructor_inference() {
    let text = "class Marker<T> {}
                fun <T> invalid(): Marker<T> = Marker()";
    let (_, _, _, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0144"]);
    assert!(typed.constructions().is_empty());
}

#[test]
fn constructor_overload_trials_commit_only_the_unique_nested_fact() {
    let text = "class Marker<T> {}
                fun choose(action: () -> Marker<Int>): Int = 1
                fun choose(action: () -> Marker<Long>): Long = 1L
                fun selected(): Int = choose({ Marker<Int>() })";
    let (_, _, _, typed) = checked(text);

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.constructions().len(), 1);
    assert_eq!(
        typed.constructions()[0].instance().type_arguments().len(),
        1
    );
}

#[test]
fn candidate_local_expected_does_not_infer_nested_constructor_arguments() {
    let text = "class Marker<T> {}
                fun choose(action: () -> Marker<Int>): Int = 1
                fun choose(action: () -> Marker<Long>): Long = 1L
                fun rejected(): Unit { val result = choose({ Marker() }) }";
    let (_, _, _, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0123"]);
    assert!(typed.constructions().is_empty());
}

#[test]
fn source_box_name_keeps_nominal_constructor_identity() {
    let text = "class Box<T>(val item: T)
                fun source(): Box<Int> = Box(1)";
    let (_, _, _, typed) = checked(text);

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(matches!(
        typed.constructions()[0].target(),
        lang_frontend::type_checking::ConstructionTarget::Nominal(_)
    ));
}

#[test]
fn source_rc_name_never_gains_intrinsic_construction_or_share_facts() {
    let text = "class Rc<T>(val item: T) {
                    fun share(): Rc<T> = Rc(item)
                }
                fun source(): Rc<Int> {
                    val owner = Rc(1)
                    return owner.share()
                }";
    let (_, _, _, typed) = checked(text);

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.constructions().iter().all(|construction| matches!(
        construction.target(),
        lang_frontend::type_checking::ConstructionTarget::Nominal(_)
    )));
    assert!(typed.rc_operations().is_empty());
    assert!(typed.calls().iter().any(|call| matches!(
        call.instance().target(),
        lang_frontend::type_checking::CallableTarget::Source(_)
    )));
}

#[test]
fn construction_reuses_named_arity_mode_and_type_diagnostics() {
    let text = "class Pair(val first: Int, val second: Int)
                fun named(): Unit { val result = Pair(missing = 1, second = 2) }
                fun arity(): Unit { val result = Pair(1) }
                fun mode(): Unit { val result = Pair(borrow 1, 2) }
                fun typed(): Unit { val result = Pair(true, 2) }";
    let (sources, _, _, typed) = checked(text);

    assert_eq!(
        codes(typed.diagnostics()),
        ["L0120", "L0121", "L0122", "L0084"]
    );
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("named span"),
        "missing"
    );
    assert!(typed.constructions().is_empty());
}

#[test]
fn construction_reuses_capability_bound_diagnostics() {
    let text = "class Resource {}
                class NeedsCopy<T: Copyable> {}
                fun invalid(): Unit { val result = NeedsCopy<Resource>() }";
    let (_, _, _, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0115"]);
    assert!(typed.constructions().is_empty());
}

#[test]
fn when_negative_and_comma_coverage_keep_only_shared_flow_facts() {
    let valid = "enum class Shape { Circle(radius: Int), Point }
                 fun classify(shape: Shape): Int = when (shape) {
                     !is Shape.Circle -> 0
                     is Shape.Circle -> shape.radius
                 }";
    let (_, _, _, typed) = checked(valid);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

    let invalid = "enum class Shape { Circle(radius: Int), Point }
                   fun merged(shape: Shape): Int = when (shape) {
                       is Shape.Circle, is Shape.Point -> shape.radius
                   }";
    let (sources, _, _, typed) = checked(invalid);
    assert_eq!(codes(typed.diagnostics()), ["L0113"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("payload span"),
        "radius"
    );
}

#[test]
fn when_context_and_coverage_diagnostics_are_deterministic() {
    let text = "enum class Shape { Circle(radius: Int), Point }
                fun statement(shape: Shape): Int {
                    when (shape) { is Shape.Circle -> shape.radius }
                    return 0
                }
                fun nestedStatement(flag: Boolean, shape: Shape): Int {
                    if (flag) {
                        when (shape) { is Shape.Circle -> shape.radius }
                    }
                    return 0
                }
                fun missing(shape: Shape): Int = when (shape) {
                    is Shape.Circle -> shape.radius
                }
                fun repeated(flag: Boolean): Int = when (flag) {
                    true -> 1
                    true -> 2
                }
                fun misplaced(flag: Boolean): Int = when (flag) {
                    else -> 0
                    true -> 1
                    else -> 2
                }";
    let (sources, _, _, typed) = checked(text);
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0111", "L0111", "L0110", "L0109", "L0108"]
    );
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
            .collect::<Vec<_>>(),
        ["when", "when", "true", "else", "else"]
    );
}

#[test]
fn subjectless_when_requires_boolean_conditions_and_known_branches_join_to_any() {
    let invalid = "fun invalid(): Int = when { 1 -> 1; else -> 0 }";
    let (sources, _, _, typed) = checked(invalid);
    assert_eq!(codes(typed.diagnostics()), ["L0107"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("condition span"),
        "1"
    );

    let valid = "fun mixed(flag: Boolean): Any = when (flag) {
                     true -> 1
                     false -> \"text\"
                 }";
    let (_, _, _, typed) = checked(valid);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn when_branch_join_handles_nothing_nullable_enum_cases_and_any() {
    let text = "enum class Choice { First, Second }
                fun joins(flag: Boolean, optional: Int?, choice: Choice): Unit {
                    val nullable = when (flag) { true -> 1; false -> optional }
                    val root = when (choice) {
                        is Choice.First -> choice
                        is Choice.Second -> choice
                    }
                    val mixed = when (flag) { true -> 1; false -> \"text\" }
                }
                fun bottom(flag: Boolean): Int = when (flag) {
                    true -> return 1
                    false -> 2
                }";
    let (_, _, names, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

    let symbol_type = |name: &str| {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == name)
            .expect("local symbol");
        typed
            .symbol_type(symbol.id())
            .and_then(|ty| typed.types().get(ty))
            .expect("local type")
    };
    assert!(matches!(
        symbol_type("nullable"),
        TypeKind::Nullable(inner)
            if matches!(typed.types().get(*inner), Some(TypeKind::Builtin(BuiltinType::Int)))
    ));
    assert!(matches!(symbol_type("root"), TypeKind::Nominal { .. }));
    assert!(matches!(
        symbol_type("mixed"),
        TypeKind::Builtin(BuiltinType::Any)
    ));
}

#[test]
fn local_lambda_operator_and_type_ref_diagnostics_use_published_codes() {
    let (_, _, _, valid) = checked(
        "fun apply(): Unit {\n\
         val callback: (Int) -> Int = { x -> x + 1 }\n\
         val safe: Int? = null\n\
         val resolved = safe ?: 0\n\
         }",
    );
    assert!(valid.diagnostics().is_empty(), "{:?}", valid.diagnostics());

    let (_, _, _, invalid) = checked(
        "fun invalid(): Unit {\n\
         val missing = null\n\
         val mismatch: String = 1\n\
         val operand = false + 1\n\
         val arity: Int<String> = 1\n\
         }",
    );
    assert_eq!(
        codes(invalid.diagnostics()),
        ["L0083", "L0084", "L0085", "L0082"]
    );
}

#[test]
fn callable_flow_reports_shape_missing_return_and_branch_join() {
    let (_, _, _, typed) = checked(
        "fun missing(): Int {}\n\
         fun bare(): Int { return }\n\
         fun unit(): Unit = 1\n\
         fun branch(flag: Boolean): Int = if (flag) { 1 } else { false }\n\
         fun mixed(flag: Boolean): Unit { val result = if (flag) { 1 } else { false } }",
    );
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0088", "L0087", "L0084", "L0084", "L0089"]
    );
}

#[test]
fn unbraced_if_branches_receive_the_outer_expected_type() {
    let (sources, _, _, typed) =
        checked("fun contextual(flag: Boolean): Byte = if (flag) 127 else 128");

    assert_eq!(codes(typed.diagnostics()), ["L0090"]);
    assert_eq!(
        sources.slice(typed.diagnostics()[0].primary_span()),
        Ok("128")
    );
}

#[test]
fn jump_targets_stop_at_callable_boundaries() {
    let (_, _, _, valid) = checked(
        "fun valid(flag: Boolean): Unit {\n\
         while (flag) { if (flag) { continue } else { break } }\n\
         loop {\n\
         val callback: () -> Unit = { loop { break } }\n\
         break\n\
         }\n\
         }",
    );
    assert!(valid.diagnostics().is_empty(), "{:?}", valid.diagnostics());

    let (sources, _, _, invalid) = checked(
        "val top = break\n\
         fun invalid(): Unit {\n\
         continue\n\
         loop {\n\
         val callback: () -> Unit = { break }\n\
         break\n\
         }\n\
         }",
    );
    assert_eq!(codes(invalid.diagnostics()), ["L0142", "L0142", "L0142"]);
    assert_eq!(
        invalid
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message())
            .collect::<Vec<_>>(),
        [
            "break is not inside an enclosing loop in this callable",
            "continue is not inside an enclosing loop in this callable",
            "break is not inside an enclosing loop in this callable",
        ]
    );
    let primary_text = invalid
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic primary span")
        })
        .collect::<Vec<_>>();
    assert_eq!(primary_text, ["break", "continue", "break"]);
}

#[test]
fn bottom_non_null_and_numeric_boundaries_do_not_widen_silently() {
    let (_, _, _, typed) = checked(
        "val outside = return\n\
         fun boundaries(): Unit {\n\
             val min: Byte = -128\n\
             val max: Byte = 127\n\
             val unsigned: UByte = 255u\n\
             val optional: Int? = 1\n\
             val required = optional!!\n\
             val invalidAssert = 1!!\n\
             val negativeUnsigned = -1u\n\
             val overflow: Byte = 128\n\
             val noSignedToUnsigned: UInt = 1\n\
         }",
    );
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0086", "L0085", "L0085", "L0090", "L0084"]
    );
}

#[test]
fn repeated_checks_are_deterministic_and_foreign_source_maps_fail() {
    let (sources, parsed) = parse(
        "enum class Choice { First, Second }
         fun select(input: Choice): Int = when (input) {
             is Choice.First -> 1
             is Choice.Second -> 2
         }",
    );
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let first = check_types(&sources, &parsed, &resolution, &types).expect("first");
    let second = check_types(&sources, &parsed, &resolution, &types).expect("second");
    assert_eq!(first.types(), second.types());
    assert_eq!(first.diagnostics(), second.diagnostics());
    for (id, _) in parsed.ast().expressions().iter() {
        assert_eq!(first.expression_type(id), second.expression_type(id));
    }

    let foreign = SourceMap::new();
    assert!(matches!(
        check_types(&foreign, &parsed, &resolution, &types),
        Err(TypeCheckingError::Source(_))
    ));
}

#[test]
fn long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic() {
    let mut text = String::from("enum class Large { ");
    for index in 0..128 {
        if index != 0 {
            text.push_str(", ");
        }
        text.push_str(&format!("C{index}"));
    }
    text.push_str(" }\nfun select(input: Large): Int = when (input) {\n");
    for index in 0..128 {
        text.push_str(&format!("Large.C{index} -> {index}\n"));
    }
    text.push_str("}\n");
    text.push_str("enum class Shape { Circle(radius: Int), Point }\n");
    text.push_str("fun deep(shape: Shape): Boolean = shape is Shape.Circle");
    for _ in 0..96 {
        text.push_str(" && shape.radius > 0");
    }
    text.push('\n');

    let (_, _, _, first) = checked(&text);
    let (_, _, _, second) = checked(&text);
    assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
    assert_eq!(first.diagnostics(), second.diagnostics());
    assert_eq!(first.enum_cases(), second.enum_cases());
}

#[test]
fn environment_identity_is_explicit_and_duplicate_or_wrong_bindings_fail_loud() {
    let (sources, parsed) = parse("val datum: Int = 1");
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let (_, foreign_types) = environments();
    assert!(matches!(
        check_types(&sources, &parsed, &resolution, &foreign_types),
        Err(TypeCheckingError::MismatchedNameEnvironment)
    ));

    let mut duplicate = types.clone();
    let int = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "Int")
        .expect("Int")
        .id();
    assert!(matches!(
        duplicate.bind_builtin(int, BuiltinType::Int),
        Err(TypeCheckingError::InvalidExternalBinding)
    ));
    let copyable = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "Copyable")
        .expect("Copyable")
        .id();
    assert!(matches!(
        duplicate.bind_capability(copyable, Capability::Copyable),
        Err(TypeCheckingError::InvalidExternalBinding)
    ));
}

#[test]
fn numeric_overflow_is_a_source_diagnostic_not_an_internal_failure() {
    let (_, _, _, typed) = checked("val huge = 340282366920938463463374607431768211456");
    assert_eq!(codes(typed.diagnostics()), ["L0090"]);
}

#[test]
fn nominal_and_type_parameter_identity_is_known_and_arity_is_exact() {
    let text = "class Box<T>(val item: T) {}\n\
                class Left {}\n\
                class Right {}\n\
                fun inspect(left: Box<Int>, again: Box<Int>, missing: Box, many: Left<Int>): Unit";
    let (sources, parsed, resolution, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0091", "L0091"]);
    assert_eq!(
        diagnostic_spans(&sources, &typed.diagnostics()[0]),
        ("Box", "Box")
    );
    assert_eq!(
        diagnostic_spans(&sources, &typed.diagnostics()[1]),
        ("Left", "Left")
    );
    assert_eq!(typed.nominals().len(), 3);
    assert_ne!(typed.nominals()[1].id(), typed.nominals()[2].id());

    let mut box_types = parsed
        .ast()
        .type_refs()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("Box<Int>"))
        .map(|(id, _)| typed.type_ref_type(id).expect("typed Box<Int>"));
    let first = box_types.next().expect("first Box<Int>");
    assert_eq!(box_types.next(), Some(first));
    assert!(
        matches!(typed.types().get(first), Some(TypeKind::Nominal { arguments, .. }) if arguments.len() == 1)
    );

    let type_parameter = resolution
        .symbols()
        .iter()
        .find(|symbol| {
            symbol.name() == "T"
                && matches!(
                    symbol.kind(),
                    lang_frontend::name_resolution::SymbolKind::TypeParameter
                )
        })
        .expect("type parameter");
    assert!(matches!(
        typed.symbol_type(type_parameter.id()).and_then(|id| typed.types().get(id)),
        Some(TypeKind::TypeParameter(symbol)) if *symbol == type_parameter.id()
    ));
}

#[test]
fn bounds_are_static_but_interfaces_are_not_runtime_value_types() {
    let text = "interface Protocol {}\n\
                class Concrete {}\n\
                class Good<T: Protocol> : Protocol {}\n\
                class AnyBound<T: Any> {}\n\
                class CopyBound<T: Copyable> {}\n\
                class BadClass<T: Concrete> {}\n\
                class BadNullable<T: Protocol?> {}\n\
                fun bad(input: Protocol): Unit";
    let (sources, _, _, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0092", "L0092", "L0094"]);
    assert_eq!(
        diagnostic_spans(&sources, &typed.diagnostics()[0]),
        ("Concrete", "T")
    );
    assert_eq!(
        diagnostic_spans(&sources, &typed.diagnostics()[1]),
        ("Protocol?", "T")
    );
    assert_eq!(
        diagnostic_spans(&sources, &typed.diagnostics()[2]),
        ("Protocol", "Protocol")
    );
    assert!(typed.type_parameters().iter().any(|parameter| matches!(
        parameter.bound(),
        lang_frontend::type_checking::TypeParameterBound::Interface(_)
    )));
}

#[test]
fn direct_interface_edges_reject_classes_duplicates_and_cycles() {
    let (_, _, _, non_interface) = checked("class Parent {}\nclass Child : Parent {}");
    assert_eq!(codes(non_interface.diagnostics()), ["L0095"]);

    let (_, _, _, duplicate) =
        checked("interface Protocol<T> {}\nclass Both : Protocol<Int>, Protocol<Long> {}");
    assert_eq!(codes(duplicate.diagnostics()), ["L0095"]);

    let (_, _, _, cycle) = checked("interface Left : Right {}\ninterface Right : Left {}");
    assert_eq!(codes(cycle.diagnostics()), ["L0096"]);
}

#[test]
fn interface_closure_substitutes_invariant_arguments_and_checks_bounds() {
    let text = "interface Base<T> {}\n\
                interface Mid<U> : Base<U> {}\n\
                class Good : Mid<Int> {}\n\
                class Wrong : Mid<Long> {}\n\
                class Holder<T: Base<Int>> {}\n\
                class Generic<U: Mid<Int>> { fun pass(input: Holder<U>): Unit {} }\n\
                fun inspect(good: Holder<Good>, bad: Holder<Wrong>): Unit";
    let (sources, _, resolution, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0093"]);
    assert_eq!(
        diagnostic_spans(&sources, &typed.diagnostics()[0]),
        ("Wrong", "T")
    );
    let good = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "Good")
        .expect("Good");
    let descriptor = typed
        .nominals()
        .iter()
        .find(|descriptor| descriptor.id().symbol() == good.id())
        .expect("Good descriptor");
    assert_eq!(descriptor.direct_interfaces().len(), 1);
    assert_eq!(descriptor.interfaces().len(), 2);
}

#[test]
fn overload_shape_is_alpha_equivalent_and_concrete_members_need_bodies() {
    let text = "fun <T> pick(input: T): Int = 1\n\
                fun <U> pick(other: U): Long = 1L\n\
                fun mode(input: Int): Unit {}\n\
                fun mode(borrow other: Int): Unit {}\n\
                fun mode(own consumed: Int): Unit {}\n\
                interface Contract { fun required(input: Int): Unit }\n\
                class Concrete { fun missing(input: Int): Unit; fun okay(): Unit {} }";
    let (_, _, _, typed) = checked(text);
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0097", "L0097", "L0097", "L0098"]
    );
}

#[test]
fn receiver_modes_are_contracts_but_not_overload_shapes() {
    let (_, _, _, overload) = checked(
        "class Duplicate {\n\
             fun collide(input: Int): Unit {}\n\
             inout fun collide(input: Int): Unit {}\n\
         }",
    );
    assert_eq!(codes(overload.diagnostics()), ["L0097"]);

    let (_, _, _, replacement) = checked(
        "interface Base { fun act(): Unit }\n\
         interface Child : Base { inout fun act(): Unit }",
    );
    assert_eq!(codes(replacement.diagnostics()), ["L0099"]);

    let (_, _, _, implementation) = checked(
        "interface Required { fun run(): Unit }\n\
         class Bad : Required { override inout fun run(): Unit {} }",
    );
    assert_eq!(codes(implementation.diagnostics()), ["L0100"]);
}

#[test]
fn interface_replacements_and_overrides_require_exact_contracts() {
    let (_, _, _, replacement) = checked(
        "interface Base { fun act(input: Int): Int }\n\
         interface Child : Base { fun act(input: Int): Long }",
    );
    assert_eq!(codes(replacement.diagnostics()), ["L0099"]);

    let text = "interface Required { fun run(input: Int): Int }\n\
                class Missing : Required {}\n\
                class NeedsOverride : Required { fun run(input: Int): Int = 1 }\n\
                class BadReturn : Required { override fun run(input: Int): Long = 1L }\n\
                class BadMode : Required { override fun run(own input: Int): Int = 1 }\n\
                class Extra { override fun lone(): Unit {} }\n\
                class Hidden : Required { private override fun run(input: Int): Int = 1 }\n\
                class Good : Required { override fun run(borrow input: Int): Int = 1 }";
    let (_, _, _, typed) = checked(text);
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0101", "L0100", "L0100", "L0100", "L0100", "L0100"]
    );
}

#[test]
fn abstract_requirements_accept_one_default_but_multiple_defaults_conflict() {
    let text = "interface Abstract { fun ping(): Int }\n\
                interface Left { fun ping(): Int = 1 }\n\
                interface Right { fun ping(): Int = 2 }\n\
                interface Child : Left { fun ping(): Int = 3 }\n\
                class Covered : Abstract, Left {}\n\
                class Shadowed : Child {}\n\
                class Conflict : Left, Right {}\n\
                class Resolved : Left, Right { override fun ping(): Int = 3 }";
    let (_, _, _, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0102"]);
}

#[test]
fn super_interface_member_uses_a_static_type_position() {
    let text = "interface Left { fun ping(): Int = 1 }\n\
                interface Right { fun ping(): Int = 2 }\n\
                class Resolved : Left, Right {\n\
                    override fun ping(): Int {\n\
                        val left = super<Left>.ping()\n\
                        val right = super<Right>.ping()\n\
                        return left + right\n\
                    }\n\
                }";
    let (_, parsed, _, typed) = checked(text);

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let super_members = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            Expression::SuperMember { interface, .. } => Some((id, *interface)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(super_members.len(), 2);
    for (expression, interface) in super_members {
        assert!(matches!(
            typed
                .expression_type(expression)
                .and_then(|ty| typed.types().get(ty)),
            Some(TypeKind::Function { .. })
        ));
        assert!(matches!(
            typed
                .type_ref_type(interface)
                .and_then(|ty| typed.types().get(ty)),
            Some(TypeKind::Nominal { .. })
        ));
    }
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| {
                matches!(
                    call.receiver().map(|receiver| receiver.origin()),
                    Some(CallReceiverOrigin::ImplicitThis(_))
                )
            })
            .count(),
        2
    );
}

#[test]
fn super_interface_requires_inheritance_and_receiver_capability() {
    let (_, _, _, capability) = checked(
        "interface Mutable { inout fun touch(): Unit {} }
         class Bad : Mutable { fun run(): Unit { super<Mutable>.touch() } }",
    );
    assert_eq!(codes(capability.diagnostics()), ["L0100"]);

    let (_, _, _, unrelated) = checked(
        "interface Other { fun ping(): Unit {} }
         class Bad { fun run(): Unit { super<Other>.ping() } }",
    );
    assert_eq!(codes(unrelated.diagnostics()), ["L0099"]);

    let (_, _, _, overloaded) = checked(
        "interface Mixed {\n\
             inout fun choose(input: Int): Int = 1\n\
             fun choose(input: String): Int = 2\n\
         }\n\
         class Good : Mixed { fun run(): Int = super<Mixed>.choose(\"ok\") }",
    );
    assert!(
        overloaded.diagnostics().is_empty(),
        "{:?}",
        overloaded.diagnostics()
    );

    let (_, _, _, interface_default) = checked(
        "interface Base { fun ping(): Int = 1 }\n\
         interface Child : Base { fun run(): Int = super<Base>.ping() }",
    );
    assert!(
        interface_default.diagnostics().is_empty(),
        "{:?}",
        interface_default.diagnostics()
    );
}

#[test]
fn expression_tail_fallthrough_preserves_nullable_and_diverging_paths() {
    let text = "fun missing(input: Int?): Int { input ?: return 0 }\n\
                fun closed(input: Nothing?): Int { input ?: return 0 }\n\
                fun casted(): Int { (return 1) as Int }\n\
                fun propagated(): Int { (return 1)? }\n\
                fun referenced(): Int { (return 1)::next }";
    let (_, _, _, typed) = checked(text);

    assert_eq!(codes(typed.diagnostics()), ["L0088"]);
}

#[test]
fn interface_delegation_validates_targets_types_and_conflicts() {
    let valid = "interface Draw { fun draw(): Unit }\n\
                 class Renderer : Draw { override fun draw(): Unit {} }\n\
                 class Screen(val renderer: Renderer) : Draw by renderer {}\n\
                 class Generic<T: Draw>(val target: T) : Draw by target {}";
    let (_, _, _, typed) = checked(valid);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.delegations().len(), 2);

    let invalid = "interface Draw { fun draw(): Unit }\n\
                   class Renderer : Draw { override fun draw(): Unit {} }\n\
                   class Other {}\n\
                   val outside: Renderer = this\n\
                   class Mutable(var renderer: Renderer) : Draw by renderer {}\n\
                   class WrongScope(val actual: Renderer) : Draw by outside {}\n\
                   class Wrong(val other: Other) : Draw by other {}";
    let (_, _, _, typed) = checked(invalid);
    assert_eq!(codes(typed.diagnostics()), ["L0103", "L0103", "L0104"]);

    let conflicts = "interface Draw { fun act(): Unit }\n\
                     interface Reset { fun act(): Unit }\n\
                     interface Default { fun act(): Unit {} }\n\
                     class Drawer : Draw { override fun act(): Unit {} }\n\
                     class Resetter : Reset { override fun act(): Unit {} }\n\
                     class Two(val draw: Drawer, val reset: Resetter) : Draw by draw, Reset by reset {}\n\
                     class Mixed(val draw: Drawer) : Draw by draw, Default {}\n\
                     class Resolved(val draw: Drawer, val reset: Resetter) : Draw by draw, Reset by reset { override fun act(): Unit {} }";
    let (_, _, _, typed) = checked(conflicts);
    assert_eq!(codes(typed.diagnostics()), ["L0105", "L0105"]);
}

#[test]
fn generic_signatures_and_legal_this_types_are_known() {
    let text = "fun <T> identity(item: T): T = item\n\
                class Sample { fun self(): Sample = this }\n\
                interface Protocol { fun touch(): Unit { val current = this } }";
    let (_, parsed, resolution, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let identity = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "identity")
        .expect("identity");
    assert!(matches!(
        typed
            .symbol_type(identity.id())
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Function { .. })
    ));
    let identity_descriptor = typed
        .callables()
        .iter()
        .find(|callable| callable.symbol() == identity.id())
        .expect("identity descriptor");
    assert!(identity_descriptor.owner().is_none());
    assert_eq!(identity_descriptor.type_parameters().len(), 1);
    assert!(
        typed
            .callables()
            .iter()
            .any(|callable| callable.owner().is_some())
    );
    let this_kinds = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), lang_frontend::parser::Expression::This))
        .map(|(id, _)| {
            typed
                .expression_type(id)
                .and_then(|ty| typed.types().get(ty))
                .expect("typed this")
        })
        .collect::<Vec<_>>();
    assert!(matches!(this_kinds[0], TypeKind::Nominal { .. }));
    assert!(matches!(this_kinds[1], TypeKind::StaticSelf(_)));
}

#[test]
fn deep_generic_and_long_interface_graphs_stay_deterministic() {
    let mut text = String::from("class Box<T> {}\ninterface I0 {}\n");
    for index in 1..64 {
        text.push_str(&format!("interface I{index} : I{} {{}}\n", index - 1));
    }
    text.push_str("class Leaf : I63 {}\nfun deep(input: ");
    for _ in 0..96 {
        text.push_str("Box<");
    }
    text.push_str("Int");
    for _ in 0..96 {
        text.push('>');
    }
    text.push_str("): Unit {}\n");
    let (_, _, resolution, typed) = checked(&text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let leaf = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "Leaf")
        .expect("Leaf");
    let descriptor = typed
        .nominals()
        .iter()
        .find(|descriptor| descriptor.id().symbol() == leaf.id())
        .expect("Leaf descriptor");
    assert_eq!(descriptor.interfaces().len(), 64);
}

#[test]
fn later_phase_nodes_keep_distinct_deferred_reasons() {
    let text = "fun deferred(input: Int): Int = input\n\
                val forward = later\n\
                val later = 1\n\
                class Sample {\n\
                    fun inspect(flag: Boolean): Unit {\n\
                        val top: Any = this\n\
                        val nominal: Sample = this\n\
                        val qualified: Sample.Inner = this\n\
                        val member = this.field\n\
                        val callable = ::deferred\n\
                        val called = callable<Int>(1)\n\
                        val indexed = this[0]\n\
                        val casted = this as Int\n\
                        val propagated = this?\n\
                        val overloaded = inspect\n\
                        val selected = when { else -> 1 }\n\
                        val joined = if (flag) { this } else { this }\n\
                        val (left, right) = this\n\
                    }\n\
                }";
    let (sources, parsed) = parse(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(resolution.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

    let mut reasons = BTreeSet::new();
    for (id, _) in parsed.ast().expressions().iter() {
        if let Some(TypeKind::Deferred(reason)) = typed
            .expression_type(id)
            .and_then(|ty| typed.types().get(ty))
        {
            reasons.insert(*reason);
        }
    }
    for (id, _) in parsed.ast().type_refs().iter() {
        if let Some(TypeKind::Deferred(reason)) =
            typed.type_ref_type(id).and_then(|ty| typed.types().get(ty))
        {
            reasons.insert(*reason);
        }
    }
    for symbol in resolution.symbols() {
        if let Some(TypeKind::Deferred(reason)) = typed
            .symbol_type(symbol.id())
            .and_then(|ty| typed.types().get(ty))
        {
            reasons.insert(*reason);
        }
    }
    for expected in [
        DeferredReason::AnyValueRepresentation,
        DeferredReason::QualifiedType,
        DeferredReason::ForwardValueType,
        DeferredReason::MemberAccess,
        DeferredReason::Call,
        DeferredReason::Index,
        DeferredReason::CastOrTypeTest,
        DeferredReason::ErrorPropagation,
        DeferredReason::OverloadSelection,
        DeferredReason::Destructuring,
    ] {
        assert!(
            reasons.contains(&expected),
            "missing {expected:?}: {reasons:?}"
        );
    }
}

#[test]
fn single_file_member_calls_publish_receiver_contracts() {
    let (sources, parsed, _, typed) = checked(
        "class Holder<T>(val item: T) {\n\
             fun echo(input: T): T = input\n\
             borrow fun same(input: T): T = echo(input)\n\
             inout fun replace(own input: T): T = input\n\
             fun current(): T = item\n\
         }\n\
         fun use(holder: Holder<Int>): Int = holder.same(1)",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed
            .callables()
            .iter()
            .filter_map(|callable| callable.receiver().map(|receiver| receiver.mode()))
            .collect::<Vec<_>>(),
        [
            ParameterMode::Borrow,
            ParameterMode::Borrow,
            ParameterMode::Inout,
            ParameterMode::Borrow,
        ]
    );
    let item = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()) == Ok("item"))
        .expect("bare field expression")
        .0;
    assert!(matches!(
        typed
            .aggregate_projection(item)
            .expect("bare field projection")
            .receiver(),
        AggregateProjectionReceiver::This(_)
    ));
    let call = |text: &str| {
        let expression = parsed
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| sources.slice(node.span()) == Ok(text))
            .expect("call expression")
            .0;
        typed.call(expression).expect("receiver call fact")
    };
    assert!(matches!(
        call("echo(input)")
            .receiver()
            .expect("implicit receiver")
            .origin(),
        CallReceiverOrigin::ImplicitThis(_)
    ));
    assert!(matches!(
        call("holder.same(1)")
            .receiver()
            .expect("explicit receiver")
            .origin(),
        CallReceiverOrigin::Expression(_)
    ));
}

#[test]
fn checked_in_phase2_type_fixtures_execute_real_pass_and_fail_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase2");
    for (directory, should_pass) in [("type-pass", true), ("type-fail", false)] {
        let files = fs::read_dir(root.join(directory))
            .expect("fixture directory")
            .map(|entry| entry.expect("fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 10, "zero or unexpected {directory} fixtures");
        for path in files {
            let text = fs::read_to_string(&path).expect("UTF-8 fixture");
            let (_, _, _, typed) = checked(&text);
            if should_pass {
                assert!(typed.diagnostics().is_empty(), "{path:?}");
            } else {
                let expected =
                    fs::read_to_string(path.with_extension("diag")).expect("diagnostic sidecar");
                let actual = typed
                    .diagnostics()
                    .iter()
                    .map(|diagnostic| {
                        let span = diagnostic.primary_span();
                        format!("{}\t{}\t{}", diagnostic.code(), span.start(), span.end())
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";
                assert_eq!(actual, expected, "{path:?}");
            }
        }
    }
}

#[test]
fn nullable_when_null_fallthrough_narrows_else_subject() {
    // Removing null from the remaining domain must make the same stable subject usable as T.
    let (_, _, _, typed) = checked("fun extract(x: Int?): Int = when (x) { null -> 0; else -> x }");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.non_null_uses().len(), 1);
}

#[test]
fn nullable_when_mixed_alternatives_do_not_narrow_body() {
    // A null alternative keeps the body nullable even when another alternative is non-null.
    let (sources, _, _, typed) =
        checked("fun extract(x: Boolean?): Boolean = when (x) { null, true -> x; else -> false }");
    assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("body span"),
        "x"
    );
    assert!(typed.non_null_uses().is_empty());
}

#[test]
fn nullable_when_remaining_domain_covers_boolean_and_enum_orders() {
    // Both null position and comma alternatives affect the domain reaching each body.
    for text in [
        "fun extract(x: Boolean?): Boolean = when (x) { null -> false; true -> x; false -> x }",
        "fun extract(x: Boolean?): Boolean = when (x) { true, false -> x; null -> false }",
        "fun extract(x: Boolean?): Boolean = when (x) { true -> x; null -> false; else -> x }",
        "enum class Flag { On, Off }\nfun extract(x: Flag?): Flag = when (x) { null -> Flag.Off; else -> x }",
    ] {
        let (_, _, _, typed) = checked(text);
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
    }
}

#[test]
fn nullable_when_body_mutation_does_not_leak_to_unselected_entry() {
    // The null body's mutation cannot execute on the path selecting else.
    let text = "fun extract(input: Int?, replacement: Int?): Int { var x = input
        return (when (x) { null -> { x = replacement if (true) { 0 } else { 0 } }; else -> x })
    }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn nullable_when_condition_mutation_invalidates_source_proof() {
    // The tested subject is a snapshot; a later assignment changes the source binding.
    let text = "fun extract(input: Boolean?, replacement: Boolean?): Boolean { var x = input
        return (when (x) { null -> false;
            if (true) { x = replacement if (true) { true } else { false } } else { false } -> false;
            else -> x })
    }";
    let (sources, _, _, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("body span"),
        "x"
    );
}

#[test]
fn nullable_when_does_not_smart_cast_repeated_field_read() {
    // Each field read is a fresh source evaluation, not the internal subject identity.
    let text = "class Holder(val item: Int?)\n
        fun extract(h: Holder): Int = when (h.item) { null -> 0; else -> h.item }";
    let (_, _, _, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    assert!(typed.non_null_uses().is_empty());
}

#[test]
fn nullable_when_failed_overload_trials_do_not_publish_plans() {
    // Neither candidate can return a nullable null branch as its non-null result.
    let text = "fun choose(callback: (Int?) -> Int): Int = 1
        fun choose(callback: (Boolean?) -> Boolean): Boolean = true
        fun use(): Unit { val selected = choose { when (it) { null -> it; else -> it } } }";
    let (_, _, _, typed) = checked(text);
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.nullable_whens().is_empty());
}

#[test]
fn nullable_when_plans_have_stable_identity_and_single_subject_evaluation() {
    let text = "fun subject(): Int? = null
        fun extract(): Int = when (subject()) { null -> 0; else -> 1 }";
    let (sources, parsed) = parse(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    let repeated = check_types(&sources, &parsed, &resolution, &types).expect("repeat types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.nullable_whens(), repeated.nullable_whens());
    assert_eq!(typed.nullable_whens().len(), 1);
    let plan = &typed.nullable_whens()[0];
    assert_eq!(
        sources
            .slice(
                parsed
                    .ast()
                    .expressions()
                    .get(plan.subject())
                    .expect("subject")
                    .span()
            )
            .expect("text"),
        "subject()"
    );
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| call.expression() == plan.subject())
            .count(),
        1
    );
    assert_eq!(plan.entries().len(), 2);
}

#[test]
fn nullable_when_subject_categories_bound_native_eligibility() {
    use lang_frontend::type_checking::NullableWhenSubjectCategory as Category;
    for (source, category, native) in [
        (
            "fun read(own x: Node?): Int = when (x) { null -> 0; else -> 1 }",
            Category::OwnedRoot,
            true,
        ),
        (
            "fun read(x: Node?): Int = when (x) { null -> 0; else -> 1 }",
            Category::BorrowRoot,
            false,
        ),
        (
            "fun read(inout x: Node?): Int = when (x) { null -> 0; else -> 1 }",
            Category::InoutRoot,
            false,
        ),
        (
            "fun read(h: Holder): Int = when (h.item) { null -> 0; else -> 1 }",
            Category::OrdinaryField,
            false,
        ),
        (
            "fun read(xs: Array<Node?>): Int = when (xs[0]) { null -> 0; else -> 1 }",
            Category::ContainerElement,
            false,
        ),
        (
            "fun make(): Node? = null\nfun read(): Int = when (make()) { null -> 0; else -> 1 }",
            Category::Temporary,
            true,
        ),
        (
            "fun read(own x: Box<Token>?): Int = when (x) { null -> 0; else -> 1 }",
            Category::OwnedRoot,
            true,
        ),
        (
            "fun read(own x: Rc<Int>?): Int = when (x) { null -> 0; else -> 1 }",
            Category::OwnedRoot,
            true,
        ),
        (
            "fun read(own x: Int?): Int = when (x) { null -> 0; else -> 1 }",
            Category::OwnedRoot,
            false,
        ),
    ] {
        let text = format!(
            "class Node {{}}\nvalue class Token(val item: Int)\nclass Holder(val item: Node?)\n{source}"
        );
        let (_, _, _, typed) = checked(&text);
        assert!(
            typed.diagnostics().is_empty(),
            "{source}: {:?}",
            typed.diagnostics()
        );
        assert_eq!(typed.nullable_whens().len(), 1, "{source}");
        let plan = &typed.nullable_whens()[0];
        assert_eq!(plan.category(), category, "{source}");
        assert_eq!(plan.native_eligible(), native, "{source}");
        if matches!(
            category,
            Category::OrdinaryField | Category::ContainerElement | Category::Temporary
        ) {
            assert_eq!(plan.stable_symbol(), None, "{source}");
        }
    }
}

#[test]
fn nullable_when_domains_encode_null_match_and_non_null_fallthrough() {
    use lang_frontend::type_checking::WhenDomain;
    let (_, _, _, typed) = checked("fun extract(x: Int?): Int = when (x) { null -> 0; else -> x }");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let plan = &typed.nullable_whens()[0];
    let null = &plan.entries()[0];
    assert_eq!(null.input_domain(), &WhenDomain::Nullable);
    assert_eq!(null.alternatives()[0].match_domain(), &WhenDomain::Null);
    assert_eq!(
        null.alternatives()[0].fallthrough_domain(),
        &WhenDomain::NonNull
    );
    assert_eq!(null.remaining_domain(), &WhenDomain::NonNull);
    assert_eq!(null.body_type(), None);
    let otherwise = &plan.entries()[1];
    assert_eq!(otherwise.input_domain(), &WhenDomain::NonNull);
    assert_eq!(otherwise.body_domain(), &WhenDomain::NonNull);
    assert_eq!(otherwise.remaining_domain(), &WhenDomain::Empty);
    assert!(matches!(
        otherwise.body_type().and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
}

#[test]
fn nullable_when_successful_overload_trial_publishes_only_selected_plan() {
    let text = "fun choose(callback: (Int?) -> Int): Int = 1
        fun choose(callback: (Boolean?) -> Boolean): Boolean = true
        fun use(): Unit { val selected = choose { when (it) { null -> 0; else -> it } } }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.nullable_whens().len(), 1);
    let plan = &typed.nullable_whens()[0];
    assert!(matches!(
        plan.entries()[1]
            .body_type()
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
}

#[test]
fn nullable_when_captured_mutable_binding_is_not_a_stable_source() {
    let text = "fun extract(input: Int?): Int { var x = input
        val capture: () -> Int? = { x }
        return (when (x) { null -> 0; else -> x })
    }";
    let (sources, _, _, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("body span"),
        "x"
    );
}

#[test]
fn nullable_when_identity_does_not_interfere_with_later_if_refinement() {
    let text = "fun extract(x: Boolean?): Boolean {
        when (x) { null, true, false -> 0 }
        return (if (x != null) { x } else { false })
    }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn nullable_when_identity_does_not_interfere_with_body_if_refinement() {
    let text = "fun extract(x: Boolean?): Boolean = when (x) {
        null, true, false -> if (x != null) { x } else { false }
    }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn nullable_when_remaining_single_case_keeps_payload_refinement() {
    let text = "enum class Shape { Circle(radius: Int), Point }
        fun extract(x: Shape?): Int = when (x) {
            null -> 0; !is Shape.Point -> x.radius; else -> 0
        }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn nullable_when_inout_condition_call_invalidates_subject_binding() {
    let text = "fun mutate(inout input: Boolean?): Boolean = true
        fun extract(input: Boolean?): Boolean { var x = input
            return (when (x) { mutate(&x) -> false; null -> false; else -> x })
        }";
    let (sources, _, _, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("body span"),
        "x"
    );
}
