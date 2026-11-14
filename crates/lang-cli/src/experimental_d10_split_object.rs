//! Explicit builtin-only two-source experiment. Never selected by build/run or as fallback.
use crate::{
    MessageFormat,
    bootstrap::{BootstrapError, single_logical_path, source_diagnostics},
    format::CommandOutput,
    unit_build::UnitBuildError,
};
use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_parsed_unit_names},
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::SymbolKind,
    ownership_checking::d10_catalog_experiment::{
        PrepareNamedEntryError, prepare_selected_split_named_entry, prepare_split_named_entry,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{FinalTypeAdmissionError, check_compilation_unit_types, standard_environments},
};
use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

const USAGE: &str = "usage: kovenc experimental-d10-split-object <source1.ko> <source2.ko> --entry <name> -o <object>\n";

enum Failure {
    Diagnostics { human: String, json: String },
    Operational(String),
}
impl From<BootstrapError> for Failure {
    fn from(error: BootstrapError) -> Self {
        match error {
            BootstrapError::Unit(UnitBuildError::FrontendDiagnostics { human, json }) => {
                Self::Diagnostics { human, json }
            }
            error => Self::Operational(error.to_string()),
        }
    }
}
fn operational(error: impl std::fmt::Display) -> Failure {
    Failure::Operational(error.to_string())
}
fn diagnostics(sources: &SourceMap, values: &[Diagnostic]) -> Failure {
    source_diagnostics(sources, values).into()
}

pub(super) fn execute(arguments: &[OsString], message_format: MessageFormat) -> CommandOutput {
    let [first, second, entry_flag, entry, output_flag, output] = arguments else {
        return usage("expected two sources, explicit --entry and -o object");
    };
    if entry_flag != "--entry"
        || output_flag != "-o"
        || [first, second, entry, output]
            .iter()
            .any(|value| value.is_empty() || value.to_string_lossy().starts_with('-'))
    {
        return usage("expected two sources, explicit --entry and -o object");
    }
    let Some(entry) = entry.to_str() else {
        return usage("entry must be valid UTF-8");
    };
    match emit(
        [Path::new(first), Path::new(second)],
        entry,
        Path::new(output),
    ) {
        Ok(()) => CommandOutput::success(Vec::new()),
        Err(Failure::Diagnostics { human, json }) => CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: match message_format {
                MessageFormat::Human => human.into_bytes(),
                MessageFormat::Json => json.into_bytes(),
            },
        },
        Err(Failure::Operational(message)) => CommandOutput::error(format!(
            "error: experimental-d10-split-object failed: {message}\n"
        )),
    }
}
fn usage(message: &str) -> CommandOutput {
    CommandOutput::error(format!("error: {message}\n{USAGE}"))
}

// Reject all existing directory entries, including dangling symlinks. Do not reserve or create
// anything here. Canonical parent plus a single normal filename is the only output locator.
fn output_path(source: &Path, output: &Path) -> Result<PathBuf, Failure> {
    match fs::symlink_metadata(output) {
        Ok(_) => {
            return Err(operational(format!(
                "output already exists: {}",
                output.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(operational(format!(
                "cannot inspect output {}: {error}",
                output.display()
            )));
        }
    }
    let Some(Component::Normal(name)) = output.components().next_back() else {
        return Err(operational("output must end in a normal filename"));
    };
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(operational)?;
    if !parent.is_dir() {
        return Err(operational("output parent is not a directory"));
    }
    let normalized = parent.join(name);
    if fs::canonicalize(source).map_err(operational)? == normalized {
        return Err(operational("source and object paths overlap"));
    }
    // Recheck the resolved locator as well, rather than relying on lexical path inequality.
    match fs::symlink_metadata(&normalized) {
        Ok(_) => Err(operational(format!(
            "output already exists: {}",
            normalized.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(normalized),
        Err(error) => Err(operational(error)),
    }
}

fn emit(paths: [&Path; 2], entry: &str, output: &Path) -> Result<(), Failure> {
    let physical = [
        fs::canonicalize(paths[0]).map_err(operational)?,
        fs::canonicalize(paths[1]).map_err(operational)?,
    ];
    if physical[0] == physical[1] {
        return Err(operational(
            "split sources must have distinct canonical paths",
        ));
    }
    let output = output_path(&physical[0], output)?;
    let output = output_path(&physical[1], &output)?;
    let mut sources = SourceMap::new();
    let mut files = Vec::new();
    for (path, canonical) in paths.into_iter().zip(&physical) {
        let text = fs::read_to_string(canonical).map_err(operational)?;
        let name = path
            .to_str()
            .ok_or_else(|| operational("source path must be valid UTF-8"))?;
        let file_name = canonical
            .to_str()
            .ok_or_else(|| operational("canonical source path must be valid UTF-8"))?;
        let source = sources
            .add_source_with_file_name(name, text, file_name)
            .map_err(operational)?;
        let lexed = lex(&sources, source).map_err(operational)?;
        let parsed = parse_file(&sources, &lexed).map_err(operational)?;
        if !parsed.initial_package_header_is_complete() {
            return Err(diagnostics(&sources, parsed.diagnostics()));
        }
        let logical = single_logical_path(&sources, &parsed, path, None)?;
        files.push((UnitSourceDescriptor::new("split", logical, source), parsed));
    }
    let (name_environment, types) = standard_environments();
    // Both complete original sources participate in one snapshot. No standard-source append,
    // source crop, ownership rerun, or fallback to a single-source capability.
    let snapshot =
        analyze_parsed_unit_names(sources, files, name_environment).map_err(operational)?;
    let sources = snapshot.sources();
    if !snapshot.names().diagnostics().is_empty() {
        return Err(diagnostics(sources, snapshot.names().diagnostics()));
    }
    let names = snapshot
        .validated_names()
        .ok_or_else(|| operational("name validation failed without diagnostics"))?;
    let inputs = snapshot.inputs();
    let typed =
        check_compilation_unit_types(sources, &inputs, names, &types).map_err(operational)?;
    if !typed.diagnostics().is_empty() {
        return Err(diagnostics(sources, typed.diagnostics()));
    }
    let typed = typed
        .validate()
        .map_err(|_| operational("experimental source requires basic typed capability"))?;
    let requested = (|| {
        let mut matches = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.kind() == SymbolKind::Function && declaration.name() == entry
            });
        let selected = matches
            .next()
            .ok_or_else(|| operational(format!("entry `{entry}` was not found")))?;
        if matches.next().is_some() {
            return Err(operational(format!("entry `{entry}` is ambiguous")));
        }
        Ok(selected)
    })();
    let admission_failure = |error| match error {
        PrepareNamedEntryError::FinalTypeAdmission(FinalTypeAdmissionError::Diagnostics(
            values,
        )) => diagnostics(sources, &values),
        PrepareNamedEntryError::OwnershipAdmission {
            diagnostics: values,
            deferred,
        } => {
            if values.is_empty() {
                operational(format!("ownership remains deferred: {deferred:?}"))
            } else {
                diagnostics(sources, &values)
            }
        }
        PrepareNamedEntryError::EntryMismatch => {
            operational("explicit entry is not the sealed source wrapper")
        }
        error => operational(format!("{error:?}")),
    };
    // Exactly one ownership analysis in either branch. A missing/ambiguous CLI selector does
    // not hide a source admission error; successful resolution uses the library's same-object
    // membership and sealed-wrapper contract rather than duplicating its root checks here.
    let prepared = match requested {
        Ok(selected) => {
            prepare_selected_split_named_entry(sources, &inputs, names, &types, typed, selected)
                .map_err(admission_failure)?
        }
        Err(selection_error) => {
            prepare_split_named_entry(sources, &inputs, names, &types, typed)
                .map_err(admission_failure)?;
            return Err(selection_error);
        }
    };
    let publication = lang_codegen::d10_experiment::lower_prepared_split_named_entry(
        sources, &inputs, names, &types, &prepared,
    )
    .map_err(operational)?;
    // Repeat the absence check immediately before the existing atomic emission transaction.
    let output = output_path(&physical[0], &output)?;
    let output = output_path(&physical[1], &output)?;
    publication
        .emit_new_process_object(&output)
        .map_err(|error| {
            if let Some(diagnostic) = error.diagnostic() {
                diagnostics(sources, std::slice::from_ref(diagnostic))
            } else {
                operational(error)
            }
        })
}
