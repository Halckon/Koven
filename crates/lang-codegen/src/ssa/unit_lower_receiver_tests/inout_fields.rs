use super::*;

#[test]
fn inout_class_receiver_replaces_and_reads_the_same_payload_field() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Counter(var item: Int) {\n\
             inout fun set(own next: Int): Int {\n\
                 val ignored: Unit = (this).item = next\n\
                 return item\n\
             }\n\
         }\n\
         fun entry(): Int {\n\
             val counter = Counter(1)\n\
             return counter.set(42)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Inout class payload replacement must lower to verified SSA");

    let module = &program.modules[0];
    let member = function(module.functions.iter(), ".Counter.set.s");
    let EntityId::Loan(receiver) = member.blocks[0].parameters[0] else {
        panic!("Inout class receiver must remain an exclusive loan");
    };
    assert!(matches!(
        member.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            ..
        })
    ));
    let operations = member
        .instructions
        .iter()
        .map(|instruction| &instruction.operation)
        .collect::<Vec<_>>();
    let replace = operations
        .iter()
        .position(|operation| {
            matches!(
                operation,
                Operation::HeapFieldReplace {
                    receiver: actual,
                    field: 0,
                    ..
                } if *actual == receiver
            )
        })
        .expect("payload replacement");
    let read = operations
        .iter()
        .position(|operation| {
            matches!(
                operation,
                Operation::HeapFieldRead {
                    receiver: actual,
                    field: 0
                } if *actual == receiver
            )
        })
        .expect("payload read after replacement");
    assert!(replace < read);
    assert!(!operations.iter().any(|operation| matches!(
        operation,
        Operation::RootPlace { .. } | Operation::BorrowBegin { .. } | Operation::Mutate { .. }
    )));

    let llvm = render_verified_program(&program).expect("payload replacement must lower to LLVM");
    let member_llvm = llvm
        .split("define internal i32 @f1.koven.p.Counter.set")
        .nth(1)
        .and_then(|body| body.split("define internal").next())
        .unwrap_or_else(|| panic!("member LLVM body:\n{llvm}"));
    assert!(member_llvm.contains("load ptr, ptr %l0"), "{member_llvm}");
    assert!(member_llvm.contains("store i32"), "{member_llvm}");
    assert!(!member_llvm.contains("store ptr"), "{member_llvm}");
}

#[test]
fn inout_class_receiver_replaces_a_move_only_payload_field() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Cell(var item: String) {\n\
             inout fun set(): Unit {\n\
                 val ignored: Unit = (this.item = \"n\" + \"ew\")\n\
             }\n\
         }\n\
         fun entry(): Unit {\n\
             val cell = Cell(\"old\")\n\
             val ignored = cell.set()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("MoveOnly class payload replacement must consume the old-field drop fact");

    let module = &program.modules[0];
    let member = function(module.functions.iter(), ".Cell.set.s");
    let concat = member
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::StringConcat { .. }))
        .expect("replacement RHS concat");
    let replace = member
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.operation,
                Operation::HeapFieldReplace { field: 0, .. }
            )
        })
        .expect("MoveOnly payload replacement");
    assert!(
        concat < replace,
        "RHS must finish before old-field replacement"
    );

    let llvm = render_verified_program(&program).expect("MoveOnly field replacement LLVM");
    let member_llvm = llvm
        .split("define internal void @f1.koven.p.Cell.set")
        .nth(1)
        .and_then(|body| body.split("define internal").next())
        .unwrap_or_else(|| panic!("member LLVM body:\n{llvm}"));
    let old_load = member_llvm
        .find(".old = load")
        .unwrap_or_else(|| panic!("old field is loaded after RHS evaluation:\n{member_llvm}"));
    let old_drop = member_llvm[old_load..]
        .find("call void @koven.drop")
        .map(|offset| old_load + offset)
        .expect("old MoveOnly field is dropped");
    let replacement_store = member_llvm[old_drop..]
        .find("store")
        .map(|offset| old_drop + offset)
        .expect("new field is committed after the old drop");
    assert!(
        old_load < old_drop && old_drop < replacement_store,
        "{member_llvm}"
    );
}

#[test]
fn direct_slot_generic_receiver_replaces_a_concrete_string_field() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Cell<T>(var item: T) {\n\
             inout fun set(own replacement: T): Unit {\n\
                 val ignored: Unit = (this.item = replacement)\n\
             }\n\
         }\n\
         fun entry(): Unit {\n\
             val cell = Cell<String>(\"old\" + \"-value\")\n\
             val ignored = cell.set(\"new\" + \"-value\")\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("generic T replacement fact must lower against the concrete String layout");

    let module = &program.modules[0];
    let member = function(module.functions.iter(), ".Cell.set.s");
    assert!(member.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapFieldReplace { field: 0, .. }
    )));

    let llvm = render_verified_program(&program).expect("generic String replacement LLVM");
    let member_llvm = llvm
        .split("define internal void @f1.koven.p.Cell.set")
        .nth(1)
        .and_then(|body| body.split("define internal").next())
        .unwrap_or_else(|| panic!("generic member LLVM body:\n{llvm}"));
    let old_load = member_llvm
        .find(".old = load %koven.string")
        .unwrap_or_else(|| {
            panic!("concrete String field is loaded before replacement:\n{member_llvm}")
        });
    let old_drop = member_llvm[old_load..]
        .find("call void @koven.drop")
        .map(|offset| old_load + offset)
        .expect("old concrete String owner is dropped");
    let replacement_store = member_llvm[old_drop..]
        .find("store %koven.string")
        .map(|offset| old_drop + offset)
        .expect("new concrete String owner is stored after the old drop");
    assert!(
        old_load < old_drop && old_drop < replacement_store,
        "{member_llvm}"
    );
}

#[test]
fn direct_slot_generic_receiver_keeps_concrete_int_replacement_trivial() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Cell<T>(var item: T) {\n\
             inout fun set(own replacement: T): Unit {\n\
                 val ignored: Unit = (this.item = replacement)\n\
             }\n\
         }\n\
         fun entry(): Unit {\n\
             val cell = Cell<Int>(1)\n\
             val ignored = cell.set(2)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("generic T replacement fact must remain valid for a concrete Copyable layout");

    let member = function(program.modules[0].functions.iter(), ".Cell.set.s");
    assert!(member.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapFieldReplace { field: 0, .. }
    )));
    let llvm = render_verified_program(&program).expect("generic Int replacement LLVM");
    let member_llvm = llvm
        .split("define internal void @f1.koven.p.Cell.set")
        .nth(1)
        .and_then(|body| body.split("define internal").next())
        .unwrap_or_else(|| panic!("generic member LLVM body:\n{llvm}"));
    assert!(member_llvm.contains("store i32"), "{member_llvm}");
    assert!(!member_llvm.contains(".old = load"), "{member_llvm}");
    assert!(
        !member_llvm.contains("call void @koven.drop"),
        "{member_llvm}"
    );
}

#[test]
fn nested_generic_receiver_replaces_a_concrete_wrapper_owner() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Wrapper<T>(val item: T)\n\
         class Holder<T>(var wrapped: Wrapper<T>) {\n\
             inout fun set(own replacement: Wrapper<T>): Unit {\n\
                 val ignored: Unit = (this.wrapped = replacement)\n\
             }\n\
         }\n\
         fun entry(): Unit {\n\
             val holder = Holder<String>(Wrapper<String>(\"old\" + \"-value\"))\n\
             val ignored = holder.set(Wrapper<String>(\"new\" + \"-value\"))\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("SPEC-0219 nested Wrapper<T> layout must lower by exact Holder<String> owner");

    let member = function(program.modules[0].functions.iter(), ".Holder.set.s");
    assert!(member.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapFieldReplace { field: 0, .. }
    )));
    let llvm = render_verified_program(&program).expect("nested Wrapper owner replacement LLVM");
    let member_llvm = llvm
        .split("define internal")
        .find(|body| {
            body.split('{')
                .next()
                .is_some_and(|header| header.contains("koven.p.Holder.set"))
        })
        .unwrap_or_else(|| panic!("nested generic member LLVM body:\n{llvm}"));
    let old_load = member_llvm
        .find(".old = load ptr")
        .unwrap_or_else(|| panic!("old Wrapper owner is loaded:\n{member_llvm}"));
    let old_drop = member_llvm[old_load..]
        .find("call void @koven.drop")
        .map(|offset| old_load + offset)
        .expect("old Wrapper owner is dropped");
    let replacement_store = member_llvm[old_drop..]
        .find("store ptr")
        .map(|offset| old_drop + offset)
        .expect("new Wrapper owner is stored after old drop");
    assert!(
        old_load < old_drop && old_drop < replacement_store,
        "{member_llvm}"
    );
}

#[test]
fn generic_nullable_receiver_replaces_pointer_like_values_with_conditional_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/nullable-field.ko",
        "package p\n\
         class Node\n\
         class Holder<T>(var item: T?) {\n\
             inout fun set(own replacement: T?): Unit { this.item = replacement }\n\
         }\n\
         fun entry(): Unit {\n\
             val holder = Holder<Node>(Node())\n\
             val cleared = holder.set(null)\n\
             val next: Node? = Node()\n\
             val replaced = holder.set(next)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/nullable-field.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("pointer-like nullable generic field must lower");

    let module = &program.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::NullableHandle { .. }))
            .count(),
        1
    );
    let entry = function(module.functions.iter(), ".entry");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableWrap { .. }))
            .count(),
        2
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableNull { .. }))
            .count(),
        1
    );
    let setter = function(module.functions.iter(), ".Holder.set.s");
    assert!(setter.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapFieldReplace { field: 0, .. }
    )));

    let llvm = render_verified_program(&program).expect("nullable field replacement LLVM");
    let setter_llvm = llvm
        .split("define internal")
        .find(|body| {
            body.split('{')
                .next()
                .is_some_and(|header| header.contains("koven.p.Holder.set"))
        })
        .unwrap_or_else(|| panic!("nullable setter LLVM body:\n{llvm}"));
    let old_load = setter_llvm
        .find(".old = load ptr")
        .unwrap_or_else(|| panic!("old nullable is loaded:\n{setter_llvm}"));
    let nullable_drop = setter_llvm[old_load..]
        .find("call void @koven.drop.")
        .map(|offset| old_load + offset)
        .unwrap_or_else(|| panic!("old nullable is conditionally dropped:\n{setter_llvm}"));
    let replacement_store = setter_llvm[nullable_drop..]
        .find("store ptr")
        .map(|offset| nullable_drop + offset)
        .expect("new nullable is stored after conditional drop");
    assert!(old_load < nullable_drop && nullable_drop < replacement_store);
    assert!(
        llvm.contains("is_null = icmp eq ptr") && llvm.contains("br i1 %is_null"),
        "nullable drop glue must check the niche before dropping:\n{llvm}"
    );
}

#[test]
fn divergent_rhs_does_not_emit_an_inout_class_payload_replace() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Cell(var item: Int) {\n\
             inout fun stop(): Unit = item = error(\"stop\")\n\
         }\n\
         fun entry(): Unit {\n\
             val cell = Cell(1)\n\
             val ignored = cell.stop()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("divergent field assignment must lower without a payload write");

    let member = function(program.modules[0].functions.iter(), ".Cell.stop.s");
    assert!(!member.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapFieldRead { .. } | Operation::HeapFieldReplace { .. }
    )));
    assert!(member.blocks.iter().any(|block| matches!(
        block.terminator.as_ref().map(|terminator| &terminator.kind),
        Some(TerminatorKind::Abort)
    )));
}
