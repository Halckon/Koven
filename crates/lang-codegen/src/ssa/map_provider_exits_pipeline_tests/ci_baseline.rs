//! 原真实单 Map plan 的 MemoryDocument/rooted Unit baseline；不混入六源 Guard oracle。
use super::{Report, pipeline};
use crate::ssa::{model::Program, verify::verify_program};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

const SOURCE: &str = r#"
fun scan(first: Map<String, String>): Unit {
  for ((key, value) in first) { println(key); println(value) }
}
fun entry(): Unit {
  val first = mapOf<String, String>()
  scan(first)
}
"#;

pub(super) fn baseline(rooted: bool) -> Program {
    let mut sources = SourceMap::new();
    let id = sources.add_source("provider.ko", SOURCE).unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let mut report = Report::default();
    let (program, _) = if rooted {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |_, _, owned, r| {
                r.check(
                    "original rooted Unit Map provider",
                    owned.map_iterations().len() == 1,
                    owned.map_iterations(),
                );
            },
        )
    } else {
        pipeline::memory_document_observed(&sources, &parsed, &mut report, |_, _, owned, r| {
            r.check(
                "original MemoryDocument Unit Map provider",
                owned.map_iterations().len() == 1,
                owned.map_iterations(),
            );
        })
    }
    .unwrap();
    assert!(report.passed(), "{report:#?}");
    verify_program(&program).expect("unmodified real SSA must pass before corruption");
    crate::llvm::render_verified_program(&program).expect("unmodified real LLVM must pass");
    program
}

#[test]
fn both_real_provider_baselines_reach_verified_llvm() {
    for rooted in [false, true] {
        baseline(rooted);
        println!(
            "SPEC0331 ORDINARY CI BASELINE input={} SSA=verified LLVM=verified",
            super::input_label(rooted)
        );
    }
}
