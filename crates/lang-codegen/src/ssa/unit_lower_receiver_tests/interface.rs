use super::*;

#[test]
fn interface_default_receiver_is_specialized_to_the_concrete_owner() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface DefaultValue { fun read(): Int = 7 }\n\
         class Holder: DefaultValue {}\n\
         fun entry(): Int = Holder().read()",
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
    .expect("interface default must specialize StaticSelf to the concrete Holder receiver");

    let module = &program.modules[0];
    let default = function(module.functions.iter(), ".DefaultValue.read.s");
    let default_target = match default.receiver() {
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        }) => target,
        other => panic!("specialized default receiver must be a shared Holder loan: {other:?}"),
    };
    let entry = function(module.functions.iter(), ".entry.d");
    let holder = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::HeapAllocate { owner, .. } => Some(owner),
            _ => None,
        })
        .expect("Holder construction owner type");
    assert_eq!(default_target, holder, "StaticSelf must be concrete Holder");
    let call_receiver = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::DirectCall {
                receiver: Some(EntityId::Loan(receiver)),
                ..
            } => Some(receiver),
            _ => None,
        })
        .expect("default direct-call receiver");
    assert_eq!(
        entry
            .entity(EntityId::Loan(call_receiver))
            .map(|data| data.ty),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target: holder,
        })
    );
    render_verified_program(&program).expect("specialized default receiver must lower to LLVM");
}

#[test]
fn interface_inout_default_preserves_exclusive_receiver_abi() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Mutable { inout fun probe(): Int = 7 }\n\
         class Counter(var count: Int): Mutable { fun read(): Int = count }\n\
         fun entry(): Int {\n\
             val counter = Counter(5)\n\
             val defaultValue = counter.probe()\n\
             return counter.read() + defaultValue\n\
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
    .expect("Inout interface default must lower");

    let module = &program.modules[0];
    let probe = function(module.functions.iter(), ".Mutable.probe.s");
    let counter = match probe.receiver() {
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            target,
        }) => target,
        other => panic!("Inout default must receive an exclusive Counter loan: {other:?}"),
    };
    let entry = function(module.functions.iter(), ".entry.d");
    let counter_owner = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::HeapAllocate { owner, .. } => Some(owner),
            _ => None,
        })
        .expect("Counter owner type");
    assert_eq!(
        counter, counter_owner,
        "StaticSelf must be concrete Counter"
    );
    assert!(entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(receiver)),
            ..
        } if callee == probe.id()
            && entry.entity(EntityId::Loan(receiver)).map(|entity| entity.ty)
                == Some(EntityType::Loan { kind: LoanKind::Exclusive, target: counter })
    )));
    render_verified_program(&program).expect("Inout default receiver must lower to LLVM");
}

#[test]
fn interface_default_instances_have_distinct_concrete_symbols() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         class First: Readable {}\n\
         class Second: Readable {}\n\
         fun entry(): Int = First().read() + Second().read()",
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
    .expect("each concrete StaticSelf must get a distinct default instance");

    let module = &program.modules[0];
    let defaults = module
        .functions
        .iter()
        .filter(|function| function.name.contains(".Readable.read.s"))
        .collect::<Vec<_>>();
    assert_eq!(defaults.len(), 2);
    assert_ne!(defaults[0].name, defaults[1].name);
    assert!(defaults.iter().all(|function| function.name.contains(".r")));
    assert_ne!(defaults[0].receiver(), defaults[1].receiver());
    let entry = function(module.functions.iter(), ".entry.d");
    let mut callees = entry
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } => Some(callee),
            _ => None,
        })
        .collect::<Vec<_>>();
    callees.sort_unstable();
    let mut defaults = defaults
        .iter()
        .map(|function| function.id())
        .collect::<Vec<_>>();
    defaults.sort_unstable();
    assert_eq!(callees, defaults);
    render_verified_program(&program).expect("distinct default symbols must lower to LLVM");
}

#[test]
fn super_interface_call_keeps_the_selected_default_and_concrete_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Parent { fun read(): Int = 7 }\n\
         class Child: Parent {\n\
             override fun read(): Int = super<Parent>.read()\n\
         }\n\
         fun entry(): Int = Child().read()",
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
    .expect("super<I> must statically call the selected default with concrete Self");

    let module = &program.modules[0];
    let parent = function(module.functions.iter(), ".Parent.read.s");
    let child = function(module.functions.iter(), ".Child.read.s");
    let entry = function(module.functions.iter(), ".entry.d");
    let child_target = match child.receiver() {
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        }) => target,
        other => panic!("Child override receiver: {other:?}"),
    };
    assert_eq!(
        parent.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target: child_target,
        })
    );
    let EntityId::Loan(child_receiver) = child.blocks[0].parameters[0] else {
        panic!("Child receiver loan")
    };
    assert!(child.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(receiver)),
            ..
        } if callee == parent.id() && receiver == child_receiver
    )));
    assert!(
        !child
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
    );
    assert!(entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall { callee, .. } if callee == child.id()
    )));
    let llvm = render_verified_program(&program).expect("super<I> direct call must lower to LLVM");
    assert!(!llvm.contains("vtable"), "{llvm}");
}

#[test]
fn interface_default_propagates_concrete_self_to_super_default() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base { fun read(): Int = 7 }\n\
         interface Derived: Base {\n\
             fun inherited(): Int = super<Base>.read()\n\
         }\n\
         class Child: Derived {}\n\
         fun entry(): Int = Child().inherited()",
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
    .expect("a default body must propagate concrete Self into super<Base>");

    let module = &program.modules[0];
    let base = function(module.functions.iter(), ".Base.read.s");
    let derived = function(module.functions.iter(), ".Derived.inherited.s");
    let base_target = match base.receiver() {
        Some(EntityType::Loan { target, .. }) => target,
        other => panic!("Base default receiver: {other:?}"),
    };
    assert_eq!(
        derived.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target: base_target,
        })
    );
    let EntityId::Loan(derived_receiver) = derived.blocks[0].parameters[0] else {
        panic!("Derived default receiver loan")
    };
    assert!(derived.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(receiver)),
            ..
        } if callee == base.id() && receiver == derived_receiver
    )));
    assert!(
        !derived
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
    );
    render_verified_program(&program).expect("nested default static calls must lower to LLVM");
}

#[test]
fn interface_default_propagates_concrete_self_through_explicit_this_call() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable {\n\
             fun read(): Int = 7\n\
             fun throughThis(): Int = this.read()\n\
         }\n\
         class Child: Readable {}\n\
         fun entry(): Int = Child().throughThis()",
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
    .expect("this.member() in a default body must retain concrete StaticSelf");

    let module = &program.modules[0];
    let read = function(module.functions.iter(), ".Readable.read.s");
    let through = function(module.functions.iter(), ".Readable.throughThis.s");
    assert_eq!(read.receiver(), through.receiver());
    let EntityId::Loan(receiver) = through.blocks[0].parameters[0] else {
        panic!("throughThis concrete receiver loan")
    };
    assert!(through.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(actual)),
            ..
        } if callee == read.id() && actual == receiver
    )));
    render_verified_program(&program).expect("explicit this default call must lower to LLVM");
}

#[test]
fn interface_default_dispatches_abstract_requirement_to_concrete_override() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         class Child: Readable {\n\
             override fun read(): Int = 7\n\
         }\n\
         fun entry(): Int = Child().throughRequirement()",
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
    .expect("abstract requirement target must statically resolve to Child.read");

    let module = &program.modules[0];
    let requirement = function(module.functions.iter(), ".Readable.throughRequirement.s");
    let implementation = function(module.functions.iter(), ".Child.read.s");
    let EntityId::Loan(receiver) = requirement.blocks[0].parameters[0] else {
        panic!("requirement default concrete receiver loan")
    };
    assert!(requirement.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(actual)),
            ..
        } if callee == implementation.id() && actual == receiver
    )));
    render_verified_program(&program).expect("resolved abstract requirement must lower to LLVM");
}

#[test]
fn interface_default_dispatches_ancestor_requirement_to_inherited_default() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived: Base { fun read(): Int = 7 }\n\
         class Child: Derived {}\n\
         fun entry(): Int = Child().throughRequirement()",
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
    .expect("ancestor requirement must resolve to the inherited Derived default");

    let module = &program.modules[0];
    let requirement = function(module.functions.iter(), ".Base.throughRequirement.s");
    let implementation = function(module.functions.iter(), ".Derived.read.s");
    let EntityId::Loan(receiver) = requirement.blocks[0].parameters[0] else {
        panic!("ancestor default concrete receiver loan")
    };
    assert!(requirement.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(actual)),
            ..
        } if callee == implementation.id() && actual == receiver
    )));
    render_verified_program(&program).expect("inherited default dispatch must lower to LLVM");
}

#[test]
fn dependent_inherited_owner_key_does_not_materialize_wrapper_layout() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/class-inherited.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(): Int = Host<Int>().throughRequirement()",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/class-inherited.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    let wrapper = declaration(&names, "p", "Wrapper");
    let wrapper_int = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: wrapper,
            arguments: vec![int],
        })
        .expect("frontend canonical Wrapper<Int>");
    let wrapper_ssa_name = format!("class#d{}.u{}", wrapper.index(), wrapper_int.index());

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("inherited owner key must not require Wrapper runtime layout");
    let module = &program.modules[0];
    assert!(
        module.types.iter().all(|kind| !matches!(
            kind,
            SsaTypeKind::HeapOwner { name, .. } if name == &wrapper_ssa_name
        )),
        "Wrapper<Int> must remain instance-key-only: {:?}",
        module.types
    );
    assert_eq!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .count(),
        1,
        "only Host<Int> is constructed"
    );
    let llvm = render_verified_program(&program).expect("class inherited owner LLVM");
    assert!(!llvm.contains(&wrapper_ssa_name), "{llvm}");
}

#[test]
fn dependent_inherited_runtime_demand_materializes_exact_wrapper_layout() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/dependent-runtime.ko",
        "package p\n\
         interface Base<A> { fun read(): Int }\n\
         interface Derived<B>: Base<String> {\n\
             fun read(): Int = 7\n\
             fun echo(own input: B): B = input\n\
         }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(): Int {\n\
             val result = Host<Int>().echo(Wrapper<Int>(7))\n\
             return result.item\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/dependent-runtime.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    let wrapper = declaration(&names, "p", "Wrapper");
    let wrapper_int = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: wrapper,
            arguments: vec![int],
        })
        .expect("frontend exact Wrapper<Int> identity");
    let wrapper_ssa_name = format!("class#d{}.u{}", wrapper.index(), wrapper_int.index());

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("runtime demand must materialize the exact dependent Wrapper<Int> layout");
    let payload = program.modules[0]
        .types
        .iter()
        .find_map(|kind| match kind {
            SsaTypeKind::HeapOwner {
                name,
                payload: Some(payload),
            } if name == &wrapper_ssa_name => Some(*payload),
            _ => None,
        })
        .expect("Wrapper<Int> exact heap payload");
    assert!(matches!(
        &program.modules[0].types[payload.index()],
        SsaTypeKind::Aggregate { fields, .. }
            if matches!(fields.as_slice(), [field]
                if matches!(program.modules[0].types[field.index()], SsaTypeKind::Integer { bits: 32, signed: true }))
    ));
    let llvm = render_verified_program(&program).expect("dependent runtime owner LLVM");
    assert!(llvm.contains("type { i32 }"), "{llvm}");
}
