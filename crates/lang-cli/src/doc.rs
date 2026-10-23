//! Syntax-only API export; parsing and declaration traversal belong to frontend.
use crate::{
    MessageFormat, diagnostic_renderer::render_diagnostics, format::CommandOutput,
    machine_diagnostic_renderer::render_machine_diagnostics,
};
use lang_frontend::{
    declaration_index::index_declarations,
    lexer::lex,
    parser::{ParameterModeMarker, parse_file},
    source::{SourceMap, Span},
};
use serde_json::{Value, json};
use std::{ffi::OsString, fs, path::Path};

pub(super) fn execute(arguments: &[OsString], format: MessageFormat) -> CommandOutput {
    let [path] = arguments else {
        return CommandOutput::error(
            "error: expected one source path\nusage: kovenc doc <path>\n".into(),
        );
    };
    if path.to_string_lossy().starts_with('-') {
        return CommandOutput::error(
            "error: unknown doc option\nusage: kovenc doc <path>\n".into(),
        );
    }
    match extract(Path::new(path), format) {
        Ok(output) => output,
        Err(error) => CommandOutput::error(format!("error: {error}\n")),
    }
}

fn extract(
    path: &Path,
    format: MessageFormat,
) -> Result<CommandOutput, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    let mut sources = SourceMap::new();
    let id = sources.add_source(path.to_string_lossy(), source)?;
    let parsed = parse_file(&sources, &lex(&sources, id)?)?;
    if !parsed.diagnostics().is_empty() {
        let rendered = match format {
            MessageFormat::Human => render_diagnostics(&sources, parsed.diagnostics())?,
            MessageFormat::Json => render_machine_diagnostics(&sources, parsed.diagnostics())?,
        };
        return Ok(CommandOutput::error(rendered));
    }
    let index = index_declarations(&parsed)?;
    let mut functions = Vec::new();
    for function in index.functions {
        let signature = sources.span(id, function.span.start(), function.signature_end)?;
        let mut parameters = Vec::new();
        for parameter in function.parameters {
            let mode = match parameter.mode {
                None => "default",
                Some(ParameterModeMarker::Borrow(_)) => "borrow",
                Some(ParameterModeMarker::Own(_)) => "own",
                Some(ParameterModeMarker::Inout(_)) => "inout",
            };
            parameters.push(json!({"name": sources.slice(parameter.name)?,
                "type": sources.slice(parameter.type_span)?, "mode": mode,
                "location": location(&sources, parameter.name)?}));
        }
        functions.push(json!({"name": sources.slice(function.name)?,
            "signature": sources.slice(signature)?.trim_end(),
            "location": location(&sources, function.span)?, "parameters": parameters}));
    }
    let omitted = index
        .omitted
        .into_iter()
        .map(|span| location(&sources, span))
        .collect::<Result<Vec<_>, _>>()?;
    let document = json!({"schema": "koven.api-declarations", "version": 1,
        "guideVersion": "0.45", "compilerVersion": env!("CARGO_PKG_VERSION"),
        "validation": "syntax-only", "scope": "top-level-ordinary-functions",
        "source": path.file_name().and_then(|name| name.to_str()),
        "functions": functions, "omitted": omitted});
    let mut output = serde_json::to_vec_pretty(&document)?;
    output.push(b'\n');
    Ok(CommandOutput::success(output))
}

fn location(sources: &SourceMap, span: Span) -> Result<Value, lang_frontend::source::SourceError> {
    let position = sources.position(span.source_id(), span.start())?;
    Ok(json!({"startByte": span.start(), "endByte": span.end(),
        "line": position.line(), "column": position.column()}))
}
