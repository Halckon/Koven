use super::*;

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
