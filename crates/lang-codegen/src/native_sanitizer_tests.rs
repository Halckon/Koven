//! SPEC-0266: export actual Koven IR for bounded, Linux-only sanitizer execution.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_ARTIFACTS: AtomicU64 = AtomicU64::new(0);

use inkwell::{
    attributes::{Attribute, AttributeLoc},
    context::Context,
    memory_buffer::MemoryBuffer,
    module::Module,
    values::{BasicValue, CallSiteValue, FunctionValue, InstructionOpcode, InstructionValue},
};

const SOURCE: &str = r#"
class Cell(val number: Int) { deinit() { println("drop") } }
fun make(): Cell = Cell(37)
fun read(cell: Cell): Int = cell.number
fun entry(): Unit {
    val cell = make()
    if (read(cell) == 37) { println("read") }
}
"#;

/// The independent oracle is one Cell allocation, one drop and these two lines.
/// Leak checks use the unmodified malloc ABI, never the counter's global roots.
#[test]
fn asan_instruments_generated_user_runtime_and_drop() {
    let directory = Artifacts::create();
    fs::write(directory.0.join("fixture.ko"), SOURCE).unwrap();
    fs::write(directory.0.join("expected.stdout"), b"read\ndrop\n").unwrap();
    let original = crate::native_tests::boxed_enum_tests::lower_to_llvm("sanitizers.ko", SOURCE);
    fs::write(directory.0.join("original.ll"), &original).unwrap();
    let counter = directory.0.join("counter");
    fs::create_dir(&counter).unwrap();
    let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations_with_artifacts(
        &original, 1, &counter,
    );
    crate::native_tests::boxed_enum_tests::assert_success(&counted, b"read\ndrop\n");

    let mut manifest = String::new();
    for kind in ["clean", "user", "runtime", "drop", "leak"] {
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(original.as_bytes(), "fixture");
        let module = context.create_module_from_ir(buffer).unwrap();
        let (target, access) = inject(&context, &module, kind);
        module
            .verify()
            .expect("test mutation must remain valid LLVM IR");
        fs::write(
            directory.0.join(format!("{kind}.raw.ll")),
            module.print_to_string().to_bytes(),
        )
        .unwrap();
        let functions = mark_address_sanitizer(&context, &module);
        fs::write(
            directory.0.join(format!("{kind}.functions")),
            functions.join("\n"),
        )
        .unwrap();
        module
            .verify()
            .expect("attributes must preserve valid LLVM IR");
        fs::write(
            directory.0.join(format!("{kind}.asan.ll")),
            module.print_to_string().to_bytes(),
        )
        .unwrap();
        manifest.push_str(&format!("{kind}\t{target}\t{access}\n"));
    }
    fs::write(directory.0.join("fixtures.tsv"), manifest).unwrap();
    // The Linux driver owns compilation after this export returns. Keeping its
    // bounded commands outside Cargo avoids nested independent process groups.
    if directory.1 {
        return;
    }
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check_native_sanitizers.py");
    let result = Command::new("python3")
        .arg(script)
        .arg("--ir-checks")
        .arg(&directory.0)
        .arg("--clang")
        .arg(crate::test_support::ir_clang())
        .output()
        .expect("Python executes bounded Clang checks");
    assert!(
        result.status.success(),
        "artifacts: {}\n{result:?}",
        directory.0.display()
    );
}

/// A failed counter is useful only if its exact compile/run can be replayed.
#[test]
fn counter_failures_preserve_compile_and_run_evidence() {
    let directory = Artifacts::create();
    let original =
        crate::native_tests::boxed_enum_tests::lower_to_llvm("counter-evidence.ko", SOURCE);
    for failure in ["compile", "run"] {
        let counter = directory.0.join(failure);
        fs::create_dir(&counter).unwrap();
        if failure == "compile" {
            let invalid = format!("{original}\ninvalid llvm instruction\n");
            let result = std::panic::catch_unwind(|| {
                crate::native_tests::boxed_enum_tests::run_counted_allocations_with_artifacts(
                    &invalid, 1, &counter,
                )
            });
            assert!(
                result.is_err(),
                "malformed IR must fail its actual Clang invocation"
            );
        } else {
            let output =
                crate::native_tests::boxed_enum_tests::run_counted_allocations_with_artifacts(
                    &original, 2, &counter,
                );
            assert!(
                !output.status.success(),
                "the independent count mismatch must fail"
            );
            assert_eq!(fs::read(counter.join("run.stderr")).unwrap(), output.stderr);
            assert!(counter.join("boxed-enum-counts").is_file());
            assert!(counter.join("run.argv.nul").is_file());
            assert!(counter.join("run.status").is_file());
        }
        for name in [
            "boxed-enum-counts.ll",
            "counter.c",
            "compile.argv.nul",
            "compile.cwd",
            "compile.stderr",
            "compile.status",
            "version.stdout",
        ] {
            assert!(counter.join(name).is_file(), "{failure} must retain {name}");
        }
    }
}

fn definitions<'ctx>(module: &Module<'ctx>) -> Vec<FunctionValue<'ctx>> {
    module
        .get_functions()
        .filter(|function| function.count_basic_blocks() > 0)
        .collect()
}

/// Runtime helpers are emitted into user bodies and koven.drop functions alike.
/// Enumerating every definition prevents a user-only attribute from missing glue.
fn mark_address_sanitizer(context: &Context, module: &Module<'_>) -> Vec<String> {
    let kind = Attribute::get_named_enum_kind_id("sanitize_address");
    assert_ne!(kind, 0, "LLVM must expose the ASan function attribute");
    let functions = definitions(module);
    assert!(
        functions.len() >= 4,
        "fixture must include actual Koven callees and glue"
    );
    for function in &functions {
        assert!(
            function
                .get_enum_attribute(AttributeLoc::Function, kind)
                .is_none()
        );
        function.add_attribute(
            AttributeLoc::Function,
            context.create_enum_attribute(kind, 0),
        );
    }
    functions
        .iter()
        .map(|function| function.get_name().to_str().unwrap().to_owned())
        .collect()
}

fn named<'ctx>(module: &Module<'ctx>, suffix: &str) -> FunctionValue<'ctx> {
    let found: Vec<_> = definitions(module)
        .into_iter()
        .filter(|function| function.get_name().to_str().unwrap().ends_with(suffix))
        .collect();
    assert_eq!(found.len(), 1, "fixture must uniquely select {suffix}");
    found[0]
}

fn instructions(function: FunctionValue<'_>) -> Vec<InstructionValue<'_>> {
    function
        .get_basic_blocks()
        .into_iter()
        .flat_map(|block| block.get_instructions())
        .collect()
}

fn calls<'ctx>(
    module: &Module<'ctx>,
    name: &str,
) -> Vec<(FunctionValue<'ctx>, InstructionValue<'ctx>)> {
    definitions(module)
        .into_iter()
        .flat_map(|function| {
            instructions(function)
                .into_iter()
                .filter_map(move |instruction| {
                    let call = CallSiteValue::try_from(instruction).ok()?;
                    (call.get_called_fn_value()?.get_name().to_str().unwrap() == name)
                        .then_some((function, instruction))
                })
        })
        .collect()
}

/// Faults modify only this validated fixture's LLVM, never the Koven language.
/// Exactly one selected instruction per layer prevents a fixture drift from
/// silently testing another operation (or no operation at all).
fn inject(context: &Context, module: &Module<'_>, kind: &str) -> (String, &'static str) {
    let builder = context.create_builder();
    let (function, access) = match kind {
        "clean" => return ("-".into(), "-"),
        "user" => {
            let function = named(module, ".read");
            let loads: Vec<_> = instructions(function)
                .into_iter()
                .filter(|instruction| {
                    instruction.get_opcode() == InstructionOpcode::Load
                        && instruction.get_type() == context.i32_type().into()
                })
                .collect();
            assert_eq!(
                loads.len(),
                1,
                "read must contain one actual Int field load"
            );
            let load = loads[0];
            let pointer = load
                .get_operand(0)
                .unwrap()
                .value()
                .unwrap()
                .into_pointer_value();
            builder.position_before(&load);
            // SAFETY: i8 plus one integer index is a well-typed non-inbounds GEP.
            // The intentionally invalid address is used only by this child probe.
            let past_end = unsafe {
                builder.build_gep(
                    context.i8_type(),
                    pointer,
                    &[context.i64_type().const_int(4, false)],
                    "probe.past.end",
                )
            }
            .unwrap();
            assert!(load.set_operand(0, past_end));
            load.set_volatile(true).unwrap();
            (function, "load")
        }
        "runtime" => {
            let allocations = calls(module, "malloc");
            assert_eq!(
                allocations.len(),
                1,
                "fixture must allocate exactly one Cell"
            );
            let (function, allocation) = allocations[0];
            assert_eq!(function, named(module, ".make"));
            let size = allocation
                .get_operand(0)
                .unwrap()
                .value()
                .unwrap()
                .into_int_value();
            assert_eq!(size.get_zero_extended_constant(), Some(4));
            // Keep the actual runtime-generated payload store; only shrink malloc.
            assert!(allocation.set_operand(0, size.get_type().const_int(1, false)));
            (function, "store")
        }
        "drop" | "leak" => {
            let releases: Vec<_> = calls(module, "free")
                .into_iter()
                .filter(|(function, release)| {
                    // String literal glue also declares a conditional free. Select
                    // the Cell owner parameter itself, not an extracted buffer.
                    let owner = function.get_first_param();
                    owner.is_some_and(|value| value.is_pointer_value())
                        && release.get_operand(0).and_then(|operand| operand.value()) == owner
                })
                .collect();
            assert_eq!(releases.len(), 1, "fixture must have one owning drop free");
            let (function, release) = releases[0];
            assert!(
                function
                    .get_name()
                    .to_str()
                    .unwrap()
                    .starts_with("koven.drop.")
            );
            if kind == "leak" {
                release.erase_from_basic_block();
                (function, "-")
            } else {
                let pointer = release
                    .get_operand(0)
                    .unwrap()
                    .value()
                    .unwrap()
                    .into_pointer_value();
                builder.position_before(
                    &release
                        .get_next_instruction()
                        .expect("free precedes drop return"),
                );
                let read = builder
                    .build_load(context.i8_type(), pointer, "probe.after.free")
                    .unwrap();
                read.as_instruction_value()
                    .unwrap()
                    .set_volatile(true)
                    .unwrap();
                (function, "load")
            }
        }
        _ => panic!("unknown private fixture mutation: {kind}"),
    };
    (function.get_name().to_str().unwrap().to_owned(), access)
}

/// Preserve failures and explicitly requested CI artifacts; delete only this
/// successful test's own temporary directory.
struct Artifacts(PathBuf, bool);

impl Artifacts {
    fn create() -> Self {
        let requested = std::env::var_os("KOVEN_SANITIZER_ARTIFACTS");
        let keep = requested.is_some();
        let path = requested.map(PathBuf::from).unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "koven-sanitizers-{}-{}",
                std::process::id(),
                NEXT_ARTIFACTS.fetch_add(1, Ordering::Relaxed)
            ))
        });
        fs::create_dir(&path).expect("sanitizer artifact directory must be new");
        Self(path, keep)
    }
}

impl Drop for Artifacts {
    fn drop(&mut self) {
        if self.1 || std::thread::panicking() {
            eprintln!("sanitizer artifacts retained: {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).expect("remove only successful private probe artifacts");
        }
    }
}
