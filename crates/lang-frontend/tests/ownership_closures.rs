//! SPEC-0032 closure capture identity and Transferability fact tests.

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameEnvironment, NameResolution, SymbolId, resolve_names},
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, OwnershipCheckedFile,
        Transferability, check_ownership,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        BuiltinType, Capability, IntrinsicTypeConstructor, TypeEnvironment, TypedFile, check_types,
    },
};

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

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BUILTINS.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        )
    });
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
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, capability) in capabilities {
        types
            .bind_capability(symbol, capability)
            .expect("capability binding");
    }
    for (symbol, constructor) in containers {
        types
            .bind_intrinsic(symbol, constructor)
            .expect("intrinsic binding");
    }
    (names, types)
}

fn checked(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    NameResolution,
    TypedFile,
    OwnershipCheckedFile,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("closures.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "closure source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    (sources, parsed, names, typed, owned)
}

fn symbol(names: &NameResolution, name: &str) -> SymbolId {
    names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .unwrap_or_else(|| panic!("missing symbol {name}"))
        .id()
}

fn lambdas(parsed: &ParsedFile) -> Vec<ExpressionId> {
    let mut lambdas = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some((node.span().start(), id))
        })
        .collect::<Vec<_>>();
    lambdas.sort_by_key(|(start, _)| *start);
    lambdas.into_iter().map(|(_, id)| id).collect()
}

fn ability(
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    name: &str,
) -> Transferability {
    let ty = typed
        .symbol_type(symbol(names, name))
        .unwrap_or_else(|| panic!("missing type for {name}"));
    owned
        .transferability(ty)
        .unwrap_or_else(|| panic!("missing transferability for {name}"))
}

#[test]
fn default_and_move_lambdas_publish_borrow_copy_and_move_facts() {
    let text = "class Resource {}\n\
                fun captures(own resource: Resource, number: Int): Unit {\n\
                    val shared: () -> Unit = {\n\
                        val first = number\n\
                        val second = resource\n\
                        val again = number\n\
                    }\n\
                    val owned: move () -> Unit = move {\n\
                        val first = number\n\
                        val second = resource\n\
                    }\n\
                    val empty: () -> Unit = {}\n\
                }";
    let (_, parsed, names, _, owned) = checked(text);
    let lambdas = lambdas(&parsed);
    assert_eq!(lambdas.len(), 3);

    let resource = symbol(&names, "resource");
    let number = symbol(&names, "number");
    let shared = owned.captures_of(lambdas[0]).copied().collect::<Vec<_>>();
    assert_eq!(shared.len(), 2);
    assert_eq!(
        shared
            .iter()
            .map(|capture| capture.source())
            .collect::<Vec<_>>(),
        [
            ClosureCaptureSource::Symbol(number),
            ClosureCaptureSource::Symbol(resource)
        ]
    );
    assert!(
        shared
            .iter()
            .all(|capture| capture.mode() == ClosureCaptureMode::Shared
                && capture.effect() == ClosureCaptureEffect::Borrow)
    );

    let moved = owned.captures_of(lambdas[1]).copied().collect::<Vec<_>>();
    assert_eq!(moved.len(), 2);
    assert_eq!(moved[0].source(), ClosureCaptureSource::Symbol(number));
    assert_eq!(moved[0].effect(), ClosureCaptureEffect::Copy);
    assert_eq!(moved[1].source(), ClosureCaptureSource::Symbol(resource));
    assert_eq!(moved[1].effect(), ClosureCaptureEffect::Move);
    assert!(
        moved
            .iter()
            .all(|capture| capture.mode() == ClosureCaptureMode::Owned)
    );

    assert_eq!(
        owned.closure(lambdas[0]).expect("shared").transferability(),
        Transferability::NotTransferable
    );
    assert_eq!(
        owned.closure(lambdas[1]).expect("move").transferability(),
        Transferability::Transferable
    );
    assert_eq!(
        owned.closure(lambdas[2]).expect("empty").transferability(),
        Transferability::Transferable
    );
}

#[test]
fn nested_capture_uses_symbol_identity_and_shadowing_does_not_capture() {
    let text = "fun nested(number: Int): Unit {\n\
                    val outer: () -> Unit = {\n\
                        val local = 1\n\
                        val inner: () -> Unit = { val sum = number + local }\n\
                    }\n\
                    val shadow: (Int) -> Unit = { number -> val used = number }\n\
                }";
    let (_, parsed, names, _, owned) = checked(text);
    let lambdas = lambdas(&parsed);
    assert_eq!(lambdas.len(), 3);
    let number = symbol(&names, "number");
    let local = symbol(&names, "local");

    assert_eq!(
        owned
            .captures_of(lambdas[0])
            .map(|capture| capture.source())
            .collect::<Vec<_>>(),
        [ClosureCaptureSource::Symbol(number)]
    );
    assert_eq!(
        owned
            .captures_of(lambdas[1])
            .map(|capture| capture.source())
            .collect::<Vec<_>>(),
        [
            ClosureCaptureSource::Symbol(number),
            ClosureCaptureSource::Symbol(local)
        ]
    );
    assert_eq!(owned.captures_of(lambdas[2]).count(), 0);
}

#[test]
fn unqualified_field_reference_normalizes_to_this_capture() {
    let text = "class Counter(val count: Int) {\n\
                    fun reader(): () -> Unit = { val result = count }\n\
                }";
    let (_, parsed, _, _, owned) = checked(text);
    let lambda = lambdas(&parsed)[0];
    let captures = owned.captures_of(lambda).copied().collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].source(), ClosureCaptureSource::This);
    assert_eq!(captures[0].mode(), ClosureCaptureMode::Shared);
}

#[test]
fn transferability_is_structural_and_independent_from_copyability() {
    let text = "value class Point(val x: Int)\n\
                class Packet(val point: Point)\n\
                class CallbackBox(val callback: () -> Unit)\n\
                enum class Choice { Value(packet: Packet), Empty }\n\
                fun <T : Transferable, U> abilities(\n\
                    packet: Packet, callback: CallbackBox, text: String, maybe: Packet?,\n\
                    packets: List<Packet>, callbacks: List<CallbackBox>, action: () -> Unit,\n\
                    choice: Choice, proven: T, unknown: U, top: Any\n\
                ): Unit {}";
    let (_, _, names, typed, owned) = checked(text);
    for name in ["packet", "text", "maybe", "packets", "choice", "proven"] {
        assert_eq!(
            ability(&names, &typed, &owned, name),
            Transferability::Transferable,
            "{name}"
        );
    }
    for name in ["callback", "callbacks", "action", "unknown", "top"] {
        assert_eq!(
            ability(&names, &typed, &owned, name),
            Transferability::NotTransferable,
            "{name}"
        );
    }
    assert_eq!(
        ability(&names, &typed, &owned, "packet"),
        Transferability::Transferable,
        "ordinary classes can be transferable while remaining MoveOnly"
    );
}
