//! The tutorial's exact three-source program, with owner-identity checks in test-only LLVM.
use super::*;
use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;

#[test]
fn unit_for_report_releases_previous_final_parent_and_argv_owners_once_in_order() {
    for constants in [false, true] {
        let (llvm, string) = report_program(constants);
        for arguments in [
            vec![],
            vec!["alpha"],
            vec!["alpha", "你好", "tail"],
            vec![""],
        ] {
            let run = counted_report(&llvm, string, &arguments);
            let expected = format!(
                "{}processed\ndone\n",
                arguments
                    .iter()
                    .map(|argument| format!("{argument}\n"))
                    .collect::<String>()
            );
            crate::native_tests::boxed_enum_tests::assert_success(&run, expected.as_bytes());
        }
    }
}

fn tutorial_source(name: &str) -> &str {
    let tutorial = include_str!("../../../../docs/tutorials/koven-tour.md");
    let opener = format!("```koven {name}\n");
    let (_, rest) = tutorial
        .split_once(&opener)
        .expect("named tutorial fixture exists");
    let (source, _) = rest.split_once("\n```").expect("tutorial fixture closes");
    assert!(!source.is_empty());
    source
}

fn report_program(constants: bool) -> (String, usize) {
    let mut sources = SourceMap::new();
    let parts = [
        ("app/model.ko", "parameter-report-model"),
        ("app/processor.ko", "parameter-report-processor"),
        ("app/main.ko", "parameter-report"),
    ];
    let files = parts
        .iter()
        .map(|(path, name)| parsed(&mut sources, path, tutorial_source(name)))
        .collect::<Vec<_>>();
    let inputs = parts
        .iter()
        .zip(&files)
        .map(|((path, _), (source, file))| SourceUnitInput::new("root", path, *source, file))
        .collect::<Vec<_>>();
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "main")
        .unwrap()
        .id();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
    let (program, function) = if constants {
        let typed = result.validate_constants().unwrap();
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .unwrap()
        .validate()
        .unwrap();
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap()
    } else {
        let typed = result.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap()
    };
    let string = program.modules[0]
        .types
        .iter()
        .position(|ty| *ty == crate::ssa::model::SsaTypeKind::StringOwner)
        .unwrap();
    let plan = super::super::native_unit_entry_plan(
        &program,
        NativeUnitEntry::BorrowedArguments(entry),
        function,
    )
    .unwrap();
    (
        crate::llvm::render_verified_program_with_entry_plan(&program, plan).unwrap(),
        string,
    )
}

fn counted_report(llvm: &str, string: usize, arguments: &[&str]) -> std::process::Output {
    let bits = usize::BITS;
    let mut instrumented = String::new();
    let mut owner_parameter = None;
    let mut injected = false;
    for line in llvm.lines() {
        instrumented.push_str(
            &line
                .replace("@malloc(", "@counted_malloc(")
                .replace("@free(", "@counted_free("),
        );
        instrumented.push('\n');
        if line.starts_with("define ") && line.contains(&format!("@koven.drop.t{string}(")) {
            let parameters = line.split_once('(').unwrap().1.split_once(')').unwrap().0;
            owner_parameter = Some(parameters.to_owned());
        } else if owner_parameter.is_some() && line.ends_with(':') {
            let owner = owner_parameter.take().unwrap();
            instrumented.push_str(&format!("  %checked.bytes = extractvalue {owner}, 0\n  %checked.length = extractvalue {owner}, 1\n  %checked.capacity = extractvalue {owner}, 2\n  call void @counted_string_drop(ptr %checked.bytes, i{bits} %checked.length, i{bits} %checked.capacity)\n"));
            injected = true;
        }
    }
    assert!(injected, "counter must observe the String owner destructor");
    instrumented.push_str(&format!(
        "declare void @counted_string_drop(ptr, i{bits}, i{bits})\n"
    ));
    let nonempty = arguments
        .iter()
        .filter(|argument| !argument.is_empty())
        .count();
    let directory = TestDirectory::create();
    let ir = directory.join("report.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("report");
    fs::write(&ir, instrumented).unwrap();
    fs::write(
        &counter,
        format!(
            "#define ARGUMENTS {}\n#define NONEMPTY {nonempty}\n{COUNTER}",
            arguments.len()
        ),
    )
    .unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    Command::new(executable).args(arguments).output().unwrap()
}

const COUNTER: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#define HAS_BUFFER (ARGUMENTS > 0)
#define REPORT (HAS_BUFFER + NONEMPTY)
#define CLONES (REPORT + 1)
#define ALLOCATIONS (HAS_BUFFER + NONEMPTY * 2 + 1)
static void *live[ALLOCATIONS];
static int allocations, releases, dropped[ALLOCATIONS], start_drops, empty_drops, processed_drops, done_drops;
void *counted_malloc(size_t size) {
    assert(allocations < ALLOCATIONS);
    void *pointer = malloc(size);
    assert(pointer);
    live[allocations++] = pointer;
    return pointer;
}
void counted_string_drop(void *pointer, size_t length, size_t capacity) {
    if (capacity) {
        for (int i = 0; i < allocations; ++i) {
            if (live[i] == pointer) {
                assert((i >= HAS_BUFFER && i < REPORT) || i >= CLONES);
                assert(dropped[i]++ == 0);
                return;
            }
        }
        abort();
    }
    if (!length) { ++empty_drops; return; }
    if (length == 5 && !memcmp(pointer, "start", 5)) { ++start_drops; return; }
    if (length == 9 && !memcmp(pointer, "processed", 9)) { ++processed_drops; return; }
    if (length == 4 && !memcmp(pointer, "done", 4)) { ++done_drops; return; }
    abort();
}
void counted_free(void *pointer) {
    /* Previous fields release in iteration order, then final field, Report,
       argv strings in reverse order, and finally the argv element buffer. */
    int expected;
    if (releases < NONEMPTY) expected = CLONES + releases;
    else if (releases == NONEMPTY) expected = REPORT;
    else if (releases <= NONEMPTY * 2) expected = REPORT - (releases - NONEMPTY);
    else expected = 0;
    assert(releases < ALLOCATIONS && expected < allocations && live[expected] == pointer);
    if (expected != REPORT && !(HAS_BUFFER && expected == 0)) assert(dropped[expected] == 1);
    live[expected] = 0;
    ++releases;
    free(pointer);
}
__attribute__((destructor)) static void verify_owners(void) {
    assert(allocations == ALLOCATIONS && releases == ALLOCATIONS);
    for (int i = 0; i < ALLOCATIONS; ++i) assert(!live[i]);
    assert(start_drops == 1 && processed_drops == 1 && done_drops == 1);
    assert(empty_drops == (ARGUMENTS - NONEMPTY) * 2);
}
"#;
