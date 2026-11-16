//! Execute experimental LLVM fixtures and exact objects from checked source entry packages.
use super::model::*;
use super::native_bounded::execute_binary;
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::two_source_experiment::analyze_two_source_experiment,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
use std::{fs, process::Command};
fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(id) = entity else {
        panic!("value")
    };
    id
}
fn append(
    f: &mut Function,
    block: BlockId,
    op: Operation,
    ty: EntityType,
    origin: &Origin,
) -> EntityId {
    f.append_instruction(block, op, vec![ty], origin.clone())
        .unwrap()
        .1[0]
}
fn root(
    f: &mut Function,
    block: BlockId,
    owner: ValueId,
    ty: SsaTypeId,
    origin: &Origin,
) -> PlaceId {
    let EntityId::Place(place) = append(
        f,
        block,
        Operation::RootPlace { owner },
        EntityType::Place(ty),
        origin,
    ) else {
        panic!("place")
    };
    place
}
fn borrow(
    f: &mut Function,
    block: BlockId,
    place: PlaceId,
    ty: SsaTypeId,
    origin: &Origin,
) -> LoanId {
    let EntityId::Loan(loan) = append(
        f,
        block,
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        },
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: ty,
        },
        origin,
    ) else {
        panic!("loan")
    };
    loan
}
#[test]
fn e2_two_source_native_complete_caller_returns_and_preserves_shared_inputs() {
    let base = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}";
    for reversed in [false, true] {
        let source = if reversed {
            base.replace(
                "left:String,right:String,rounds:Int,choose:Boolean",
                "choose:Boolean,right:String,rounds:Int,left:String",
            )
        } else {
            base.to_owned()
        };
        let mut sources = SourceMap::new();
        let id = sources.add_source("native-e2.ko", &source).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "native-e2.ko", id, &parsed)];
        let (env, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .unwrap();
        let [fact] = experiment.facts() else {
            panic!("one complete fact")
        };
        let shell = experiment.function_shell(fact, &typed).unwrap();
        let mut emitted = super::e2_function::emit(&experiment, fact, &typed).unwrap();
        super::e2_function_tests::assert_caller(&emitted, shell);
        let module = &mut emitted.fragments.leaves.program.modules[0];
        let string = module.intern_type(SsaTypeKind::StringOwner);
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let origin = Origin::Synthetic {
            anchor: shell.body(),
            reason: "native E2 test driver, not source lowering".to_owned(),
        };
        let driver = module
            .add_function("e2_native_test_driver", vec![], origin.clone())
            .unwrap();
        let f = module.function_mut(driver).unwrap();
        let block = f.add_block(vec![], origin.clone()).unwrap();
        let mut expected = String::new();
        for rounds in [i32::MIN, 0, 3] {
            for choose in [false, true] {
                for alias in [false, true] {
                    let mut owners = Vec::new();
                    for bytes in [b"left".to_vec(), b"right".to_vec()] {
                        owners.push(value(append(
                            f,
                            block,
                            Operation::StringLiteral { string, bytes },
                            EntityType::Value(string),
                            &origin,
                        )));
                    }
                    let roots = [
                        root(f, block, owners[0], string, &origin),
                        root(f, block, owners[1], string, &origin),
                    ];
                    let left = borrow(f, block, roots[0], string, &origin);
                    let right = borrow(f, block, roots[usize::from(!alias)], string, &origin);
                    assert_ne!(
                        left, right,
                        "same owner still has distinct Shared loan identities"
                    );
                    let count = value(append(
                        f,
                        block,
                        Operation::Constant(ScalarConstant::Integer(i128::from(rounds))),
                        EntityType::Value(integer),
                        &origin,
                    ));
                    let select = value(append(
                        f,
                        block,
                        Operation::Constant(ScalarConstant::Boolean(choose)),
                        EntityType::Value(boolean),
                        &origin,
                    ));
                    let count_place = root(f, block, count, integer, &origin);
                    let select_place = root(f, block, select, boolean, &origin);
                    let count = borrow(f, block, count_place, integer, &origin);
                    let select = borrow(f, block, select_place, boolean, &origin);
                    let loans = [left, right, count, select];
                    let mut arguments = vec![EntityId::Loan(left); 4];
                    for (role, loan) in shell.roles().into_iter().zip(loans) {
                        arguments[role] = EntityId::Loan(loan);
                    }
                    f.append_instruction(
                        block,
                        Operation::DirectCall {
                            callee: emitted.caller,
                            receiver: None,
                            arguments,
                        },
                        vec![],
                        origin.clone(),
                    )
                    .unwrap();
                    for loan in [left, right] {
                        f.append_instruction(
                            block,
                            Operation::PrintString { value: loan },
                            vec![],
                            origin.clone(),
                        )
                        .unwrap();
                    }
                    expected.push_str(if alias {
                        "left\nleft\n"
                    } else {
                        "left\nright\n"
                    });
                    for loan in loans {
                        f.append_instruction(
                            block,
                            Operation::BorrowEnd { loan },
                            vec![],
                            origin.clone(),
                        )
                        .unwrap();
                    }
                    for owner in owners {
                        f.append_instruction(
                            block,
                            Operation::Drop { owner },
                            vec![],
                            origin.clone(),
                        )
                        .unwrap();
                    }
                }
            }
        }
        f.set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
            .unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(
            &emitted.fragments.leaves.program,
            driver,
        )
        .unwrap();
        execute_llvm(&llvm, expected.as_bytes());
    }
}

/// Execute the exact rendered module without modifying IR or appending a driver.
pub(super) fn execute_llvm(llvm: &str, expected: &[u8]) {
    let directory = crate::native_tests::TestDirectory::create();
    let ir = directory.join("e2.ll");
    let executable = directory.join("e2");
    fs::write(&ir, llvm).unwrap();
    let compiled = Command::new(crate::test_support::clang())
        .arg("-O0")
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("clang must launch");
    assert!(compiled.status.success(), "{compiled:?}");
    execute_binary(&executable, expected);
}

/// Reject foreign identity before side effects, then link and execute the exact committed object.
pub(super) fn check_published_object(
    package: &crate::ssa::ExperimentalPublishedEntrySet<'_>,
    handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<
        '_,
        '_,
    >,
    foreign_handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<'_, '_>,
    foreign_entry: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
    entry: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
) {
    let directory = crate::native_tests::TestDirectory::create();
    let object = directory.join("joint.o");
    std::fs::write(&object, b"previous object").unwrap();
    let before = directory.snapshot();
    assert!(
        crate::native::emit_experimental_e2_object(package, foreign_handoff, entry, &object)
            .is_err()
    );
    assert_eq!(directory.snapshot(), before);
    assert!(
        crate::native::emit_experimental_e2_object(package, handoff, foreign_entry, &object)
            .is_err()
    );
    assert_eq!(directory.snapshot(), before);
    crate::native::emit_experimental_e2_object(package, handoff, entry, &object).unwrap();
    crate::test_support::assert_native_object(&std::fs::read(&object).unwrap());
    assert_eq!(
        directory.snapshot().len(),
        1,
        "no sibling temporary remains"
    );
    // Link the transaction's exact artifact, not regenerated LLVM or a synthetic driver.
    let executable = directory.join("joint");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    execute_binary(&executable, b"");
}

/// Execute an object produced from the original printing entry, without IR rewriting.
pub(super) fn execute_published(
    package: &crate::ssa::ExperimentalPublishedEntrySet<'_>,
    handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<
        '_,
        '_,
    >,
    entry: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
    expected: &[u8],
) {
    let directory = crate::native_tests::TestDirectory::create();
    let object = directory.join("print.o");
    crate::native::emit_experimental_e2_object(package, handoff, entry, &object).unwrap();
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    assert_eq!(directory.snapshot().len(), 1);
    let executable = directory.join("print");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    execute_binary(&executable, expected);
}

/// Reject foreign identity before side effects, then link and execute the exact committed object.
pub(super) fn check_common_object(
    package: &crate::ssa::ExperimentalPublishedCommon<'_>,
    handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<
        '_,
        '_,
    >,
    foreign_handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<'_, '_>,
    foreign_entry: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
    entry: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
) {
    let directory = crate::native_tests::TestDirectory::create();
    let object = directory.join("joint.o");
    std::fs::write(&object, b"previous object").unwrap();
    let before = directory.snapshot();
    assert!(
        crate::native::emit_experimental_e2_common_object(package, foreign_handoff, entry, &object)
            .is_err()
    );
    assert_eq!(directory.snapshot(), before);
    assert!(
        crate::native::emit_experimental_e2_common_object(package, handoff, foreign_entry, &object)
            .is_err()
    );
    assert_eq!(directory.snapshot(), before);
    crate::native::emit_experimental_e2_common_object(package, handoff, entry, &object).unwrap();
    crate::test_support::assert_native_object(&std::fs::read(&object).unwrap());
    assert_eq!(
        directory.snapshot().len(),
        1,
        "no sibling temporary remains"
    );
    // Link the transaction's exact artifact, not regenerated LLVM or a synthetic driver.
    let executable = directory.join("joint");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    execute_binary(&executable, b"");
}

/// Execute an object produced from the original printing entry, without IR rewriting.
pub(super) fn execute_common(
    package: &crate::ssa::ExperimentalPublishedCommon<'_>,
    handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<
        '_,
        '_,
    >,
    entry: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
    expected: &[u8],
) {
    let directory = crate::native_tests::TestDirectory::create();
    let object = directory.join("print.o");
    crate::native::emit_experimental_e2_common_object(package, handoff, entry, &object).unwrap();
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    assert_eq!(directory.snapshot().len(), 1);
    let executable = directory.join("print");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    execute_binary(&executable, expected);
}

/// Keep legacy and common source allocations under the same exact object/entry acceptance matrix.
pub(super) fn check_both_object_paths(
    legacy: &crate::ssa::ExperimentalPublishedEntrySet<'_>,
    common: &crate::ssa::ExperimentalPublishedCommon<'_>,
    handoff: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<
        '_,
        '_,
    >,
    foreign: &lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff<
        '_,
        '_,
    >,
    foreign_entry: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
    entries: &[&lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor],
) {
    for descriptor in entries {
        check_common_object(common, handoff, foreign, foreign_entry, descriptor);
        check_published_object(legacy, handoff, foreign, foreign_entry, descriptor);
        let id = legacy.entry(descriptor, handoff.types()).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(legacy.program(), id).unwrap();
        execute_llvm(&llvm, b"");
    }
}
