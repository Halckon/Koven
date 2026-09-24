use lang_frontend::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::check_ownership,
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{check_types, standard_environments},
};

use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::{
        EntityId, EntityType, LoanKind, Operation, Origin, Program, ScalarConstant,
        SequentialContainerKind, TerminatorKind,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};
use crate::llvm::render_verified_program;

fn origin() -> (SourceMap, Origin, Span) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "borrowed-array.ko",
            "fun inspect(args: Array<String>): Unit",
        )
        .expect("source");
    let span = sources.span(source, 0, 3).expect("span");
    (sources, Origin::Source(span), span)
}

#[test]
fn source_borrowed_array_string_index_lowers_to_checked_element_loan() {
    let text = "fun inspect(args: Array<String>, index: Int): Unit { println(args[index]) }";
    let mut sources = SourceMap::new();
    let source = sources.add_source("argv-index.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lexer");
    let parsed = parse_file(&sources, &lexed).expect("parser");
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(parsed.diagnostics().is_empty());
    assert!(names.diagnostics().is_empty());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());

    let program = lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
        .expect("borrowed Array<String> indexing must lower");
    let ssa = render_program(&program);
    assert!(ssa.contains("container.element_place %l0"), "{ssa}");
    assert!(ssa.contains("borrow.shared %p0"), "{ssa}");
    assert!(ssa.contains("print.string"), "{ssa}");
    let llvm = render_verified_program(&program).expect("borrowed index LLVM must verify");
    let negative = llvm
        .find("icmp slt i32")
        .expect("signed Int negative check");
    let upper = llvm
        .find("icmp sge i32")
        .expect("signed Int logical upper-bound check");
    let branch = llvm.find("br i1 %p0.invalid").expect("abort branch");
    let widen = llvm
        .find("zext i32")
        .expect("checked Int to target-size widening");
    let address = llvm
        .find("getelementptr %koven.string")
        .expect("element address");
    assert!(negative < upper && upper < branch && branch < widen && widen < address);
}

#[test]
fn verifier_accepts_active_shared_container_loan_and_rejects_wrong_or_ended_loan() {
    for (bits, kind, end_first, expected_error) in [
        (32, LoanKind::Shared, false, None),
        (64, LoanKind::Shared, false, Some("contract")),
        (32, LoanKind::Exclusive, false, Some("contract")),
        (32, LoanKind::Shared, true, Some("inactive")),
    ] {
        let (_sources, origin, _) = origin();
        let mut program = Program::default();
        let module_id = program.add_module("borrowed-container");
        let module = program.module_mut(module_id).expect("module");
        let integer = module.intern_type(super::model::SsaTypeKind::Integer { bits, signed: true });
        let string = module.intern_type(super::model::SsaTypeKind::StringOwner);
        let array = module
            .add_sequential_container_type(SequentialContainerKind::Array, string)
            .expect("array");
        let function_id = module
            .add_function("inspect", Vec::new(), origin.clone())
            .expect("function");
        let function = module.function_mut(function_id).expect("function");
        let entry = function
            .add_block(
                vec![EntityType::Loan {
                    kind,
                    target: array,
                }],
                origin.clone(),
            )
            .expect("entry");
        let EntityId::Loan(owner) = function.block(entry).expect("entry").parameters[0] else {
            panic!("loan parameter")
        };
        let (_, index) = function
            .append_instruction(
                entry,
                Operation::Constant(ScalarConstant::Integer(0)),
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .expect("index");
        if end_first {
            function
                .append_instruction(
                    entry,
                    Operation::BorrowEnd { loan: owner },
                    Vec::new(),
                    origin.clone(),
                )
                .expect("end loan");
        }
        function
            .append_instruction(
                entry,
                Operation::ContainerElementPlace {
                    owner: EntityId::Loan(owner),
                    index: match index[0] {
                        EntityId::Value(value) => value,
                        _ => panic!("value index"),
                    },
                },
                vec![EntityType::Place(string)],
                origin.clone(),
            )
            .expect("element place");
        function
            .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
            .expect("return");

        match expected_error {
            None => verify_program(&program).expect("active shared loan must verify"),
            Some(expected) => {
                let errors = verify_program(&program).expect_err("invalid loan must fail");
                assert!(
                    errors.errors.iter().any(|error| match expected {
                        "contract" =>
                            matches!(error.kind, VerifyErrorKind::OperationContract { .. }),
                        "inactive" => matches!(error.kind, VerifyErrorKind::LoanInactive { .. }),
                        _ => false,
                    }),
                    "{:?}",
                    errors.errors
                );
            }
        }
    }
}
