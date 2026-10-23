use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::{
    model::{
        EntityId, EntityType, LoanKind, Operation, Origin, Program, ScalarConstant,
        SequentialContainerKind, TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_owned_unit_with_entry,
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
    assert!(lexed.diagnostics().is_empty());
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid source input");
    let names =
        resolve_compilation_unit_names(&sources, &inputs, &index, &environment).expect("names");
    assert!(names.diagnostics().is_empty());
    let names = names.validate().expect("valid names");
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
        .expect("complete types");
    let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
        .expect("ownership")
        .into_result()
        .expect("valid types");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let owned = owned.validate().expect("valid ownership");
    let mut entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "inspect"
                && declaration.kind() == SymbolKind::Function
                && declaration.name_span().source_id() == source
        });
    let entry = entries.next().expect("source inspect exists").id();
    assert!(entries.next().is_none(), "source inspect is unique");
    let unit = owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
        .expect("same-analysis owned unit");
    let (program, _) = lower_owned_unit_with_entry(&unit, entry)
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
