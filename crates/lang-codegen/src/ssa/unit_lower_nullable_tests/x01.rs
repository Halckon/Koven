//! Original ordinary x01 source: nullable root Borrow across a nested if condition.

use crate::test_support::diagnostics::diagnostic;
use std::process::Command;

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, SymbolKind, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::ssa::{
    model::{
        EntityId, EntityType, FunctionId, LoanKind, Operation, Program, ScalarConstant,
        SsaTypeKind, TerminatorKind,
    },
    unit_lower::lower_owned_unit_with_entry,
};

const SOURCE: &str =
    include_str!("../../../tests/fixtures/single-unit-0341/x01_nullable_class_retest.ko");

pub(super) fn run_original() {
    for rooted in [false, true] {
        run_nullable_class_borrow("x01_nullable_class_retest.ko", SOURCE, rooted, false);
    }
}

pub(super) fn run_null_control() {
    // Keep all original declarations and control flow; change only the input initializer.
    assert_eq!(SOURCE.matches("Node(3)").count(), 1);
    let text = SOURCE.replacen("Node(3)", "null", 1);
    assert_eq!(text.replacen("= null;", "= Node(3);", 1), SOURCE);
    for rooted in [false, true] {
        run_nullable_class_borrow("x01_nullable_class_null_control.ko", &text, rooted, true);
    }
}

fn run_nullable_class_borrow(path: &str, text: &str, rooted: bool, null_input: bool) {
    let mut sources = SourceMap::new();
    let source = sources.add_source(path, text).expect("original source");
    let lexed = lex(&sources, source).expect("real lexing");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).expect("real parsing");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [if rooted {
        SourceUnitInput::new("root", path, source, &parsed)
    } else {
        SourceUnitInput::memory_document(source, &parsed)
    }];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("original unit index");
    assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
    let [unit] = index.source_units() else {
        panic!("one exact original source");
    };
    assert_eq!(unit.source_id(), source);
    assert_eq!(matches!(unit.key(), SourceUnitKey::MemoryDocument), !rooted);
    assert_eq!(sources.source_text(source), Ok(text));
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("real names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().expect("validated names");
    let typed =
        check_compilation_unit_types(&sources, &inputs, &names, &environment).expect("real types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    complete_compilation_unit_types(&sources, &inputs, &names, &environment, &typed)
        .expect("complete final type admission");
    let typed = typed.validate().expect("basic type capability");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("real ownership");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let owned = owned.validate().expect("complete ownership capability");
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .expect("same-round sealed view");
    let declaration = |name: &str| {
        let matches = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|item| {
                item.name() == name
                    && item.kind() == SymbolKind::Function
                    && item.name_span().source_id() == source
                    && names.names().index().packages()[item.package().index()]
                        .name()
                        .expect("named package")
                        .is_default()
            })
            .collect::<Vec<_>>();
        let [item] = matches.as_slice() else {
            panic!("unique original {name} declaration")
        };
        *item
    };
    let main = declaration("main");
    let use_function = declaration("use");
    let main_offset = text.find("fun main").unwrap() + "fun ".len();
    assert_eq!(main.name_span().start(), main_offset);
    assert_eq!(main.name_span().end(), main_offset + "main".len());
    let use_span = parsed
        .ast()
        .items()
        .get(use_function.root())
        .unwrap()
        .span();
    let (program, entry) = lower_owned_unit_with_entry(&view, main.id())
        .expect("original nullable class source must reach verified SSA");
    crate::ssa::verify::verify_program(&program).expect("complete SSA verifier");
    assert_original_borrow(&program, entry, use_span, text, null_input);
    let ssa = crate::ssa::render_program(&program);
    let llvm = crate::llvm::render_verified_program(&program).expect("complete verified LLVM");
    diagnostic!(
        "x01 rooted={rooted} null_input={null_input} queried SSA:\n{ssa}\nx01 queried LLVM:\n{llvm}"
    );

    // The public native chain lowers the same sealed view again, not this queried Program.
    let directory = crate::native_tests::TestDirectory::create();
    let object = directory.join("program.o");
    let executable = directory.join("program");
    crate::emit_native_owned_unit_object(
        &view,
        crate::NativeUnitEntry::NoArguments(main.id()),
        &object,
    )
    .expect("original main native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("original linker helper");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("original main process");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let stdout: &[u8] = if null_input { b"" } else { b"retest\n" };
    assert_eq!(run.stdout, stdout, "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");

    // A second native execution instruments only the verified LLVM's allocation calls.
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    let executable_llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    if null_input {
        crate::ssa::unit_field_replace_tests::link_and_run(
            &executable_llvm
                .replace("@malloc(", "@unexpected_alloc(")
                .replace("@free(", "@unexpected_free("),
            r#"
#include <stdlib.h>
void *unexpected_alloc(size_t size) { (void)size; abort(); }
void unexpected_free(void *pointer) { (void)pointer; abort(); }
"#,
            stdout,
        );
    } else {
        assert_success(
            &run_counted_allocations_in_order(&executable_llvm, &[0]),
            stdout,
        );
    }
}

fn assert_original_borrow(
    program: &Program,
    entry: FunctionId,
    use_span: Span,
    text: &str,
    null_input: bool,
) {
    let module = program.module(entry.module()).unwrap();
    let main = module.function(entry).unwrap();
    let original = |span: Span| {
        assert_eq!(span.source_id(), use_span.source_id());
        &text[span.start()..span.end()]
    };
    let branches = main
        .blocks
        .iter()
        .filter_map(|block| {
            let terminator = block.terminator.as_ref()?;
            // SPEC-0350: `main` is inside the refinement subset, which attributes the
            // proof branch to the null comparison of the outer `if`.
            if original(terminator.origin.span()) == "n != null"
                && matches!(terminator.kind, TerminatorKind::NullableBranch { .. })
            {
                Some((block.id, &terminator.kind))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [
        (
            header,
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            },
        ),
    ] = branches.as_slice()
    else {
        panic!("the original outer if must publish a non-null view")
    };
    assert_ne!(when_null.target, when_non_null.target);
    let definition = |value| {
        main.instructions
            .iter()
            .find(|inst| inst.results.as_slice() == [EntityId::Value(value)])
            .expect("actual nullable subject definition")
    };
    let input = definition(*owner);
    assert!(original(input.origin.span()).ends_with(if null_input { "null" } else { "Node(3)" }));
    if null_input {
        assert!(matches!(input.operation, Operation::NullableNull { .. }));
        assert!(
            !main
                .instructions
                .iter()
                .any(|inst| matches!(inst.operation, Operation::HeapAllocate { .. }))
        );
    } else {
        let Operation::NullableWrap {
            owner: constructed, ..
        } = input.operation
        else {
            panic!("original construction must supply the owner")
        };
        let allocation = definition(constructed);
        assert!(matches!(
            allocation.operation,
            Operation::HeapAllocate { .. }
        ));
        assert_eq!(original(allocation.origin.span()), "Node(3)");
    }
    let owner_type = main
        .entity(EntityId::Value(*owner))
        .unwrap()
        .ty
        .semantic_type();
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(owner_type) else {
        panic!("the original owner stays nullable")
    };
    assert_eq!(
        main.entity(EntityId::Loan(*view)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: *inner
        }
    );
    let non_null = main.block(when_non_null.target).unwrap();
    assert_eq!(non_null.parameters.last(), Some(&EntityId::Loan(*view)));
    let owners = when_non_null
        .arguments
        .iter()
        .enumerate()
        .filter(|(_, entity)| **entity == EntityId::Value(*owner))
        .collect::<Vec<_>>();
    let [(owner_column, _)] = owners.as_slice() else {
        panic!("one original owner column")
    };
    assert_eq!(
        main.entity(non_null.parameters[*owner_column]).unwrap().ty,
        EntityType::Value(owner_type)
    );
    let incoming = main
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .terminator
                .as_ref()
                .unwrap()
                .kind
                .targets()
                .into_iter()
                .filter(|target| *target == non_null.id)
                .map(move |_| block.id)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        incoming,
        [*header],
        "all incoming edges belong to the real proof birth"
    );
    let calls = main
        .instructions
        .iter()
        .filter(|instruction| {
            original(instruction.origin.span()) == "use(n)"
                && matches!(instruction.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one original use(n) call")
    };
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &call.operation
    else {
        unreachable!()
    };
    let target = module.function(*callee).unwrap().origin.span();
    assert_eq!(target.source_id(), use_span.source_id());
    assert!(use_span.start() <= target.start() && target.end() <= use_span.end());
    assert_eq!(*receiver, None);
    assert_eq!(arguments, &[EntityId::Loan(*view)]);
    assert_eq!(
        call.block, non_null.id,
        "use observes the original non-null successor view"
    );
    let EntityId::Value(rebound) = non_null.parameters[*owner_column] else {
        panic!("the original nullable owner survives into the proof block")
    };
    let operations = non_null
        .instructions
        .iter()
        .map(|id| main.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let [borrow_call, end, drop, three, compare] = operations.as_slice() else {
        panic!("field read result, proof end, owner retirement, then scalar nested test")
    };
    assert_eq!(borrow_call.id, call.id);
    assert_eq!(end.operation, Operation::BorrowEnd { loan: *view });
    assert_eq!(drop.operation, Operation::Drop { owner: rebound });
    assert_eq!(original(drop.origin.span()), "n");
    assert_eq!(
        three.operation,
        Operation::Constant(ScalarConstant::Integer(3))
    );
    let [EntityId::Value(field)] = call.results.as_slice() else {
        panic!("actual field result")
    };
    let [EntityId::Value(expected)] = three.results.as_slice() else {
        panic!("actual comparator literal")
    };
    let Operation::Compare { left, right, .. } = compare.operation else {
        panic!("nested compare")
    };
    assert_eq!((left, right), (*field, *expected));
    let TerminatorKind::Conditional { condition, .. } = &non_null.terminator.as_ref().unwrap().kind
    else {
        panic!("the nested If uses the comparison")
    };
    assert_eq!(compare.results.as_slice(), [EntityId::Value(*condition)]);

    let null = main.block(when_null.target).unwrap();
    assert_eq!(when_null.arguments.as_slice(), [EntityId::Value(*owner)]);
    let [EntityId::Value(empty)] = null.parameters.as_slice() else {
        panic!("only the empty wrapper crosses the null edge")
    };
    let [drop] = null.instructions.as_slice() else {
        panic!("null skips use and retires only the wrapper")
    };
    assert_eq!(
        main.instruction(*drop).unwrap().operation,
        Operation::Drop { owner: *empty }
    );

    let callee = module.function(*callee).unwrap();
    let block = callee.block(callee.entry_block().unwrap()).unwrap();
    let [EntityId::Loan(argument)] = block.parameters.as_slice() else {
        panic!("use accepts the actual Shared ABI")
    };
    let [read] = callee.instructions.as_slice() else {
        panic!("only read the original v field")
    };
    assert_eq!(original(read.origin.span()), "n.v");
    assert_eq!(
        read.operation,
        Operation::HeapFieldRead {
            receiver: *argument,
            field: 0
        }
    );
    let [EntityId::Value(result)] = read.results.as_slice() else {
        panic!("read result")
    };
    assert!(matches!(&block.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.as_slice() == [*result]));
}
