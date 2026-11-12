// 所有 receiver 测试与共享 helper 仅通过 ssa/mod.rs 的 cfg(test) 入口编译。
use lang_frontend::{
    name_resolution::SourceUnitInput,
    source::SourceMap,
    type_checking::{BuiltinType, UnitTypeKind, standard_environments},
};

use super::{
    model::{
        EntityId, EntityType, Function, LoanKind, Operation, PlaceAccess, SsaTypeKind,
        TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::llvm::render_verified_program;

mod borrowing {
    use super::*;
    use crate::test_support::diagnostics::assert_diagnostic;

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
        assert_diagnostic!(rendered.contains("receiver %l0"), "{rendered}");
        let llvm = render_verified_program(&program).expect("Borrow receiver must lower to LLVM");
        assert_diagnostic!(llvm.contains("Counter.answer"), "{llvm}");
        assert_diagnostic!(llvm.contains("ptr %l0, i32 %v0"), "{llvm}");
        assert_diagnostic!(llvm.contains("(ptr %p0, i32 40)"), "{llvm}");
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
        assert_diagnostic!(llvm.contains("Counter.relay"), "{llvm}");
        assert_diagnostic!(llvm.contains("Counter.answer"), "{llvm}");
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
        assert_diagnostic!(llvm.contains("Resource.answer"), "{llvm}");
        assert_diagnostic!(llvm.contains("call void @free"), "{llvm}");
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
        assert_diagnostic!(llvm.contains("Resource.touch"), "{llvm}");
    }
}
mod delegation {
    use super::*;

    #[test]
    fn borrow_delegation_projects_one_heap_field_loan_and_forwards_it_directly() {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "p/main.ko",
            "package p\n\
             interface Readable { fun read(message: String): Int = 1 }\n\
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
    fn delegation_chain_projects_each_heap_field_loan_in_source_order() {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "p/main.ko",
            "package p\n\
             interface Readable { fun read(message: String): Int }\n\
             class Reader: Readable { override fun read(message: String): Int = 7 }\n\
             class Middle(val tag: Int, val reader: Reader): Readable by reader {}\n\
             class Host(val tag: Int, val middle: Middle): Readable by middle {}\n\
             fun entry(): Int = Host(0, Middle(1, Reader())).read(\"argument\")",
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
        .expect("same-requirement delegation chain must lower");

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
                    Some((instruction.id, base, *result))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [
            (outer_instruction, outer, middle),
            (inner_instruction, inner_base, reader_loan),
        ] = field_loans.as_slice()
        else {
            panic!("exactly two chained heap field loans: {field_loans:?}");
        };
        assert_eq!(
            middle, inner_base,
            "the second hop must derive from the first"
        );
        let (call, argument) = entry
            .instructions
            .iter()
            .find_map(|instruction| match &instruction.operation {
                Operation::DirectCall {
                    callee,
                    receiver: Some(EntityId::Loan(receiver)),
                    arguments,
                } if *callee == reader.id() && receiver == reader_loan => {
                    let [EntityId::Loan(argument)] = arguments.as_slice() else {
                        panic!("one Borrow argument");
                    };
                    Some((instruction.id, *argument))
                }
                _ => None,
            })
            .expect("chain endpoint direct call");
        let position = |id| {
            entry
                .instructions
                .iter()
                .position(|instruction| instruction.id == id)
                .expect("instruction position")
        };
        let end_position = |loan| {
            entry
                .instructions
                .iter()
                .position(|instruction| {
                    matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)
                })
                .expect("loan end")
        };
        assert!(
            position(*outer_instruction) < position(*inner_instruction)
                && position(*inner_instruction) < position(call)
                && position(call) < end_position(argument)
                && end_position(argument) < end_position(*reader_loan)
                && end_position(*reader_loan) < end_position(*middle)
                && end_position(*middle) < end_position(*outer)
        );
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::HeapFieldRead { .. }))
                .count(),
            0,
            "each hop must forward a field place instead of reading its owner value"
        );
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
                .count(),
            3,
            "only Reader, Middle and Host source constructions may allocate"
        );
        assert!(entry.instructions.iter().all(|instruction| !matches!(
            instruction.operation,
            Operation::Copy { .. } | Operation::SharedAllocate { .. } | Operation::SharedRetain { .. }
        )));
        render_verified_program(&program).expect("delegation loan chain must lower to LLVM");
    }
}
mod inout_fields;
mod inout_inline;
mod interface;
mod representation {
    use super::*;
    use crate::test_support::diagnostics::assert_diagnostic;

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
        assert_diagnostic!(llvm.contains("alloca"), "{llvm}");
        assert_diagnostic!(!llvm.contains("@malloc"), "{llvm}");
        assert_diagnostic!(!llvm.contains(" global "), "{llvm}");
    }

    #[test]
    fn enum_receivers_preserve_tagged_identity_for_borrow_and_value_modes() {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "p/enum-receiver.ko",
            "package p\n\
             enum class Signal {\n\
                 Ready;\n\
                 fun code(): Int = 7\n\
             }\n\
             enum class Owned {\n\
                 Full(text: String);\n\
                 own fun consume(): Int = 8\n\
             }\n\
             fun entry(): Int {\n\
                 val signal = Signal.Ready\n\
                 val borrowed = signal.code()\n\
                 val consumed = Owned.Full(\"owned\").consume()\n\
                 return borrowed + consumed\n\
             }",
        );
        let inputs = [SourceUnitInput::new(
            "root",
            "p/enum-receiver.ko",
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
        .expect("enum Borrow and Value receivers must lower with the root tagged identity");

        let module = &program.modules[0];
        let tagged = module
            .types
            .iter()
            .enumerate()
            .filter_map(|(index, ty)| matches!(ty, SsaTypeKind::TaggedUnion { .. }).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(tagged.len(), 2, "Signal and Owned tagged identities");
        let entry = function(module.functions.iter(), ".entry");
        let receiver_types = entry
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::DirectCall {
                    receiver: Some(receiver),
                    ..
                } => entry
                    .entity(receiver)
                    .map(|entity| entity.ty.semantic_type().index()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(receiver_types.len(), 2);
        assert!(tagged.contains(&receiver_types[0]));
        assert!(tagged.contains(&receiver_types[1]));
        assert_ne!(receiver_types[0], receiver_types[1]);
        render_verified_program(&program).expect("enum receiver program must lower to LLVM");
    }
}
mod value_delivery;

fn function<'a>(
    mut functions: impl Iterator<Item = &'a Function>,
    name_fragment: &str,
) -> &'a Function {
    functions
        .find(|function| function.name.contains(name_fragment))
        .expect("planned function exists")
}
