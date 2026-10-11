//! Healthy source pipeline; negative verifier checks never compile defective IR.
use super::{model::*, verify::verify_program};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[path = "range_lowering_tests/temporary.rs"]
mod temporary;

#[path = "range_lowering_tests/view_source.rs"]
mod view_source;

#[path = "range_lowering_tests/call_prefix.rs"]
mod call_prefix;

#[path = "range_lowering_tests/receiver.rs"]
mod receiver;

fn unit(consumer: &str) -> Program {
    unit_with_entry(consumer).0
}

fn unit_with_entry(consumer: &str) -> (Program, FunctionId) {
    unit_with_provider_entry(consumer, "")
}

fn unit_with_provider_entry(consumer: &str, trusted: &str) -> (Program, FunctionId) {
    unit_with_provider_order(consumer, trusted, false)
}

fn unit_with_provider_order(consumer: &str, trusted: &str, reverse: bool) -> (Program, FunctionId) {
    let mut sources = SourceMap::new();
    let (p, provider) = super::unit_lower_test_support::parsed(
        &mut sources,
        "ranges.ko",
        &format!(
            "{}\n{trusted}",
            include_str!("../../../lang-std/koven/algorithms/ranges.ko")
        ),
    );
    let (q, consumer) = super::unit_lower_test_support::parsed(&mut sources, "main.ko", consumer);
    let mut inputs = [
        SourceUnitInput::new("std", "koven/algorithms/ranges.ko", p, &provider),
        SourceUnitInput::new("app", "app/main.ko", q, &consumer),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, p).unwrap();
    types.authorize_range_extension_source(&sources, p).unwrap();
    let (names, typed, owned) =
        super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let result = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        super::unit_lower_test_support::declaration(&names, "app", "main"),
    );
    let (program, entry) = result.unwrap_or_else(|error| {
        panic!(
            "{error:?}: {:?}",
            error.span.and_then(|span| sources.slice(span).ok())
        )
    });
    verify_program(&program).unwrap();
    (program, entry)
}

#[test]
fn range_temporary_for_single_and_unit_preserve_root_until_metadata_ends() {
    let consumer = "class Token(val text:String){deinit(){println(this.text)}}\nfun source():List<Token>{println(\"source\");return listOf(Token(\"first\"),Token(\"second\"))}\nfun main():Unit{for(item in take(source(),1)){println(item.text)}}";
    single(consumer);
    unit(&format!(
        "package app\nimport koven.algorithms.take\n{consumer}"
    ));
}

#[test]
fn range_temporary_call_single_and_unit_preserve_root_until_metadata_ends() {
    let consumer = "class Token(val text:String){deinit(){println(this.text)}}\nfun source():List<Token>{println(\"source\");return listOf(Token(\"first\"),Token(\"second\"))}\nfun read(view:View<Token>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{read(take(source(),1))}";
    single(consumer);
    unit(&format!(
        "package app\nimport koven.algorithms.take\n{consumer}"
    ));
}

#[test]
fn range_list_producer_size_uses_metadata_and_source_loan() {
    let program = unit(
        "package app\nimport koven.algorithms.take\nfun check(source:View<String>,expected:Int):Unit { if(source.size != expected){error(\"wrong\")} }\nfun main():Unit {val source=listOf(\"kept\"); borrow val part=take(source,2147483647);check(part,1);println(\"done\")}",
    );
    let module = &program.modules[0];
    assert!(module.functions.iter().any(|f| f.carrier_return.is_some()));
    assert!(module.functions.iter().any(|f| {
        f.instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::RangeEnd { .. }))
    }));
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn range_single_and_unit_for_deliver_only_shared_elements() {
    let consumer = "class Item(val text:String){}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{val source=listOf(Item(\"first\"),Item(\"excluded\"));borrow val part=take(source,1);read(part)}";
    for program in [
        single(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let reader = program.modules[0]
            .functions
            .iter()
            .find(|f| {
                f.instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::RangeElementPlace { .. }))
            })
            .unwrap();
        assert!(reader.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::RangeLength {
                view: EntityId::Loan(_)
            }
        )));
        assert!(!reader.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::Copy { .. }
                | Operation::HeapAllocate { .. }
                | Operation::SharedRetain { .. }
        )));
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_single_and_unit_named_for_transport_descriptor_and_ancestor_loans() {
    for exit in ["", "break", "continue"] {
        let consumer = format!(
            "class Item(val text:String){{}}\nfun consume(own source:List<Item>):Unit{{}}\nfun main():Unit{{val source=listOf(Item(\"first\"),Item(\"excluded\"));{{borrow val part=take(source,1);for(item in part){{println(item.text);{exit}}}}};consume(source)}}"
        );
        for program in [
            single(&consumer),
            unit(&format!(
                "package app\nimport koven.algorithms.take\n{consumer}"
            )),
        ] {
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}

#[test]
fn range_verifier_rejects_metadata_end_while_an_element_loan_is_active() {
    let mut program = unit(
        "package app\nimport koven.algorithms.take\nfun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{val source=listOf(\"first\");borrow val part=take(source,1);read(part)}",
    );
    let function = program.modules[0]
        .functions
        .iter_mut()
        .find(|f| {
            f.instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::RangeElementPlace { .. }))
        })
        .unwrap();
    let (place, source) = function
        .instructions
        .iter()
        .find_map(|i| match (&i.operation, i.results.as_slice()) {
            (Operation::RangeElementPlace { view, .. }, [EntityId::Place(place)]) => {
                Some((*place, *view))
            }
            _ => None,
        })
        .unwrap();
    let begin = function
        .instructions
        .iter()
        .find(|i| matches!(i.operation,Operation::BorrowBegin {place:actual,..} if actual==place))
        .unwrap()
        .id;
    let block = function
        .blocks
        .iter()
        .find(|block| block.instructions.contains(&begin))
        .unwrap()
        .id;
    let terminator = function.blocks[block.index()].terminator.take();
    let (end, _) = function
        .append_instruction(
            block,
            Operation::BorrowEnd { loan: source },
            Vec::new(),
            function.origin.clone(),
        )
        .unwrap();
    function.blocks[block.index()].terminator = terminator;
    let instructions = &mut function.blocks[block.index()].instructions;
    assert_eq!(instructions.pop(), Some(end));
    let position = instructions.iter().position(|id| *id == begin).unwrap() + 1;
    instructions.insert(position, end);
    let errors = verify_program(&program).expect_err("metadata must outlive its shared element");
    assert!(
        errors.errors.iter().any(|e| matches!(
            e.kind,
            super::verify::VerifyErrorKind::LoanDependencyActive { .. }
        )),
        "{errors:?}"
    );
    // Invalid SSA is checked only by the verifier, never emitted or executed.
}

#[test]
fn range_verifier_preserves_descriptor_root_pairing_through_loop_edges() {
    let mut program = unit(
        "package app\nimport koven.algorithms.take\nfun main():Unit{val a=listOf(\"a\");val b=listOf(\"b\");{borrow val left=take(a,1);borrow val right=take(b,1);for(item in left){println(item)};for(item in right){println(item)}}}",
    );
    let caller = program.modules[0]
        .functions
        .iter_mut()
        .find(|f| {
            f.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::RangeCall { .. }))
                .count()
                == 2
        })
        .unwrap();
    let ends = caller
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::RangeEnd { view, source } => Some((i.id, view, source)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 2);
    assert_eq!(
        caller.entity(EntityId::Loan(ends[0].2)).unwrap().ty,
        caller.entity(EntityId::Loan(ends[1].2)).unwrap().ty
    );
    caller.instructions[ends[0].0.index()].operation = Operation::RangeEnd {
        view: ends[0].1,
        source: ends[1].2,
    };
    caller.instructions[ends[1].0.index()].operation = Operation::RangeEnd {
        view: ends[1].1,
        source: ends[0].2,
    };
    let errors = verify_program(&program)
        .expect_err("same List type cannot substitute a different descriptor's root");
    assert!(
        errors
            .errors
            .iter()
            .any(|e| matches!(e.kind, super::verify::VerifyErrorKind::ReturnType { .. })),
        "{errors:?}"
    );
    // Crossed pairs stay within this pure verifier test; no LLVM/native call follows.
}

fn single(consumer: &str) -> Program {
    single_with_entry(consumer).0
}

fn single_with_entry(consumer: &str) -> (Program, FunctionId) {
    use lang_frontend::{
        lexer::lex, name_resolution::resolve_names, ownership_checking::check_ownership,
        parser::parse_file, type_checking::check_types,
    };
    let mut sources = SourceMap::new();
    let text = format!(
        "{}\n{consumer}",
        include_str!("../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    let source = sources.add_source("trusted.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let entry = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "main")
        .unwrap()
        .id();
    let (program, entry) = super::lower_frontend::orchestrate::lower_scalar_file_with_entry(
        &sources, &parsed, &names, &typed, &owned, entry,
    )
    .unwrap_or_else(|error| {
        panic!(
            "{error:?}: {:?}",
            error.span.and_then(|span| sources.slice(span).ok())
        )
    });
    verify_program(&program).unwrap();
    (program, entry)
}

#[test]
fn range_single_and_unit_keep_the_same_new_descriptor_abi() {
    let consumer = "class Item(val n:Int){}\nfun check(view:View<Item>):Unit{if(view.size!=1){error(\"wrong\")}}\nfun main():Unit{val source=listOf(Item(7));borrow val part=take(source,1);check(part);println(\"done\")}";
    for program in [
        single(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let module = &program.modules[0];
        assert!(module.functions.iter().any(|f| f.carrier_return.is_some()));
        assert!(module.functions.iter().all(|f| f.borrow_return.is_none()));
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_metadata_borrow_is_separate_from_constructing_a_new_descriptor() {
    let consumer = "fun metadata(source:View<String>):borrow View<String> from source=source\nfun check(source:View<String>):Unit{if(source.size!=1){error(\"size\")}}\nfun consume(own source:List<String>):Unit{}\nfun main():Unit{val source=listOf(\"kept\");borrow val part=take(source,1);borrow val alias=part;borrow val returned=metadata(alias);check(returned);consume(source)}";
    for program in [
        single(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let module = &program.modules[0];
        assert!(module.functions.iter().any(|f| f.borrow_return.is_some()));
        assert_eq!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.instructions)
                .filter(|i| matches!(i.operation, Operation::RangeConstruct { .. }))
                .count(),
            1
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_verifier_rejects_erasure_and_premature_source_end_without_emitting_ir() {
    use super::verify::VerifyErrorKind;
    let consumer = "package app\nimport koven.algorithms.take\nfun check(source:View<String>):Unit{}\nfun main():Unit{val source=listOf(\"kept\");borrow val part=take(source,1);check(part)}";
    for variant in 0..5 {
        let mut program = unit(consumer);
        let module = &mut program.modules[0];
        if variant == 3 {
            module
                .functions
                .iter_mut()
                .find(|f| f.carrier_return.is_some())
                .unwrap()
                .carrier_return = Some(1);
        } else {
            let caller = module
                .functions
                .iter_mut()
                .find(|f| {
                    f.instructions
                        .iter()
                        .any(|i| matches!(i.operation, Operation::RangeCall { .. }))
                })
                .unwrap();
            let (view, child, parent) = caller
                .instructions
                .iter()
                .find_map(|i| {
                    let Operation::RangeCall { source, .. } = i.operation else {
                        return None;
                    };
                    let [EntityId::Value(view), EntityId::Loan(child)] = i.results.as_slice()
                    else {
                        panic!("pair");
                    };
                    Some((*view, *child, source))
                })
                .unwrap();
            let end = caller
                .instructions
                .iter_mut()
                .find(|i| matches!(i.operation, Operation::RangeEnd { .. }))
                .unwrap();
            end.operation = match variant {
                0 => Operation::BorrowEnd { loan: parent },
                1 => Operation::BorrowEnd { loan: child },
                2 => Operation::Drop { owner: view },
                4 => Operation::Consume { owner: view },
                _ => unreachable!(),
            };
        }
        let errors =
            verify_program(&program).expect_err("invalid descriptor lifetime must be rejected");
        assert!(
            errors.errors.iter().any(|e| matches!(
                e.kind,
                VerifyErrorKind::LoanDependencyActive { .. }
                    | VerifyErrorKind::OwnerLoanConflict { .. }
                    | VerifyErrorKind::OperationContract { .. }
                    | VerifyErrorKind::ReturnType { .. }
            )),
            "variant {variant}: {errors:?}"
        );
        if variant == 2 || variant == 4 {
            assert!(
                errors
                    .errors
                    .iter()
                    .any(|e| matches!(e.kind, VerifyErrorKind::OperationContract { .. })),
                "erased descriptor was not rejected by its operation contract: {errors:?}"
            );
        }
        // No defective Program is ever given to an LLVM or native API.
    }
}
