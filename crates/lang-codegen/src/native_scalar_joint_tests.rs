//! New joint Unit integration coverage, independent of all unchanged legacy fixtures.
//! One main reaches 23 scalar helpers together; this is not whole-file legacy equivalence.
//! The native checks use the same source bytes and ordinary CI object/link/run helper.

use std::collections::BTreeSet;

use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use super::{boxed_enum_tests::unit_source::UnitSource, emit_link_and_run};
use crate::{
    llvm::{native_target_triple, render_verified_program_with_entry},
    ssa::{
        model::{
            CheckedArithmeticOperator, EntityId, EntityType, Function, FunctionId, LoanKind,
            Module, Operation, Origin, Program, SsaTypeKind, TerminatorKind,
        },
        render_program,
        verify::verify_program,
    },
};

const SOURCE_NAME: &str = "unit-scalar-joint.ko";
const SOURCE: &str = include_str!("../tests/fixtures/unit-scalar-joint.ko");
const STDOUT: &[u8] = b"scalar-joint-unit\nscalar-joint-ok\n";

// Name, result width (0 = Unit, 1 = Boolean), signedness, Borrow parameter count.
const HELPERS: &[(&str, u16, bool, usize)] = &[
    ("identityByte", 8, true, 1),
    ("identityUByte", 8, false, 1),
    ("identityShort", 16, true, 1),
    ("identityUShort", 16, false, 1),
    ("identityInt", 32, true, 1),
    ("identityUInt", 32, false, 1),
    ("identityLong", 64, true, 1),
    ("identityULong", 64, false, 1),
    ("negateBoolean", 1, false, 1),
    ("signedAdd", 32, true, 2),
    ("unsignedAdd", 32, false, 2),
    ("unsignedSubtract", 32, false, 2),
    ("unsignedMultiply", 32, false, 2),
    ("signedSubtract", 32, true, 2),
    ("signedMultiply", 32, true, 2),
    ("signedDivide", 32, true, 2),
    ("signedRemainder", 32, true, 2),
    ("unsignedDivide", 32, false, 2),
    ("unsignedRemainder", 32, false, 2),
    ("choose", 32, true, 3),
    ("countTo", 32, true, 1),
    ("invoke", 32, true, 2),
    ("consumeUnit", 0, false, 1),
];

fn analyze() -> UnitSource {
    let unit = UnitSource::analyze(SOURCE_NAME, SOURCE).expect("strict joint Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [source] = unit.names().names().index().source_units() else {
        panic!("one real MemoryDocument, not split or synthetic roots");
    };
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(source.source_id()),
        Ok(SOURCE_NAME)
    );
    assert_eq!(unit.sources().source_text(source.source_id()), Ok(SOURCE));
    assert_eq!(unit.parsed().roots().len(), HELPERS.len() + 1 + 4);
    unit
}

fn lower(unit: &UnitSource, name: &str) -> (Program, FunctionId, String) {
    let (program, entry) = unit
        .lower_entry(unit.declaration(name, SymbolKind::Function))
        .expect("selected entry must lower through the owned Unit view");
    verify_program(&program).expect("complete joint SSA verifies");
    assert_eq!(program.modules.len(), 1);
    let module = program.module(entry.module()).expect("returned module");
    assert_eq!(source_function(unit, module, name).id, entry);
    let llvm = render_verified_program_with_entry(&program, entry)
        .expect("complete joint LLVM, including native main wrapper, verifies");
    (program, entry, llvm)
}

fn source_function<'a>(unit: &UnitSource, module: &'a Module, name: &str) -> &'a Function {
    let origin =
        Origin::Source(unit.declaration_span(unit.declaration(name, SymbolKind::Function)));
    let functions = module
        .functions
        .iter()
        .filter(|function| function.origin == origin)
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("one source-qualified declaration target for {name}");
    };
    function
}

fn llvm_body<'a>(llvm: &'a str, function: &Function) -> &'a str {
    let marker = format!("@f{}.{}(", function.id.index(), function.name);
    let definitions = llvm
        .split("define ")
        .skip(1)
        .filter(|definition| {
            definition
                .split_once('{')
                .is_some_and(|(header, _)| header.contains(&marker))
        })
        .collect::<Vec<_>>();
    let [definition] = definitions.as_slice() else {
        panic!("one actual LLVM definition for {marker}");
    };
    definition.split_once("\n}").expect("LLVM definition end").0
}

fn assert_abi(module: &Module, function: &Function, spec: (&str, u16, bool, usize), llvm: &str) {
    let (name, bits, signed, parameters) = spec;
    let scalar = if bits <= 1 {
        SsaTypeKind::Boolean
    } else {
        SsaTypeKind::Integer { bits, signed }
    };
    assert!(function.receiver.is_none());
    assert!(function.borrow_return.is_none());
    assert!(function.carrier_return.is_none());
    if bits == 0 {
        assert!(function.return_types.is_empty());
    } else {
        let [result] = function.return_types.as_slice() else {
            panic!("one scalar return")
        };
        assert_eq!(module.type_kind(*result), Some(&scalar), "{name}");
    }
    assert_eq!(function.blocks[0].parameters.len(), parameters, "{name}");
    let arguments = function.blocks[0]
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let EntityId::Loan(loan) = parameter else {
                panic!("default scalar parameter remains Borrow")
            };
            let EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            } = function.entity(*parameter).unwrap().ty
            else {
                panic!("shared scalar loan")
            };
            let expected = if name == "choose" && index == 0 {
                &SsaTypeKind::Boolean
            } else {
                &scalar
            };
            assert_eq!(
                module.type_kind(target),
                Some(expected),
                "{name} parameter {index}"
            );
            format!("ptr %l{}", loan.index())
        })
        .collect::<Vec<_>>()
        .join(", ");
    let result = if bits == 0 {
        "void".to_owned()
    } else {
        format!("i{bits}")
    };
    let body = llvm_body(llvm, function);
    assert!(
        body.starts_with(&format!(
            "internal {result} @f{}.{}({arguments})",
            function.id.index(),
            function.name
        )),
        "{name}: {body}"
    );
}

#[test]
fn joint_unit_preserves_reachable_targets_borrow_abi_and_complete_llvm_determinism() {
    let unit = analyze();
    let (program, entry, llvm) = lower(&unit, "main");
    let module = program.module(entry.module()).unwrap();
    let main = module.function(entry).unwrap();
    assert_eq!(module.functions.len(), HELPERS.len() + 1);
    assert!(main.blocks[0].parameters.is_empty());
    assert!(main.return_types.is_empty());
    let targets = HELPERS
        .iter()
        .map(|spec| {
            let function = source_function(&unit, module, spec.0);
            assert_abi(module, function, *spec, &llvm);
            function.id
        })
        .collect::<BTreeSet<_>>();
    let calls = main
        .instructions
        .iter()
        .filter_map(|instruction| {
            let Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } = &instruction.operation
            else {
                return None;
            };
            assert!(receiver.is_none());
            let target = module
                .function(*callee)
                .expect("call target belongs to joint module");
            assert_eq!(arguments.len(), target.blocks[0].parameters.len());
            for (argument, parameter) in arguments.iter().zip(&target.blocks[0].parameters) {
                assert_eq!(
                    main.entity(*argument).unwrap().ty,
                    target.entity(*parameter).unwrap().ty
                );
            }
            let Origin::Source(span) = instruction.origin else {
                panic!("source call origin")
            };
            let text = unit.sources().slice(span).unwrap();
            let name = HELPERS
                .iter()
                .find(|spec| source_function(&unit, module, spec.0).id == *callee)
                .unwrap()
                .0;
            assert!(
                text.starts_with(&format!("{name}(")),
                "actual call {name}: {text}"
            );
            Some(*callee)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls.iter().copied().collect::<BTreeSet<_>>(),
        targets,
        "all 23 helpers coexist and are actually called by main"
    );
    assert_eq!(calls.len(), 27, "four helpers are each called twice");
    let main_body = llvm_body(&llvm, main);
    for &(name, bits, _, _) in HELPERS {
        let target = source_function(&unit, module, name);
        let expected = match name {
            "negateBoolean" | "choose" | "countTo" | "consumeUnit" => 2,
            _ => 1,
        };
        assert_eq!(
            calls.iter().filter(|callee| **callee == target.id).count(),
            expected,
            "main SSA call count for {name}"
        );
        let result = if bits == 0 {
            "void".to_owned()
        } else {
            format!("i{bits}")
        };
        let call = format!("call {result} @f{}.{}(", target.id.index(), target.name);
        assert_eq!(
            main_body.matches(&call).count(),
            expected,
            "main LLVM call count for source-qualified {name}"
        );
    }

    let invoke = source_function(&unit, module, "invoke");
    let add = source_function(&unit, module, "signedAdd");
    let direct = invoke
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => Some((callee, receiver, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(callee, receiver, arguments)] = direct.as_slice() else {
        panic!("one nested named-argument call")
    };
    assert_eq!(**callee, add.id);
    assert!(receiver.is_none());
    assert_eq!(arguments.as_slice(), invoke.blocks[0].parameters.as_slice());
    let invoke_body = llvm_body(&llvm, invoke);
    let add_call = format!("call i32 @f{}.{}(", add.id.index(), add.name);
    assert_eq!(
        invoke_body.matches(&add_call).count(),
        1,
        "invoke LLVM must call the unique source-qualified signedAdd target"
    );

    for (name, operator, fragment) in [
        (
            "signedAdd",
            CheckedArithmeticOperator::Add,
            "llvm.sadd.with.overflow.i32",
        ),
        (
            "unsignedAdd",
            CheckedArithmeticOperator::Add,
            "llvm.uadd.with.overflow.i32",
        ),
        (
            "unsignedSubtract",
            CheckedArithmeticOperator::Subtract,
            "llvm.usub.with.overflow.i32",
        ),
        (
            "unsignedMultiply",
            CheckedArithmeticOperator::Multiply,
            "llvm.umul.with.overflow.i32",
        ),
        (
            "signedSubtract",
            CheckedArithmeticOperator::Subtract,
            "llvm.ssub.with.overflow.i32",
        ),
        (
            "signedMultiply",
            CheckedArithmeticOperator::Multiply,
            "llvm.smul.with.overflow.i32",
        ),
        (
            "signedDivide",
            CheckedArithmeticOperator::Divide,
            " sdiv i32 ",
        ),
        (
            "signedRemainder",
            CheckedArithmeticOperator::Remainder,
            " srem i32 ",
        ),
        (
            "unsignedDivide",
            CheckedArithmeticOperator::Divide,
            " udiv i32 ",
        ),
        (
            "unsignedRemainder",
            CheckedArithmeticOperator::Remainder,
            " urem i32 ",
        ),
        (
            "countTo",
            CheckedArithmeticOperator::Add,
            "llvm.sadd.with.overflow.i32",
        ),
    ] {
        let function = source_function(&unit, module, name);
        let checked = function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::CheckedArithmetic { operator, .. } => Some(operator),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(checked, [operator], "{name}");
        assert_eq!(
            function
                .blocks
                .iter()
                .filter(|block| matches!(
                    block.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Abort
                ))
                .count(),
            1,
            "{name}"
        );
        let body = llvm_body(&llvm, function);
        assert!(body.contains(fragment), "{name}: {body}");
        assert!(
            body.contains("call void @koven.abort.report"),
            "{name}: {body}"
        );
        assert!(body.contains("unreachable"), "{name}: {body}");
    }
    for name in ["choose", "countTo"] {
        let body = llvm_body(&llvm, source_function(&unit, module, name));
        assert!(body.contains("phi i32"), "{name}: {body}");
        assert!(body.contains("br i1"), "{name}: {body}");
    }
    assert!(llvm.contains(&format!(
        "target triple = \"{}\"",
        native_target_triple().unwrap()
    )));
    assert_eq!(llvm.matches("define i32 @main(").count(), 1);
    let wrapper = llvm
        .split("define i32 @main(")
        .nth(1)
        .unwrap()
        .split_once("\n}")
        .unwrap()
        .0;
    assert!(wrapper.contains(&format!("call void @f{}.{}()", entry.index(), main.name)));
    assert!(wrapper.contains("ret i32 0"));
    assert!(!llvm.contains("landingpad"));
    assert!(!llvm.contains("invoke "));
    assert_eq!(
        llvm,
        render_verified_program_with_entry(&program, entry).unwrap()
    );
    let (repeated, repeated_entry, repeated_llvm) = lower(&unit, "main");
    assert_eq!(
        (entry.module().index(), entry.index()),
        (repeated_entry.module().index(), repeated_entry.index())
    );
    assert_eq!(render_program(&program), render_program(&repeated));
    assert_eq!(
        llvm, repeated_llvm,
        "repeat lowering, with no normalization"
    );
    let rebuilt_unit = analyze();
    let (rebuilt, rebuilt_entry, rebuilt_llvm) = lower(&rebuilt_unit, "main");
    assert_eq!(
        (entry.module().index(), entry.index()),
        (rebuilt_entry.module().index(), rebuilt_entry.index())
    );
    assert_eq!(render_program(&program), render_program(&rebuilt));
    assert_eq!(
        llvm, rebuilt_llvm,
        "fresh frontend rebuild, with no normalization"
    );
}

#[test]
fn joint_unit_scalar_results_link_and_run() {
    let run = emit_link_and_run(SOURCE_NAME, SOURCE, "main");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, STDOUT, "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
#[cfg(unix)]
fn joint_unit_checked_failures_abort_after_the_complete_success_graph() {
    use std::os::unix::process::ExitStatusExt;

    let unit = analyze();
    for (entry_name, failed_function, message) in [
        ("abortSignedOverflow", "signedAdd", "integer overflow in +"),
        (
            "abortUnsignedUnderflow",
            "unsignedSubtract",
            "integer overflow in -",
        ),
        (
            "abortSignedDivision",
            "signedDivide",
            "integer overflow in /",
        ),
        (
            "abortUnsignedDivision",
            "unsignedDivide",
            "division by zero",
        ),
    ] {
        let (program, entry, _) = lower(&unit, entry_name);
        let module = program.module(entry.module()).unwrap();
        assert_eq!(module.functions.len(), HELPERS.len() + 2);
        let main = source_function(&unit, module, "main");
        for spec in HELPERS {
            source_function(&unit, module, spec.0);
        }
        let failed = source_function(&unit, module, failed_function);
        let calls = module
            .function(entry)
            .unwrap()
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::DirectCall { callee, .. } => Some(callee),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(calls, [main.id, failed.id]);
        let run = emit_link_and_run(SOURCE_NAME, SOURCE, entry_name);
        assert_eq!(run.status.signal(), Some(6), "{entry_name}: {run:?}");
        assert_eq!(run.stdout, STDOUT, "{entry_name}: {run:?}");
        let line = SOURCE
            .lines()
            .position(|line| line.starts_with(&format!("fun {failed_function}(")))
            .unwrap()
            + 1;
        let stderr = String::from_utf8(run.stderr).expect("UTF-8 abort report");
        assert!(
            stderr.starts_with(&format!("abort: {message}\n  at {SOURCE_NAME}:{line}:")),
            "{entry_name}: {stderr:?}"
        );
        assert!(
            stderr.ends_with('\n') && stderr.lines().count() == 2,
            "{entry_name}: {stderr:?}"
        );
    }
}
