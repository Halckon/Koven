//! Original nullable scalar Copy contracts, with and without an additional call loan.
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey},
    source::SourceMap,
    type_checking::standard_environments,
};

use super::{
    model::{
        Block, Edge, EntityId, EntityType, Function, Instruction, LoanKind, Module, Operation,
        PlaceAccess, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
    },
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
};

#[test]
fn original_nullable_when_copy_under_shared_loan() {
    let original = include_str!(
        "../../../lang-frontend/tests/ownership_nullable_when/fixtures/copy-while-shared.ko"
    );
    check_original_copy(
        original,
        "nullable-copy-while-shared.ko",
        |module, test, sources| {
            assert_scalar_copy_during_call_loan(
                module,
                test,
                function(module, sources, "fun combine("),
            );
        },
    );
}

#[test]
fn original_nullable_when_body_copy_reaches_native() {
    let original = include_str!(
        "../../../lang-frontend/tests/ownership_nullable_when/fixtures/copy-when-body.ko"
    );
    check_original_copy(original, "copy-when-body.ko", |module, test, _| {
        assert_body_copy(module, test)
    });
}

fn check_original_copy(original: &str, path: &str, check: impl Fn(&Module, &Function, &SourceMap)) {
    let text = format!(
        "{original}\nfun entry(): Unit {{ val absent: Int? = null; val present: Int? = 9; if (test(absent) == 0 && test(present) == 9) {{ println(\"ok\") }} else {{ error(\"wrong result\") }} }}"
    );
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, path, &text);
    assert_eq!(
        sources.source_text(source).unwrap().get(..original.len()),
        Some(original)
    );
    let (name_environment, environment) = standard_environments();
    for (input, rooted) in [
        (SourceUnitInput::memory_document(source, &file), false),
        (SourceUnitInput::new("root", path, source, &file), true),
    ] {
        let inputs = [input];
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let index = names.names().index();
        let [unit] = index.source_units() else {
            panic!("one actual source");
        };
        assert_eq!(unit.source_id(), source);
        assert_eq!(matches!(unit.key(), SourceUnitKey::MemoryDocument), !rooted);
        let entry = index
            .declarations()
            .iter()
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .expect("exact nullable source");
        super::verify::verify_program(&program).unwrap();
        let module = &program.modules[entry.module().index()];
        let test = function(module, &sources, "fun test(");
        let wrapper = module.function(entry).unwrap();
        assert_eq!(
            wrapper.instructions.iter().filter(|instruction| {
                matches!(instruction.operation, Operation::DirectCall { callee, .. } if callee == test.id())
            }).count(),
            2,
        );
        check(module, test, &sources);
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        super::unit_field_replace_tests::link_and_run(&llvm, "", b"ok\n");
    }
}

fn function<'a>(module: &'a Module, sources: &SourceMap, prefix: &str) -> &'a Function {
    let mut functions = module.functions.iter().filter(|function| {
        sources
            .slice(function.origin.span())
            .unwrap()
            .starts_with(prefix)
    });
    let result = functions.next().expect("original reachable declaration");
    assert!(functions.next().is_none());
    result
}

fn assert_body_copy(module: &Module, test: &Function) {
    let enter = test.block(test.entry_block().unwrap()).unwrap();
    let [EntityId::Loan(root)] = enter.parameters.as_slice() else {
        panic!("the original parameter is a borrow");
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: nullable,
    } = test.entity(EntityId::Loan(*root)).unwrap().ty
    else {
        panic!("the original parameter remains shared")
    };
    let SsaTypeKind::InlineNullable {
        inner,
        niche: false,
    } = module.types[nullable.index()]
    else {
        panic!("the original Int? is an inline scalar")
    };
    assert_eq!(
        module.types[inner.index()],
        SsaTypeKind::Integer {
            bits: 32,
            signed: true
        }
    );
    let [read]: [_; 1] = operations(test, enter).try_into().expect("one root read");
    assert_eq!(
        read.operation,
        Operation::Read {
            source: PlaceAccess::Loan(*root)
        }
    );
    let snapshot = value_result(read);
    assert_eq!(
        test.entity(EntityId::Value(snapshot)).unwrap().ty,
        EntityType::Value(nullable)
    );

    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &enter.terminator.as_ref().unwrap().kind
    else {
        panic!("the nullable snapshot controls the split")
    };
    assert_eq!(*owner, snapshot);
    assert_ne!(when_null.target, when_non_null.target);
    let absent = test.block(when_null.target).unwrap();
    let [zero]: [_; 1] = operations(test, absent).try_into().expect("one zero");
    assert_eq!(
        zero.operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    let present = test.block(when_non_null.target).unwrap();
    let [copy]: [_; 1] = operations(test, present)
        .try_into()
        .expect("one payload extraction");
    let EntityId::Value(copied_owner) = carried(test, when_non_null, EntityId::Value(snapshot))
    else {
        panic!("the present arm operates on the copied value, never its borrowed source")
    };
    assert_eq!(
        copy.operation,
        Operation::NullableTake {
            owner: copied_owner,
            proof: *view
        }
    );
    assert_eq!(
        test.entity(EntityId::Value(value_result(copy))).unwrap().ty,
        EntityType::Value(inner)
    );

    let absent_exit = next(absent);
    let present_exit = next(present);
    assert_eq!(absent_exit.target, present_exit.target);
    let result = carried(test, present_exit, EntityId::Value(value_result(copy)));
    assert_eq!(
        carried(test, absent_exit, EntityId::Value(value_result(zero))),
        result
    );
    let EntityId::Value(result) = result else {
        panic!("the scalar return is a value")
    };
    let join = test.block(present_exit.target).unwrap();
    assert!(join.instructions.is_empty());
    assert!(matches!(
        &join.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.as_slice() == [result]
    ));
    assert_eq!(test.blocks.len(), 4);
}

fn assert_scalar_copy_during_call_loan(module: &Module, test: &Function, combine: &Function) {
    let enter = test.block(test.entry_block().unwrap()).unwrap();
    let [EntityId::Loan(root)] = enter.parameters.as_slice() else {
        panic!("test receives shared nullable storage");
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: nullable,
    } = test.entity(EntityId::Loan(*root)).unwrap().ty
    else {
        panic!("original borrowed root must remain a loan");
    };
    let SsaTypeKind::InlineNullable {
        inner,
        niche: false,
    } = module.types[nullable.index()]
    else {
        panic!("plain scalar nullable storage");
    };
    assert_eq!(
        module.types[inner.index()],
        SsaTypeKind::Integer {
            bits: 32,
            signed: true
        }
    );
    let [begin, snapshot] = operations(test, enter)
        .try_into()
        .expect("only reborrow then read");
    assert_eq!(begin.operation, Operation::SharedReborrow { source: *root });
    let [call_loan @ EntityId::Loan(_)] = begin.results.as_slice() else {
        panic!("actual first argument loan");
    };
    assert_eq!(
        snapshot.operation,
        Operation::Read {
            source: PlaceAccess::Loan(*root)
        }
    );
    let snapshot = value_result(snapshot);
    assert_eq!(
        test.entity(EntityId::Value(snapshot)).unwrap().ty,
        EntityType::Value(nullable)
    );

    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &enter.terminator.as_ref().unwrap().kind
    else {
        panic!("test must branch on the copied nullable storage");
    };
    assert_eq!(*owner, snapshot);
    assert_ne!(when_null.target, when_non_null.target);
    let zero = test.block(when_null.target).unwrap();
    let [const_zero] = operations(test, zero)
        .try_into()
        .expect("only the null arm value");
    assert_eq!(
        const_zero.operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    let zero_exit = next(zero);
    let zero_loan = carried(test, when_null, *call_loan);
    let zero_join = (
        zero_exit.target,
        carried(test, zero_exit, zero_loan),
        carried(test, zero_exit, EntityId::Value(value_result(const_zero))),
    );

    let present = test.block(when_non_null.target).unwrap();
    let [take] = operations(test, present)
        .try_into()
        .expect("only scalar payload extraction");
    let copied_nullable = carried(test, when_non_null, EntityId::Value(snapshot));
    // NullableTake consumes the edge proof, but its input is the Read's Copyable value,
    // never the parameter loan, its original owner, or the separately active call loan.
    let EntityId::Value(copied_nullable) = copied_nullable else {
        panic!("the payload must come from copied storage");
    };
    assert_eq!(
        take.operation,
        Operation::NullableTake {
            owner: copied_nullable,
            proof: *view
        }
    );
    assert_eq!(
        test.entity(EntityId::Value(value_result(take))).unwrap().ty,
        EntityType::Value(inner)
    );
    let present_exit = next(present);
    let present_loan = carried(test, when_non_null, *call_loan);
    let present_join = (
        present_exit.target,
        carried(test, present_exit, present_loan),
        carried(test, present_exit, EntityId::Value(value_result(take))),
    );
    assert_eq!(zero_join, present_join);
    let (join, reborrow, argument) = present_join;
    let EntityId::Loan(reborrow) = reborrow else {
        panic!("same active reborrow at join");
    };
    let join = test.block(join).unwrap();
    let [call, end] = operations(test, join)
        .try_into()
        .expect("actual call before loan end");
    assert_eq!(
        call.operation,
        Operation::DirectCall {
            callee: combine.id(),
            receiver: None,
            arguments: vec![EntityId::Loan(reborrow), argument],
        }
    );
    assert_eq!(end.operation, Operation::BorrowEnd { loan: reborrow });
    assert!(matches!(
        &join.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.as_slice() == [value_result(call)]
    ));
    assert_eq!(
        test.blocks.len(),
        4,
        "no other ownership edge in the original caller"
    );
}

fn operations<'a>(function: &'a Function, block: &Block) -> Vec<&'a Instruction> {
    block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect()
}

fn value_result(instruction: &Instruction) -> ValueId {
    let [EntityId::Value(value)] = instruction.results.as_slice() else {
        panic!("expected one result: {instruction:?}");
    };
    *value
}

fn next(block: &Block) -> &Edge {
    let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
        panic!("both actual arms join the outer call");
    };
    edge
}

fn carried(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    let block = function.block(edge.target).unwrap();
    let mut matches = edge
        .arguments
        .iter()
        .zip(&block.parameters)
        .filter_map(|(argument, parameter)| (*argument == entity).then_some(*parameter));
    let parameter = matches
        .next()
        .expect("actual source identity crosses the edge");
    assert!(matches.next().is_none());
    parameter
}
