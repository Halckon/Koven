//! SPEC-0030 顺序容器构造、element place、loan 与 replacement 所有权测试。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameEnvironment, resolve_names},
    ownership_checking::{
        DropPoint, DropTarget, ElementIndexIdentity, LoanKind, LoanTarget, OwnershipDeferredReason,
        check_ownership,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        BuiltinType, IntrinsicCallable, IntrinsicTypeConstructor, TypeEnvironment, check_types,
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
    let containers = [
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
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
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

fn checked(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    lang_frontend::ownership_checking::OwnershipCheckedFile,
) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("ownership-containers.ko", text)
        .expect("source");
    let parsed = parse_file_twice(&sources, source, "container ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &resolution, &typed).expect("ownership");
    (sources, parsed, owned)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn list_form_construction_moves_only_move_only_elements() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun build(own endpoint: Resource, number: Int): Unit {\n\
                    val resources = listOf(endpoint)\n\
                    val invalid = take(endpoint)\n\
                    val numbers = listOf(number)\n\
                    val copied = number\n\
                }";
    let (sources, _, owned) = checked(text);
    assert_eq!(codes(owned.diagnostics()), ["L0131"]);
    assert_eq!(
        sources
            .slice(owned.diagnostics()[0].primary_span())
            .unwrap(),
        "endpoint"
    );
}

#[test]
fn runtime_length_construction_reuses_its_two_borrow_contracts() {
    let text = "class Resource {}\n\
                fun build(size: Int, initializer: (Int) -> Resource): Unit {\n\
                    val list = List<Resource>(size, initializer)\n\
                }";
    let (_, _, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.loans().len(), 2);
    assert!(
        owned
            .loans()
            .iter()
            .all(|loan| loan.kind() == LoanKind::Shared)
    );
}

#[test]
fn element_owned_read_copies_copyable_and_rejects_move_only() {
    let passing = "fun takeInt(own item: Int): Unit {}\n\
                   fun copy(own list: List<Int>): Unit {\n\
                       val first = list[0]\n\
                       val second = takeInt(list[1])\n\
                       val third = list[0]\n\
                   }";
    let (_, _, passing) = checked(passing);
    assert!(
        passing.diagnostics().is_empty(),
        "{:?}",
        passing.diagnostics()
    );

    let failing = "class Resource {}\n\
                   fun take(own item: Resource): Unit {}\n\
                   fun invalid(own list: List<Resource>): Unit {\n\
                       val first = list[0]\n\
                       val second = take(list[1])\n\
                   }";
    let (sources, _, failing) = checked(failing);
    assert_eq!(codes(failing.diagnostics()), ["L0136", "L0136"]);
    for diagnostic in failing.diagnostics() {
        assert!(
            sources
                .slice(diagnostic.primary_span())
                .unwrap()
                .starts_with("list[")
        );
        assert!(
            diagnostic
                .details()
                .iter()
                .any(|detail| matches!(detail, DiagnosticDetail::Label(_)))
        );
    }
}

#[test]
fn element_loans_publish_logical_indices_and_allow_proven_siblings() {
    let text = "class Resource {}\n\
                fun inspect(first: Resource, second: Resource): Unit {}\n\
                fun mutate(inout first: Resource, inout second: Resource): Unit {}\n\
                fun valid(own list: MutableList<Resource>): Unit {\n\
                    val shared = inspect(list[0], list[0])\n\
                    val exclusive = mutate(&list[0], &list[1])\n\
                }";
    let (_, _, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.loans().len(), 4);
    assert_eq!(
        owned
            .loans()
            .iter()
            .map(|loan| loan.kind())
            .collect::<Vec<_>>(),
        [
            LoanKind::Shared,
            LoanKind::Shared,
            LoanKind::Exclusive,
            LoanKind::Exclusive,
        ]
    );
    let indices = owned
        .loans()
        .iter()
        .map(|loan| match loan.target() {
            LoanTarget::Place(place) => place.element().expect("element identity"),
            LoanTarget::Temporary(_) => panic!("named list must publish a stable place"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        indices,
        [
            ElementIndexIdentity::Known(0),
            ElementIndexIdentity::Known(0),
            ElementIndexIdentity::Known(0),
            ElementIndexIdentity::Known(1),
        ]
    );
    let root = match owned.loans()[0].target() {
        LoanTarget::Place(place) => place.root(),
        LoanTarget::Temporary(_) => unreachable!(),
    };
    assert!(owned.drops().iter().any(|fact| {
        fact.target() == DropTarget::Named(root) && matches!(fact.point(), DropPoint::CallReturn(_))
    }));
}

#[test]
fn borrow_and_inout_cover_each_intrinsic_container_capability() {
    let text = "class Resource {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun valid(own array: Array<Resource>, own list: List<Resource>, own mutable: MutableList<Resource>): Unit {\n\
                    val arrayRead = inspect(array[0])\n\
                    val listRead = inspect(list[0])\n\
                    val mutableRead = inspect(mutable[0])\n\
                    val arrayWrite = mutate(&array[0])\n\
                    val mutableWrite = mutate(&mutable[0])\n\
                }";
    let (_, _, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.loans().len(), 5);
    assert_eq!(
        owned
            .loans()
            .iter()
            .map(|loan| loan.kind())
            .collect::<Vec<_>>(),
        [
            LoanKind::Shared,
            LoanKind::Shared,
            LoanKind::Shared,
            LoanKind::Exclusive,
            LoanKind::Exclusive,
        ]
    );
}

#[test]
fn same_and_unknown_element_indices_conflict_but_distinct_constants_do_not() {
    for text in [
        "class Resource {}\nfun mutate(inout first: Resource, inout second: Resource): Unit {}\nfun invalid(own list: MutableList<Resource>): Unit { val result = mutate(&list[0], &list[0]) }",
        "class Resource {}\nfun mutate(inout first: Resource, inout second: Resource): Unit {}\nfun invalid(own list: MutableList<Resource>, first: Int, second: Int): Unit { val result = mutate(&list[first], &list[second]) }",
    ] {
        let (_, _, owned) = checked(text);
        assert_eq!(codes(owned.diagnostics()), ["L0135"]);
        assert!(
            owned.loans().is_empty(),
            "invalid plan must not be published"
        );
    }
}

#[test]
fn field_container_paths_preserve_parent_element_overlap() {
    let passing = "class Resource {}\n\
                   class Holder(var items: MutableList<Resource>)\n\
                   fun mutate(inout item: Resource): Unit {}\n\
                   fun valid(own holder: Holder): Unit {\n\
                       val result = mutate(&holder.items[0])\n\
                   }";
    let (_, _, passing) = checked(passing);
    assert!(
        passing.diagnostics().is_empty(),
        "{:?}",
        passing.diagnostics()
    );
    let LoanTarget::Place(place) = passing.loans()[0].target() else {
        panic!("field-backed element must have a stable place");
    };
    assert_eq!(place.fields().len(), 1);
    assert_eq!(place.element(), Some(ElementIndexIdentity::Known(0)));

    let failing = "class Resource {}\n\
                   class Holder(var items: MutableList<Resource>)\n\
                   fun mutate(inout item: Resource): Unit {}\n\
                   fun outer(items: MutableList<Resource>, own effect: Unit): Unit {}\n\
                   fun invalid(own holder: Holder): Unit {\n\
                       val result = outer(holder.items, mutate(&holder.items[0]))\n\
                   }";
    let (_, _, failing) = checked(failing);
    assert_eq!(codes(failing.diagnostics()), ["L0135"]);
}

#[test]
fn active_element_loan_blocks_owner_move_and_overlapping_replacement() {
    let moving = "class Resource {}\n\
                  fun takeList(own item: MutableList<Resource>): Unit {}\n\
                  fun outer(first: Resource, own second: Unit): Unit {}\n\
                  fun invalid(own list: MutableList<Resource>): Unit {\n\
                      val result = outer(list[0], takeList(list))\n\
                  }";
    let (_, _, moving) = checked(moving);
    assert_eq!(codes(moving.diagnostics()), ["L0135"]);

    let replacement = "class Resource {}\n\
                       fun outer(first: Resource, own second: Unit): Unit {}\n\
                       fun invalid(own list: MutableList<Resource>, own replacement: Resource): Unit {\n\
                           val result = outer(list[0], (list[0] = replacement))\n\
                       }";
    let (_, _, replacement) = checked(replacement);
    assert_eq!(codes(replacement.diagnostics()), ["L0135"]);

    let sibling = "class Resource {}\n\
                   fun outer(first: Resource, own second: Unit): Unit {}\n\
                   fun valid(own list: MutableList<Resource>, own replacement: Resource): Unit {\n\
                       val result = outer(list[0], (list[1] = replacement))\n\
                   }";
    let (_, _, sibling) = checked(sibling);
    assert!(
        sibling.diagnostics().is_empty(),
        "{:?}",
        sibling.diagnostics()
    );
}

#[test]
fn replacement_rejects_moved_owner_and_drops_the_old_element_after_commit() {
    let failing = "class Resource {}\n\
                   fun consumeAndCreate(own list: MutableList<Resource>): Resource\n\
                   fun invalid(own list: MutableList<Resource>): Unit {\n\
                       val result = (list[0] = consumeAndCreate(list))\n\
                   }";
    let (sources, _, failing) = checked(failing);
    assert_eq!(codes(failing.diagnostics()), ["L0131"]);
    assert_eq!(
        sources
            .slice(failing.diagnostics()[0].primary_span())
            .unwrap(),
        "list[0]"
    );

    let passing = "class Resource {}\n\
                   fun valid(own list: MutableList<Resource>, own replacement: Resource): Unit {\n\
                       val result = (list[0] = replacement)\n\
                   }";
    let (_, _, passing) = checked(passing);
    assert!(
        passing.diagnostics().is_empty(),
        "{:?}",
        passing.diagnostics()
    );
    assert!(passing.drops().iter().any(|fact| {
        matches!(fact.point(), DropPoint::AfterReplacement(_))
            && matches!(fact.target(), DropTarget::ReplacedElement(_))
    }));
}

#[test]
fn already_moved_container_reports_only_the_root_cause_for_element_access() {
    let text = "class Resource {}\n\
                fun takeList(own item: List<Resource>): Unit {}\n\
                fun invalid(own list: List<Resource>): Unit {\n\
                    val moved = takeList(list)\n\
                    val element = list[0]\n\
                }";
    let (_, _, owned) = checked(text);
    assert_eq!(codes(owned.diagnostics()), ["L0131"]);
}

#[test]
fn temporary_container_lifetime_matches_element_access_mode() {
    let text = "class Resource {}\n\
                fun makeNumbers(): List<Int>\n\
                fun makeResources(): MutableList<Resource>\n\
                fun inspect(item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun temporary(): Unit {\n\
                    val copied = makeNumbers()[0]\n\
                    val borrowed = inspect(makeResources()[0])\n\
                    val exclusive = mutate(&makeResources()[0])\n\
                }";
    let (_, _, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.loans().iter().any(|loan| {
        matches!(loan.target(), LoanTarget::Temporary(_)) && loan.kind() == LoanKind::Shared
    }));
    assert!(owned.loans().iter().any(|loan| {
        matches!(loan.target(), LoanTarget::Temporary(_)) && loan.kind() == LoanKind::Exclusive
    }));
    assert!(owned.drops().iter().any(|fact| {
        matches!(fact.point(), DropPoint::AfterExpression(_))
            && matches!(fact.target(), DropTarget::Temporary(_))
    }));
    assert!(owned.drops().iter().any(|fact| {
        matches!(fact.point(), DropPoint::CallReturn(_))
            && matches!(fact.target(), DropTarget::Temporary(_))
    }));
    assert!(
        !owned
            .deferred()
            .iter()
            .any(|fact| fact.reason() == OwnershipDeferredReason::IndexPlace)
    );

    let (_, _, repeated) = checked(text);
    assert_eq!(
        format!("{:?}", owned.loans()),
        format!("{:?}", repeated.loans())
    );
    assert_eq!(
        format!("{:?}", owned.drops()),
        format!("{:?}", repeated.drops())
    );
}

#[test]
fn post_index_field_projection_stays_deferred_without_an_early_drop_plan() {
    let text = "class Resource {}\n\
                class Holder(var payload: Resource)\n\
                fun deferred(own list: List<Holder>): Unit {\n\
                    val projected = list[0].payload\n\
                }";
    let (_, _, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(
        owned
            .deferred()
            .iter()
            .any(|fact| fact.reason() == OwnershipDeferredReason::IndexPlace)
    );
    assert!(owned.drops().is_empty());
}
