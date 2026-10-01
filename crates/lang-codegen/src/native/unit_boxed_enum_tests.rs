//! SPEC-0230: boxed enum owners cross compilation-unit call boundaries.
use super::{Command, TestDirectory, UnitAnalysis, analyze_sources, emit_native_unit_object};
use crate::native_tests::boxed_enum_tests::{
    RECURSIVE_ALLOCATIONS, RECURSIVE_STDOUT, SIMPLE_DECLARATIONS, SIMPLE_STDOUT, assert_success,
    recursive_declarations, run_counted_allocations,
};

fn simple_analysis() -> UnitAnalysis {
    analyze_sources(
        &format!("package p\n{SIMPLE_DECLARATIONS}"),
        "package q\nfun entry(): Unit {\n\
         val first = p.take(p.make())\n\
         p.take(p.makeEmpty())\n\
         }",
    )
}

fn recursive_analysis(enum_first: bool) -> UnitAnalysis {
    let declarations = recursive_declarations(enum_first);
    analyze_sources(
        &format!("package p\n{declarations}"),
        "package q\nfun entry(): Unit {\n\
         val boxed = p.makeDeepTree()\n\
         val first = p.takeBox(boxed)\n\
         val inline = p.makeInline()\n\
         p.takeInline(inline)\n\
         }",
    )
}

#[test]
fn boxed_enum_cross_file_return_and_value_delivery_run_natively() {
    emit_link_run(&simple_analysis(), SIMPLE_STDOUT);
}

#[test]
fn recursive_boxed_enum_cross_file_tree_and_inline_root_run_natively() {
    for enum_first in [false, true] {
        emit_link_run(&recursive_analysis(enum_first), RECURSIVE_STDOUT);
    }
}

#[test]
fn boxed_enum_cross_file_cases_allocate_and_free_each_owner_once() {
    let llvm = lower_to_llvm(&simple_analysis());
    let run = run_counted_allocations(&llvm, 2);
    assert_success(&run, SIMPLE_STDOUT);
}

#[test]
fn recursive_boxed_enum_cross_file_drop_frees_all_descendants_once() {
    for enum_first in [false, true] {
        let llvm = lower_to_llvm(&recursive_analysis(enum_first));
        let run = run_counted_allocations(&llvm, RECURSIVE_ALLOCATIONS);
        assert_success(&run, RECURSIVE_STDOUT);
    }
}

fn lower_to_llvm(analysis: &UnitAnalysis) -> String {
    let inputs = analysis.inputs();
    let reversed = [inputs[1], inputs[0]];
    let lower = |inputs: &[_]| {
        super::lower_scalar_unit_with_entry(
            &analysis.sources,
            inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("boxed enum unit must lower to verified SSA")
    };
    let (program, entry) = lower(&inputs);
    let (backward, backward_entry) = lower(&reversed);
    assert_eq!(entry.index(), backward_entry.index());
    assert_eq!(entry.module().index(), backward_entry.module().index());
    assert_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&backward),
        "input order must not change recursive type registration"
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("boxed enum unit LLVM must verify");
    assert_eq!(
        llvm,
        crate::llvm::render_verified_program_with_entry(&backward, backward_entry)
            .expect("reversed boxed enum unit LLVM must verify")
    );
    llvm
}

fn emit_link_run(analysis: &UnitAnalysis, stdout: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("boxed-enum.o");
    let executable = directory.join("boxed-enum");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("boxed enum unit must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("boxed enum executable must launch");
    assert_success(&run, stdout);
}
