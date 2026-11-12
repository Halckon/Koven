//! SPEC-0291 CI-only live Map chain observations of real ordinary source.
//!
//! These tests intentionally execute raw LLVM and a read-only instrumented copy.
//! They are not ordinary local regression tests; no ignore or environment skip.

#[path = "native_ordered_map_tests/fixtures.rs"]
pub(crate) mod fixtures;
mod probe {
//! Single-source identity checks and one-call-only instrumentation of its checkpoint.

use inkwell::{
    context::Context,
    memory_buffer::MemoryBuffer,
    targets::TargetData,
    values::{AnyValue, InstructionOpcode},
};
use lang_frontend::name_resolution::SymbolKind;

use crate::{
    llvm::ordered_map_test_support::{self, OrderedMapLayout},
    ssa::model::{
        EntityType, FunctionId, LoanKind, MapContainerKind, Operation, Origin, Program, SsaTypeId,
        SsaTypeKind,
    },
};

use super::fixtures::Fixture;

const OBSERVER: &str = "koven_ordered_map_observe";

pub(super) struct Product {
    program: Program,
    checkpoint: FunctionId,
    map_type: SsaTypeId,
    pub(super) raw: String,
}

pub(super) struct Probe {
    pub(super) llvm: String,
    pub(super) layout: OrderedMapLayout,
    pub(super) identity: String,
}

pub(super) fn lower(fixture: &Fixture) -> Product {
    let analysis = super::super::UnitSource::analyze(fixture.name, fixture.source)
        .expect("original checkpoint source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let declaration =
        analysis.declaration_span(analysis.declaration("checkpoint", SymbolKind::Function));
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .expect("ordinary checkpoint source must lower without bypassing SSA verification");
    crate::ssa::verify::verify_program(&program).expect("real source SSA must verify");
    assert_eq!(program.modules.len(), 1, "single-source route only");
    let module = program.module(entry.module()).unwrap();
    let checkpoints: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.origin == Origin::Source(declaration))
        .collect();
    assert_eq!(checkpoints.len(), 1, "unique source-bound FunctionId");
    // Its source item fixes identity; instrumentation uses this returned SSA body's actual name.
    let checkpoint = checkpoints[0];
    let Origin::Source(span) = &checkpoint.origin else {
        panic!("checkpoint must be source-authored")
    };
    assert_eq!(
        analysis.sources().source_name(span.source_id()).unwrap(),
        fixture.name
    );
    assert!(
        analysis
            .sources()
            .slice(*span)
            .unwrap()
            .starts_with("fun checkpoint(")
    );
    assert!(checkpoint.return_types.is_empty());
    assert_eq!(checkpoint.borrow_return, None);
    assert_eq!(checkpoint.carrier_return, None);
    assert_eq!(checkpoint.receiver, None);
    let parameters = &checkpoint.blocks[0].parameters;
    assert_eq!(parameters.len(), 2);
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    } = checkpoint.entity(parameters[0]).unwrap().ty
    else {
        panic!("checkpoint's first parameter must be a shared Map loan")
    };
    let EntityType::Value(step_type) = checkpoint.entity(parameters[1]).unwrap().ty else {
        panic!("checkpoint's explicit own step must be a Value")
    };
    let integer = SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    };
    assert_eq!(module.type_kind(step_type), Some(&integer));
    let (kind, key, value) = module.map_container(map_type).unwrap();
    assert_eq!(
        kind,
        if fixture.readonly {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        }
    );
    assert_eq!(module.type_kind(value), Some(&integer));
    assert_eq!(
        module.type_kind(key),
        Some(if fixture.string_keys {
            &SsaTypeKind::StringOwner
        } else {
            &integer
        })
    );
    let sizes: Vec<_> = checkpoint
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::MapSize { owner } = &instruction.operation {
                Some(*owner)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        sizes,
        [parameters[0]],
        "the checkpoint must read precisely its borrowed Map"
    );
    let calls = module.functions.iter().flat_map(|function| &function.instructions).filter(|instruction| {
        matches!(&instruction.operation, Operation::DirectCall { callee, .. } if *callee == checkpoint.id)
    }).count();
    assert_eq!(
        calls,
        fixture.snapshots.len(),
        "every declared checkpoint must have one source call"
    );
    let checkpoint_id = checkpoint.id;
    let raw = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("untouched production LLVM must verify");
    Product {
        program,
        checkpoint: checkpoint_id,
        map_type,
        raw,
    }
}

pub(super) fn instrument(product: &Product) -> Probe {
    let context = Context::create();
    let llvm = context
        .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
            product.raw.as_bytes(),
            "ordered-map.raw.ll",
        ))
        .expect("parse untouched production LLVM");
    llvm.verify().expect("raw LLVM verification");
    assert!(
        llvm.get_function(OBSERVER).is_none(),
        "observer symbol must not preexist"
    );
    assert!(
        llvm.get_global(OBSERVER).is_none(),
        "observer global must not preexist"
    );
    let module = product.program.module(product.checkpoint.module()).unwrap();
    let checkpoint = module.function(product.checkpoint).unwrap();
    let symbol = format!("f{}.{}", checkpoint.id.index(), checkpoint.name);
    let target = llvm
        .get_function(&symbol)
        .expect("exact real FunctionId/name LLVM symbol");
    assert_eq!(target.count_params(), 2);
    assert!(target.get_type().get_return_type().is_none());
    assert!(!target.get_type().is_var_arg());
    let header_pointer = target.get_nth_param(0).unwrap();
    assert!(header_pointer.is_pointer_value());
    let step = target.get_nth_param(1).unwrap();
    assert_eq!(step.into_int_value().get_type().get_bit_width(), 32);
    let data_layout = llvm.get_data_layout().as_str().to_str().unwrap().to_owned();
    assert!(!data_layout.is_empty());
    let layout = ordered_map_test_support::describe(module, product.map_type, &data_layout);
    let header = context
        .get_struct_type(&layout.header_name)
        .expect("bound named header in raw LLVM");
    let data = TargetData::create(&data_layout);
    assert_eq!(
        llvm.get_triple().as_str().to_str().unwrap(),
        crate::llvm::native_target_triple().unwrap()
    );
    assert_eq!(data.get_pointer_byte_size(None) * 8, layout.pointer_bits);
    assert_eq!(header.count_fields(), 6);
    assert!(header.get_field_type_at_index(0).unwrap().is_pointer_type());
    for index in 1..6 {
        assert_eq!(
            header
                .get_field_type_at_index(index)
                .unwrap()
                .into_int_type()
                .get_bit_width(),
            layout.pointer_bits
        );
    }
    assert_eq!(data.get_abi_size(&header), layout.header_size);
    assert_eq!(data.get_abi_alignment(&header), layout.header_alignment);
    for (index, offset) in layout.header_offsets.iter().enumerate() {
        assert_eq!(data.offset_of_element(&header, index as u32), Some(*offset));
    }
    let loads: Vec<_> = target
        .get_basic_blocks()
        .into_iter()
        .flat_map(|block| block.get_instructions())
        .filter(|instruction| {
            instruction.get_opcode() == InstructionOpcode::Load
                && instruction.get_type().is_struct_type()
                && instruction.get_type().into_struct_type() == header
        })
        .collect();
    assert_eq!(loads.len(), 1, "one actual header load from MapSize");
    assert_eq!(loads[0].get_type().into_struct_type(), header);
    assert_eq!(
        loads[0].get_operand(0).unwrap().unwrap_value(),
        header_pointer
    );
    let slots: Vec<_> = llvm
        .get_functions()
        .flat_map(|function| function.get_basic_blocks())
        .flat_map(|block| block.get_instructions())
        .filter(|instruction| instruction.get_opcode() == InstructionOpcode::GetElementPtr)
        .filter_map(|instruction| {
            let ty = instruction.get_gep_source_element_type().unwrap();
            (ty.print_to_string().to_string() == layout.slot_type).then(|| ty.into_struct_type())
        })
        .collect();
    assert!(
        !slots.is_empty(),
        "actual raw Map slot GEP must match the real production slot type"
    );
    for slot in slots {
        if let Some(offsets) = layout.string_offsets {
            let string = slot.get_field_type_at_index(1).unwrap().into_struct_type();
            assert_eq!(string.count_fields(), 3);
            assert!(string.get_field_type_at_index(0).unwrap().is_pointer_type());
            for index in [1, 2] {
                assert_eq!(
                    string
                        .get_field_type_at_index(index)
                        .unwrap()
                        .into_int_type()
                        .get_bit_width(),
                    layout.pointer_bits
                );
            }
            for (index, offset) in offsets.iter().enumerate() {
                assert_eq!(data.offset_of_element(&string, index as u32), Some(*offset));
            }
        }
        assert_eq!(data.get_abi_size(&slot), layout.slot_stride);
        assert_eq!(data.get_abi_alignment(&slot), layout.slot_alignment);
        for (index, offset) in layout.slot_offsets.iter().enumerate() {
            assert_eq!(data.offset_of_element(&slot, index as u32), Some(*offset));
        }
    }
    let before = llvm.print_to_string().to_string();
    let original_functions: Vec<_> = llvm
        .get_functions()
        .map(|function| {
            (
                function.get_name().to_str().unwrap().to_owned(),
                function.print_to_string().to_string(),
            )
        })
        .collect();
    let original_blocks: Vec<_> = target
        .get_basic_blocks()
        .into_iter()
        .map(|block| {
            (
                block.get_name().to_str().unwrap().to_owned(),
                block
                    .get_instructions()
                    .map(|instruction| instruction.print_to_string().to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let observer = llvm.add_function(OBSERVER, target.get_type(), None);
    let first = target
        .get_first_basic_block()
        .unwrap()
        .get_instructions()
        .find(|instruction| instruction.get_opcode() != InstructionOpcode::Phi)
        .unwrap();
    let builder = context.create_builder();
    builder.position_before(&first);
    let inserted = builder
        .build_call(observer, &[header_pointer.into(), step.into()], "")
        .expect("insert exactly one void observer call")
        .try_as_basic_value()
        .unwrap_instruction();
    llvm.verify()
        .expect("one-call checkpoint instrumentation must verify");
    let after_blocks: Vec<_> = target
        .get_basic_blocks()
        .into_iter()
        .map(|block| {
            (
                block.get_name().to_str().unwrap().to_owned(),
                block
                    .get_instructions()
                    .filter(|instruction| *instruction != inserted)
                    .map(|instruction| instruction.print_to_string().to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(
        original_blocks, after_blocks,
        "original checkpoint instructions and CFG must remain byte-identical"
    );
    for (name, original) in &original_functions {
        if name != &symbol {
            assert_eq!(
                &llvm
                    .get_function(name)
                    .unwrap()
                    .print_to_string()
                    .to_string(),
                original,
                "unrelated original function changed: {name}"
            );
        }
    }
    assert_eq!(llvm.get_functions().count(), original_functions.len() + 1);
    let instrumented = llvm.print_to_string().to_string();
    let added_call = inserted.print_to_string().to_string();
    let declaration = observer.print_to_string().to_string();
    let normalize = |text: &str| {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let after_without_additions: Vec<_> = instrumented
        .lines()
        .filter(|line| {
            !line.trim().is_empty()
                && line.trim() != added_call.trim()
                && line.trim() != declaration.trim()
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(
        normalize(&before),
        after_without_additions,
        "globals, types, attributes and all other original module text must remain unchanged"
    );
    let contract = module.map_container(product.map_type).unwrap();
    let identity = format!(
        "checkpoint_id={:?}\nsource checkpoint={:?}\nLLVM symbol={symbol}\nmap={:?}\nMapKind/K/V={contract:?}\ntriple={}\ndata_layout={data_layout}\nlayout={layout:#?}\n",
        checkpoint.id,
        checkpoint.origin,
        product.map_type,
        llvm.get_triple().as_str().to_str().unwrap()
    );
    Probe {
        llvm: instrumented,
        layout,
        identity,
    }
}
}
mod runner {
//! Bounded native fixture runs; Clang follows the existing synchronous helper contract.
//! Compiler/version calls have no test-internal deadline. Failure artifacts are retained.

use crate::test_support::diagnostics::assert_diagnostic;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Artifacts(PathBuf);

struct Run {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Artifacts {
    pub(super) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-ordered-map-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).expect("unique ordered-map artifact directory");
        Self(path)
    }

    pub(super) fn write(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).expect("write ordered-map evidence");
        path
    }

    fn run(&self, label: &str, command: &mut Command, budget: Duration) -> Run {
        let command_text = format!("{command:?}\nbudget={budget:?}\n");
        self.write(&format!("{label}.command"), &command_text);
        let stdout_path = self.0.join(format!("{label}.stdout"));
        let stderr_path = self.0.join(format!("{label}.stderr"));
        // Files avoid pipe backpressure while the parent performs bounded try_wait.
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(File::create(&stdout_path).unwrap()))
            .stderr(Stdio::from(File::create(&stderr_path).unwrap()));
        let mut child = command.spawn().unwrap_or_else(|error| {
            panic!(
                "{label}: spawn failed: {error}\n{command_text}\nartifacts={}",
                self.0.display()
            )
        });
        let deadline = Instant::now() + budget;
        let mut failure = None;
        let mut status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    failure = Some(format!("timeout after {budget:?}"));
                    break None;
                }
                Err(error) => {
                    failure = Some(format!("try_wait failed: {error}"));
                    break None;
                }
            }
        };
        if let Some(reason) = &mut failure {
            // The generated fixture has no subprocess-spawning operation.
            // Kill only this Child, and never block indefinitely after kill fails.
            let child_kill = child.kill();
            let cleanup_deadline = Instant::now() + Duration::from_secs(2);
            let mut reap_error = None;
            loop {
                match child.try_wait() {
                    Ok(Some(reaped)) => {
                        status = Some(reaped);
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        reap_error = Some(error.to_string());
                    }
                }
                if Instant::now() >= cleanup_deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            *reason = format!(
                "{reason}; child_kill={child_kill:?}; reaped={}; reap_error={reap_error:?}",
                status.is_some()
            );
            // Even failed kill/reap is a bounded infrastructure failure. There
            // is deliberately no unbounded Child::wait fallback on any path.
        }
        let stdout = fs::read(&stdout_path).unwrap();
        let stderr = fs::read(&stderr_path).unwrap();
        self.write(
            &format!("{label}.status"),
            format!("{status:?}\nfailure={failure:?}\n"),
        );
        assert_diagnostic!(
            failure.is_none() && status.is_some_and(|status| status.success()),
            "{label}: failure={failure:?}; status={status:?}\n{command_text}\nstdout:\n{}\nstderr:\n{}\nartifacts={}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
            self.0.display()
        );
        Run {
            status: status.expect("successful child was reaped"),
            stdout,
            stderr,
        }
    }

    fn compiler(&self, label: &str, command: &mut Command) -> Run {
        // Like the existing native LLVM helpers, compilation is synchronous;
        // its driver/cc1/linker tree is not claimed to have an internal deadline.
        let command_text = format!("{command:?}\ncompiler_deadline=none\n");
        self.write(&format!("{label}.command"), &command_text);
        let output = command.output().unwrap_or_else(|error| {
            panic!(
                "{label}: compiler launch failed: {error}\n{command_text}\nartifacts={}",
                self.0.display()
            )
        });
        self.write(&format!("{label}.stdout"), &output.stdout);
        self.write(&format!("{label}.stderr"), &output.stderr);
        self.write(&format!("{label}.status"), format!("{}\n", output.status));
        assert_diagnostic!(
            output.status.success(),
            "{label}: status={}\n{command_text}\nstdout:\n{}\nstderr:\n{}\nartifacts={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            self.0.display()
        );
        Run {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }

    pub(super) fn check_compiler(&self) {
        let run = self.compiler(
            "clang-version",
            Command::new(crate::test_support::ir_clang()).arg("--version"),
        );
        let version = String::from_utf8_lossy(&run.stdout);
        assert!(
            version.contains("clang version 21."),
            "LLVM 21 IR requires Clang 21: {version}; artifacts={}",
            self.0.display()
        );
    }

    pub(super) fn compile_and_run(
        &self,
        label: &str,
        ir: &Path,
        observer: Option<&Path>,
        stdout: &str,
        stderr: &str,
    ) {
        let executable = self.0.join(label);
        let mut command = Command::new(crate::test_support::ir_clang());
        command.arg("-O0").arg(ir);
        if let Some(observer) = observer {
            command.arg("-std=c11").arg(observer);
        }
        command.arg("-o").arg(&executable);
        self.compiler(&format!("{label}-compile"), &mut command);
        // The fixture itself runs in well under a second, but the full parallel
        // suite can starve the child long enough to trip a tight wall clock.
        let run = self.run(
            &format!("{label}-run"),
            &mut Command::new(&executable),
            Duration::from_secs(60),
        );
        assert_eq!(run.status.code(), Some(0), "{label} must exit normally");
        assert_diagnostic!(
            run.stdout == stdout.as_bytes() && run.stderr == stderr.as_bytes(),
            "{label}: strict output mismatch\nexpected stdout:\n{stdout}\nactual stdout:\n{}\nexpected stderr:\n{stderr}\nactual stderr:\n{}\nartifacts={}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
            self.0.display()
        );
    }

    pub(super) fn finish(self) {
        // This is temporary evidence, not a claim of durable CI artifact publication.
        fs::remove_dir_all(&self.0).expect("remove successful owned fixture artifacts");
    }
}
}

use std::fmt::Write;

use crate::llvm::ordered_map_test_support::OrderedMapLayout;

use fixtures::{Fixture, Key};

fn observer(fixture: &Fixture, layout: &OrderedMapLayout) -> String {
    let mut text = String::from(
        "#include <stdint.h>\n#include <stddef.h>\n\
         typedef struct { int64_t integer; const char *bytes; size_t length; int64_t value; int slot; } ExpectedEntry;\n\
         typedef struct { uint64_t step, capacity, tombstones; size_t size; const ExpectedEntry *entries; } Snapshot;\n",
    );
    writeln!(
        text,
        "enum {{ KEY_STRING={}, POINTER_BITS={}, S_STRIDE={},",
        u8::from(fixture.string_keys),
        layout.pointer_bits,
        layout.slot_stride
    )
    .unwrap();
    for (name, offset) in [
        "H_BUFFER",
        "H_SIZE",
        "H_CAPACITY",
        "H_TOMBSTONES",
        "H_FIRST",
        "H_LAST",
    ]
    .into_iter()
    .zip(layout.header_offsets)
    {
        writeln!(text, "{name}={offset},").unwrap();
    }
    for (name, offset) in ["S_STATE", "S_KEY", "S_VALUE", "S_PREV", "S_NEXT"]
        .into_iter()
        .zip(layout.slot_offsets)
    {
        writeln!(text, "{name}={offset},").unwrap();
    }
    let strings = layout.string_offsets.unwrap_or([0, 0]);
    writeln!(
        text,
        "STRING_DATA={}, STRING_LENGTH={} }};",
        strings[0], strings[1]
    )
    .unwrap();
    for (index, snapshot) in fixture.snapshots.iter().enumerate() {
        if snapshot.entries.is_empty() {
            continue;
        }
        writeln!(text, "static const ExpectedEntry ENTRIES_{index}[] = {{").unwrap();
        for entry in &snapshot.entries {
            let (integer, bytes, length) = match entry.key {
                Key::Int(integer) => (integer, "NULL".to_owned(), 0),
                Key::String(value) => {
                    let mut bytes = String::from("\"");
                    for byte in value.as_bytes() {
                        write!(bytes, "\\x{byte:02x}").unwrap();
                    }
                    bytes.push('"');
                    (0, bytes, value.len())
                }
            };
            let slot = entry.slot.map_or(-1, |value| i32::try_from(value).unwrap());
            writeln!(
                text,
                "{{ {integer}, {bytes}, {length}, {}, {slot} }},",
                entry.value
            )
            .unwrap();
        }
        text.push_str("};\n");
    }
    text.push_str("static const Snapshot EXPECTED[] = {\n");
    for (index, snapshot) in fixture.snapshots.iter().enumerate() {
        let entries = if snapshot.entries.is_empty() {
            "NULL".to_owned()
        } else {
            format!("ENTRIES_{index}")
        };
        writeln!(
            text,
            "{{ {}, {}, {}, {}, {entries} }},",
            snapshot.step,
            snapshot.capacity,
            snapshot.tombstones,
            snapshot.entries.len()
        )
        .unwrap();
    }
    text.push_str(
        "};\nstatic const size_t SNAPSHOT_COUNT = sizeof(EXPECTED) / sizeof(EXPECTED[0]);\n",
    );
    text.push_str(include_str!("native_ordered_map_tests/observer.c"));
    text
}

fn run(fixture: Fixture) {
    assert!(!fixture.snapshots.is_empty());
    let artifacts = runner::Artifacts::new(fixture.name);
    artifacts.write("fixture.ko", fixture.source);
    artifacts.write(
        "expected.txt",
        format!(
            "{:#?}\nstdout:\n{}stderr:\n{}",
            fixture.snapshots,
            fixture.stdout(),
            fixture.stderr()
        ),
    );
    let product = probe::lower(&fixture);
    let raw = artifacts.write("raw.ll", &product.raw);
    artifacts.check_compiler();
    // Do not even instrument until the real unmodified artifact has passed.
    artifacts.compile_and_run("raw", &raw, None, &fixture.stdout(), "");
    let probe = probe::instrument(&product);
    artifacts.write("identity.txt", &probe.identity);
    let ir = artifacts.write("probe.ll", &probe.llvm);
    let observer = artifacts.write("observer.c", observer(&fixture, &probe.layout));
    artifacts.compile_and_run(
        "probe",
        &ir,
        Some(&observer),
        &fixture.stdout(),
        &fixture.stderr(),
    );
    artifacts.finish();
}

#[test]
fn ordered_map_empty_singleton_and_endpoint_deletions() {
    let cases = fixtures::empty_and_endpoints();
    assert_eq!(
        cases.iter().map(|fixture| fixture.name).collect::<Vec<_>>(),
        ["int_readonly.ko", "int_empty.ko", "int_endpoints.ko"]
    );
    for fixture in cases {
        run(fixture);
    }
}

#[test]
fn ordered_map_overwrite_and_reinsert_preserve_order() {
    run(fixtures::overwrite_reinsert());
}

#[test]
fn ordered_map_collision_tombstone_reuse_preserves_order() {
    run(fixtures::collision());
}

#[test]
fn ordered_map_overwrite_at_growth_boundary_preserves_order() {
    run(fixtures::overwrite_growth());
}

#[test]
fn ordered_map_multiple_rehashes_preserve_order() {
    run(fixtures::multiple_rehashes());
}

#[test]
fn ordered_map_empty_tombstone_rehash_resets_endpoints() {
    run(fixtures::empty_tombstone_growth());
}

#[test]
fn ordered_map_string_key_cfg_preserves_order() {
    let cases = fixtures::string_cases();
    assert_eq!(
        cases.iter().map(|fixture| fixture.name).collect::<Vec<_>>(),
        [
            "string_readonly.ko",
            "string_empty.ko",
            "string_endpoints.ko",
            "string_overwrite.ko",
            "string_collision.ko",
            "string_growth.ko",
            "string_rehash.ko",
            "string_empty_growth.ko",
            "string_empty_key.ko",
        ]
    );
    for fixture in cases {
        run(fixture);
    }
}
