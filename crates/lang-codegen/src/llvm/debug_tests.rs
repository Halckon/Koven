use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::source::SourceMap;

use crate::ssa::model::{Operation, Origin, Program, ScalarConstant, SsaTypeKind, TerminatorKind};

use super::{
    LlvmAdapterError, debug::DebugPlan, emit_verified_object, render_verified_program_with_debug,
};

static NEXT_OBJECT: AtomicU64 = AtomicU64::new(0);
const NATIVE_SOURCE: &str = "fun helper(): Unit {\n}\nfun app(): Unit {\n    helper()\n}\n";

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-dwarf-test-{}-{}",
            std::process::id(),
            NEXT_OBJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be creatable");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
    }
}

fn debug_program() -> (SourceMap, Program, crate::ssa::model::FunctionId) {
    let mut sources = SourceMap::default();
    let helper_source = sources
        .add_source("z-helper.ko", "fun helper() {}\n")
        .expect("helper source");
    let primary_text = "fun app() {\r\n    π\r\n}\r\n";
    let primary_source = sources
        .add_source("primary.ko", primary_text)
        .expect("primary source");
    let alpha_source = sources
        .add_source("a-helper.ko", "fun alpha() {}\n")
        .expect("alpha source");
    let helper_origin = Origin::Source(sources.span(helper_source, 0, 3).expect("helper span"));
    let function_origin =
        Origin::Source(sources.span(primary_source, 0, 3).expect("function span"));
    let pi_start = primary_text.find('π').expect("pi offset");
    let pi_span = sources
        .span(primary_source, pi_start, pi_start + 'π'.len_utf8())
        .expect("pi span");
    let return_start = primary_text.find('}').expect("return offset");
    let return_origin = Origin::Source(
        sources
            .span(primary_source, return_start, return_start + 1)
            .expect("return span"),
    );

    let mut program = Program::default();
    let module_id = program.add_module("debug");
    let module = program.module_mut(module_id).expect("module");
    let boolean = module.intern_type(SsaTypeKind::Boolean);

    let helper = module
        .add_function("helper", vec![], helper_origin.clone())
        .expect("helper function");
    let helper_block = module
        .function_mut(helper)
        .expect("helper function")
        .add_block(vec![], helper_origin.clone())
        .expect("helper block");
    module
        .function_mut(helper)
        .expect("helper function")
        .set_terminator(
            helper_block,
            TerminatorKind::Return { values: vec![] },
            helper_origin,
        )
        .expect("helper return");

    let alpha_origin = Origin::Source(sources.span(alpha_source, 0, 3).expect("alpha span"));
    let alpha = module
        .add_function("alpha", vec![], alpha_origin.clone())
        .expect("alpha function");
    let alpha_block = module
        .function_mut(alpha)
        .expect("alpha function")
        .add_block(vec![], alpha_origin.clone())
        .expect("alpha block");
    module
        .function_mut(alpha)
        .expect("alpha function")
        .set_terminator(
            alpha_block,
            TerminatorKind::Return { values: vec![] },
            alpha_origin,
        )
        .expect("alpha return");

    let fails = module
        .add_function(
            "fails",
            vec![],
            Origin::Source(sources.span(alpha_source, 4, 9).expect("fails span")),
        )
        .expect("fails function");
    let fails_origin = Origin::Synthetic {
        anchor: sources.span(alpha_source, 4, 9).expect("fails anchor"),
        reason: "runtime abort".to_owned(),
    };
    let fails_block = module
        .function_mut(fails)
        .expect("fails function")
        .add_block(vec![], fails_origin.clone())
        .expect("fails block");
    module
        .function_mut(fails)
        .expect("fails function")
        .set_terminator(fails_block, TerminatorKind::Abort, fails_origin)
        .expect("fails abort");

    let entry = module
        .add_function("app", vec![], function_origin.clone())
        .expect("app function");
    let entry_block = module
        .function_mut(entry)
        .expect("app function")
        .add_block(vec![], function_origin)
        .expect("app block");
    let function = module.function_mut(entry).expect("app function");
    function
        .append_instruction(
            entry_block,
            Operation::Constant(ScalarConstant::Boolean(true)),
            vec![crate::ssa::model::EntityType::Value(boolean)],
            Origin::Source(pi_span),
        )
        .expect("constant");
    function
        .append_instruction(
            entry_block,
            Operation::DirectCall {
                callee: helper,
                arguments: vec![],
            },
            vec![],
            Origin::Synthetic {
                anchor: pi_span,
                reason: "debug anchor".to_owned(),
            },
        )
        .expect("call");
    function
        .set_terminator(
            entry_block,
            TerminatorKind::Return { values: vec![] },
            return_origin,
        )
        .expect("return");
    (sources, program, entry)
}

fn native_debug_program(source_name: &str) -> (SourceMap, Program, crate::ssa::model::FunctionId) {
    let source_text = NATIVE_SOURCE;
    let mut sources = SourceMap::default();
    let source = sources
        .add_source(source_name, source_text)
        .expect("native debug source");
    let helper_start = source_text.find("fun helper").expect("helper offset");
    let helper_return = source_text[helper_start..]
        .find('}')
        .map(|offset| helper_start + offset)
        .expect("helper return offset");
    let app_start = source_text.find("fun app").expect("app offset");
    let call_start = source_text.rfind("helper()").expect("call offset");
    let app_return = source_text.rfind('}').expect("app return offset");
    let origin = |start: usize, len: usize| {
        Origin::Source(
            sources
                .span(source, start, start + len)
                .expect("native debug span"),
        )
    };

    let mut program = Program::default();
    let module_id = program.add_module("native-debug");
    let module = program.module_mut(module_id).expect("module");
    let helper = module
        .add_function("helper", vec![], origin(helper_start, 3))
        .expect("helper function");
    let helper_block = module
        .function_mut(helper)
        .expect("helper function")
        .add_block(vec![], origin(helper_start, 3))
        .expect("helper block");
    module
        .function_mut(helper)
        .expect("helper function")
        .set_terminator(
            helper_block,
            TerminatorKind::Return { values: vec![] },
            origin(helper_return, 1),
        )
        .expect("helper return");

    let entry = module
        .add_function("app", vec![], origin(app_start, 3))
        .expect("app function");
    let entry_block = module
        .function_mut(entry)
        .expect("app function")
        .add_block(vec![], origin(app_start, 3))
        .expect("app block");
    let function = module.function_mut(entry).expect("app function");
    function
        .append_instruction(
            entry_block,
            Operation::DirectCall {
                callee: helper,
                arguments: vec![],
            },
            vec![],
            origin(call_start, "helper()".len()),
        )
        .expect("helper call");
    function
        .set_terminator(
            entry_block,
            TerminatorKind::Return { values: vec![] },
            origin(app_return, 1),
        )
        .expect("app return");
    (sources, program, entry)
}

fn run(command: &mut Command, operation: &str) -> std::process::Output {
    command
        .output()
        .unwrap_or_else(|error| panic!("{operation} must launch: {error}"))
}

fn assert_success(output: &std::process::Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn debug_ir_maps_files_functions_unicode_and_synthetic_anchor_deterministically() {
    let (sources, program, entry) = debug_program();
    let plan = DebugPlan::build(&sources, &program.modules[0], entry).expect("debug plan");
    assert_eq!(
        plan.ordered_source_names(),
        ["a-helper.ko", "primary.ko", "z-helper.ko"]
    );
    let first = render_verified_program_with_debug(&program, &sources, entry).expect("debug IR");
    let second = render_verified_program_with_debug(&program, &sources, entry).expect("debug IR");

    assert_eq!(first, second);
    assert_eq!(first.matches("!DICompileUnit(").count(), 1);
    assert!(first.contains("language: DW_LANG_C"));
    assert!(first.contains("producer: \"kovenc\""));
    assert!(first.contains("emissionKind: LineTablesOnly"));
    assert!(first.contains("filename: \"primary.ko\", directory: \"\""));
    assert!(first.contains("filename: \"a-helper.ko\", directory: \"\""));
    assert!(first.contains("filename: \"z-helper.ko\", directory: \"\""));
    assert_eq!(first.matches("!DISubprogram(").count(), 4);
    assert!(first.contains("name: \"app\", linkageName: \"f3.app\""));
    assert!(first.contains("name: \"helper\", linkageName: \"f0.helper\""));
    assert!(first.contains("name: \"alpha\", linkageName: \"f1.alpha\""));
    assert!(first.contains("name: \"fails\", linkageName: \"f2.fails\""));
    assert!(first.contains("declare void @abort()"));
    assert!(!first.contains("!DISubprogram(name: \"abort\""));
    assert!(first.contains("line: 2, column: 5"));
    assert!(first.contains("line: 3, column: 1"));
    assert!(!first.contains("name: \"main\""));
}

#[test]
fn foreign_source_map_fails_before_debug_object_is_written() {
    let (_sources, program, entry) = debug_program();
    let mut foreign = SourceMap::default();
    foreign
        .add_source("primary.ko", "fun app() {}")
        .expect("foreign source");
    let object = PathBuf::from(format!(
        "/tmp/koven-foreign-debug-{}-{}.o",
        std::process::id(),
        NEXT_OBJECT.fetch_add(1, Ordering::Relaxed)
    ));
    assert!(matches!(
        emit_verified_object(&program, &foreign, entry, &object),
        Err(LlvmAdapterError::Debug(_))
    ));
    assert!(!object.exists());
}

#[test]
fn macho_line_table_resolves_a_koven_source_breakpoint_in_lldb() {
    let directory = TestDirectory::create();
    let source = directory.join("debug.ko");
    let source_name = source.to_str().expect("temporary path must be UTF-8");
    let (sources, program, entry) = native_debug_program(source_name);
    fs::write(&source, NATIVE_SOURCE).expect("source snapshot write");
    let object = directory.join("debug.o");
    let executable = directory.join("debug");
    emit_verified_object(&program, &sources, entry, &object).expect("debug object");

    let dwarf = run(
        Command::new("/usr/bin/dwarfdump")
            .arg("--debug-line")
            .arg(&object),
        "dwarfdump",
    );
    assert_success(&dwarf, "dwarfdump");
    let dwarf_text = String::from_utf8_lossy(&dwarf.stdout);
    assert!(
        dwarf_text.contains(source.parent().unwrap().to_str().unwrap()),
        "{dwarf_text}"
    );
    assert!(dwarf_text.contains("name: \"debug.ko\""), "{dwarf_text}");
    assert!(
        dwarf_text.lines().any(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            fields.get(1) == Some(&"4") && fields.get(2) == Some(&"5")
        }),
        "{dwarf_text}"
    );

    let link = run(
        Command::new("/usr/bin/clang")
            .arg(&object)
            .arg("-o")
            .arg(&executable),
        "clang link",
    );
    assert_success(&link, "clang link");
    let normal = run(&mut Command::new(&executable), "native executable");
    assert_eq!(normal.status.code(), Some(0));

    let lldb = run(
        Command::new("/usr/bin/lldb")
            .arg("--batch")
            .arg("--file")
            .arg(&executable)
            .arg("-o")
            .arg("breakpoint set --file debug.ko --line 4")
            .arg("-o")
            .arg("breakpoint list 1")
            .arg("-o")
            .arg("image lookup -n app"),
        "lldb",
    );
    assert_success(&lldb, "lldb");
    let lldb_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lldb.stdout),
        String::from_utf8_lossy(&lldb.stderr)
    );
    assert!(lldb_text.contains("locations = 1"), "{lldb_text}");
    assert!(lldb_text.contains("app + 4 at debug.ko:4:5"), "{lldb_text}");
}
