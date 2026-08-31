use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{EntityId, EntityType, Function, LoanKind, Operation, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::llvm::render_verified_program;

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
fn borrow_delegation_projects_one_heap_field_loan_and_forwards_it_directly() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(message: String): Int }\n\
         class Reader: Readable { override fun read(message: String): Int = 7 }\n\
         class Host(val tag: Int, val delegate: Reader): Readable by delegate {}\n\
         fun entry(): Int = Host(0, Reader()).read(\"argument\")",
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
    .expect("validated Borrow delegation must lower");

    let module = &program.modules[0];
    let entry = function(module.functions.iter(), ".entry.d");
    let reader = function(module.functions.iter(), ".Reader.read.s");
    let field_loans = entry
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedHeapFieldLoan { base, field: 1 } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    panic!("heap field loan result");
                };
                Some((base, *result))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(field_loans.len(), 1);
    let (outer, delegate) = field_loans[0];
    let (call, argument) = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(receiver)),
                arguments,
            } if *callee == reader.id() && *receiver == delegate => {
                let [EntityId::Loan(argument)] = arguments.as_slice() else {
                    panic!("one Borrow argument");
                };
                Some((instruction, *argument))
            }
            _ => None,
        })
        .expect("delegate implementation direct call");
    let field_loan_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. })
        })
        .expect("field loan index");
    let call_index = entry
        .instructions
        .iter()
        .position(|instruction| instruction.id == call.id)
        .expect("call index");
    let argument_begin_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowBegin { .. })
                && instruction.results == [EntityId::Loan(argument)]
        })
        .expect("argument loan begin");
    let argument_end_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == argument)
        })
        .expect("argument loan end");
    let delegate_end_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == delegate)
        })
        .expect("delegate field loan end");
    let outer_end_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == outer)
        })
        .expect("temporary outer receiver loan end");
    assert!(
        field_loan_index < argument_begin_index
            && argument_begin_index < call_index
            && call_index < argument_end_index
            && argument_end_index < delegate_end_index
            && delegate_end_index < outer_end_index
    );
    assert_ne!(outer, delegate);
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapFieldRead { .. }))
            .count(),
        0,
        "delegation must forward the field place, not read/copy the delegate owner"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .count(),
        2,
        "only the source Reader and Host constructions may allocate"
    );
    assert!(entry.instructions.iter().all(|instruction| !matches!(
        instruction.operation,
        Operation::Copy { .. } | Operation::SharedAllocate { .. } | Operation::SharedRetain { .. }
    )));
    render_verified_program(&program).expect("delegation route must lower to LLVM");
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
fn stateless_object_receiver_uses_zst_addressization_without_runtime_storage() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         object Registry { fun ping(): Int = 7 }\n\
         fun entry(): Int = (Registry).ping()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(
        owned.ownership().drops().is_empty(),
        "stateless object receiver has no runtime drop: {:?}",
        owned.ownership().drops()
    );
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("stateless object Borrow receiver must lower through a temporary ZST address");

    let module = &program.modules[0];
    let member = function(module.functions.iter(), ".Registry.ping.s");
    assert!(matches!(
        member.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    let entry = function(module.functions.iter(), ".entry.d");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::AggregateConstruct { ref fields, .. } if fields.is_empty()
            ))
            .count(),
        1,
        "the object receiver is materialized once as a ZST value"
    );
    assert!(entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::BorrowBegin {
            kind: LoanKind::Shared,
            ..
        }
    )));
    assert!(entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(_)),
            ..
        }
    )));
    assert!(!entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapAllocate { .. }
            | Operation::SharedAllocate { .. }
            | Operation::SharedRetain { .. }
            | Operation::Drop { .. }
    )));

    let llvm = render_verified_program(&program).expect("object ZST receiver must lower to LLVM");
    assert!(llvm.contains("alloca"), "{llvm}");
    assert!(!llvm.contains("@malloc"), "{llvm}");
    assert!(!llvm.contains(" global "), "{llvm}");
}

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

#[test]
fn lowers_borrow_member_receiver_before_explicit_arguments() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         value class Counter(val item: Int) {\n\
             fun answer(own delta: Int): Int = item + delta\n\
         }\n\
         fun entry(): Int = Counter(1).answer(40)",
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
    .expect("Borrow member call must lower to verified SSA");

    let module = &program.modules[0];
    let member = function(module.functions.iter(), ".Counter.answer.s");
    assert!(matches!(
        member.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    assert!(matches!(member.blocks[0].parameters[0], EntityId::Loan(_)));
    assert!(
        member
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::SharedFieldLoan { .. }))
    );

    let entry = function(module.functions.iter(), ".entry.d");
    let operations = entry
        .instructions
        .iter()
        .map(|instruction| &instruction.operation)
        .collect::<Vec<_>>();
    let call = operations
        .iter()
        .position(|operation| matches!(operation, Operation::DirectCall { .. }))
        .expect("member direct call");
    let borrow = operations
        .iter()
        .position(|operation| matches!(operation, Operation::BorrowBegin { .. }))
        .expect("receiver borrow begins");
    let argument = operations[..call]
        .iter()
        .rposition(|operation| matches!(operation, Operation::Constant(_)))
        .expect("explicit argument is evaluated");
    assert!(matches!(
        operations[borrow - 1],
        Operation::RootPlace { .. }
    ));
    assert!(matches!(
        operations[borrow],
        Operation::BorrowBegin {
            kind: LoanKind::Shared,
            ..
        }
    ));
    assert!(borrow < argument && argument < call);
    assert!(matches!(
        operations[call],
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(_)),
            arguments,
            ..
        } if matches!(arguments.as_slice(), [EntityId::Value(_)])
    ));
    assert!(matches!(operations[call + 1], Operation::BorrowEnd { .. }));

    let rendered = render_program(&program);
    assert!(rendered.contains("receiver %l0"), "{rendered}");
    let llvm = render_verified_program(&program).expect("Borrow receiver must lower to LLVM");
    assert!(llvm.contains("Counter.answer"), "{llvm}");
    assert!(llvm.contains("ptr %l0, i32 %v0"), "{llvm}");
    assert!(llvm.contains("(ptr %p0, i32 40)"), "{llvm}");
}

#[test]
fn forwards_implicit_this_loan_without_readdressing_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         value class Counter(val item: Int) {\n\
             fun answer(own delta: Int): Int = item + delta\n\
             fun relay(own delta: Int): Int = answer(delta)\n\
         }\n\
         fun entry(): Int {\n\
             val counter = Counter(1)\n\
             return counter.relay(40)\n\
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
    .expect("stable and implicit Borrow receivers must lower");

    let module = &program.modules[0];
    let relay = function(module.functions.iter(), ".Counter.relay.s");
    let EntityId::Loan(this_loan) = relay.blocks[0].parameters[0] else {
        panic!("relay receiver must be a shared loan");
    };
    assert!(!relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::RootPlace { .. } | Operation::BorrowBegin { .. }
    )));
    assert!(relay.instructions.iter().any(|instruction| matches!(
        &instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            arguments,
            ..
        } if *receiver == this_loan && matches!(arguments.as_slice(), [EntityId::Value(_)])
    )));

    let entry = function(module.functions.iter(), ".entry.d");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
            .count(),
        1
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
            .count(),
        1
    );
    let llvm = render_verified_program(&program).expect("forwarded this loan must lower to LLVM");
    assert!(llvm.contains("Counter.relay"), "{llvm}");
    assert!(llvm.contains("Counter.answer"), "{llvm}");
}

#[test]
fn forwards_explicit_borrow_binding_as_member_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource { fun answer(): Int = 41 }\n\
         fun relay(resource: Resource): Int = resource.answer()\n\
         fun entry(): Int {\n\
             val resource = Resource()\n\
             return relay(resource)\n\
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
    .expect("Borrow parameter loan must be reusable as a Borrow member receiver");

    let relay = function(program.modules[0].functions.iter(), ".relay.d");
    let EntityId::Loan(parameter) = relay.blocks[0].parameters[0] else {
        panic!("relay parameter must be a shared loan");
    };
    assert!(relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            ..
        } if receiver == parameter
    )));
    assert!(!relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::RootPlace { .. } | Operation::BorrowBegin { .. }
    )));
    render_verified_program(&program).expect("forwarded Borrow parameter must lower to LLVM");
}

#[test]
fn value_this_can_borrow_for_an_implicit_member_call() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             fun answer(): Int = 41\n\
             own fun relay(): Int = answer()\n\
         }\n\
         fun entry(): Int = Resource().relay()",
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
    .expect("Value this must create a call-scoped shared loan for Borrow member calls");

    let relay = function(program.modules[0].functions.iter(), ".Resource.relay.s");
    assert!(relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::BorrowBegin {
            kind: LoanKind::Shared,
            ..
        }
    )));
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
    );
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    render_verified_program(&program).expect("Value-to-Borrow receiver loan must lower to LLVM");
}

#[test]
fn inout_this_reborrows_shared_for_an_implicit_member_call() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             fun answer(): Int = 41\n\
             inout fun relay(): Int = answer()\n\
         }\n\
         fun entry(): Int {\n\
             var resource = Resource()\n\
             return resource.relay()\n\
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
    .expect("Inout this must create a call-scoped shared reborrow");

    let relay = function(program.modules[0].functions.iter(), ".Resource.relay.s");
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::SharedReborrow { .. }))
    );
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
    );
    render_verified_program(&program).expect("exclusive-to-shared reborrow must lower to LLVM");
}

#[test]
fn borrow_class_receiver_preserves_owner_until_post_call_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource(val id: Int) {\n\
             fun answer(): Int = 41\n\
         }\n\
         fun entry(): Int {\n\
             val resource = Resource(1)\n\
             return resource.answer()\n\
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
    .expect("Borrow class receiver must preserve and later drop its owner");

    let entry = function(program.modules[0].functions.iter(), ".entry.d");
    let call = entry
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .expect("member call");
    let borrow_end = entry
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
        .expect("receiver loan end");
    let drop = entry
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .expect("class owner drop");
    assert!(call < borrow_end && borrow_end < drop);
    let llvm = render_verified_program(&program).expect("Borrow class receiver must lower to LLVM");
    assert!(llvm.contains("Resource.answer"), "{llvm}");
    assert!(llvm.contains("call void @free"), "{llvm}");
}

#[test]
fn inout_class_receiver_uses_exclusive_call_scoped_loan() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             inout fun touch(): Unit {}\n\
         }\n\
         fun entry(): Unit {\n\
             var resource = Resource()\n\
             val result = resource.touch()\n\
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
    .expect("Inout class receiver must lower as an exclusive call-scoped loan");

    let module = &program.modules[0];
    let touch = function(module.functions.iter(), ".Resource.touch.s");
    assert!(matches!(
        touch.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            ..
        })
    ));

    let entry = function(module.functions.iter(), ".entry.d");
    let operations = entry
        .instructions
        .iter()
        .map(|instruction| &instruction.operation)
        .collect::<Vec<_>>();
    let call = operations
        .iter()
        .position(|operation| matches!(operation, Operation::DirectCall { .. }))
        .expect("member call");
    assert!(matches!(
        operations[call - 1],
        Operation::BorrowBegin {
            kind: LoanKind::Exclusive,
            ..
        }
    ));
    assert!(matches!(
        operations[call],
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(_)),
            ..
        }
    ));
    assert!(matches!(operations[call + 1], Operation::BorrowEnd { .. }));
    assert!(
        operations[call + 2..]
            .iter()
            .any(|operation| matches!(operation, Operation::Drop { .. }))
    );

    let llvm = render_verified_program(&program).expect("Inout receiver must lower to LLVM");
    assert!(llvm.contains("Resource.touch"), "{llvm}");
}

#[test]
fn value_receiver_reuses_copyable_inline_value() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         value class Counter(val item: Int) {\n\
             own fun add(own delta: Int): Int = item + delta\n\
         }\n\
         fun entry(): Int {\n\
             val counter = Counter(1)\n\
             val first = counter.add(1)\n\
             return counter.add(first)\n\
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
    .expect("Value receiver must preserve a Copyable inline value");

    let module = &program.modules[0];
    let add = function(module.functions.iter(), ".Counter.add.s");
    assert!(matches!(add.receiver(), Some(EntityType::Value(_))));
    assert!(
        add.instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::AggregateProject { .. }))
    );

    let entry = function(module.functions.iter(), ".entry.d");
    let calls = entry
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                receiver: Some(receiver),
                ..
            } => Some(*receiver),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert!(matches!(calls[0], EntityId::Value(_)));
    assert_eq!(calls[0], calls[1], "Copyable receiver remains reusable");
    render_verified_program(&program).expect("Value receiver ABI must lower to LLVM");
}

#[test]
fn value_receiver_moves_class_owner_to_callee_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource(val id: Int) {\n\
             own fun finish(): Int = 40\n\
         }\n\
         fun entry(): Int {\n\
             val resource = Resource(1)\n\
             return resource.finish()\n\
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
    .expect("MoveOnly class receiver must transfer to the callee");

    let module = &program.modules[0];
    let finish = function(module.functions.iter(), ".Resource.finish.s");
    assert!(matches!(finish.receiver(), Some(EntityType::Value(_))));
    assert_eq!(
        finish
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "callee owns and drops the moved class receiver"
    );
    let entry = function(module.functions.iter(), ".entry.d");
    assert!(entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Value(_)),
            ..
        }
    )));
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        0,
        "moved class receiver has no caller-side drop"
    );
    render_verified_program(&program).expect("moved Value receiver must lower to LLVM");
}

#[test]
fn value_receiver_can_return_this_without_callee_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             own fun pass(): Resource = this\n\
         }\n\
         fun entry(): Resource = Resource().pass()",
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
    .expect("return this must transfer the Value receiver exactly once");

    let pass = function(program.modules[0].functions.iter(), ".Resource.pass.s");
    assert!(matches!(pass.receiver(), Some(EntityType::Value(_))));
    assert!(
        !pass
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    render_verified_program(&program).expect("returned receiver owner must lower to LLVM");
}

#[test]
fn value_receiver_is_carried_and_dropped_on_each_conditional_exit() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             own fun choose(flag: Boolean): Int = if (flag) 1 else 2\n\
         }\n\
         fun entry(): Int = Resource().choose(true)",
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
    .expect("Value receiver must remain linear across conditional CFG edges");

    let choose = function(program.modules[0].functions.iter(), ".Resource.choose.s");
    assert_eq!(
        choose
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "both normal branches merge the still-owned receiver before its control-transfer drop"
    );
    render_verified_program(&program).expect("conditional receiver drops must lower to LLVM");
}

#[test]
fn value_receiver_is_rebound_across_while_edges() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             own fun wait(flag: Boolean): Int {\n\
                 while (flag) {}\n\
                 return 1\n\
             }\n\
         }\n\
         fun entry(): Int = Resource().wait(false)",
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
    .expect("Value receiver identity must be rebound on loop header, body and false edges");

    let wait = function(program.modules[0].functions.iter(), ".Resource.wait.s");
    assert!(wait.blocks.len() >= 4);
    assert_eq!(
        wait.instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1
    );
    render_verified_program(&program).expect("loop-carried receiver must lower to LLVM");
}

fn function<'a>(
    mut functions: impl Iterator<Item = &'a Function>,
    name_fragment: &str,
) -> &'a Function {
    functions
        .find(|function| function.name.contains(name_fragment))
        .expect("planned function exists")
}
