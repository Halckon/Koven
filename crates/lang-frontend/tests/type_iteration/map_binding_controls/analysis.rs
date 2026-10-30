//! rooted/MemoryDocument Unit 消费原始输入；Single 对照分支在有界 Phase 2 候选中退休。
//! 前置失败与最终准入失败分开记录；不扩大到 ownership 或其他旧 checker 消费者。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexedFile, lex},
    name_resolution::{
        SourceUnitInput, SourceUnitKey, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, FinalTypeAdmissionError, TypeEnvironment,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

const LOGICAL_PATH: &str = "iteration.ko";

#[derive(Clone, Copy, Debug)]
pub(super) enum Entry {
    Unit,
    MemoryUnit,
}

impl Entry {
    pub(super) fn matches_key(self, key: &SourceUnitKey) -> bool {
        match (self, key) {
            (Self::Unit, SourceUnitKey::Rooted { root, logical_path }) => {
                root.as_str() == "root" && logical_path.as_str() == LOGICAL_PATH
            }
            (Self::MemoryUnit, SourceUnitKey::MemoryDocument) => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub(super) struct Blocked {
    pub(super) stage: &'static str,
    pub(super) detail: String,
}

pub(super) fn blocked(stage: &'static str, detail: impl std::fmt::Debug) -> Blocked {
    Blocked {
        stage,
        detail: format!("{detail:?}"),
    }
}

pub(super) fn diagnostics(sources: &SourceMap, stage: &str, items: &[Diagnostic]) -> String {
    let slices = items
        .iter()
        .map(|item| (item.primary_span(), sources.slice(item.primary_span())))
        .collect::<Vec<_>>();
    // Debug 保留 severity/code/message/primary/details，不以精简消息替代原诊断。
    format!("{stage}: diagnostics={items:#?}\nprimary_slices={slices:?}")
}

fn clean(
    sources: &SourceMap,
    trace: &mut Vec<String>,
    stage: &'static str,
    items: &[Diagnostic],
) -> Result<(), Blocked> {
    trace.push(diagnostics(sources, stage, items));
    if items.is_empty() {
        Ok(())
    } else {
        Err(blocked(stage, items))
    }
}

pub(super) struct Fixture {
    pub(super) sources: SourceMap,
    pub(super) lexed: LexedFile,
    pub(super) file: ParsedFile,
    types: TypeEnvironment,
}

pub(super) struct UnitAnalysis {
    pub(super) names: ValidatedCompilationUnitNames,
    pub(super) raw: CompilationUnitTypes,
    pub(super) complete: Result<bool, FinalTypeAdmissionError>,
}

pub(super) struct Observed {
    pub(super) entry: Entry,
    pub(super) fixture: Fixture,
    pub(super) analysis: UnitAnalysis,
}

pub(super) struct Observation {
    pub(super) entry: Entry,
    pub(super) trace: Vec<String>,
    pub(super) result: Result<Observed, Blocked>,
}

pub(super) fn observe(text: &str, entry: Entry) -> Observation {
    let mut trace = Vec::new();
    let result = analyze(text, entry, &mut trace);
    Observation {
        entry,
        trace,
        result,
    }
}

fn analyze(text: &str, entry: Entry, trace: &mut Vec<String>) -> Result<Observed, Blocked> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(LOGICAL_PATH, text)
        .map_err(|error| blocked("source infrastructure", error))?;
    let lexed = lex(&sources, source).map_err(|error| blocked("lex infrastructure", error))?;
    clean(&sources, trace, "lex", lexed.diagnostics())?;
    let file =
        parse_file(&sources, &lexed).map_err(|error| blocked("parse infrastructure", error))?;
    clean(&sources, trace, "parse", file.diagnostics())?;
    let (environment, types) = standard_environments();
    let fixture = Fixture {
        sources,
        lexed,
        file,
        types,
    };
    let sources = &fixture.sources;
    let file = &fixture.file;
    let types = &fixture.types;
    let analysis = {
        let inputs = [if matches!(entry, Entry::MemoryUnit) {
            SourceUnitInput::memory_document(file.source_id(), file)
        } else {
            SourceUnitInput::new("root", LOGICAL_PATH, source, file)
        }];
        let index = index_compilation_unit(sources, &inputs)
            .map_err(|error| blocked("unit index infrastructure", error))?;
        clean(sources, trace, "unit index", index.diagnostics())?;
        let names = resolve_compilation_unit_names(sources, &inputs, &index, &environment)
            .map_err(|error| blocked("unit names infrastructure", error))?;
        clean(sources, trace, "unit names", names.diagnostics())?;
        let names = names
            .validate()
            .map_err(|error| blocked("unit names qualification", error.diagnostics()))?;
        trace.push("unit names qualification: validated original names".into());
        let raw = check_compilation_unit_types(sources, &inputs, &names, types)
            .map_err(|error| blocked("unit raw infrastructure", error))?;
        trace.push(diagnostics(sources, "unit raw", raw.diagnostics()));
        let complete = complete_compilation_unit_types(sources, &inputs, &names, types, &raw)
            .map(|final_types| final_types.is_same_analysis(&raw));
        UnitAnalysis {
            names,
            raw,
            complete,
        }
    };
    Ok(Observed {
        entry,
        fixture,
        analysis,
    })
}
