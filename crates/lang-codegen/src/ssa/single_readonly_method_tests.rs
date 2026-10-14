//! SPEC-0316: real source -> unchanged SSA/LLVM/native; no injected runtime or IR.
use super::{LoweringError, LoweringErrorKind, model::*, verify::verify_program};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_constant_ownership, check_ownership},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};
use std::{fs, process::Command};

#[derive(Clone, Copy, Debug)]
enum Pipeline {
    Single,
    Unit,
    ConstantUnit,
}

fn lower(
    text: &str,
    pipeline: Pipeline,
) -> (SourceMap, Result<(Program, FunctionId), LoweringError>) {
    let mut sources = SourceMap::new();
    let (source, parsed) = super::unit_lower_test_support::parsed(
        &mut sources,
        "app/readonly-method.ko",
        &format!("package app\n{text}"),
    );
    let (environment, types) = standard_environments();
    let result = match pipeline {
        Pipeline::Single => {
            let names = resolve_names(&sources, &parsed, &environment).unwrap();
            assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
            let typed = check_types(&sources, &parsed, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            let entry = names
                .symbols()
                .iter()
                .find(|s| s.name() == "entry")
                .unwrap()
                .id();
            super::lower_frontend::orchestrate::lower_scalar_file_with_entry(
                &sources, &parsed, &names, &typed, &owned, entry,
            )
        }
        Pipeline::Unit | Pipeline::ConstantUnit => {
            let inputs = [SourceUnitInput::new(
                "app",
                "app/readonly-method.ko",
                source,
                &parsed,
            )];
            if matches!(pipeline, Pipeline::ConstantUnit) {
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .unwrap()
                    .validate_constants()
                    .unwrap();
                let owned = check_compilation_unit_constant_ownership(
                    &sources, &inputs, &names, &types, &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                let entry = super::unit_lower_test_support::declaration(&names, "app", "entry");
                super::unit_lower::constant::lower_constant_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            } else {
                let (names, typed, owned) = super::unit_lower_test_support::analyze(
                    &sources,
                    &inputs,
                    &environment,
                    &types,
                );
                let entry = super::unit_lower_test_support::declaration(&names, "app", "entry");
                super::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            }
        }
    };
    (sources, result)
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
        format!(
            "const val SEVEN: Int = 7\nconst val ENABLED: Boolean = true\n{}",
            fixture
                .replace("Reading(7)", "Reading(SEVEN)")
                .replace("Switch(true)", "Switch(ENABLED)")
        )
    } else {
        fixture.to_owned()
    };
    let (sources, result) = lower(&text, pipeline);
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
    let (program, entry) = result.unwrap();
    verify_program(&program).unwrap();
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
