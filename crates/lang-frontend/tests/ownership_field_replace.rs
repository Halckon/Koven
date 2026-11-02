//! Direct class-field exchange has its own capability, never an owned-root commit.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipPrimitiveValueTransfer, UnitDropPoint, UnitDropTarget,
        check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        FinalTypeAdmissionError, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, standard_environments,
    },
};

fn checked(text: &str) -> CompilationUnitOwnership {
    let mut sources = SourceMap::new();
    let source = sources.add_source("field.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    // Both Unit entry forms observe the same source and AST. The ordinary field
    // contracts intentionally retire Single observations; the non-equivalent global
    // receiver contract below retains its independent Single pipeline.
    let mut observations = Vec::new();
    for input in [
        SourceUnitInput::new("root", "field.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let [indexed] = names.names().index().source_units() else {
            panic!("one actual source");
        };
        let source_unit = indexed.id();
        assert_eq!(indexed.source_id(), source);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        // This observes admission; matching rejection is NOT a success witness.
        // A deferred fixture can legitimately fail final admission. Preserve its full
        // diagnostics instead of equating basic validation with complete admission.
        let complete =
            match complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed) {
                Ok(complete) => {
                    assert!(complete.is_same_analysis(&typed));
                    Ok(())
                }
                Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => Err(diagnostics),
                Err(other) => panic!("internal final-admission failure: {other:?}"),
            };
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        for plan in owned.ownership_primitives() {
            let descriptor = plan.descriptor();
            assert_eq!(descriptor.expression().source_unit(), source_unit);
            assert_eq!(
                owned.ownership_primitive(descriptor.expression()),
                Some(plan)
            );
            assert_eq!(
                typed.types().ownership_primitive(descriptor.expression()),
                Some(*descriptor)
            );
            for operand in descriptor.operands() {
                assert_eq!(operand.source_unit(), source_unit);
            }
            for place in plan.places() {
                assert_eq!(place.root().source_unit(), source_unit);
            }
        }
        for plan in owned.field_replacements() {
            let descriptor = plan.descriptor();
            assert_eq!(descriptor.expression().source_unit(), source_unit);
            assert_eq!(owned.field_replacement(descriptor.expression()), Some(plan));
            assert_eq!(
                typed.types().ownership_primitive(descriptor.expression()),
                Some(*descriptor)
            );
            assert_eq!(plan.place().root().source_unit(), source_unit);
            for field in plan.place().fields() {
                assert_eq!(field.source_unit(), source_unit);
            }
            for operand in descriptor.operands() {
                assert_eq!(operand.source_unit(), source_unit);
            }
        }
        for fact in owned.deferred() {
            assert_eq!(fact.expression().source_unit(), source_unit);
        }
        observations.push((owned, complete));
    }
    let [(unit, complete), (memory, memory_complete)] = observations.as_slice() else {
        panic!("rooted and MemoryDocument observations");
    };
    // Diagnostic equality retains severity, code, message, primary span and every detail.
    assert_eq!(unit.diagnostics(), memory.diagnostics(), "{text}");
    assert_eq!(complete, memory_complete, "{text}");
    assert_eq!(unit.ownership_primitives(), memory.ownership_primitives());
    assert_eq!(unit.field_replacements(), memory.field_replacements());
    assert_eq!(unit.loans(), memory.loans());
    assert_eq!(unit.drops(), memory.drops());
    assert_eq!(unit.deferred(), memory.deferred());
    assert_eq!(
        unit.call_argument_contracts(),
        memory.call_argument_contracts()
    );
    assert_eq!(unit.value_deliveries(), memory.value_deliveries());
    assert_eq!(
        unit.clone().validate().is_ok(),
        memory.clone().validate().is_ok()
    );
    unit.clone()
}

#[test]
fn direct_field_replace_publishes_distinct_facts_for_val_and_var_class_receivers() {
    for binding in ["val", "var"] {
        let unit = checked(&format!(
            "class Holder(var state: Int)\nfun run(): Int {{\n{binding} holder = Holder(1)\nval replacement = 2\nval old = replace(&(holder.state), replacement)\nreturn old + holder.state\n}}"
        ));
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.ownership_primitives().is_empty());
        assert_eq!(unit.field_replacements().len(), 1);

        let u = &unit.field_replacements()[0];
        assert_eq!(u.place().fields().len(), 1);
        assert!(u.place().element().is_none());
        assert_eq!(
            u.new_value_transfer(),
            OwnershipPrimitiveValueTransfer::Copy
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
        let unit = checked(&format!(
            "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(): Int {{\nval holder = Holder(Item(1))\nval replacement = Item(2)\nval old = replace(&holder.state, {new})\nreturn old.n + holder.state.n\n}}"
        ));
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(unit.field_replacements()[0].new_value_transfer(), transfer);
        let expected = if new == "replacement" { 2 } else { 3 };
        assert_eq!(unit.drops().len(), expected, "{:?}", unit.drops());
        assert!(
            unit.drops()
                .iter()
                .all(|drop| matches!(drop.target(), UnitDropTarget::Named(_)))
        );
        assert!(unit.validate().is_ok());
    }
    let unit = checked(
        "class Item(val n: Int)\nclass Holder(var state: Item)\nfun take(own item: Item): Unit {}\nfun run(): Unit {\nval holder = Holder(Item(1))\nval old = replace(&holder.state, Item(2))\ntake(old)\ntake(old)\n}",
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0131")
    );
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
        let unit = checked(&format!(
            "class Item(val n: Int)\nclass Holder(var state: Item)\n{read}\nfun run(): Unit {{\nval holder = Holder(Item(1))\nreplace(&holder.state, {new})\n}}"
        ));
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{:?}",
            unit.diagnostics()
        );
        assert!(unit.field_replacements().is_empty());
    }
}

#[test]
fn direct_field_replace_rejects_val_field_and_clears_prior_commit() {
    let unit = checked(
        "class Holder(var state: Int, val fixed: Int)\nfun run(): Unit {\nval holder = Holder(1, 2)\nreplace(&holder.state, 3)\nreplace(&holder.fixed, 4)\n}",
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0134")
    );
    assert!(unit.field_replacements().is_empty());
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
    let mut diagnostics = Vec::new();
    for input in [
        SourceUnitInput::new("root", "temporary.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
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
        diagnostics.push(typed.diagnostics().to_vec());
    }
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0], diagnostics[1]);
}

#[test]
fn direct_field_replace_control_exit_never_commits() {
    for tail in [
        "replace(&holder.state, return)",
        "replace(&holder.state, error(\"stop\"))",
        "loop { replace(&holder.state, break) }",
        "loop { replace(&holder.state, continue) }",
    ] {
        let unit = checked(&format!(
            "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(): Unit {{\nval holder = Holder(Item(1))\n{tail}\n}}"
        ));
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.field_replacements().is_empty());
        assert!(unit.ownership_primitives().is_empty());
        if tail.ends_with("return)") {
            assert_eq!(unit.drops().len(), 1);
            assert!(matches!(
                unit.drops()[0].point(),
                UnitDropPoint::ControlTransfer(_)
            ));
        } else if tail.contains("error") {
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
        let unit = checked(text);
        assert!(
            unit.diagnostics().is_empty(),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert!(unit.field_replacements().is_empty());
        assert!(unit.ownership_primitives().is_empty());
    }
}

#[test]
fn direct_field_replace_discarded_old_value_drops_once_without_replacing_parent() {
    let unit = checked(
        "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(): Int {\nval holder = Holder(Item(1))\nreplace(&holder.state, Item(2))\nreplace(&holder.state, Item(3))\nreturn holder.state.n\n}",
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(unit.field_replacements().len(), 2);
    assert_eq!(unit.drops().len(), 3, "{:?}", unit.drops());
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
        let unit = checked(&format!(
            "class Holder(var state: Int, var other: Int)\nfun run(): Int {{\nval holder = Holder(1, 2)\nval old = replace(&holder.state, {new})\nreturn old + holder.state + holder.other\n}}"
        ));
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(
            unit.field_replacements().len(),
            if new.starts_with("replace") { 2 } else { 1 }
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
    let unit = checked(
        "class Item(val n: Int)\nclass Holder(var state: Item)\nfun run(other: Item): Unit {\nval holder = Holder(Item(1))\nreplace(&holder.state, other)\n}",
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0133"),
        "{:?}",
        unit.diagnostics()
    );
    assert!(unit.field_replacements().is_empty());
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

        let mut diagnostics = Vec::new();
        for input in [
            SourceUnitInput::new("root", "global.ko", source, &file),
            SourceUnitInput::memory_document(source, &file),
        ] {
            let inputs = [input];
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

            diagnostics.push(typed.diagnostics().to_vec());
            assert!(typed.validate().is_err());
        }
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0], diagnostics[1]);
    }
}

#[test]
fn direct_field_replace_captured_receiver_defers_owner_provenance() {
    let unit = checked(
        "class Holder(var state: Int)\nfun run(): Int {\nval holder = Holder(1)\nval callback: move () -> Int = move { replace(&holder.state, 2) }\nreturn callback()\n}",
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert!(
        unit.field_replacements().is_empty(),
        "captured owner is not a body-local owner"
    );
    assert!(unit.ownership_primitives().is_empty());
    assert!(unit.deferred().iter().any(|fact| fact.reason() == lang_frontend::ownership_checking::OwnershipDeferredReason::OwnershipPrimitiveClosureTransport));
    assert!(unit.validate().is_err());
}

#[test]
fn direct_field_replace_shared_capture_never_gains_exclusive_permission() {
    let text = "class Holder(var state: Int)\nfun run(): Int {\nval holder = Holder(1)\nval callback: () -> Int = { replace(&holder.state, 2) }\nreturn callback()\n}";
    let unit = checked(text);
    {
        let diagnostics = unit.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code().to_string(), "L0135");
        let span = diagnostics[0].primary_span();
        assert_eq!(&text[span.start()..span.end()], "&");
        assert!(diagnostics[0].details().iter().any(|detail| {
            let lang_frontend::diagnostic::DiagnosticDetail::Label(label) = detail else {
                return false;
            };
            let span = label.span();
            &text[span.start()..span.end()] == "holder"
        }));
    }
    assert!(unit.field_replacements().is_empty());
    assert!(unit.ownership_primitives().is_empty());
}
