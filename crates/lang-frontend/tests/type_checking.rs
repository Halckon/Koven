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

#[path = "type_checking/control_join_regressions.rs"]
mod control_join_regressions;

#[path = "type_checking/resource_unit.rs"]
mod resource_unit;

#[path = "type_checking/non_null_unit.rs"]
mod non_null_unit;

#[path = "type_checking/unit_entries.rs"]
mod unit_entries;

#[path = "type_checking/scalar_unit.rs"]
mod scalar_unit;

#[path = "type_checking/interface_unit.rs"]
mod interface_unit;

#[path = "type_checking/raw_followup.rs"]
mod raw_followup;

#[path = "type_checking/nullable_when_unit.rs"]
mod nullable_when_unit;

#[path = "type_checking/nominal_unit.rs"]
mod nominal_unit;

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
        (
            names.declare_function("replace").expect("replace"),
            IntrinsicCallable::Replace,
        ),
        (
            names.declare_function("swap").expect("swap"),
            IntrinsicCallable::Swap,
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
        assert_eq!(
            files.len(),
            if should_pass { 10 } else { 11 },
            "zero or unexpected {directory} fixtures"
        );
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
fn nullable_when_remaining_single_case_keeps_payload_refinement() {
    let text = "enum class Shape { Circle(radius: Int), Point }
        fun extract(x: Shape?): Int = when (x) {
            null -> 0; !is Shape.Point -> x.radius; else -> 0
        }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn calling_deinit_as_unqualified_name_is_rejected_as_unresolved() {
    let text = "class Resource {\n\
                    deinit() {}\n\
                    fun test(): Unit {\n\
                        deinit()\n\
                    }\n\
                }";
    let (sources, parsed) = parse(text);
    let (names, _) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert_eq!(codes(resolution.diagnostics()), ["L0080"]);
}
