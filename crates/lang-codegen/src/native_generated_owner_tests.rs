//! SPEC-0269: export bounded generated owner cases without launching native tools.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use inkwell::{
    context::Context,
    memory_buffer::MemoryBuffer,
    module::Module,
    values::{CallSiteValue, InstructionOpcode},
};
use lang_frontend::{
    analysis::{SingleFileAnalysisError, SingleFileStage, analyze_single_file},
    diagnostic::DiagnosticDetail,
    source::SourceMap,
    type_checking::standard_environments,
};

use crate::native_tests::boxed_enum_tests::{lower_to_llvm, write_counted_allocations};

const MINIMAL: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun inspect(item: Leaf): Unit { println(item.name) }
fun entry(): Unit {
    val leaf = Leaf("leaf-0")
    inspect(leaf)
    println("done")
}
"#;

#[allow(dead_code)]
pub const EXPORT_CALIBRATION_TEST: &str =
    "native_generated_owner_tests::export_generated_owner_calibration";

#[test]
fn export_generated_owner_case() {
    if let Some(path) = std::env::var_os("KOVEN_GENERATED_OWNER_CASE") {
        assert!(!path.is_empty(), "generated case path must not be empty");
        export_case(Path::new(&path));
    } else {
        let directory = CaseDirectory::new();
        directory.input(MINIMAL, 1);
        export_case(&directory.0);
        for name in [
            "case.raw.ll",
            "case.asan.ll",
            "case.counter.ll",
            "counter.c",
            "stages.tsv",
            "diagnostics.tsv",
        ] {
            assert!(directory.0.join(name).is_file(), "missing {name}");
        }
        assert_eq!(
            fs::read_to_string(directory.0.join("stages.tsv")).unwrap(),
            "parse\t0\nnames\t0\ntypes\t0\nownership\t0\n"
        );
        assert!(
            fs::read(directory.0.join("diagnostics.tsv"))
                .unwrap()
                .is_empty()
        );
        assert!(!directory.0.join("mutants.tsv").exists());
        assert!(!directory.0.join("case").exists(), "export never executes");
    }
}

#[test]
fn export_generated_owner_calibration() {
    if let Some(path) = std::env::var_os("KOVEN_GENERATED_OWNER_CALIBRATION") {
        assert!(!path.is_empty(), "calibration path must not be empty");
        export_calibration(Path::new(&path));
    } else {
        let directory = CaseDirectory::new();
        export_calibration(&directory.0);
        assert!(directory.0.join("mutants.tsv").is_file());
        let mutants = fs::read_to_string(directory.0.join("mutants.tsv")).unwrap();
        assert_eq!(mutants.lines().count(), 4, "{mutants}");
        for name in [
            "v1/clean.raw.ll",
            "v1/clean.asan.ll",
            "v1/clean.counter.ll",
            "v1/counter.c",
            "v1/fault-address.raw.ll",
            "v1/fault-address.asan.ll",
            "v1/fault-leak.raw.ll",
            "v1/fault-leak.counter.ll",
            "v1/fault-missing_deinit.raw.ll",
            "v1/fault-missing_deinit.counter.ll",
            "v2/clean.raw.ll",
            "v2/clean.asan.ll",
            "v2/clean.counter.ll",
            "v2/counter.c",
            "v2/fault-premature_holder_free.raw.ll",
            "v2/fault-premature_holder_free.counter.ll",
        ] {
            assert!(directory.0.join(name).is_file(), "missing {name}");
        }
    }
}

/// Observe only real frontend stages, including an accepted input with no backend inputs.
#[test]
fn export_generated_owner_frontend_case() {
    if let Some(path) = std::env::var_os("KOVEN_GENERATED_OWNER_CASE") {
        assert!(!path.is_empty(), "generated case path must not be empty");
        let _ = export_frontend(Path::new(&path));
    } else {
        let directory = CaseDirectory::new();
        fs::write(directory.0.join("case.ko"), MINIMAL).unwrap();
        assert!(export_frontend(&directory.0).is_some());
        assert_eq!(
            fs::read_to_string(directory.0.join("stages.tsv")).unwrap(),
            "parse\t0\nnames\t0\ntypes\t0\nownership\t0\n"
        );
        assert!(
            fs::read(directory.0.join("diagnostics.tsv"))
                .unwrap()
                .is_empty()
        );
        assert_frontend_only(&directory.0);
    }
}

#[test]
fn generated_owner_frontend_export_records_borrow_rejections() {
    let prefix = "class Leaf(val name: String) { deinit() {} }\n\
        fun inspect(item: Leaf): Unit { println(item.name) }\n\
        fun consume(own item: Leaf): Unit {}\n";
    for (source, code) in [
        (
            format!(
                "{prefix}fun entry(): Unit {{ val source = Leaf(\"枝\"); \
                 val moved = source; inspect(source); inspect(moved) }}"
            ),
            "L0131",
        ),
        (
            format!(
                "{prefix}fun invalid(item: Leaf): Unit {{ consume(item) }}\n\
                 fun entry(): Unit {{}}"
            ),
            "L0133",
        ),
    ] {
        let directory = CaseDirectory::new();
        fs::write(directory.0.join("case.ko"), source).unwrap();
        assert!(export_frontend(&directory.0).is_none());
        assert_eq!(
            fs::read_to_string(directory.0.join("stages.tsv")).unwrap(),
            "parse\t0\nnames\t0\ntypes\t0\nownership\t1\n"
        );
        let diagnostics = fs::read_to_string(directory.0.join("diagnostics.tsv")).unwrap();
        assert_eq!(diagnostics.lines().count(), 1, "{diagnostics}");
        assert!(diagnostics.starts_with(&format!("ownership\t{code}\t")));
        assert_frontend_only(&directory.0);
    }
}

fn assert_frontend_only(directory: &Path) {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["case.ko", "diagnostics.tsv", "stages.tsv"]);
}

/// A rejected input records the first failed phase, never LLVM or a native file.
#[test]
fn generated_owner_export_records_rejection_without_executing() {
    let prefix = "class Leaf { deinit() {} }\nfun consume(own item: Leaf): Unit {}\n";
    for (source, stage, code) in [
        ("fun entry(: Unit {}".to_owned(), "parse", None),
        ("fun entry(): Unit { missing() }".to_owned(), "names", None),
        (
            "fun entry(): Unit { val n: Int = true }".to_owned(),
            "types",
            None,
        ),
        (
            format!(
                "{prefix}fun entry(): Unit {{ val source = Leaf(); consume(source); consume(source) }}"
            ),
            "ownership",
            Some("L0131"),
        ),
        (
            format!(
                "{prefix}fun invalid(item: Leaf): Unit {{ consume(item) }}\nfun entry(): Unit {{}}"
            ),
            "ownership",
            Some("L0133"),
        ),
    ] {
        let directory = CaseDirectory::new();
        directory.input(&source, 0);
        export_case(&directory.0);
        let stages = fs::read_to_string(directory.0.join("stages.tsv")).unwrap();
        let last = stages.lines().last().expect("rejection must reach a phase");
        assert!(last.starts_with(&format!("{stage}\t")), "{stages}");
        assert!(!last.ends_with("\t0"), "{stages}");
        let diagnostics = fs::read_to_string(directory.0.join("diagnostics.tsv")).unwrap();
        assert!(!diagnostics.is_empty());
        if let Some(code) = code {
            assert_eq!(diagnostics.lines().count(), 1, "{diagnostics}");
            assert!(diagnostics.starts_with(&format!("ownership\t{code}\t")));
        }
        for artifact in [
            "case.raw.ll",
            "case.asan.ll",
            "case.counter.ll",
            "counter.c",
        ] {
            assert!(!directory.0.join(artifact).exists());
        }
    }
}

/// Invalid source is a frontend observation, never an expected exporter panic.
fn export_frontend(directory: &Path) -> Option<String> {
    assert!(directory.is_dir(), "generated case directory must exist");
    for name in ["stages.tsv", "diagnostics.tsv", "case.raw.ll", "counter.c"] {
        assert!(
            !directory.join(name).exists(),
            "generated output must be new: {name}"
        );
    }
    let source = fs::read_to_string(directory.join("case.ko")).expect("read generated case.ko");
    assert!(source.len() <= 8192, "generated source exceeds 8 KiB");
    let mut sources = SourceMap::new();
    let id = sources.add_source("case.ko", &source).unwrap();
    let (names, types) = standard_environments();
    let mut stages = String::new();
    let mut diagnostics = String::new();
    let analysis = analyze_single_file(
        &sources,
        id,
        &names,
        &types,
        |stage, found| {
            let stage = match stage {
                // Parser diagnostics include lexical diagnostics with their identity.
                SingleFileStage::Lexer => return Ok(()),
                SingleFileStage::Parser => "parse",
                SingleFileStage::NameResolution => "names",
                SingleFileStage::TypeChecking => "types",
                SingleFileStage::OwnershipChecking => "ownership",
            };
            stages.push_str(&format!("{stage}\t{}\n", found.len()));
            for diagnostic in found {
                let span = diagnostic.primary_span();
                let labels = diagnostic
                    .details()
                    .iter()
                    .filter_map(|detail| match detail {
                        DiagnosticDetail::Label(label) => {
                            Some(format!("{}:{}", label.span().start(), label.span().end()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                diagnostics.push_str(&format!(
                    "{stage}\t{}\t{}\t{}\t{labels}\n",
                    diagnostic.code(),
                    span.start(),
                    span.end(),
                ));
            }
            if found.is_empty() { Ok(()) } else { Err(()) }
        },
        |_| Ok(()),
    );
    fs::write(directory.join("stages.tsv"), stages).unwrap();
    fs::write(directory.join("diagnostics.tsv"), diagnostics).unwrap();
    match analysis {
        Ok(_) => Some(source),
        Err(SingleFileAnalysisError::Host(())) => None,
        Err(error) => panic!("internal frontend failure, not an invalid-source verdict: {error:?}"),
    }
}

fn export_case(directory: &Path) {
    let Some(source) = export_frontend(directory) else {
        return;
    };
    let expected: usize = fs::read_to_string(directory.join("expected-allocations.txt"))
        .expect("read expected allocation count")
        .trim()
        .parse()
        .expect("expected allocation count must be an integer");
    let order = if directory.join("expected-order.txt").exists() {
        Some(
            fs::read_to_string(directory.join("expected-order.txt"))
                .expect("read expected release order")
                .trim()
                .split(',')
                .map(|value| value.trim().parse::<usize>().expect("allocation identity"))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    assert!(
        (1..=9).contains(&expected),
        "generated valid fixture must allocate between one and nine owners"
    );
    let mut llvm = lower_to_llvm("case.ko", &source);
    if directory.join("fault.txt").is_file() {
        let fault = fs::read_to_string(directory.join("fault.txt")).unwrap();
        let fault = fault.trim();
        if fault == "missing_deinit" {
            let context = Context::create();
            let buffer = MemoryBuffer::create_from_memory_range_copy(llvm.as_bytes(), "fault-case");
            let module = context.create_module_from_ir(buffer).unwrap();
            inject_missing_holder_deinit_fault(&module);
            module
                .verify()
                .expect("fault missing_deinit module verifies");
            llvm = module.print_to_string().to_string();
        } else {
            panic!("unsupported fault: {fault}");
        }
    }
    fs::write(directory.join("case.raw.ll"), &llvm).unwrap();
    let context = Context::create();
    let buffer = MemoryBuffer::create_from_memory_range_copy(llvm.as_bytes(), "generated-case");
    let module = context.create_module_from_ir(buffer).unwrap();
    crate::native_sanitizer_tests::mark_address_sanitizer(&context, &module);
    module
        .verify()
        .expect("ASan attributes preserve valid LLVM");
    fs::write(
        directory.join("case.asan.ll"),
        module.print_to_string().to_bytes(),
    )
    .unwrap();
    write_counted_allocations(
        &llvm,
        expected,
        order.as_deref(),
        &directory.join("case.counter.ll"),
        &directory.join("counter.c"),
    );
}

const DEFAULT_CALIBRATION_V1: &str = "\
// 资源程序：UTF-8 span witness\n\
class Leaf(val name: String) { deinit() { println(this.name) } }\n\
fun inspect(item: Leaf): Unit { println(\"borrow\"); println(item.name) }\n\
fun consume(own item: Leaf): Unit { println(\"consume\") }\n\
fun entry(): Unit {\n\
    val source = Leaf(\"drop:leaf_b7af\")\n\
    val extra0 = Leaf(\"drop:extra0_b7af\")\n\
    val moved0 = source\n\
    val moved1 = moved0\n\
    inspect(moved1)\n\
    consume(moved1)\n\
    println(\"done\")\n\
}\n";

const DEFAULT_CALIBRATION_V2: &str = "\
// 资源程序：UTF-8 span witness\n\
class Leaf(val name: String) { deinit() { println(this.name) } }\n\
fun inspect(item: Leaf): Unit { println(\"borrow\"); println(item.name) }\n\
fun consume(own item: Leaf): Unit { println(\"consume\") }\n\
class Holder(var state: Leaf) { deinit() { println(\"drop:holder\") } }\n\
fun work(own stop: Boolean): Unit {\n\
    val holder = Holder(Leaf(\"drop:old_b7af\"))\n\
    val local = Leaf(\"drop:local_b7af\")\n\
    if (stop) { return }\n\
    val old = replace(&holder.state, Leaf(\"drop:new_b7af\"))\n\
    inspect(old)\n\
    println(\"helper-done\")\n\
}\n\
fun entry(): Unit { work(false); println(\"done\") }\n";

fn export_calibration(directory: &Path) {
    let v1_dir = directory.join("v1");
    let v2_dir = directory.join("v2");
    fs::create_dir_all(&v1_dir).unwrap();
    fs::create_dir_all(&v2_dir).unwrap();

    let v1_source = if v1_dir.join("case.ko").is_file() {
        fs::read_to_string(v1_dir.join("case.ko")).unwrap()
    } else {
        fs::write(v1_dir.join("case.ko"), DEFAULT_CALIBRATION_V1).unwrap();
        DEFAULT_CALIBRATION_V1.to_owned()
    };
    let v2_source = if v2_dir.join("case.ko").is_file() {
        fs::read_to_string(v2_dir.join("case.ko")).unwrap()
    } else {
        fs::write(v2_dir.join("case.ko"), DEFAULT_CALIBRATION_V2).unwrap();
        DEFAULT_CALIBRATION_V2.to_owned()
    };

    let v1_llvm = lower_to_llvm("v1.ko", &v1_source);
    let v2_llvm = lower_to_llvm("v2.ko", &v2_source);

    fs::write(v1_dir.join("clean.raw.ll"), &v1_llvm).unwrap();
    fs::write(v2_dir.join("clean.raw.ll"), &v2_llvm).unwrap();
    fs::write(v1_dir.join("clean.allocations.txt"), "2\n").unwrap();
    fs::write(v2_dir.join("clean.allocations.txt"), "4\n").unwrap();
    fs::write(v2_dir.join("clean.order.txt"), "0,2,3,1\n").unwrap();
    fs::write(
        v1_dir.join("clean.stdout"),
        "borrow\ndrop:leaf_b7af\nconsume\ndrop:leaf_b7af\ndone\ndrop:extra0_b7af\n",
    )
    .unwrap();
    fs::write(
        v2_dir.join("clean.stdout"),
        "borrow\ndrop:old_b7af\nhelper-done\ndrop:old_b7af\ndrop:local_b7af\ndrop:holder\ndrop:new_b7af\ndone\n",
    )
    .unwrap();

    // Clean ASan & Counter for V1
    {
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(v1_llvm.as_bytes(), "v1-clean");
        let module = context.create_module_from_ir(buffer).unwrap();
        crate::native_sanitizer_tests::mark_address_sanitizer(&context, &module);
        module.verify().expect("V1 clean ASan verifies");
        fs::write(
            v1_dir.join("clean.asan.ll"),
            module.print_to_string().to_bytes(),
        )
        .unwrap();
        write_counted_allocations(
            &v1_llvm,
            2,
            None,
            &v1_dir.join("clean.counter.ll"),
            &v1_dir.join("counter.c"),
        );
    }

    // Clean ASan & Counter for V2
    {
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(v2_llvm.as_bytes(), "v2-clean");
        let module = context.create_module_from_ir(buffer).unwrap();
        crate::native_sanitizer_tests::mark_address_sanitizer(&context, &module);
        module.verify().expect("V2 clean ASan verifies");
        fs::write(
            v2_dir.join("clean.asan.ll"),
            module.print_to_string().to_bytes(),
        )
        .unwrap();
        write_counted_allocations(
            &v2_llvm,
            4,
            Some(&[0, 2, 3, 1]),
            &v2_dir.join("clean.counter.ll"),
            &v2_dir.join("counter.c"),
        );
    }

    let mut mutants = Vec::new();

    // Mutant 1: Address fault in V1 inspect (load past Leaf struct)
    {
        let context = Context::create();
        let buffer =
            MemoryBuffer::create_from_memory_range_copy(v1_llvm.as_bytes(), "fault-address");
        let module = context.create_module_from_ir(buffer).unwrap();
        let target_fn = inject_address_fault(&context, &module);
        module.verify().expect("address fault verifies");
        let mutated = module.print_to_string().to_string();
        fs::write(v1_dir.join("fault-address.raw.ll"), &mutated).unwrap();
        crate::native_sanitizer_tests::mark_address_sanitizer(&context, &module);
        module.verify().expect("address fault ASan verifies");
        fs::write(
            v1_dir.join("fault-address.asan.ll"),
            module.print_to_string().to_bytes(),
        )
        .unwrap();
        mutants.push(format!(
            "address\taddress\tv1\t{target_fn}\tasan\theap-buffer-overflow:{target_fn}"
        ));
    }

    // Mutant 2: Leak fault in V1 drop (erase @free)
    {
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(v1_llvm.as_bytes(), "fault-leak");
        let module = context.create_module_from_ir(buffer).unwrap();
        let target_fn = inject_leak_fault(&module);
        module.verify().expect("leak fault verifies");
        let mutated = module.print_to_string().to_string();
        fs::write(v1_dir.join("fault-leak.raw.ll"), &mutated).unwrap();
        write_counted_allocations(
            &mutated,
            2,
            None,
            &v1_dir.join("fault-leak.counter.ll"),
            &v1_dir.join("fault-leak.counter.c"),
        );
        mutants.push(format!(
            "leak\tleak\tv1\t{target_fn}\tcounter\tpointer-ledger"
        ));
    }

    // Mutant 3: Missing deinit output in V1 Leaf.__deinit (erase write)
    {
        let context = Context::create();
        let buffer =
            MemoryBuffer::create_from_memory_range_copy(v1_llvm.as_bytes(), "fault-missing_deinit");
        let module = context.create_module_from_ir(buffer).unwrap();
        let target_fn = inject_missing_deinit_fault(&module);
        module.verify().expect("missing_deinit fault verifies");
        let mutated = module.print_to_string().to_string();
        fs::write(v1_dir.join("fault-missing_deinit.raw.ll"), &mutated).unwrap();
        write_counted_allocations(
            &mutated,
            2,
            None,
            &v1_dir.join("fault-missing_deinit.counter.ll"),
            &v1_dir.join("fault-missing_deinit.counter.c"),
        );
        mutants.push(format!(
            "missing_deinit\tmissing_deinit\tv1\t{target_fn}\toutput\tdrop:leaf_b7af"
        ));
    }

    // Mutant 4: Premature Holder free in V2 drop (free Holder before field is dropped)
    {
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(
            v2_llvm.as_bytes(),
            "fault-premature_holder_free",
        );
        let module = context.create_module_from_ir(buffer).unwrap();
        let target_fn = inject_premature_holder_free(&context, &module);
        module
            .verify()
            .expect("premature_holder_free fault verifies");
        let mutated = module.print_to_string().to_string();
        fs::write(v2_dir.join("fault-premature_holder_free.raw.ll"), &mutated).unwrap();
        write_counted_allocations(
            &mutated,
            4,
            Some(&[0, 2, 3, 1]),
            &v2_dir.join("fault-premature_holder_free.counter.ll"),
            &v2_dir.join("fault-premature_holder_free.counter.c"),
        );
        mutants.push(format!(
            "premature_holder_free\tpremature_holder_free\tv2\t{target_fn}\tcounter\tpointer-ledger"
        ));
    }

    fs::write(directory.join("mutants.tsv"), mutants.join("\n") + "\n").unwrap();
}

#[allow(clippy::collapsible_if)]
fn inject_address_fault<'ctx>(context: &'ctx Context, module: &Module<'ctx>) -> String {
    let function = module
        .get_functions()
        .find(|f| f.get_name().to_str().unwrap_or("").contains("inspect"))
        .expect("must find inspect function in module");
    let name = function.get_name().to_str().unwrap().to_owned();
    let mut target = None;
    for block in function.get_basic_blocks() {
        for instruction in block.get_instructions() {
            if instruction.get_opcode() == InstructionOpcode::Load
                && instruction.get_type().is_struct_type()
            {
                if let Some(op) = instruction.get_operand(0) {
                    if let Some(val) = op.value() {
                        if val.is_pointer_value() {
                            target = Some((instruction, val.into_pointer_value()));
                            break;
                        }
                    }
                }
            }
        }
        if target.is_some() {
            break;
        }
    }
    let (load, pointer) = target.expect("must find Leaf field load in inspect");
    let builder = context.create_builder();
    builder.position_before(&load);
    // SAFETY: i8 plus offset 32 creates an out-of-bounds pointer past the 24-byte Leaf allocation.
    let past_end = unsafe {
        builder.build_gep(
            context.i8_type(),
            pointer,
            &[context.i64_type().const_int(32, false)],
            "fault.past.end",
        )
    }
    .unwrap();
    assert!(load.set_operand(0, past_end));
    load.set_volatile(true).unwrap();
    name
}

#[allow(clippy::collapsible_if)]
fn inject_leak_fault(module: &Module<'_>) -> String {
    let mut target = None;
    for function in module.get_functions() {
        let fn_name = function.get_name().to_str().unwrap_or("");
        if fn_name.starts_with("koven.drop.") {
            if let Some(param) = function.get_first_param() {
                if param.is_pointer_value() {
                    for block in function.get_basic_blocks() {
                        for instruction in block.get_instructions() {
                            if let Ok(call) = CallSiteValue::try_from(instruction) {
                                if let Some(called) = call.get_called_fn_value() {
                                    if called.get_name().to_str().unwrap_or("") == "free"
                                        && instruction.get_operand(0).and_then(|op| op.value())
                                            == Some(param)
                                    {
                                        target = Some((function, instruction));
                                        break;
                                    }
                                }
                            }
                        }
                        if target.is_some() {
                            break;
                        }
                    }
                }
            }
        }
        if target.is_some() {
            break;
        }
    }
    let (function, free_call) = target.expect("must find owner free call in koven.drop");
    let name = function.get_name().to_str().unwrap().to_owned();
    free_call.erase_from_basic_block();
    name
}

#[allow(clippy::collapsible_if)]
fn inject_missing_deinit_fault(module: &Module<'_>) -> String {
    let mut target = None;
    for function in module.get_functions() {
        let fn_name = function.get_name().to_str().unwrap_or("");
        if fn_name.starts_with("koven.drop.") {
            for block in function.get_basic_blocks() {
                for instruction in block.get_instructions() {
                    if let Ok(call) = CallSiteValue::try_from(instruction) {
                        if let Some(called) = call.get_called_fn_value() {
                            let cname = called.get_name().to_str().unwrap_or("");
                            if cname.contains("__deinit") {
                                target = Some((
                                    called.get_name().to_str().unwrap().to_owned(),
                                    instruction,
                                ));
                                break;
                            }
                        }
                    }
                }
                if target.is_some() {
                    break;
                }
            }
        }
        if target.is_some() {
            break;
        }
    }
    let (name, call_inst) = target.expect("must find __deinit call in koven.drop");
    call_inst.erase_from_basic_block();
    name
}

#[allow(clippy::collapsible_if)]
fn inject_missing_holder_deinit_fault(module: &Module<'_>) -> String {
    let mut target = None;
    for function in module.get_functions() {
        let fn_name = function.get_name().to_str().unwrap_or("");
        if fn_name.starts_with("koven.drop.") {
            for block in function.get_basic_blocks() {
                for instruction in block.get_instructions() {
                    if let Ok(call) = CallSiteValue::try_from(instruction) {
                        if let Some(called) = call.get_called_fn_value() {
                            let cname = called.get_name().to_str().unwrap_or("");
                            if cname.contains("__deinit") && called.to_string().contains("11") {
                                target = Some((
                                    called.get_name().to_str().unwrap().to_owned(),
                                    instruction,
                                ));
                                break;
                            }
                        }
                    }
                }
                if target.is_some() {
                    break;
                }
            }
        }
        if target.is_some() {
            break;
        }
    }
    let (name, call_inst) = target.expect("must find Holder __deinit call in koven.drop");
    call_inst.erase_from_basic_block();
    name
}

#[allow(clippy::collapsible_if)]
fn inject_premature_holder_free<'ctx>(context: &'ctx Context, module: &Module<'ctx>) -> String {
    let mut target = None;
    for function in module.get_functions() {
        let fn_name = function.get_name().to_str().unwrap_or("");
        if fn_name.starts_with("koven.drop.") {
            let mut free_call = None;
            let mut field_drop = None;
            for block in function.get_basic_blocks() {
                for instruction in block.get_instructions() {
                    if let Ok(call) = CallSiteValue::try_from(instruction) {
                        if let Some(called) = call.get_called_fn_value() {
                            let cname = called.get_name().to_str().unwrap_or("");
                            if cname == "free" {
                                free_call = Some(instruction);
                            } else if cname.contains(".drop.t15") {
                                field_drop = Some(instruction);
                            }
                        }
                    }
                }
            }
            if let (Some(free_inst), Some(drop_inst)) = (free_call, field_drop) {
                target = Some((function, free_inst, drop_inst));
                break;
            }
        }
    }
    let (function, free_inst, drop_inst) =
        target.expect("must find Holder drop function with field drop and free");
    let name = function.get_name().to_str().unwrap().to_owned();
    let ptr_operand = free_inst
        .get_operand(0)
        .unwrap()
        .value()
        .unwrap()
        .into_pointer_value();
    let builder = context.create_builder();
    builder.position_before(&drop_inst);
    let free_fn = module.get_function("free").expect("free declared");
    builder
        .build_call(free_fn, &[ptr_operand.into()], "premature.free")
        .unwrap();
    free_inst.erase_from_basic_block();
    name
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct CaseDirectory(PathBuf);

impl CaseDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-generated-owner-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("generated test directory must be new");
        Self(path)
    }

    fn input(&self, source: &str, allocations: usize) {
        fs::write(self.0.join("case.ko"), source).unwrap();
        fs::write(
            self.0.join("expected-allocations.txt"),
            format!("{allocations}\n"),
        )
        .unwrap();
    }
}

impl Drop for CaseDirectory {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("generated owner artifacts retained: {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).expect("remove only this test's own artifacts");
        }
    }
}

#[test]
fn export_generated_owner_case_with_missing_deinit_fault() {
    let directory = CaseDirectory::new();
    directory.input(DEFAULT_CALIBRATION_V2, 4);
    fs::write(directory.0.join("fault.txt"), "missing_deinit\n").unwrap();
    export_case(&directory.0);
    for name in [
        "case.raw.ll",
        "case.asan.ll",
        "case.counter.ll",
        "counter.c",
        "stages.tsv",
        "diagnostics.tsv",
    ] {
        assert!(directory.0.join(name).is_file(), "missing {name}");
    }
    let raw_ll = fs::read_to_string(directory.0.join("case.raw.ll")).unwrap();
    assert!(raw_ll.contains("define internal void @f1.__deinit.t20"));
    assert_eq!(raw_ll.matches("call void @f1.__deinit.t20").count(), 0);
}
