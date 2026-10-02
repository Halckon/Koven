//! Owned mutable root exchange/swap facts and executable commit boundaries.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, DropTarget, LoanEndPoint, OwnershipCheckedFile,
        OwnershipPrimitiveValueTransfer, UnitDropPoint, UnitDropTarget,
        check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(text: &str) -> (OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("primitive.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &file, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new("root", "primitive.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (single, unit)
}

#[test]
fn owned_root_primitives_publish_only_normal_commit_facts() {
    let (single, unit) = checked(
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nvar b = Item(2)\nval old = replace(&a, Item(3))\nswap(&a, &b)\n}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(single.ownership_primitives().len(), 2);
    assert_eq!(unit.ownership_primitives().len(), 2);
    for (s, u) in single
        .ownership_primitives()
        .iter()
        .zip(unit.ownership_primitives())
    {
        assert_eq!(s.places().len(), u.places().len());
        assert_eq!(s.new_value_transfer(), u.new_value_transfer());
        assert_eq!(
            single.ownership_primitive(s.descriptor().expression()),
            Some(s)
        );
        assert_eq!(
            unit.ownership_primitive(u.descriptor().expression()),
            Some(u)
        );
    }
    assert_eq!(
        single.ownership_primitives()[0].new_value_transfer(),
        Some(OwnershipPrimitiveValueTransfer::Temporary)
    );
    assert_eq!(single.ownership_primitives()[1].new_value_transfer(), None);
    assert_eq!(
        single.drops().len(),
        3,
        "each of three values drops exactly once"
    );
    assert_eq!(
        unit.drops().len(),
        3,
        "each of three values drops exactly once"
    );
    assert!(
        single
            .drops()
            .iter()
            .all(|drop| matches!(drop.target(), DropTarget::Named(_)))
    );
    assert!(
        unit.drops()
            .iter()
            .all(|drop| matches!(drop.target(), UnitDropTarget::Named(_)))
    );
    let swap = single.ownership_primitives()[1].descriptor().expression();
    assert_eq!(
        single
            .drops()
            .iter()
            .filter(|drop| drop.point() == DropPoint::CallReturn(swap))
            .count(),
        2
    );
    let swap = unit.ownership_primitives()[1].descriptor().expression();
    assert_eq!(
        unit.drops()
            .iter()
            .filter(|drop| drop.point() == UnitDropPoint::CallReturn(swap))
            .count(),
        2
    );
    assert!(unit.validate().is_ok());
}

#[test]
fn copy_and_move_replacement_delivery_remain_distinct() {
    for (text, transfer) in [
        (
            "fun run(): Unit {\nvar a = 1\nval b = 2\nval old = replace(&a, b)\n}",
            OwnershipPrimitiveValueTransfer::Copy,
        ),
        (
            "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nval b = Item(2)\nval old = replace(&a, b)\n}",
            OwnershipPrimitiveValueTransfer::Move,
        ),
    ] {
        let (single, unit) = checked(text);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(
            single.ownership_primitives()[0].new_value_transfer(),
            Some(transfer)
        );
        assert_eq!(
            unit.ownership_primitives()[0].new_value_transfer(),
            Some(transfer)
        );
    }
}

#[test]
fn earlier_exclusive_loan_rejects_self_read_self_move_and_overlap() {
    for call in ["replace(&a, a)", "swap(&a, &a)", "replace(&a, read(a))"] {
        let text = format!(
            "class Item(val n: Int)\nfun read(value: Item): Item = Item(value.n)\nfun run(): Unit {{\nvar a = Item(1)\n{call}\n}}"
        );
        let (single, unit) = checked(&text);
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{call}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{call}: {:?}",
            unit.diagnostics()
        );
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
    }
}

#[test]
fn interrupted_replacement_has_no_commit_fact() {
    for text in [
        "fun run(): Unit {\nvar a = 1\nreplace(&a, error(\"stop\"))\n}",
        "fun run(): Unit {\nvar a = 1\nreplace(&a, return)\n}",
        "fun run(): Unit {\nvar a = 1\nloop { replace(&a, break) }\n}",
        "fun run(): Unit {\nvar a = 1\nloop { replace(&a, continue) }\n}",
    ] {
        let (single, unit) = checked(text);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
    }
}

#[test]
fn borrowed_roots_and_field_places_do_not_gain_owned_root_commit() {
    for text in [
        "fun run(inout a: Int): Unit {\nreplace(&a, 2)\n}",
        "class Item(var n: Int)\nfun run(): Unit {\nval a = Item(1)\nreplace(&a.n, 2)\n}",
    ] {
        let (single, unit) = checked(text);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
    }
}

#[test]
fn closure_exchange_explicitly_defers_provenance_transfer() {
    for text in [
        "fun run(): Unit {\nvar a: () -> Int = { 1 }\nval old = replace(&a, { 2 })\n}",
        "class Holder(val callback: () -> Int)\nfun run(): Unit {\nvar a = Holder({ 1 })\nval old = replace(&a, Holder({ 2 }))\n}",
        "class Holder(val callback: () -> Int)\nclass Outer(val inner: Holder)\nfun run(): Unit {\nvar a = Outer(Holder({ 1 }))\nvar b = Outer(Holder({ 2 }))\nswap(&a, &b)\n}",
    ] {
        let (single, unit) = checked(text);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(!single.deferred().is_empty());
        assert!(!unit.deferred().is_empty());
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
        assert!(unit.validate().is_err());
    }
}

#[test]
fn root_copyable_temporary_has_no_unique_drop_obligation() {
    let (single, unit) = checked("fun run(): Unit {\nvar a = 1\nval old = replace(&a, 2)\n}");
    assert!(single.diagnostics().is_empty());
    assert!(unit.diagnostics().is_empty());
    assert_eq!(
        single.ownership_primitives()[0].new_value_transfer(),
        Some(OwnershipPrimitiveValueTransfer::Temporary)
    );
    assert_eq!(
        unit.ownership_primitives()[0].new_value_transfer(),
        Some(OwnershipPrimitiveValueTransfer::Temporary)
    );
    assert!(single.drops().is_empty());
    assert!(unit.drops().is_empty());
    assert!(unit.validate().is_ok());
}

#[test]
fn root_return_cleans_loan_before_original_owner_and_abort_does_not_unwind() {
    let (single, unit) = checked(
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nreplace(&a, return)\n}",
    );
    assert!(single.diagnostics().is_empty());
    assert!(unit.diagnostics().is_empty());
    assert!(single.ownership_primitives().is_empty());
    assert!(unit.ownership_primitives().is_empty());
    assert_eq!(single.drops().len(), 1);
    assert_eq!(unit.drops().len(), 1);
    assert!(matches!(
        single.drops()[0].point(),
        DropPoint::ControlTransfer(_)
    ));
    assert!(matches!(
        unit.drops()[0].point(),
        UnitDropPoint::ControlTransfer(_)
    ));
    assert!(
        single
            .loan_ends()
            .iter()
            .any(|fact| matches!(fact.point(), LoanEndPoint::ControlTransfer(_)))
    );
    let (single, unit) = checked(
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nreplace(&a, error(\"stop\"))\n}",
    );
    assert!(single.ownership_primitives().is_empty());
    assert!(unit.ownership_primitives().is_empty());
    assert!(
        single
            .drops()
            .iter()
            .all(|drop| !matches!(drop.target(), DropTarget::Named(_)))
    );
    assert!(
        unit.drops()
            .iter()
            .all(|drop| !matches!(drop.target(), UnitDropTarget::Named(_)))
    );
}

#[test]
fn root_nonowning_move_and_later_errors_clear_every_commit() {
    for text in [
        "class Item(val n: Int)\nfun run(other: Item): Unit {\nvar a = Item(1)\nreplace(&a, other)\n}",
        "class Item(val n: Int)\nfun take(own value: Item): Unit {}\nfun run(): Unit {\nvar a = Item(1)\nval b = Item(2)\nreplace(&a, b)\ntake(b)\n}",
        "fun run(): Unit {\nvar a = 1\nreplace(&a, 2)\nval b = 3\nreplace(&b, 4)\n}",
    ] {
        let (single, unit) = checked(text);
        assert!(!single.diagnostics().is_empty());
        assert!(!unit.diagnostics().is_empty());
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
        assert!(single.loans().is_empty());
        assert!(unit.loans().is_empty());
        assert!(single.drops().is_empty());
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn root_unit_identity_and_order_are_source_qualified() {
    let mut sources = SourceMap::new();
    let mut files = Vec::new();
    for name in ["first", "second"] {
        let source = sources
            .add_source(
                format!("{name}.ko"),
                format!(
                    "fun {name}(): Unit {{\nvar a = 1\nvar b = 2\nreplace(&a, b)\nswap(&a, &b)\n}}"
                ),
            )
            .unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        files.push((source, file));
    }
    let mut inputs = vec![
        SourceUnitInput::new("root", "first.ko", files[0].0, &files[0].1),
        SourceUnitInput::new("root", "second.ko", files[1].0, &files[1].1),
    ];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let forward =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(forward.is_compatible_with(&typed));
    inputs.reverse();
    let other_typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let reverse =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &other_typed).unwrap();
    assert!(!forward.is_compatible_with(&other_typed));
    assert_eq!(
        forward.ownership_primitives(),
        reverse.ownership_primitives()
    );
    assert_eq!(forward.ownership_primitives().len(), 4);
    let first = forward.ownership_primitives()[0].descriptor().expression();
    let second = forward.ownership_primitives()[2].descriptor().expression();
    assert_eq!(first.expression(), second.expression());
    assert_ne!(first.source_unit(), second.source_unit());
    assert!(forward.validate().is_ok());
    assert!(reverse.validate().is_ok());
}

#[test]
fn root_commit_coalesces_complete_nonclosure_branch_values_before_drop() {
    for text in [
        "fun run(flag: Boolean): Unit {\nvar a = \"old\"\nval old = replace(&a, if (flag) \"new\" else \"other\")\n}",
        "fun run(flag: Boolean): Unit {\nvar a = if (flag) \"old\" else \"other\"\nval old = replace(&a, \"new\")\n}",
        "fun run(flag: Boolean): Unit {\nvar a = if (flag) \"first\" else \"second\"\nvar b = \"other\"\nswap(&a, &b)\n}",
    ] {
        let (single, unit) = checked(text);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(single.ownership_primitives().len(), 1);
        assert_eq!(unit.ownership_primitives().len(), 1);
        assert!(
            single.drops().iter().all(|drop| drop.condition().is_none()),
            "complete roots must have one unconditional drop: {:?}",
            single.drops()
        );
        assert_eq!(single.drops().len(), 2);
        assert_eq!(unit.drops().len(), 2);
    }
}

#[test]
fn root_copyable_reads_also_conflict_with_the_earlier_exclusive_loan() {
    for call in ["replace(&a, a)", "replace(&a, read(a))"] {
        let text =
            format!("fun read(value: Int): Int = value\nfun run(): Unit {{\nvar a = 1\n{call}\n}}");
        let (single, unit) = checked(&text);
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135")
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135")
        );
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
    }
}

#[test]
fn root_concrete_generic_nominal_is_not_confused_with_its_field_template() {
    let (single, unit) = checked(
        "class Holder<T>(val value: T)\nfun run(): Unit {\nvar a = Holder<Int>(1)\nvar b = Holder<Int>(2)\nswap(&a, &b)\n}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(single.ownership_primitives().len(), 1);
    assert_eq!(unit.ownership_primitives().len(), 1);
    assert!(unit.validate().is_ok());
}

#[test]
fn root_commit_transports_plain_owner_identity_before_later_capture() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupOwnerValue, ClosureCaptureSource, IterationCleanupAction,
    };
    let (single, unit) = checked(
        "fun run(): Unit {\nvar a = \"old\"\nval old = replace(&a, \"new\")\nval callback = move { a + old }\ncallback()\n}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let plan = &single.ownership_primitives()[0];
    let root = plan.places()[0].root();
    let input = single
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture { input, .. }
                if input.source() == ClosureCaptureSource::Symbol(root) =>
            {
                Some(*input)
            }
            _ => None,
        })
        .expect("the later closure captures the current root owner");
    let CleanupCaptureValue::Owner(owner) = input.value() else {
        panic!("owned source identity");
    };
    assert!(
        matches!(single.cleanup_conditions().owner_value(owner), Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == plan.descriptor().operands()[1])
    );
}
