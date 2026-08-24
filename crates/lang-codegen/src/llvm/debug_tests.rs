use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::source::SourceMap;

use crate::ssa::model::{Operation, Origin, Program, ScalarConstant, SsaTypeKind, TerminatorKind};

use super::{
    LlvmAdapterError, debug::DebugPlan, emit_verified_object, render_verified_program_with_debug,
};

static NEXT_OBJECT: AtomicU64 = AtomicU64::new(0);

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
    assert!(first.contains("name: \"app\", linkageName: \"f2.app\""));
    assert!(first.contains("name: \"helper\", linkageName: \"f0.helper\""));
    assert!(first.contains("name: \"alpha\", linkageName: \"f1.alpha\""));
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
