//! SPEC-0188 construction Value delivery、root obligation 与 ASAP drop 集成测试。

use std::{fs, path::Path};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameEnvironment, resolve_names},
    ownership_checking::{
        ConstructionDeliveryKind, ConstructionRootKind, DropPoint, DropTarget,
        OwnershipCheckedFile, check_ownership,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        BuiltinType, ConstructionTarget, IntrinsicTypeConstructor, TypeEnvironment, check_types,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        )
    });
    let box_symbol = names.declare_type("Box").expect("Box");
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    types
        .bind_intrinsic(box_symbol, IntrinsicTypeConstructor::Box)
        .expect("Box binding");
    (names, types)
}

fn checked(text: &str) -> (SourceMap, ParsedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("construction-ownership.ko", text)
        .expect("source");
    let parsed = parse_file_twice(&sources, source, "construction ownership source");
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
    (sources, parsed, owned)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn construction_plans_publish_source_order_copy_move_temporary_and_root_kinds() {
    let text = "class Resource {}
                value class Point(val x: Int)
                value class Pair(val first: Resource, val second: Resource)
                enum class Event { Item(resource: Resource), Empty }
                enum class Flag { On, Off }
                fun inspect(resource: Resource): Unit {}
                fun plans(number: Int, pointInput: Point, own left: Resource, own right: Resource, own payload: Resource): Unit {
                    val point = Point(number)
                    val pair = Pair(second = right, first = left)
                    val event = Event.Item(payload)
                    val flag = Flag.On
                    val boxed = Box(Point(number))
                    val copiedBox = Box(pointInput)
                    val pointAgain = pointInput
                    val inspected = inspect(Resource())
                }";
    let (sources, parsed, owned) = checked(text);
    let (_, _, repeated) = checked(text);

    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.construction_plans(), repeated.construction_plans());
    assert_eq!(owned.construction_plans().len(), 8);

    let pair = owned
        .construction_plans()
        .iter()
        .find(|plan| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(plan.construction())
                    .unwrap()
                    .span(),
            ) == Ok("Pair(second = right, first = left)")
        })
        .expect("Pair plan");
    assert_eq!(pair.deliveries().len(), 2);
    assert!(
        pair.deliveries()
            .iter()
            .all(|effect| effect.parameter_symbol().is_some())
    );
    assert_eq!(
        pair.deliveries()
            .iter()
            .map(|effect| (
                effect.evaluation_index(),
                effect.parameter_index(),
                effect.kind()
            ))
            .collect::<Vec<_>>(),
        [
            (0, 1, ConstructionDeliveryKind::Move),
            (1, 0, ConstructionDeliveryKind::Move),
        ]
    );
    assert_eq!(
        pair.root_obligation().expect("Pair root").kind(),
        ConstructionRootKind::Inline
    );

    let point = owned
        .construction_plans()
        .iter()
        .find(|plan| {
            plan.deliveries().first().is_some_and(|effect| {
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(effect.argument())
                        .unwrap()
                        .span(),
                ) == Ok("number")
                    && effect.kind() == ConstructionDeliveryKind::Copy
            })
        })
        .expect("Point copy plan");
    assert!(point.root_obligation().is_none());

    let item = owned
        .construction_plans()
        .iter()
        .find(|plan| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(plan.construction())
                    .unwrap()
                    .span(),
            ) == Ok("Event.Item(payload)")
        })
        .expect("enum payload plan");
    assert_eq!(item.deliveries()[0].kind(), ConstructionDeliveryKind::Move);
    assert_eq!(
        item.root_obligation().expect("enum root").kind(),
        ConstructionRootKind::Inline
    );

    let flag = owned
        .construction_plans()
        .iter()
        .find(|plan| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(plan.construction())
                    .unwrap()
                    .span(),
            ) == Ok("Flag.On")
        })
        .expect("bare enum plan");
    assert!(flag.deliveries().is_empty());
    assert!(flag.root_obligation().is_none());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());

    let copied_box = owned
        .construction_plans()
        .iter()
        .find(|plan| {
            plan.target() == ConstructionTarget::IntrinsicBox
                && plan
                    .deliveries()
                    .first()
                    .is_some_and(|effect| effect.kind() == ConstructionDeliveryKind::Copy)
        })
        .expect("Box place-copy plan");
    assert_eq!(
        copied_box.root_obligation().expect("Box root").kind(),
        ConstructionRootKind::HeapOwner
    );
    assert!(owned.drops().iter().any(|fact| {
        matches!(fact.point(), DropPoint::CallReturn(_))
            && matches!(fact.target(), DropTarget::Temporary(_))
    }));
}

#[test]
fn construction_roots_transfer_or_drop_once_across_local_branch_loop_and_return_paths() {
    let text = "class Resource {}
                value class Holder(val resource: Resource)
                fun paths(flag: Boolean): Resource {
                    val resource = Resource()
                    val holder = Holder(resource)
                    if (flag) {
                        val branch = Resource()
                    } else {
                        val branch = Resource()
                    }
                    while (flag) {
                        val loopOwner = Resource()
                        break
                    }
                    return Resource()
                }";
    let (sources, parsed, owned) = checked(text);

    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.construction_plans().len(), 6);
    assert!(owned.construction_plans().iter().all(|plan| {
        plan.root_obligation().is_some_and(|root| {
            root.kind() == ConstructionRootKind::HeapOwner
                || root.kind() == ConstructionRootKind::Inline
        })
    }));

    let named_origins = owned
        .drops()
        .iter()
        .filter_map(|fact| match fact.target() {
            DropTarget::Named(_) => Some(sources.slice(fact.value_origin()).unwrap()),
            DropTarget::Temporary(_)
            | DropTarget::ReplacedElement(_)
            | DropTarget::Captured { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        named_origins
            .iter()
            .filter(|&&origin| origin == "resource")
            .count(),
        0,
        "the resource owner moved into Holder"
    );
    assert_eq!(
        named_origins
            .iter()
            .filter(|&&origin| origin == "holder")
            .count(),
        1
    );
    assert_eq!(
        named_origins
            .iter()
            .filter(|&&origin| origin == "branch")
            .count(),
        2
    );
    assert_eq!(
        named_origins
            .iter()
            .filter(|&&origin| origin == "loopOwner")
            .count(),
        1
    );

    let returned = owned
        .construction_plans()
        .iter()
        .find(|plan| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(plan.construction())
                    .unwrap()
                    .span(),
            ) == Ok("Resource()")
                && plan.construction().index()
                    == owned
                        .construction_plans()
                        .iter()
                        .map(|candidate| candidate.construction().index())
                        .max()
                        .unwrap()
        })
        .expect("returned Resource");
    assert!(
        !owned
            .drops()
            .iter()
            .any(|fact| { fact.target() == DropTarget::Temporary(returned.construction()) })
    );
}

#[test]
fn construction_moves_follow_evaluation_order_and_reuse_move_diagnostics_atomically() {
    let text = "class Resource {}
                value class Pair(val first: Resource, val second: Resource)
                fun duplicate(own item: Resource): Unit {
                    val pair = Pair(second = item, first = item)
                    val after = item
                }";
    let (sources, _, owned) = checked(text);

    assert_eq!(codes(owned.diagnostics()), ["L0131", "L0131"]);
    assert_eq!(
        owned
            .diagnostics()
            .iter()
            .map(|diagnostic| sources.slice(diagnostic.primary_span()).unwrap())
            .collect::<Vec<_>>(),
        ["item", "item"]
    );
    for diagnostic in owned.diagnostics() {
        assert!(diagnostic.details().iter().any(|detail| matches!(
            detail,
            DiagnosticDetail::Label(label)
                if sources.slice(label.span()) == Ok("item")
        )));
    }
    assert!(owned.construction_plans().is_empty());
    assert!(owned.drops().is_empty());

    let boxed = "class Resource {}
                 value class OwnedPoint(val resource: Resource)
                 fun invalid(own point: OwnedPoint): Unit {
                     val boxed = Box(point)
                     val reused = point
                 }";
    let (_, _, boxed) = checked(boxed);
    assert_eq!(codes(boxed.diagnostics()), ["L0131"]);
    assert!(boxed.construction_plans().is_empty());
}

#[test]
fn construction_rejects_non_owning_moves_and_active_loans_with_existing_codes() {
    let borrowed = "class Resource {}
                    value class Holder(val item: Resource)
                    fun invalid(shared: Resource, inout exclusive: Resource): Unit {
                        val first = Holder(shared)
                        val second = Holder(exclusive)
                    }";
    let (_, _, borrowed) = checked(borrowed);
    assert_eq!(codes(borrowed.diagnostics()), ["L0133", "L0133"]);
    assert!(borrowed.construction_plans().is_empty());

    let conflict = "class Resource {}
                    value class Holder(val item: Resource)
                    fun use(shared: Resource, own holder: Holder): Unit {}
                    fun invalid(own input: Resource): Unit {
                        val result = use(input, Holder(input))
                    }";
    let (_, _, conflict) = checked(conflict);
    assert_eq!(codes(conflict.diagnostics()), ["L0135"]);
    assert!(conflict.construction_plans().is_empty());
}

#[test]
fn nothing_operand_stops_later_delivery_and_never_establishes_a_root() {
    let text = "class Resource {}
                class Triple(val first: Resource, val second: Resource, val third: Resource)
                fun stop(): Nothing
                fun terminate(own first: Resource, own last: Resource): Unit {
                    val result = Triple(first, stop(), last)
                    val unreachable = last
                }";
    let (sources, parsed, owned) = checked(text);

    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let triple = owned
        .construction_plans()
        .iter()
        .find(|plan| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(plan.construction())
                    .unwrap()
                    .span(),
            ) == Ok("Triple(first, stop(), last)")
        })
        .expect("terminating construction plan");
    assert_eq!(triple.deliveries().len(), 1);
    assert_eq!(
        triple.deliveries()[0].kind(),
        ConstructionDeliveryKind::Move
    );
    assert!(triple.root_obligation().is_none());
    assert!(triple.terminating_operand().is_some());
    assert_ne!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(triple.terminating_operand().unwrap())
                .unwrap()
                .span()
        ),
        Ok("last")
    );
}

#[test]
fn checked_in_phase3_construction_fixtures_execute_pass_and_fail_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase3");
    for (directory, should_pass) in [("construction-pass", true), ("construction-fail", false)] {
        let files = fs::read_dir(root.join(directory))
            .expect("construction fixture directory")
            .map(|entry| entry.expect("construction fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1, "zero or unexpected {directory} fixtures");
        for path in files {
            let text = fs::read_to_string(&path).expect("UTF-8 construction fixture");
            let (_, _, checked) = checked(&text);
            if should_pass {
                assert!(checked.diagnostics().is_empty(), "{path:?}");
                assert!(!checked.construction_plans().is_empty(), "{path:?}");
            } else {
                let expected =
                    fs::read_to_string(path.with_extension("diag")).expect("diagnostic sidecar");
                let actual = checked
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
                assert!(checked.construction_plans().is_empty(), "{path:?}");
            }
        }
    }
}
