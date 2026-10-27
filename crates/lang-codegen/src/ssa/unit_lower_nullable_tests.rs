//! Original ordinary x01 source: nullable root Borrow across a nested if condition.

use std::process::Command;

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::{
    model::{
        EntityId, EntityType, FunctionId, LoanKind, Operation, Program, SsaTypeKind, TerminatorKind,
    },
    unit_lower::lower_owned_unit_with_entry,
};

const SOURCE: &str =
    include_str!("../../tests/fixtures/single-unit-0341/x01_nullable_class_retest.ko");

#[test]
fn original_nullable_class_borrow_nested_if_reaches_verified_native() {
    let path = "x01_nullable_class_retest.ko";
    let mut sources = SourceMap::new();
    let source = sources.add_source(path, SOURCE).expect("original source");
    let lexed = lex(&sources, source).expect("real lexing");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).expect("real parsing");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", path, source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("original unit index");
    assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
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
    let main_offset = SOURCE.find("fun main").unwrap() + "fun ".len();
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
    super::verify::verify_program(&program).expect("complete SSA verifier");
    assert_original_borrow(&program, entry, use_span);
    let ssa = super::render_program(&program);
    let llvm = crate::llvm::render_verified_program(&program).expect("complete verified LLVM");
    println!("x01 queried SSA:\n{ssa}\nx01 queried LLVM:\n{llvm}");

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
    assert_eq!(run.stdout, b"retest\n", "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

fn assert_original_borrow(program: &Program, entry: FunctionId, use_span: Span) {
    let module = program.module(entry.module()).unwrap();
    let main = module.function(entry).unwrap();
    let original = |span: Span| {
        assert_eq!(span.source_id(), use_span.source_id());
        &SOURCE[span.start()..span.end()]
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
        "use consumes the original non-null successor view"
    );
}

// Stage A: retain all three original whole-file tests. Ten unchanged lowering.ko
// sources require sixteen independent Unit roots, including unreachable helpers.
fn verify_original_assertion_roots(text: &str, roots: &[&str], callees: &[&str]) {
    use crate::native_unit_source_test_support::UnitSource;

    let source = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("original assertion source admission: {error:?}\n{text}"));
    source.assert_frontend_clean();
    for &root in roots {
        let declaration = source.declaration(root, SymbolKind::Function);
        let span = source.declaration_span(declaration);
        let (program, entry) = source.lower_entry(declaration).unwrap_or_else(|error| {
            panic!("original assertion root {root} lowering: {error:?}\n{text}")
        });
        let module = program.module(entry.module()).expect("entry's own Program");
        let function = module.function(entry).expect("real root function");
        assert_eq!(function.origin.span(), span, "original root {root}");
        if root == "inspect" {
            for &callee in callees {
                let declaration = source.declaration(callee, SymbolKind::Function);
                let span = source.declaration_span(declaration);
                let emitted = program
                    .modules
                    .iter()
                    .flat_map(|module| &module.functions)
                    .filter(|function| function.origin.span() == span)
                    .collect::<Vec<_>>();
                let [target] = emitted.as_slice() else {
                    panic!("original callee {callee} must be emitted exactly once: {text}");
                };
                assert!(
                    function.instructions.iter().any(|instruction| matches!(
                        instruction.operation,
                        Operation::DirectCall { callee: id, .. } if id == target.id
                    )),
                    "inspect must call the original {callee} in its own Program: {text}"
                );
            }
        }
        super::verify::verify_program(&program).unwrap_or_else(|error| {
            panic!("original assertion root {root} SSA verification: {error:?}\n{text}")
        });
        // Parameterized roots remain real functions; no fabricated no-argument native entry.
        crate::llvm::render_verified_program(&program).unwrap_or_else(|error| {
            panic!("original assertion root {root} complete LLVM: {error:?}\n{text}")
        });
    }
}

#[test]
fn non_null_assertion_preserves_pending_value_arguments_across_its_branch() {
    // The first owner has been evaluated, but only the eventual call consumes it.
    let analysis = String::from(
        "class Node()\nfun take(own first: Node, own second: Node): Node = first\nfun inspect(own first: Node, own second: Node?): Node = take(first, second!!)",
    );
    verify_original_assertion_roots(&analysis, &["inspect"], &["take"]);
}

#[test]
fn non_null_assertion_temporary_group_and_borrowed_call_results_keep_one_owner() {
    for (case, body) in [
        "fun inspect(): Node = make()!!",
        "fun inspect(own node: Node?): Node = ((node))!!",
        "fun inspect(own node: Node?): Unit = observe(Node(), node!!)",
        "fun inspect(own node: Node?, flag: Boolean): Node = (if (flag) { node } else { make() })!!",
        "fun inspect(own probe: Node?, own node: Node?): Node = (if (probe == null) { node } else { make() })!!",
    ]
    .into_iter()
    .enumerate()
    {
        let analysis = format!(
            "class Node()\nfun make(): Node? {{ val result: Node? = Node()\nreturn result }}\nfun observe(first: Node, second: Node): Unit {{}}\n{body}"
        );
        // The old whole-file planner also emitted helpers not called by inspect.
        let (roots, callees): (&[&str], &[&str]) = match case {
            0 | 3 | 4 => (&["inspect", "observe"], &["make"]),
            1 => (&["inspect", "make", "observe"], &[]),
            2 => (&["inspect", "make"], &["observe"]),
            _ => unreachable!("five original bodies"),
        };
        verify_original_assertion_roots(&analysis, roots, callees);
    }
}

#[test]
fn non_null_assertion_later_transfer_cleans_pending_value_owner() {
    for body in [
        "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node = take(first, (if (flag) { return Node() } else { second })!!)",
        "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node { loop { val result = take(first, (if (flag) { break } else { second })!!)\nbreak }\nreturn Node() }",
        "fun inspect(flag: Boolean): Node { loop { val first = Node()\nval second: Node? = Node()\nval result = take(first, (if (flag) { continue } else { second })!!)\nbreak }\nreturn Node() }",
        "fun inspect(own first: Node, own second: Node?, flag: Boolean): Node = take(first, (if (flag) { loop { break }\nsecond } else { second })!!)",
    ] {
        let analysis = format!(
            "class Node()\nfun take(own first: Node, own second: Node): Node = first\n{body}"
        );
        verify_original_assertion_roots(&analysis, &["inspect"], &["take"]);
    }
}
