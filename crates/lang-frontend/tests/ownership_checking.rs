//! SPEC-0027 所有权阶段边界与变量状态测试。

use std::{fs, path::Path};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameEnvironment, resolve_names},
    ownership_checking::{
        DropPoint, DropTarget, LoanKind, LoanTarget, OwnershipBindingKind, OwnershipCheckedFile,
        OwnershipCheckingError, OwnershipDeferredReason, check_ownership,
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
    let declarations = BUILTINS.map(|builtin| {
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
    for (symbol, builtin) in declarations {
        types.bind_builtin(symbol, builtin).expect("binding");
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

fn checked(text: &str) -> (SourceMap, ParsedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("ownership.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).ok()
            ))
            .collect::<Vec<_>>()
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
fn ownership_stage_preserves_source_identity_and_rejects_mismatched_inputs() {
    let mut sources = SourceMap::new();
    let first_source = sources
        .add_source("first.ko", "val first = 1")
        .expect("first");
    let second_source = sources
        .add_source("second.ko", "val second = 2")
        .expect("second");
    let first = parse_file_twice(&sources, first_source, "first ownership source");
    let second = parse_file_twice(&sources, second_source, "second ownership source");
    let (environment, types) = environments();
    let first_names = resolve_names(&sources, &first, &environment).expect("first names");
    let second_names = resolve_names(&sources, &second, &environment).expect("second names");
    let first_typed = check_types(&sources, &first, &first_names, &types).expect("first types");
    let second_typed = check_types(&sources, &second, &second_names, &types).expect("second types");
    let (foreign_environment, foreign_types) = environments();
    let foreign_names =
        resolve_names(&sources, &first, &foreign_environment).expect("foreign names");
    let foreign_typed =
        check_types(&sources, &first, &foreign_names, &foreign_types).expect("foreign types");
    let repeated_names = resolve_names(&sources, &first, &environment).expect("repeated names");
    let repeated_names_typed =
        check_types(&sources, &first, &repeated_names, &types).expect("repeated names types");
    let repeated_typed =
        check_types(&sources, &first, &first_names, &types).expect("repeated typed analysis");

    let checked = check_ownership(&sources, &first, &first_names, &first_typed).expect("ownership");
    assert_eq!(checked.source_id(), first_source);
    assert!(checked.diagnostics().is_empty());
    assert!(first_typed.is_compatible_with_names(&first_names));
    assert!(checked.is_compatible_with(&first_names, &first_typed));
    assert!(!foreign_typed.is_compatible_with_names(&first_names));
    assert!(!repeated_names_typed.is_compatible_with_names(&first_names));
    assert!(!checked.is_compatible_with(&first_names, &foreign_typed));
    assert!(repeated_typed.is_compatible_with_names(&first_names));
    assert!(!checked.is_compatible_with(&first_names, &repeated_typed));
    assert!(matches!(
        check_ownership(&sources, &first, &second_names, &first_typed),
        Err(OwnershipCheckingError::MismatchedNameSource)
    ));
    assert!(matches!(
        check_ownership(&sources, &first, &first_names, &second_typed),
        Err(OwnershipCheckingError::MismatchedTypedSource)
    ));
    assert!(matches!(
        check_ownership(&sources, &first, &first_names, &foreign_typed),
        Err(OwnershipCheckingError::MismatchedAnalysisIdentity)
    ));
    assert!(matches!(
        check_ownership(&sources, &first, &first_names, &repeated_names_typed),
        Err(OwnershipCheckingError::MismatchedAnalysisIdentity)
    ));
    assert!(matches!(
        check_ownership(&SourceMap::new(), &first, &first_names, &first_typed),
        Err(OwnershipCheckingError::Source(_))
    ));
}

#[test]
fn value_delivery_moves_only_once_while_copy_and_borrow_preserve_sources() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun create(): Resource\n\
                fun inspect(item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun takeNumber(own item: Int): Unit {}\n\
                fun local(own input: Resource): Unit {\n\
                    val moved = input\n\
                    val after = take(input)\n\
                }\n\
                fun calls(own input: Resource): Unit {\n\
                    val first = take(input)\n\
                    val second = take(input)\n\
                    val third = take(input)\n\
                }\n\
                fun copy(number: Int): Unit {\n\
                    val first = number\n\
                    val second = number\n\
                    val firstCall = takeNumber(number)\n\
                    val secondCall = takeNumber(number)\n\
                }\n\
                fun temporary(): Unit {\n\
                    val first = take(create())\n\
                    val second = take(create())\n\
                }\n\
                fun borrows(own input: Resource): Unit {\n\
                    var local = input\n\
                    val first = inspect(local)\n\
                    val second = inspect(borrow local)\n\
                    val third = mutate(&local)\n\
                    val fourth = take(local)\n\
                    val fifth = take(local)\n\
                }\n\
                fun reset(own input: Resource, own replacement: Resource): Unit {\n\
                    var local = input\n\
                    val first = take(local)\n\
                    { local = replacement }\n\
                    val second = take(local)\n\
                    val third = take(local)\n\
                    { local = local }\n\
                    val fourth = take(local)\n\
                }";
    let (sources, _, checked) = checked(text);
    assert_eq!(codes(checked.diagnostics()), vec!["L0131"; 7]);
    let expected_primaries = [
        "input", "input", "input", "local", "local", "local", "local",
    ];
    for (diagnostic, expected) in checked.diagnostics().iter().zip(expected_primaries) {
        assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), expected);
        let label = diagnostic
            .details()
            .iter()
            .find_map(|detail| match detail {
                DiagnosticDetail::Label(label) => Some(label.span()),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .expect("move label");
        assert_eq!(sources.slice(label).unwrap(), expected);
    }
}

#[test]
fn error_nodes_do_not_create_ownership_cascades() {
    let text = "class Resource {}\n\
                fun broken(input: Resource): Unit {\n\
                    val missing =\n\
                }";
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("ownership-error.ko", text)
        .expect("source");
    let parsed = parse_file_twice(&sources, source, "ownership error source");
    assert!(!parsed.diagnostics().is_empty());
    let (environment, types) = environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let checked = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(checked.diagnostics().is_empty());
}

#[test]
fn control_flow_joins_possible_moves_but_ignores_returning_paths() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun branch(flag: Boolean, own input: Resource): Unit {\n\
                    if (flag) { take(input) }\n\
                    take(input)\n\
                }\n\
                fun choose(number: Int, own input: Resource): Unit {\n\
                    when (number) {\n\
                        0 -> take(input)\n\
                        else -> {}\n\
                    }\n\
                    take(input)\n\
                }\n\
                fun looping(flag: Boolean, own input: Resource): Unit {\n\
                    while (flag) {\n\
                        take(input)\n\
                        break\n\
                    }\n\
                    take(input)\n\
                }\n\
                fun terminating(flag: Boolean, own input: Resource): Resource {\n\
                    if (flag) { return input }\n\
                    return input\n\
                }";
    let (sources, _, checked) = checked(text);
    assert_eq!(codes(checked.diagnostics()), vec!["L0131"; 3]);
    for diagnostic in checked.diagnostics() {
        assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "input");
    }
}

#[test]
fn shadowed_symbols_keep_independent_move_origins() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun shadow(own input: Resource): Unit {\n\
                    {\n\
                        val input = input\n\
                        val first = take(input)\n\
                        val second = take(input)\n\
                    }\n\
                    val outer = take(input)\n\
                }";
    let (sources, _, checked) = checked(text);
    assert_eq!(codes(checked.diagnostics()), ["L0131", "L0131"]);

    let outer_origin = text.find("val input = input").unwrap() + "val input = ".len();
    let inner_origin = text.find("val first = take(input)").unwrap() + "val first = take(".len();
    let inner_use = text.find("val second = take(input)").unwrap() + "val second = take(".len();
    let outer_use = text.find("val outer = take(input)").unwrap() + "val outer = take(".len();
    for (diagnostic, primary, origin) in [
        (&checked.diagnostics()[0], inner_use, inner_origin),
        (&checked.diagnostics()[1], outer_use, outer_origin),
    ] {
        assert_eq!(
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ),
            (primary, primary + "input".len())
        );
        let label = diagnostic
            .details()
            .iter()
            .find_map(|detail| match detail {
                DiagnosticDetail::Label(label) => Some(label.span()),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .expect("move label");
        assert_eq!(
            (label.start(), label.end()),
            (origin, origin + "input".len())
        );
        assert_eq!(sources.slice(label).unwrap(), "input");
    }
}

#[test]
fn parameter_bindings_and_successful_call_loans_are_queryable() {
    let text = "class Resource {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun exercise(own owned: Resource, shared: Resource, inout exclusive: Resource): Unit {\n\
                    val first = inspect(owned)\n\
                    val second = inspect(borrow shared)\n\
                    val third = mutate(&exclusive)\n\
                }";
    let (_, parsed, checked) = checked(text);
    assert!(checked.diagnostics().is_empty());

    let parameter_kinds = checked
        .bindings()
        .iter()
        .map(|binding| binding.kind())
        .collect::<Vec<_>>();
    assert_eq!(
        parameter_kinds,
        [
            OwnershipBindingKind::Shared,
            OwnershipBindingKind::Exclusive,
            OwnershipBindingKind::Owned,
            OwnershipBindingKind::Shared,
            OwnershipBindingKind::Exclusive,
        ]
    );
    assert_eq!(checked.loans().len(), 3);
    assert_eq!(
        checked
            .loans()
            .iter()
            .map(|loan| loan.kind())
            .collect::<Vec<_>>(),
        [LoanKind::Shared, LoanKind::Shared, LoanKind::Exclusive]
    );
    assert!(
        checked
            .loans()
            .iter()
            .all(|loan| matches!(loan.target(), LoanTarget::Place(_)))
    );
    for loan in checked.loans() {
        assert_eq!(checked.loan_begin(loan.argument()), Some(loan));
        assert_eq!(checked.loans_ending_at(loan.call()).count(), 1);
        assert_eq!(
            parsed.ast().expressions().get(loan.call()).unwrap().span(),
            loan.end_span()
        );
    }
}

#[test]
fn call_loans_apply_in_source_order_and_use_field_path_overlap() {
    let passing = "class Resource {}\n\
                   class Pair(var left: Resource, var right: Resource)\n\
                   fun readBoth(left: Resource, right: Resource): Unit {}\n\
                   fun mutateBoth(inout left: Resource, inout right: Resource): Unit {}\n\
                   fun valid(own pair: Pair): Unit {\n\
                       val first = readBoth(pair.left, pair.left)\n\
                       val second = mutateBoth(&pair.left, &pair.right)\n\
                   }";
    let (_, _, passing_checked) = checked(passing);
    assert!(
        passing_checked.diagnostics().is_empty(),
        "{:?}",
        passing_checked.diagnostics()
    );

    for failing in [
        "class Resource {}\nclass Pair(var left: Resource, var right: Resource)\nfun conflict(inout first: Resource, inout second: Resource): Unit {}\nfun bad(own pair: Pair): Unit { val result = conflict(&pair.left, &pair.left) }",
        "class Resource {}\nfun mixed(inout first: Resource, second: Resource): Unit {}\nfun bad(own input: Resource): Unit { var local = input\n val result = mixed(&local, local) }",
        "class Resource {}\nfun mixed(first: Resource, inout second: Resource): Unit {}\nfun bad(own input: Resource): Unit { var local = input\n val result = mixed(local, &local) }",
        "class Resource {}\nfun take(own input: Resource): Unit {}\nfun outer(first: Resource, own second: Unit): Unit {}\nfun bad(own input: Resource): Unit { var local = input\n val result = outer(local, take(local)) }",
    ] {
        let (_, _, checked) = checked(failing);
        assert_eq!(codes(checked.diagnostics()), ["L0135"]);
        assert!(
            checked.loans().is_empty(),
            "invalid plan must not be published"
        );
    }
}

#[test]
fn non_owning_move_and_immutable_inout_have_dedicated_diagnostics() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun borrowed(input: Resource): Unit { val result = take(input) }\n\
                fun exclusive(inout input: Resource): Resource = input\n\
                fun immutable(own input: Resource): Unit {\n\
                    val local = input\n\
                    val result = mutate(&local)\n\
                }\n\
                fun ownedParameter(own input: Resource): Unit { val result = mutate(&input) }\n\
                fun movedInout(own input: Resource): Unit {\n\
                    val moved = take(input)\n\
                    val invalid = mutate(&input)\n\
                }";
    let (sources, _, checked) = checked(text);
    assert_eq!(
        codes(checked.diagnostics()),
        ["L0133", "L0133", "L0134", "L0134", "L0131"]
    );
    for diagnostic in checked.diagnostics() {
        assert!(
            matches!(
                sources.slice(diagnostic.primary_span()).unwrap(),
                "input" | "&"
            ),
            "{:?}",
            diagnostic
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
fn asap_drop_facts_cover_last_use_temporary_replacement_and_control_edges() {
    let text = "class Resource {}\n\
                fun create(): Resource\n\
                fun inspect(item: Resource): Unit {}\n\
                fun drops(flag: Boolean, own unusedParameter: Resource, own branchOwner: Resource): Unit {\n\
                    val unused = create()\n\
                    val used = create()\n\
                    val first = inspect(used)\n\
                    var replaced = create()\n\
                    { replaced = create() }\n\
                    val temporary = inspect(create())\n\
                    if (flag) {\n\
                        val branchRead = inspect(branchOwner)\n\
                        val early = create()\n\
                        if (flag) { return }\n\
                        val after = inspect(early)\n\
                    } else {\n\
                        val branch = create()\n\
                    }\n\
                    while (flag) {\n\
                        val loopRead = inspect(replaced)\n\
                        break\n\
                    }\n\
                }";
    let (sources, _, checked) = checked(text);
    assert!(
        checked.diagnostics().is_empty(),
        "{:?}",
        checked.diagnostics()
    );

    let named_origins = checked
        .drops()
        .iter()
        .filter_map(|fact| match fact.target() {
            DropTarget::Named(_) => Some(sources.slice(fact.value_origin()).unwrap()),
            DropTarget::Temporary(_)
            | DropTarget::ReplacedElement(_)
            | DropTarget::Captured { .. } => None,
        })
        .collect::<Vec<_>>();
    for expected in [
        "unusedParameter",
        "unused",
        "used",
        "replaced",
        "early",
        "branch",
    ] {
        assert!(
            named_origins.contains(&expected),
            "missing {expected}: {named_origins:?}"
        );
    }
    assert!(
        checked
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), DropPoint::FunctionEntry(_)))
    );
    assert!(
        checked
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), DropPoint::CallReturn(_))
                && matches!(
                    fact.target(),
                    DropTarget::Named(_) | DropTarget::Temporary(_)
                ))
    );
    assert!(
        checked
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), DropPoint::ControlTransfer(_)))
    );
    assert!(
        checked
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), DropPoint::BranchExit { .. }))
    );
    assert!(
        checked
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), DropPoint::LoopExit(_))),
        "{:?}",
        checked.drops()
    );
    assert!(
        checked
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), DropTarget::Temporary(_)))
    );
}

#[test]
fn string_binary_views_drop_operands_only_after_the_operation() {
    let text = "fun compare(own left: String, own right: String): Boolean {\n\
                    val joined = left + \"!\"\n\
                    return joined == right\n\
                }";
    let (sources, parsed, checked) = checked(text);
    assert!(
        checked.diagnostics().is_empty(),
        "{:?}",
        checked.diagnostics()
    );

    let binaries = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(
                node.payload(),
                lang_frontend::parser::Expression::Binary { .. }
            )
            .then_some((sources.slice(node.span()).expect("binary span"), id))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let concat = binaries["left + \"!\""];
    let equal = binaries["joined == right"];
    let completion = |binary| {
        checked
            .drops()
            .iter()
            .filter(|fact| fact.point() == DropPoint::AfterBinaryOperands(binary))
            .map(|fact| {
                (
                    fact.target(),
                    sources.slice(fact.value_origin()).expect("drop origin"),
                )
            })
            .collect::<Vec<_>>()
    };

    let concat_drops = completion(concat);
    assert_eq!(concat_drops.len(), 2, "{concat_drops:?}");
    assert!(matches!(concat_drops[0].0, DropTarget::Temporary(_)));
    assert_eq!(concat_drops[0].1, "\"!\"");
    assert!(matches!(concat_drops[1].0, DropTarget::Named(_)));
    assert_eq!(concat_drops[1].1, "left");

    let equality_drops = completion(equal);
    assert_eq!(equality_drops.len(), 2, "{equality_drops:?}");
    assert!(
        equality_drops
            .iter()
            .all(|(target, _)| matches!(target, DropTarget::Named(_)))
    );
    assert_eq!(
        equality_drops
            .iter()
            .map(|(_, origin)| *origin)
            .collect::<Vec<_>>(),
        ["right", "joined"]
    );
    assert!(!checked.drops().iter().any(|fact| {
        matches!(fact.point(), DropPoint::AfterExpression(expression)
            if expression == concat || expression == equal)
    }));
}

#[test]
fn moved_copyable_and_non_owning_values_never_gain_unique_drop_facts() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun valid(shared: Resource, inout exclusive: Resource, own owned: Resource, number: Int): Unit {\n\
                    val moved = take(owned)\n\
                    val sharedRead = inspect(shared)\n\
                    val exclusiveRead = inspect(exclusive)\n\
                    val copied = number\n\
                }";
    let (sources, _, checked) = checked(text);
    assert!(checked.diagnostics().is_empty());
    let origins = checked
        .drops()
        .iter()
        .map(|fact| sources.slice(fact.value_origin()).unwrap())
        .collect::<Vec<_>>();
    assert!(!origins.contains(&"owned"));
    assert!(!origins.contains(&"shared"));
    assert!(!origins.contains(&"exclusive"));
    assert!(!origins.contains(&"number"));
}

#[test]
fn inout_replacement_and_class_field_mutability_follow_the_closed_rules() {
    let passing = "class Resource {}\n\
                   class Holder(var payload: Resource)\n\
                   value class Inline(var payload: Resource)\n\
                   fun mutate(inout item: Resource): Unit {}\n\
                   fun replace(inout target: Resource, own replacement: Resource): Unit {\n\
                       { target = replacement }\n\
                       val check = mutate(&target)\n\
                   }\n\
                   fun fields(own holder: Holder, own replacement: Resource): Unit {\n\
                       val changed = mutate(&holder.payload)\n\
                       var inline = Inline(replacement)\n\
                       val inlineChanged = mutate(&inline.payload)\n\
                   }";
    let (_, _, passing_checked) = checked(passing);
    assert!(
        passing_checked.diagnostics().is_empty(),
        "{:?}",
        passing_checked.diagnostics()
    );

    let failing = "class Resource {}\n\
                   class Holder(var payload: Resource)\n\
                   value class Inline(var payload: Resource)\n\
                   fun mutate(inout item: Resource): Unit {}\n\
                   fun borrowed(holder: Holder): Unit { val bad = mutate(&holder.payload) }\n\
                   fun inlineField(own item: Inline): Unit { val bad = mutate(&item.payload) }";
    let (_, _, checked) = checked(failing);
    assert_eq!(codes(checked.diagnostics()), ["L0134", "L0134"]);
}

#[test]
fn intrinsic_index_and_member_receiver_have_distinct_ownership_boundaries() {
    let text = "class Resource {}\n\
                class Worker { fun inspect(item: Resource): Unit {} }\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun deferred(own worker: Worker, own list: MutableList<Resource>, own captured: Resource): Unit {\n\
                    val indexed = mutate(&list[0])\n\
                    val unknown = worker[0]\n\
                    val member = worker.inspect(captured)\n\
                    val callback: (own Resource) -> Unit = { input -> val nested = worker.inspect(captured) }\n\
                }";
    let (_, _, checked) = checked(text);
    assert!(
        checked.diagnostics().is_empty(),
        "{:?}",
        checked.diagnostics()
    );
    let reasons = checked
        .deferred()
        .iter()
        .map(|fact| fact.reason())
        .collect::<Vec<_>>();
    assert!(reasons.contains(&OwnershipDeferredReason::IndexPlace));
    assert!(!reasons.contains(&OwnershipDeferredReason::MemberReceiver));
    assert_eq!(checked.captures().len(), 2);
    assert!(!checked.drops().is_empty());
}

#[test]
fn ownership_facts_are_deterministic_across_repeated_checks() {
    let text = "class Resource {}\n\
                fun create(): Resource\n\
                fun inspect(item: Resource): Unit {}\n\
                fun stable(own input: Resource): Unit {\n\
                    val local = create()\n\
                    val first = inspect(input)\n\
                    val second = inspect(local)\n\
                }";
    let (_, _, first) = checked(text);
    let (_, _, second) = checked(text);

    assert_eq!(first.bindings(), second.bindings());
    // Span 保留所属 SourceMap 的 owner identity，跨 map 不直接相等；稳定 debug 只暴露
    // 可复现的 map-local SourceId、byte offset 与 AST/symbol identity。
    assert_eq!(
        format!("{:?}", first.loans()),
        format!("{:?}", second.loans())
    );
    assert_eq!(
        format!("{:?}", first.drops()),
        format!("{:?}", second.drops())
    );
    assert_eq!(first.deferred(), second.deferred());
    assert_eq!(
        format!("{:?}", first.diagnostics()),
        format!("{:?}", second.diagnostics())
    );
}

#[test]
fn checked_in_phase3_ownership_fixtures_execute_pass_and_fail_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase3");
    for (directory, should_pass) in [("ownership-pass", true), ("ownership-fail", false)] {
        let files = fs::read_dir(root.join(directory))
            .expect("ownership fixture directory")
            .map(|entry| entry.expect("ownership fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1, "zero or unexpected {directory} fixtures");
        for path in files {
            let text = fs::read_to_string(&path).expect("UTF-8 ownership fixture");
            let (_, _, checked) = checked(&text);
            if should_pass {
                assert!(checked.diagnostics().is_empty(), "{path:?}");
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
            }
        }
    }
}

#[test]
fn nullable_when_unselected_condition_does_not_move_first_branch_owner() {
    // A null match skips later conditions, so their owned arguments cannot move this branch's value.
    let text = "class Resource {}
        fun probe(own resource: Resource): Boolean = true
        fun read(resource: Resource): Int = 0
        fun test(flag: Boolean?, own resource: Resource): Int = when (flag) {
            null -> read(resource)
            probe(resource) -> 0
            else -> 0
        }";
    let (_, _, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn non_null_assertion_consumes_owned_source_even_for_borrowed_result() {
    // Borrowing the extracted result must not turn !! into a borrowed nullable view.
    let text = "class Resource {}
        fun read(item: Resource): Int = 0
        fun readNullable(item: Resource?): Int = 0
        fun test(own source: Resource?): Int {
            val first = read(source!!)
            val second = readNullable(source)
            return 0
        }";
    let (sources, _, owned) = checked(text);
    assert_eq!(codes(owned.diagnostics()), ["L0131"]);
    assert_eq!(
        sources
            .slice(owned.diagnostics()[0].primary_span())
            .unwrap(),
        "source"
    );
}

#[test]
fn non_null_assertion_rejects_borrowed_and_partial_move_sources() {
    // Extraction cannot leave a borrowed binding or a field with a missing owner.
    for (parameter, operand, expected) in [
        ("source: Resource?", "source", "L0133"),
        ("inout source: Resource?", "source", "L0133"),
        ("own source: Holder", "source.item", "L0132"),
        ("own source: Array<Resource?>", "source[0]", "L0136"),
    ] {
        let text = format!(
            "class Resource {{}}\nclass Holder(val item: Resource?) {{}}\nfun test({parameter}): Resource = {operand}!!"
        );
        let (sources, _, owned) = checked(&text);
        assert_eq!(codes(owned.diagnostics()), [expected], "{text}");
        // Field moves retain the shared diagnostic contract: highlight the field name.
        let expected_span = if expected == "L0132" { "item" } else { operand };
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .unwrap(),
            expected_span
        );
    }
}

#[test]
fn non_null_assertion_copy_preserves_borrowed_source() {
    // A Copyable extraction retains the source for a second extraction.
    let (_, _, owned) = checked(
        "fun test(source: Int?): Int { val first = source!!
return first + source!! }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn non_null_assertion_rejects_active_loan() {
    // A prior argument's shared loan lasts through extraction in the next argument.
    let text = "class Resource {}
        fun use(first: Resource?, second: Resource): Int = 0
        fun test(own source: Resource?): Int = use(source, source!!)";
    let (sources, _, owned) = checked(text);
    assert_eq!(codes(owned.diagnostics()), ["L0135"]);
    assert_eq!(
        sources
            .slice(owned.diagnostics()[0].primary_span())
            .unwrap(),
        "source"
    );
    assert!(owned.loans().is_empty());
    assert!(owned.drops().is_empty());
}

#[test]
fn non_null_assertion_transfers_drop_to_borrowed_result() {
    // The nullable root has transferred its obligation; only the extracted temporary is dropped.
    for operand in ["source", "create()"] {
        let text = format!(
            "class Resource {{}}
            fun create(): Resource? = Resource()
            fun read(item: Resource): Int = 0
            fun test(own source: Resource?): Int = read({operand}!!)"
        );
        let (sources, _, owned) = checked(&text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let extraction = format!("{operand}!!");
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| {
                sources
                    .slice(fact.value_origin())
                    .is_ok_and(|span| span == extraction)
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1, "{operand}: {:?}", owned.drops());
        assert!(matches!(drops[0].target(), DropTarget::Temporary(_)));
        assert!(matches!(drops[0].point(), DropPoint::CallReturn(_)));
        if operand == "source" {
            assert!(
                owned
                    .drops()
                    .iter()
                    .all(|fact| sources.slice(fact.value_origin()).unwrap() != "source")
            );
        }
    }
}

#[test]
fn non_null_assertion_copy_from_field_during_shared_loan() {
    // Copying the inner Int neither mutates the field nor conflicts with a shared root loan.
    let (_, _, owned) = checked(
        "class Holder(val item: Int?) {}
        fun pair(first: Holder, second: Int): Int = second
        fun test(holder: Holder): Int = pair(holder, holder.item!!)",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}
