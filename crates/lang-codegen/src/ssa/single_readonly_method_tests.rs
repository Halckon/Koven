//! SPEC-0316: real source -> unchanged SSA/LLVM/native; no injected runtime or IR.
use super::{LoweringError, LoweringErrorKind, model::*, verify::verify_program};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
use std::{fs, process::Command};

#[derive(Clone, Copy, Debug)]
enum Pipeline {
    // The original single source shape now uses strict MemoryDocument Unit products.
    Single,
    Unit,
    ConstantUnit,
}

fn lower(text: &str, pipeline: Pipeline, name: &str, call: &str) -> (Program, FunctionId) {
    if matches!(pipeline, Pipeline::Single) {
        let analysis =
            UnitSource::analyze("app/readonly-method.ko", &format!("package app\n{text}"))
                .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        analysis.assert_frontend_clean();
        let declaration = analysis.declaration("entry", SymbolKind::Function);
        let (program, entry) = checked_lowering(
            name,
            call,
            analysis.sources(),
            analysis.lower_entry(declaration),
        );
        assert_eq!(
            program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap()
                .origin,
            Origin::Source(analysis.declaration_span(declaration))
        );
        return (program, entry);
    }
    let mut sources = SourceMap::new();
    let (source, parsed) = super::unit_lower_test_support::parsed(
        &mut sources,
        "app/readonly-method.ko",
        &format!("package app\n{text}"),
    );
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::new(
        "app",
        "app/readonly-method.ko",
        source,
        &parsed,
    )];
    // Keep the independent package Unit / ConstantUnit controls unchanged.
    let result = if matches!(pipeline, Pipeline::ConstantUnit) {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate_constants()
            .unwrap();
        let owned =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
        let entry = super::unit_lower_test_support::declaration(&names, "app", "entry");
        super::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let (names, typed, owned) =
            super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = super::unit_lower_test_support::declaration(&names, "app", "entry");
        super::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    checked_lowering(name, call, &sources, result)
}

fn checked_lowering(
    name: &str,
    call: &str,
    sources: &SourceMap,
    result: Result<(Program, FunctionId), LoweringError>,
) -> (Program, FunctionId) {
    if let Err(error) = result {
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{name}: {error:?}"
        );
        assert_eq!(
            error.span.and_then(|s| sources.slice(s).ok()),
            Some(call),
            "{name}: {error:?}"
        );
        panic!("{name}: positive source rejected at exact receiver call {call}: {error:?}");
    }
    result.unwrap()
}

fn run_case(
    name: &str,
    fixture: &str,
    call: &str,
    constant: bool,
    pipeline: Pipeline,
    expected: &[u8],
) {
    let text = if constant {
        let substituted = fixture
            .replace("Reading(3)", "Reading(THREE)")
            .replace("Reading(7)", "Reading(SEVEN)")
            .replace("Reading(7,", "Reading(SEVEN,")
            .replace("Switch(true)", "Switch(ENABLED)");
        assert_ne!(
            substituted, fixture,
            "constant fixture must use a constant operand"
        );
        format!(
            "const val THREE: Int = 3\nconst val SEVEN: Int = 7\nconst val ENABLED: Boolean = true\n{substituted}"
        )
    } else {
        fixture.to_owned()
    };
    let (program, entry) = lower(&text, pipeline, name, call);
    verify_program(&program).unwrap();
    assert_receiver_abi(&program);
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "koven-readonly-method-{}-{name}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let ir = directory.join("program.ll");
    let executable = directory.join("program");
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{name}: {linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{name}: {run:?}");
    assert_eq!(run.stdout, expected, "{name}: {run:?}");
    assert!(run.stderr.is_empty(), "{name}: {run:?}");
    fs::remove_dir_all(directory).unwrap();
}

macro_rules! case {
    ($name:ident, $file:literal, $call:literal, $constant:literal, $pipeline:ident, $stdout:literal) => {
        #[test]
        fn $name() {
            run_case(
                stringify!($name),
                include_str!(concat!(
                    "../../tests/fixtures/readonly_methods/",
                    $file,
                    ".ko"
                )),
                $call,
                $constant,
                Pipeline::$pipeline,
                $stdout,
            );
        }
    };
}
case!(
    readonly_int_single,
    "int",
    "reading.readNumber()",
    false,
    Single,
    b"seven\n"
);
case!(
    readonly_int_single_constant,
    "int",
    "reading.readNumber()",
    true,
    Single,
    b"seven\n"
);
case!(
    readonly_int_unit,
    "int",
    "reading.readNumber()",
    false,
    Unit,
    b"seven\n"
);
case!(
    readonly_int_unit_constant,
    "int",
    "reading.readNumber()",
    true,
    ConstantUnit,
    b"seven\n"
);
case!(
    readonly_bool_single,
    "bool",
    "switch.readEnabled()",
    false,
    Single,
    b"enabled\n"
);
case!(
    readonly_bool_single_constant,
    "bool",
    "switch.readEnabled()",
    true,
    Single,
    b"enabled\n"
);
case!(
    readonly_bool_unit,
    "bool",
    "switch.readEnabled()",
    false,
    Unit,
    b"enabled\n"
);
case!(
    readonly_bool_unit_constant,
    "bool",
    "switch.readEnabled()",
    true,
    ConstantUnit,
    b"enabled\n"
);
case!(
    readonly_borrow_parameter_single,
    "borrow_parameter",
    "reading.readNumber()",
    false,
    Single,
    b"seven\nseven\n"
);
case!(
    readonly_borrow_parameter_unit,
    "borrow_parameter",
    "reading.readNumber()",
    false,
    Unit,
    b"seven\nseven\n"
);
case!(
    readonly_temporary_single,
    "temporary",
    "Reading(7).readNumber()",
    false,
    Single,
    b"seven\n"
);
case!(
    readonly_temporary_unit,
    "temporary",
    "Reading(7).readNumber()",
    false,
    Unit,
    b"seven\n"
);
case!(
    readonly_implicit_single,
    "implicit",
    "reading.relay()",
    false,
    Single,
    b"seven\n"
);
case!(
    readonly_implicit_unit,
    "implicit",
    "reading.relay()",
    false,
    Unit,
    b"seven\n"
);
case!(
    readonly_identities_single,
    "identities",
    "reading.readNumber()",
    false,
    Single,
    b"seven\neight\nnine\n"
);
case!(
    readonly_identities_unit,
    "identities",
    "reading.readNumber()",
    false,
    Unit,
    b"seven\neight\nnine\n"
);
case!(
    readonly_order_single,
    "order",
    "makeReading().readNumber(argument())",
    false,
    Single,
    b"receiver\nargument\nseven\n"
);
case!(
    readonly_order_unit,
    "order",
    "makeReading().readNumber(argument())",
    false,
    Unit,
    b"receiver\nargument\nseven\n"
);
case!(
    readonly_moveonly_single,
    "moveonly",
    "reading.readNumber()",
    false,
    Single,
    b"seven\nseven\nseven\n"
);
case!(
    readonly_moveonly_unit,
    "moveonly",
    "reading.readNumber()",
    false,
    Unit,
    b"seven\nseven\nseven\n"
);
case!(
    readonly_unused_single,
    "unused",
    "not called",
    false,
    Single,
    b"entry\n"
);
case!(
    readonly_unused_unit,
    "unused",
    "not called",
    false,
    Unit,
    b"entry\n"
);

fn assert_receiver_abi(program: &Program) {
    let module = &program.modules[0];
    for function in &module.functions {
        if let Some(receiver_type) = function.receiver() {
            assert!(matches!(
                receiver_type,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                }
            ));
            let parent = function.blocks[0].parameters[0];
            assert_eq!(function.entity(parent).unwrap().ty, receiver_type);
            assert!(function.instructions.iter().all(|i| {
                !matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == parent)
                    && !matches!(i.operation, Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(loan) == parent)
                    && !matches!(i.operation, Operation::AggregateProject { .. })
            }));
            for instruction in &function.instructions {
                if let Operation::SharedFieldLoan { base, .. } = instruction.operation {
                    assert_eq!(EntityId::Loan(base), parent);
                    let child = instruction.results[0];
                    let ends = function
                        .instructions
                        .iter()
                        .filter(|i| {
                            matches!(i.operation,
                        Operation::BorrowEnd { loan } if EntityId::Loan(loan) == child)
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(ends.len(), 1);
                    assert!(ends[0].id.index() > instruction.id.index());
                    assert!(function.instructions.iter().any(|i| matches!(i.operation,
                        Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(loan) == child)
                        && instruction.id.index() < i.id.index() && i.id.index() < ends[0].id.index()));
                }
            }
        }
        for instruction in &function.instructions {
            if let Operation::DirectCall {
                callee,
                receiver: Some(receiver),
                arguments,
            } = &instruction.operation
            {
                let target = module.function(*callee).unwrap();
                assert_eq!(
                    target.receiver(),
                    Some(function.entity(*receiver).unwrap().ty)
                );
                assert_eq!(target.blocks[0].parameters.len(), arguments.len() + 1);
                assert!(matches!(target.blocks[0].parameters[0], EntityId::Loan(_)));
                if function.blocks[0].parameters.contains(receiver) {
                    assert!(!function.instructions.iter().any(|i| matches!(i.operation,
                        Operation::BorrowEnd { loan } if EntityId::Loan(loan) == *receiver)));
                } else {
                    let ends = function
                        .instructions
                        .iter()
                        .filter(|i| {
                            matches!(i.operation,
                        Operation::BorrowEnd { loan } if EntityId::Loan(loan) == *receiver)
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(ends.len(), 1);
                    assert!(ends[0].id.index() > instruction.id.index());
                }
            }
        }
    }
}

#[test]
fn readonly_map_0299_single_same_method_source() {
    let text = crate::native_owned_map_borrow_cases::inline_field(true);
    run_case(
        "map-0299-single",
        &text,
        "value.number()",
        false,
        Pipeline::Single,
        b"borrowed inline\nborrowed inline\nowned inline\nend\n",
    );
}

#[test]
fn readonly_map_0299_unit_same_method_source() {
    let text = crate::native_owned_map_borrow_cases::inline_field(true);
    run_case(
        "map-0299-unit",
        &text,
        "value.number()",
        false,
        Pipeline::Unit,
        b"borrowed inline\nborrowed inline\nowned inline\nend\n",
    );
}
case!(
    readonly_borrow_parameter_single_constant,
    "borrow_parameter",
    "reading.readNumber()",
    true,
    Single,
    b"seven\nseven\n"
);
case!(
    readonly_borrow_parameter_unit_constant,
    "borrow_parameter",
    "reading.readNumber()",
    true,
    ConstantUnit,
    b"seven\nseven\n"
);
case!(
    readonly_temporary_single_constant,
    "temporary",
    "Reading(7).readNumber()",
    true,
    Single,
    b"seven\n"
);
case!(
    readonly_temporary_unit_constant,
    "temporary",
    "Reading(7).readNumber()",
    true,
    ConstantUnit,
    b"seven\n"
);
case!(
    readonly_implicit_single_constant,
    "implicit",
    "reading.relay()",
    true,
    Single,
    b"seven\n"
);
case!(
    readonly_implicit_unit_constant,
    "implicit",
    "reading.relay()",
    true,
    ConstantUnit,
    b"seven\n"
);
case!(
    readonly_identities_single_constant,
    "identities",
    "reading.readNumber()",
    true,
    Single,
    b"seven\neight\nnine\n"
);
case!(
    readonly_identities_unit_constant,
    "identities",
    "reading.readNumber()",
    true,
    ConstantUnit,
    b"seven\neight\nnine\n"
);
case!(
    readonly_order_single_constant,
    "order",
    "makeReading().readNumber(argument())",
    true,
    Single,
    b"receiver\nargument\nseven\n"
);
case!(
    readonly_order_unit_constant,
    "order",
    "makeReading().readNumber(argument())",
    true,
    ConstantUnit,
    b"receiver\nargument\nseven\n"
);
case!(
    readonly_moveonly_single_constant,
    "moveonly",
    "reading.readNumber()",
    true,
    Single,
    b"seven\nseven\nseven\n"
);
case!(
    readonly_moveonly_unit_constant,
    "moveonly",
    "reading.readNumber()",
    true,
    ConstantUnit,
    b"seven\nseven\nseven\n"
);
case!(
    readonly_named_single,
    "named",
    "makeReading().readNumber(second=second(), first=first())",
    false,
    Single,
    b"receiver\nsecond\nfirst\nfourteen\n"
);
case!(
    readonly_named_unit,
    "named",
    "makeReading().readNumber(second=second(), first=first())",
    false,
    Unit,
    b"receiver\nsecond\nfirst\nfourteen\n"
);
case!(
    readonly_named_single_constant,
    "named",
    "makeReading().readNumber(second=second(), first=first())",
    true,
    Single,
    b"receiver\nsecond\nfirst\nfourteen\n"
);
case!(
    readonly_named_unit_constant,
    "named",
    "makeReading().readNumber(second=second(), first=first())",
    true,
    ConstantUnit,
    b"receiver\nsecond\nfirst\nfourteen\n"
);

#[path = "single_readonly_method_boundaries.rs"]
mod boundaries;

case!(
    readonly_moveonly_temporary_single,
    "moveonly_temporary",
    "Reading(7,\"payload\".clone()).readNumber()",
    false,
    Single,
    b"seven\n"
);
case!(
    readonly_moveonly_temporary_unit,
    "moveonly_temporary",
    "Reading(7,\"payload\".clone()).readNumber()",
    false,
    Unit,
    b"seven\n"
);

case!(
    readonly_field_identity_single,
    "shadow",
    "reading.readNumber(1)",
    false,
    Single,
    b"field\nthird\nparameter\n"
);
case!(
    readonly_field_identity_unit,
    "shadow",
    "reading.readNumber(1)",
    false,
    Unit,
    b"field\nthird\nparameter\n"
);

#[test]
fn readonly_moveonly_temporary_drop_follows_actual_receiver_loan() {
    let source = include_str!("../../tests/fixtures/readonly_methods/moveonly_temporary.ko");
    for pipeline in [Pipeline::Single, Pipeline::Unit] {
        let (program, entry) = lower(
            source,
            pipeline,
            "readonly_moveonly_temporary_drop_follows_actual_receiver_loan",
            "Reading(7,\"payload\".clone()).readNumber()",
        );
        let function = program.modules[0].function(entry).unwrap();
        let call = function
            .instructions
            .iter()
            .find(|i| {
                matches!(
                    i.operation,
                    Operation::DirectCall {
                        receiver: Some(_),
                        ..
                    }
                )
            })
            .unwrap();
        let Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            ..
        } = call.operation
        else {
            panic!("shared receiver");
        };
        let Definition::InstructionResult { instruction, .. } = function
            .entity(EntityId::Loan(receiver))
            .unwrap()
            .definition
        else {
            panic!("new receiver loan");
        };
        let Operation::BorrowBegin { place, .. } =
            function.instruction(instruction).unwrap().operation
        else {
            panic!("receiver borrow");
        };
        let Definition::InstructionResult { instruction, .. } =
            function.entity(EntityId::Place(place)).unwrap().definition
        else {
            panic!("receiver storage");
        };
        let Operation::RootPlace { owner } = function.instruction(instruction).unwrap().operation
        else {
            panic!("exact temporary owner");
        };
        let end = function
            .instructions
            .iter()
            .find(|i| {
                matches!(i.operation,
            Operation::BorrowEnd { loan } if loan == receiver)
            })
            .unwrap();
        let drops = function
            .instructions
            .iter()
            .filter(|i| {
                matches!(i.operation,
            Operation::Drop { owner: dropped } if dropped == owner)
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert!(call.id.index() < end.id.index() && end.id.index() < drops[0].id.index());
    }
}
