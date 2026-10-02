use super::*;

#[test]
fn inline_inout_read_only_receivers_use_exclusive_call_storage() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/inline-inout-receiver.ko",
        "package p\n\
         value class Counter(var item: Int) {\n\
             inout fun read(): Int = item\n\
         }\n\
         enum class Signal {\n\
             Ready;\n\
             inout fun code(): Int = 9\n\
         }\n\
         fun entry(): Int {\n\
             var counter = Counter(7)\n\
             var signal = Signal.Ready\n\
             return counter.read() + signal.code()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/inline-inout-receiver.ko",
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
    .expect("inline Inout receivers must lower as exclusive storage loans");

    let module = &program.modules[0];
    let read = function(module.functions.iter(), ".Counter.read.s");
    let counter_type = match read.receiver() {
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            target,
        }) => target,
        other => panic!("value-class Inout receiver must be exclusive: {other:?}"),
    };
    assert!(matches!(
        module.types.get(counter_type.index()),
        Some(SsaTypeKind::Aggregate { .. })
    ));
    let reborrow = read
        .instructions
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::SharedReborrow { .. }, [EntityId::Loan(loan)]) => Some(*loan),
                _ => None,
            },
        )
        .expect("exclusive receiver shared reborrow");
    let field_loan = read
        .instructions
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::SharedFieldLoan { base, .. }, [EntityId::Loan(field_loan)])
                    if *base == reborrow =>
                {
                    Some(*field_loan)
                }
                _ => None,
            },
        )
        .expect("field loan derived from the short shared reborrow");
    let relevant_operations = read
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedReborrow { .. } => Some("reborrow"),
            Operation::SharedFieldLoan { base, .. } if base == reborrow => Some("field-loan"),
            Operation::Read {
                source: PlaceAccess::Loan(loan),
            } if loan == field_loan => Some("read"),
            Operation::BorrowEnd { loan } if loan == field_loan => Some("end-field"),
            Operation::BorrowEnd { loan } if loan == reborrow => Some("end-reborrow"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relevant_operations,
        [
            "reborrow",
            "field-loan",
            "read",
            "end-field",
            "end-reborrow",
        ]
    );

    let code = function(module.functions.iter(), ".Signal.code.s");
    let signal_type = match code.receiver() {
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            target,
        }) => target,
        other => panic!("enum Inout receiver must be exclusive: {other:?}"),
    };
    assert!(matches!(
        module.types.get(signal_type.index()),
        Some(SsaTypeKind::TaggedUnion { .. })
    ));

    let entry = function(module.functions.iter(), ".entry.d");
    let exclusive_receivers = entry
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall {
                receiver: Some(EntityId::Loan(receiver)),
                ..
            } if matches!(
                entry
                    .entity(EntityId::Loan(receiver))
                    .map(|entity| entity.ty),
                Some(EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    ..
                })
            ) =>
            {
                Some(receiver)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(exclusive_receivers.len(), 2);
    let receiver_targets = exclusive_receivers
        .iter()
        .map(|receiver| {
            let place = entry
                .instructions
                .iter()
                .find_map(|instruction| {
                    match (&instruction.operation, instruction.results.as_slice()) {
                        (
                            Operation::BorrowBegin {
                                place,
                                kind: LoanKind::Exclusive,
                            },
                            [EntityId::Loan(actual)],
                        ) if actual == receiver => Some(*place),
                        _ => None,
                    }
                })
                .expect("exclusive receiver BorrowBegin");
            let owner = entry
                .instructions
                .iter()
                .find_map(|instruction| {
                    match (&instruction.operation, instruction.results.as_slice()) {
                        (Operation::RootPlace { owner }, [EntityId::Place(actual)])
                            if *actual == place =>
                        {
                            Some(*owner)
                        }
                        _ => None,
                    }
                })
                .expect("receiver call storage RootPlace");
            let EntityType::Place(target) = entry
                .entity(EntityId::Place(place))
                .expect("receiver place")
                .ty
            else {
                panic!("receiver root must be a place");
            };
            assert_eq!(
                entry.entity(EntityId::Value(owner)).map(|entity| entity.ty),
                Some(EntityType::Value(target))
            );
            target
        })
        .collect::<Vec<_>>();
    assert_eq!(receiver_targets, [counter_type, signal_type]);
    render_verified_program(&program).expect("inline Inout receiver program must lower to LLVM");
}

#[test]
fn move_only_inline_inout_replaces_a_move_only_field() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/inline-inout-replacement.ko",
        "package p\n\
         value class Resource(val marker: Int, var item: String) {\n\
             inout fun set(): Unit { item = \"n\" + \"ew\" }\n\
         }\n\
         fun entry(): Unit {\n\
             var resource = Resource(1, \"old\")\n\
             val ignored = resource.set()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/inline-inout-replacement.ko",
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
    .expect("MoveOnly inline field replacement must consume the exact old-field fact");

    let module = &program.modules[0];
    let setter = function(module.functions.iter(), ".Resource.set.s");
    let replace = setter
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.operation,
                Operation::InlineFieldReplace { field: 1, .. }
            )
        })
        .expect("MoveOnly inline field replacement");
    let replacement = match setter.instructions[replace].operation {
        Operation::InlineFieldReplace { value, .. } => value,
        _ => unreachable!(),
    };
    assert!(setter.instructions[..replace].iter().any(|instruction| matches!(
        instruction.operation,
        Operation::StringConcat { .. } if instruction.results == [EntityId::Value(replacement)]
    )));

    let entry = function(module.functions.iter(), ".entry.d");
    let call = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.operation,
                Operation::DirectCall { callee, .. } if callee == setter.id()
            )
        })
        .expect("setter call");
    let receiver = match &entry.instructions[call].operation {
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            arguments,
            ..
        } => {
            assert!(arguments.is_empty(), "setter arguments: {arguments:?}");
            *receiver
        }
        ref other => panic!("MoveOnly setter receiver: {other:?}"),
    };
    let relevant = entry.instructions[call + 1..]
        .iter()
        .filter_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::BorrowEnd { loan }, []) if *loan == receiver => {
                    Some(("receiver-end", None))
                }
                (Operation::RootPlaceTake { .. }, [EntityId::Value(value)]) => {
                    Some(("take", Some(*value)))
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        relevant.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        ["receiver-end", "take"]
    );
    let rebound = relevant[1]
        .1
        .expect("MoveOnly caller must take the mutated root after receiver end");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::Drop { owner } if owner == rebound
            ))
            .count(),
        1
    );

    let llvm = render_verified_program(&program).expect("MoveOnly inline replacement LLVM");
    let setter_llvm = llvm
        .split("define internal void @f1.koven.p.Resource.set")
        .nth(1)
        .and_then(|body| body.split("define internal").next())
        .unwrap_or_else(|| panic!("setter LLVM body:\n{llvm}"));
    let old_load = setter_llvm
        .find(".old = load")
        .unwrap_or_else(|| panic!("old inline field load:\n{setter_llvm}"));
    let field_gep = setter_llvm
        .lines()
        .find(|line| line.contains("getelementptr") && line.contains("inline.replace"))
        .unwrap_or_else(|| panic!("inline replacement field GEP:\n{setter_llvm}"));
    assert!(field_gep.contains("i32 1"), "{field_gep}");
    let old_drop = setter_llvm[old_load..]
        .find("call void @koven.drop")
        .map(|offset| old_load + offset)
        .expect("old inline field drop");
    let replacement_store = setter_llvm[old_drop..]
        .find("store")
        .map(|offset| old_drop + offset)
        .expect("replacement store after old drop");
    assert!(
        old_load < old_drop && old_drop < replacement_store,
        "{setter_llvm}"
    );
}

#[test]
fn copyable_inline_inout_rebinds_the_mutated_value_after_the_call() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/copyable-inline-inout.ko",
        "package p\n\
         value class Counter(var item: Int) {\n\
             inout fun set(next: Int): Unit { item = next }\n\
             fun read(): Int = item\n\
         }\n\
         fun entry(): Int {\n\
             var counter = Counter(1)\n\
             val ignored = counter.set(7)\n\
             return counter.read()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/copyable-inline-inout.ko",
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
    .expect("Copyable inline Inout mutation must rebind the caller value");
    let module = &program.modules[0];
    let setter = function(module.functions.iter(), ".Counter.set.s");
    let EntityId::Loan(setter_receiver) = setter.blocks[0].parameters[0] else {
        panic!("setter receiver loan");
    };
    let EntityId::Loan(next) = setter.blocks[0].parameters[1] else {
        panic!("setter borrowed parameter");
    };
    let next_value = setter
        .instructions
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (
                    Operation::Read {
                        source: PlaceAccess::Loan(actual),
                    },
                    [EntityId::Value(value)],
                ) if *actual == next => Some(*value),
                _ => None,
            },
        )
        .expect("borrowed replacement read");
    assert!(setter.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::InlineFieldReplace {
            receiver,
            field: 0,
            value,
        } if receiver == setter_receiver && value == next_value
    )));

    let getter = function(module.functions.iter(), ".Counter.read.s");
    let entry = function(module.functions.iter(), ".entry.d");
    let set_call = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.operation,
                Operation::DirectCall { callee, .. } if callee == setter.id()
            )
        })
        .expect("setter call");
    let (receiver, argument, place) = match &entry.instructions[set_call].operation {
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            arguments,
            ..
        } => {
            let [EntityId::Loan(argument)] = arguments.as_slice() else {
                panic!("setter borrowed argument: {arguments:?}");
            };
            let place = entry.instructions[..set_call]
                .iter()
                .find_map(|instruction| {
                    match (&instruction.operation, instruction.results.as_slice()) {
                        (
                            Operation::BorrowBegin {
                                place,
                                kind: LoanKind::Exclusive,
                            },
                            [EntityId::Loan(actual)],
                        ) if actual == receiver => Some(*place),
                        _ => None,
                    }
                })
                .expect("setter receiver place");
            (*receiver, *argument, place)
        }
        ref other => panic!("setter call receiver: {other:?}"),
    };
    let post_call = entry.instructions[set_call + 1..]
        .iter()
        .take_while(|instruction| {
            !matches!(
                instruction.operation,
                Operation::DirectCall { callee, .. } if callee == getter.id()
            )
        })
        .collect::<Vec<_>>();
    let relevant = post_call
        .iter()
        .filter_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::BorrowEnd { loan }, []) if *loan == argument => {
                    Some(("argument-end", None))
                }
                (Operation::BorrowEnd { loan }, []) if *loan == receiver => {
                    Some(("receiver-end", None))
                }
                (
                    Operation::Read {
                        source: PlaceAccess::Place(actual),
                    },
                    [EntityId::Value(value)],
                ) if *actual == place => Some(("writeback", Some(*value))),
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        relevant.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        ["argument-end", "receiver-end", "writeback"]
    );
    let writeback = relevant[2].1.expect("writeback value");
    let getter_owner = entry
        .instructions
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::RootPlace { owner }, [EntityId::Place(_)]) if *owner == writeback => {
                    Some(*owner)
                }
                _ => None,
            },
        )
        .expect("getter must addressize the rebound value");
    assert_eq!(getter_owner, writeback);
    render_verified_program(&program).expect("Copyable inline mutation must lower to LLVM");
}

#[test]
fn move_only_inline_inout_rebinds_after_copyable_field_mutation() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/move-only-owner-copyable-field.ko",
        "package p\n\
         value class Resource(val owner: String, var generation: Int) {\n\
             inout fun set(next: Int): Unit { generation = next }\n\
             fun read(): Int = generation\n\
         }\n\
         fun entry(): Int {\n\
             var resource = Resource(\"owned\", 1)\n\
             val ignored = resource.set(7)\n\
             return resource.read()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/move-only-owner-copyable-field.ko",
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
    .expect("MoveOnly inline owner must allow Copyable field mutation");
    let module = &program.modules[0];
    let setter = function(module.functions.iter(), ".Resource.set.s");
    let EntityId::Loan(next) = setter.blocks[0].parameters[1] else {
        panic!("setter borrowed replacement");
    };
    let next_value = setter
        .instructions
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (
                    Operation::Read {
                        source: PlaceAccess::Loan(actual),
                    },
                    [EntityId::Value(value)],
                ) if *actual == next => Some(*value),
                _ => None,
            },
        )
        .expect("borrowed Copyable replacement read");
    assert!(setter.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::InlineFieldReplace {
            field: 1,
            value,
            ..
        } if value == next_value
    )));

    let getter = function(module.functions.iter(), ".Resource.read.s");
    let entry = function(module.functions.iter(), ".entry.d");
    let set_call = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.operation,
                Operation::DirectCall { callee, .. } if callee == setter.id()
            )
        })
        .expect("setter call");
    let (receiver, argument, original, place) = match &entry.instructions[set_call].operation {
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            arguments,
            ..
        } => {
            let [EntityId::Loan(argument)] = arguments.as_slice() else {
                panic!("setter Borrow argument: {arguments:?}");
            };
            let place = entry.instructions[..set_call]
                .iter()
                .find_map(|instruction| {
                    match (&instruction.operation, instruction.results.as_slice()) {
                        (
                            Operation::BorrowBegin {
                                place,
                                kind: LoanKind::Exclusive,
                            },
                            [EntityId::Loan(actual)],
                        ) if actual == receiver => Some(*place),
                        _ => None,
                    }
                })
                .expect("MoveOnly setter receiver place");
            let original = entry.instructions[..set_call]
                .iter()
                .find_map(|instruction| {
                    match (&instruction.operation, instruction.results.as_slice()) {
                        (Operation::RootPlace { owner }, [EntityId::Place(actual)])
                            if *actual == place =>
                        {
                            Some(*owner)
                        }
                        _ => None,
                    }
                })
                .expect("MoveOnly setter original owner");
            (*receiver, *argument, original, place)
        }
        other => panic!("MoveOnly setter receiver: {other:?}"),
    };
    let relevant = entry.instructions[set_call + 1..]
        .iter()
        .filter_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::BorrowEnd { loan }, []) if *loan == argument => {
                    Some(("argument-end", None))
                }
                (Operation::BorrowEnd { loan }, []) if *loan == receiver => {
                    Some(("receiver-end", None))
                }
                (
                    Operation::RootPlaceTake {
                        owner,
                        place: actual,
                    },
                    [EntityId::Value(value)],
                ) if *owner == original && *actual == place => Some(("take", Some(*value))),
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        relevant.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        ["argument-end", "receiver-end", "take"]
    );
    let rebound = relevant[2]
        .1
        .expect("receiver-end followed by same-root MoveOnly take");
    assert!(entry.instructions.iter().any(|instruction| matches!(
        (&instruction.operation, instruction.results.as_slice()),
        (Operation::RootPlace { owner }, [EntityId::Place(_)]) if *owner == rebound
    )));
    assert!(entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall { callee, .. } if callee == getter.id()
    )));
    assert!(!entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Drop { owner } if owner == original
    )));
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::Drop { owner } if owner == rebound
            ))
            .count(),
        1
    );
    render_verified_program(&program)
        .expect("MoveOnly owner with Copyable inline mutation must lower to LLVM");
}

#[test]
fn move_only_inline_inout_read_takes_the_same_root_back_after_the_call() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/move-only-inline-inout-read.ko",
        "package p\n\
         value class Resource(val owner: String) {\n\
             inout fun inspect(): Int = 7\n\
         }\n\
         fun entry(): Int {\n\
             var resource = Resource(\"owned\")\n\
             return resource.inspect()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/move-only-inline-inout-read.ko",
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
    .expect("MoveOnly inline Inout caller must take the same root back after the loan ends");

    let module = &program.modules[0];
    let inspect = function(module.functions.iter(), ".Resource.inspect.s");
    let entry = function(module.functions.iter(), ".entry.d");
    let call = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.operation,
                Operation::DirectCall { callee, .. } if callee == inspect.id()
            )
        })
        .expect("inspect call");
    let receiver = match entry.instructions[call].operation {
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            ..
        } => receiver,
        ref other => panic!("MoveOnly inspect receiver: {other:?}"),
    };
    let (original, place) = entry.instructions[..call]
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::RootPlace { owner }, [EntityId::Place(place)]) => {
                    Some((*owner, *place))
                }
                _ => None,
            },
        )
        .expect("MoveOnly receiver root place");
    let relevant = entry.instructions[call + 1..]
        .iter()
        .filter_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::BorrowEnd { loan }, []) if *loan == receiver => {
                    Some(("receiver-end", None))
                }
                (
                    Operation::RootPlaceTake {
                        owner,
                        place: actual,
                    },
                    [EntityId::Value(value)],
                ) if *owner == original && *actual == place => Some(("take", Some(*value))),
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        relevant.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        ["receiver-end", "take"]
    );
    let rebound = relevant[1].1.expect("MoveOnly rebound owner");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::Drop { owner } if owner == rebound
            ))
            .count(),
        1
    );
    assert!(!entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Drop { owner } if owner == original
    )));
    render_verified_program(&program).expect("MoveOnly root-place take must lower to LLVM");
}

#[test]
fn inline_inout_this_forwards_the_existing_exclusive_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/inline-inout-forward.ko",
        "package p\n\
         value class Counter(var item: Int) {\n\
             inout fun set(own next: Int): Unit { item = next }\n\
             inout fun forward(own next: Int): Unit { val ignored = set(next) }\n\
             fun read(): Int = item\n\
         }\n\
         fun entry(): Int {\n\
             var counter = Counter(1)\n\
             val ignored = counter.forward(7)\n\
             return counter.read()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/inline-inout-forward.ko",
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
    .expect("Inout this must forward its existing exclusive receiver");

    let module = &program.modules[0];
    let setter = function(module.functions.iter(), ".Counter.set.s");
    let forward = function(module.functions.iter(), ".Counter.forward.s");
    let EntityId::Loan(receiver) = forward.blocks[0].parameters[0] else {
        panic!("forward receiver loan");
    };
    assert!(forward.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(actual)),
            ..
        } if callee == setter.id() && actual == receiver
    )));
    assert!(!forward.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::RootPlace { .. }
            | Operation::BorrowBegin { .. }
            | Operation::Read {
                source: PlaceAccess::Place(_),
            }
    )));
    render_verified_program(&program).expect("forwarded inline Inout receiver must lower to LLVM");
}
