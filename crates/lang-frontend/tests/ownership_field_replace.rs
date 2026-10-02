//! Direct class-field exchange has its own capability, never an owned-root commit.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, DropTarget, OwnershipCheckedFile,
        OwnershipPrimitiveValueTransfer, UnitDropPoint, UnitDropTarget,
        check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(text: &str) -> (OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("field.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &file, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new("root", "field.ko", source, &file)];
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
fn direct_field_replace_publishes_distinct_facts_for_val_and_var_class_receivers() {
    for binding in ["val", "var"] {
        let (single, unit) = checked(&format!(
            "class Holder(var state: Int)\nfun run(): Int {{\n{binding} holder = Holder(1)\nval replacement = 2\nval old = replace(&(holder.state), replacement)\nreturn old + holder.state\n}}"
        ));
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
        assert_eq!(single.field_replacements().len(), 1);
        assert_eq!(unit.field_replacements().len(), 1);
        let s = &single.field_replacements()[0];
        let u = &unit.field_replacements()[0];
        assert_eq!(s.place().fields().len(), 1);
        assert_eq!(u.place().fields().len(), 1);
        assert!(s.place().elements().is_empty());
        assert!(u.place().element().is_none());
        assert_eq!(
            s.new_value_transfer(),
            OwnershipPrimitiveValueTransfer::Copy
        );
        assert_eq!(
            u.new_value_transfer(),
            OwnershipPrimitiveValueTransfer::Copy
        );
        assert_eq!(
            single.field_replacement(s.descriptor().expression()),
            Some(s)
        );
        assert_eq!(unit.field_replacement(u.descriptor().expression()), Some(u));
        assert!(unit.validate().is_ok());
    }
}

#[test]
fn direct_field_replace_keeps_parent_and_returns_old_owner_once() {
    for (new, transfer) in [
        ("replacement", OwnershipPrimitiveValueTransfer::Move),
        ("Item(3)", OwnershipPrimitiveValueTransfer::Temporary),
    ] {
        let (single, unit) = checked(&format!(
            "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(): Int {{\nval holder = Holder(Item(1))\nval replacement = Item(2)\nval old = replace(&holder.state, {new})\nreturn old.n + holder.state.n\n}}"
        ));
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(
            single.field_replacements()[0].new_value_transfer(),
            transfer
        );
        assert_eq!(unit.field_replacements()[0].new_value_transfer(), transfer);
        let expected = if new == "replacement" { 2 } else { 3 };
        assert_eq!(single.drops().len(), expected, "{:?}", single.drops());
        assert_eq!(unit.drops().len(), expected, "{:?}", unit.drops());
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
        assert!(unit.validate().is_ok());
    }
    let (single, unit) = checked(
        "class Item(val n: Int)\nclass Holder(var state: Item)\nfun take(own item: Item): Unit {}\nfun run(): Unit {\nval holder = Holder(Item(1))\nval old = replace(&holder.state, Item(2))\ntake(old)\ntake(old)\n}",
    );
    assert!(
        single
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0131")
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0131")
    );
    assert!(single.field_replacements().is_empty());
    assert!(unit.field_replacements().is_empty());
}

#[test]
fn direct_field_replace_rejects_self_read_self_move_and_overlapping_loan() {
    for new in ["holder.state", "read(holder)", "read(holder.state)"] {
        let read = if new == "read(holder)" {
            "fun read(value: Holder): Item = Item(value.state.n)"
        } else {
            "fun read(value: Item): Item = Item(value.n)"
        };
        let (single, unit) = checked(&format!(
            "class Item(val n: Int)\nclass Holder(var state: Item)\n{read}\nfun run(): Unit {{\nval holder = Holder(Item(1))\nreplace(&holder.state, {new})\n}}"
        ));
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{:?}",
            unit.diagnostics()
        );
        assert!(single.field_replacements().is_empty());
        assert!(unit.field_replacements().is_empty());
    }
}

#[test]
fn direct_field_replace_rejects_val_field_and_clears_prior_commit() {
    let (single, unit) = checked(
        "class Holder(var state: Int, val fixed: Int)\nfun run(): Unit {\nval holder = Holder(1, 2)\nreplace(&holder.state, 3)\nreplace(&holder.fixed, 4)\n}",
    );
    assert!(
        single
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0134")
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0134")
    );
    assert!(single.field_replacements().is_empty());
    assert!(unit.field_replacements().is_empty());
    assert!(single.drops().is_empty());
    assert!(unit.drops().is_empty());
}

#[test]
fn direct_field_replace_temporary_receiver_is_not_a_place() {
    let text = "class Holder(var state: Int)\nfun run(): Unit { replace(&Holder(1).state, 2) }";
    let mut sources = SourceMap::new();
    let source = sources.add_source("temporary.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(
        typed
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0122"),
        "{:?}",
        typed.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", "temporary.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(
        typed
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0122"),
        "{:?}",
        typed.diagnostics()
    );
}

#[test]
fn direct_field_replace_control_exit_never_commits() {
    for tail in [
        "replace(&holder.state, return)",
        "replace(&holder.state, error(\"stop\"))",
        "loop { replace(&holder.state, break) }",
        "loop { replace(&holder.state, continue) }",
    ] {
        let (single, unit) = checked(&format!(
            "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(): Unit {{\nval holder = Holder(Item(1))\n{tail}\n}}"
        ));
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(single.field_replacements().is_empty());
        assert!(unit.field_replacements().is_empty());
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
        if tail.ends_with("return)") {
            assert_eq!(single.drops().len(), 1);
            assert!(matches!(
                single.drops()[0].point(),
                DropPoint::ControlTransfer(_)
            ));
            assert_eq!(unit.drops().len(), 1);
            assert!(matches!(
                unit.drops()[0].point(),
                UnitDropPoint::ControlTransfer(_)
            ));
        } else if tail.contains("error") {
            assert!(single.drops().is_empty(), "{:?}", single.drops());
            assert!(unit.drops().is_empty(), "{:?}", unit.drops());
        }
    }
}

#[test]
fn direct_field_replace_retains_explicit_unsupported_boundaries() {
    for text in [
        "class Holder(var state: Int)\nclass Outer(val inner: Holder)\nfun run(): Unit {\nval outer = Outer(Holder(1))\nreplace(&outer.inner.state, 2)\n}",
        "class Holder(var state: Int)\nfun run(inout holder: Holder): Unit { replace(&holder.state, 2) }",
        "value class Holder(var state: Int)\nfun run(): Unit {\nvar holder = Holder(1)\nreplace(&holder.state, 2)\n}",
        "fun run(): Unit {\nval values = arrayOf<Int>(1)\nreplace(&values[0], 2)\n}",
        "class Holder(var state: Int) { inout fun update(): Unit { replace(&this.state, 2) } }",
        "class Holder<T>(var state: Int, val other: T)\nfun <T> run(own value: T): Unit {\nval holder = Holder<T>(1, value)\nreplace(&holder.state, 2)\n}",
        "class Holder(var state: () -> Int)\nfun run(): Unit {\nval holder = Holder({ 1 })\nreplace(&holder.state, { 2 })\n}",
    ] {
        let (single, unit) = checked(text);
        assert!(
            single.diagnostics().is_empty(),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert!(single.field_replacements().is_empty());
        assert!(unit.field_replacements().is_empty());
        assert!(single.ownership_primitives().is_empty());
        assert!(unit.ownership_primitives().is_empty());
    }
}

#[test]
fn direct_field_replace_discarded_old_value_drops_once_without_replacing_parent() {
    let (single, unit) = checked(
        "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(): Int {\nval holder = Holder(Item(1))\nreplace(&holder.state, Item(2))\nreplace(&holder.state, Item(3))\nreturn holder.state.n\n}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(single.field_replacements().len(), 2);
    assert_eq!(unit.field_replacements().len(), 2);
    assert_eq!(single.drops().len(), 3, "{:?}", single.drops());
    assert_eq!(unit.drops().len(), 3, "{:?}", unit.drops());
    assert_eq!(
        single
            .drops()
            .iter()
            .filter(|drop| matches!(drop.target(), DropTarget::Named(_)))
            .count(),
        1
    );
    assert_eq!(
        unit.drops()
            .iter()
            .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(_)))
            .count(),
        1
    );
    assert!(unit.validate().is_ok());
}

#[test]
fn direct_field_replace_allows_disjoint_sibling_access_and_exchange() {
    for new in ["holder.other", "replace(&holder.other, 3)"] {
        let (single, unit) = checked(&format!(
            "class Holder(var state: Int, var other: Int)\nfun run(): Int {{\nval holder = Holder(1, 2)\nval old = replace(&holder.state, {new})\nreturn old + holder.state + holder.other\n}}"
        ));
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(
            single.field_replacements().len(),
            if new.starts_with("replace") { 2 } else { 1 }
        );
        assert_eq!(
            single.field_replacements().len(),
            unit.field_replacements().len()
        );
        assert!(unit.validate().is_ok());
    }
}

#[test]
fn direct_field_replace_unit_identity_and_order_are_source_qualified() {
    let mut sources = SourceMap::new();
    let mut files = Vec::new();
    for name in ["first", "second"] {
        let source = sources.add_source(format!("{name}.ko"), format!("package {name}\nclass Holder(var state: Int)\nfun run(): Int {{\nval holder = Holder(1)\nreturn replace(&holder.state, 2)\n}}")).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        files.push((source, file));
    }
    let mut inputs = vec![
        SourceUnitInput::new("root", "first/source.ko", files[0].0, &files[0].1),
        SourceUnitInput::new("root", "second/source.ko", files[1].0, &files[1].1),
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
    assert_eq!(forward.field_replacements().len(), 2);
    assert!(forward.is_compatible_with(&typed));
    let first = &forward.field_replacements()[0];
    let second = &forward.field_replacements()[1];
    assert_ne!(
        first.descriptor().expression().source_unit(),
        second.descriptor().expression().source_unit()
    );
    for plan in forward.field_replacements() {
        assert_eq!(
            plan.place().root().source_unit(),
            plan.descriptor().expression().source_unit()
        );
    }
    inputs.reverse();
    let other_typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let backward =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &other_typed).unwrap();
    assert_eq!(forward.field_replacements(), backward.field_replacements());
    assert!(!forward.is_compatible_with(&other_typed));
    assert!(forward.validate().is_ok());
    assert!(backward.validate().is_ok());
}

#[test]
fn direct_field_replace_rejects_borrowed_replacement_owner() {
    let (single, unit) = checked(
        "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(other: Item): Unit {\nval holder = Holder(Item(1))\nreplace(&holder.state, other)\n}",
    );
    assert!(
        single
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0133"),
        "{:?}",
        single.diagnostics()
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0133"),
        "{:?}",
        unit.diagnostics()
    );
    assert!(single.field_replacements().is_empty());
    assert!(unit.field_replacements().is_empty());
    assert!(single.drops().is_empty());
    assert!(unit.drops().is_empty());
}

#[test]
fn direct_field_replace_global_receiver_never_gains_local_owner_capability() {
    for binding in ["val", "var"] {
        let text = format!(
            "class Holder(var state: Int)\n{binding} holder = Holder(1)\nfun run(): Unit {{ replace(&holder.state, 2) }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("global.ko", &text).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &file, &environment).unwrap();
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let single = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(
            single.field_replacements().is_empty(),
            "global receiver is not an owned local"
        );
        assert!(single.ownership_primitives().is_empty());

        let inputs = [SourceUnitInput::new("root", "global.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        // Unit globals have no body-local place category, so rejection precedes ownership.
        assert!(
            typed
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0122"),
            "{:?}",
            typed.diagnostics()
        );
        assert!(typed.validate().is_err());
    }
}

#[test]
fn direct_field_replace_captured_receiver_defers_owner_provenance() {
    let (single, unit) = checked(
        "class Holder(var state: Int)\nfun run(): Int {\nval holder = Holder(1)\nval callback: move () -> Int = move { replace(&holder.state, 2) }\nreturn callback()\n}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert!(
        single.field_replacements().is_empty(),
        "captured owner is not a body-local owner"
    );
    assert!(
        unit.field_replacements().is_empty(),
        "captured owner is not a body-local owner"
    );
    assert!(single.ownership_primitives().is_empty());
    assert!(unit.ownership_primitives().is_empty());
    assert!(single.deferred().iter().any(|fact| fact.reason() == lang_frontend::ownership_checking::OwnershipDeferredReason::OwnershipPrimitiveClosureTransport));
    assert!(unit.deferred().iter().any(|fact| fact.reason() == lang_frontend::ownership_checking::OwnershipDeferredReason::OwnershipPrimitiveClosureTransport));
    assert!(unit.validate().is_err());
}
