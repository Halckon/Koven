//! ADR-0021 version 1 source-set 初始化输入的严格解析与规范化。

use std::{collections::BTreeSet, error::Error, fmt};

use lang_frontend::{
    lexer::{LexemeKind, TokenKind, lex},
    source::SourceMap,
};
use lsp_types::Uri;
use serde_json::{Map, Value};

const SOURCE_SET_FIELDS: [&str; 4] = ["schema", "version", "roots", "sources"];
const SOURCE_FIELDS: [&str; 4] = ["root", "logicalPath", "uri", "text"];

/// 已验证并按稳定源码键排序的 source-set base snapshot。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceSetConfig {
    roots: Vec<String>,
    sources: Vec<BaseSource>,
}

impl SourceSetConfig {
    /// 从可选 initialization options 中提取 `koven.sourceSet`；缺席时保留 legacy 模式。
    pub(crate) fn from_initialization_options(
        options: Option<&Value>,
    ) -> Result<Option<Self>, SourceSetConfigError> {
        let Some(Value::Object(options)) = options else {
            return Ok(None);
        };
        let Some(Value::Object(koven)) = options.get("koven") else {
            return Ok(None);
        };
        let Some(source_set) = koven.get("sourceSet") else {
            return Ok(None);
        };
        Self::parse(source_set).map(Some)
    }

    fn parse(value: &Value) -> Result<Self, SourceSetConfigError> {
        let source_set = object(value, "koven.sourceSet")?;
        reject_unknown_fields(source_set, &SOURCE_SET_FIELDS, "koven.sourceSet")?;
        if string_field(source_set, "schema", "koven.sourceSet")? != "koven.lsp.source-set" {
            return Err(SourceSetConfigError::new(
                "koven.sourceSet.schema must be \"koven.lsp.source-set\"",
            ));
        }
        if source_set.get("version").and_then(Value::as_u64) != Some(1) {
            return Err(SourceSetConfigError::new(
                "koven.sourceSet.version must be integer 1",
            ));
        }

        let roots_value = array_field(source_set, "roots", "koven.sourceSet")?;
        if roots_value.is_empty() {
            return Err(SourceSetConfigError::new(
                "koven.sourceSet.roots must not be empty",
            ));
        }
        let mut roots = BTreeSet::new();
        for (index, root) in roots_value.iter().enumerate() {
            let Some(root) = root.as_str() else {
                return Err(SourceSetConfigError::new(format!(
                    "koven.sourceSet.roots[{index}] must be a string"
                )));
            };
            if root.is_empty() {
                return Err(SourceSetConfigError::new(format!(
                    "koven.sourceSet.roots[{index}] must not be empty"
                )));
            }
            if !roots.insert(root.to_owned()) {
                return Err(SourceSetConfigError::new(format!(
                    "duplicate source root {root:?}"
                )));
            }
        }

        let source_values = array_field(source_set, "sources", "koven.sourceSet")?;
        let mut source_keys = BTreeSet::new();
        let mut uris = BTreeSet::new();
        let mut sources = Vec::with_capacity(source_values.len());
        for (index, value) in source_values.iter().enumerate() {
            let context = format!("koven.sourceSet.sources[{index}]");
            let source = object(value, &context)?;
            reject_unknown_fields(source, &SOURCE_FIELDS, &context)?;
            let root = string_field(source, "root", &context)?;
            if !roots.contains(root) {
                return Err(SourceSetConfigError::new(format!(
                    "{context}.root references unknown root {root:?}"
                )));
            }
            let logical_path = string_field(source, "logicalPath", &context)?;
            validate_logical_path(logical_path, &context)?;
            let key = (root.to_owned(), logical_path.to_owned());
            if !source_keys.insert(key.clone()) {
                return Err(SourceSetConfigError::new(format!(
                    "duplicate source key ({root:?}, {logical_path:?})"
                )));
            }
            let uri_text = string_field(source, "uri", &context)?;
            let uri: Uri = uri_text.parse().map_err(|error| {
                SourceSetConfigError::new(format!("{context}.uri is invalid: {error}"))
            })?;
            if uri.scheme().is_none() {
                return Err(SourceSetConfigError::new(format!(
                    "{context}.uri must be absolute"
                )));
            }
            if !uris.insert(uri.as_str().to_owned()) {
                return Err(SourceSetConfigError::new(format!(
                    "duplicate source URI {:?}",
                    uri.as_str()
                )));
            }
            sources.push(BaseSource {
                root: key.0,
                logical_path: key.1,
                uri,
                text: string_field(source, "text", &context)?.to_owned(),
            });
        }
        sources.sort_by(|left, right| left.key().cmp(&right.key()));
        Ok(Self {
            roots: roots.into_iter().collect(),
            sources,
        })
    }
}

/// source-set 中一份不可变 base source。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BaseSource {
    root: String,
    logical_path: String,
    uri: Uri,
    text: String,
}

impl BaseSource {
    fn key(&self) -> (&str, &str) {
        (&self.root, &self.logical_path)
    }
}

fn validate_logical_path(path: &str, context: &str) -> Result<(), SourceSetConfigError> {
    let segments = path.split('/').collect::<Vec<_>>();
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || segments.iter().any(|segment| segment.is_empty())
        || segments
            .iter()
            .any(|segment| matches!(*segment, "." | ".."))
        || !path.ends_with(".ko")
    {
        return Err(SourceSetConfigError::new(format!(
            "{context}.logicalPath must be a relative slash-separated .ko path without empty, . or .. segments"
        )));
    }
    for segment in &segments[..segments.len() - 1] {
        if !is_koven_identifier(segment)? {
            return Err(SourceSetConfigError::new(format!(
                "{context}.logicalPath package segment {segment:?} is not a Koven identifier"
            )));
        }
    }
    Ok(())
}

fn is_koven_identifier(segment: &str) -> Result<bool, SourceSetConfigError> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("<lsp-logical-path-segment>", segment)
        .map_err(|error| SourceSetConfigError::new(error.to_string()))?;
    let lexed = lex(&sources, source)
        .map_err(|error| SourceSetConfigError::new(format!("could not validate path: {error}")))?;
    Ok(lexed.diagnostics().is_empty()
        && matches!(
            lexed.lexemes(),
            [lexeme, eof]
                if lexeme.kind() == LexemeKind::Token(TokenKind::Identifier)
                    && eof.kind() == LexemeKind::Eof
        ))
}

fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a Map<String, Value>, SourceSetConfigError> {
    value
        .as_object()
        .ok_or_else(|| SourceSetConfigError::new(format!("{context} must be an object")))
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    expected: &[&str],
    context: &str,
) -> Result<(), SourceSetConfigError> {
    if let Some(field) = object
        .keys()
        .find(|field| !expected.contains(&field.as_str()))
    {
        return Err(SourceSetConfigError::new(format!(
            "{context} contains unknown field {field:?}"
        )));
    }
    Ok(())
}

fn string_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<&'a str, SourceSetConfigError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| SourceSetConfigError::new(format!("{context}.{field} must be a string")))
}

fn array_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<&'a [Value], SourceSetConfigError> {
    object
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| SourceSetConfigError::new(format!("{context}.{field} must be an array")))
}

/// source-set initialization options 不满足 ADR-0021 version 1 wire。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceSetConfigError(String);

impl SourceSetConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for SourceSetConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SourceSetConfigError {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::SourceSetConfig;

    #[test]
    fn source_set_initialization_normalizes_roots_and_sources_without_reading_uris() {
        let options = json!({
            "unrelated": true,
            "koven": {
                "other": { "ignored": true },
                "sourceSet": {
                    "schema": "koven.lsp.source-set",
                    "version": 1,
                    "roots": ["generated", "main"],
                    "sources": [
                        {
                            "root": "main",
                            "logicalPath": "q/use.ko",
                            "uri": "untitled:/does-not-exist/use.ko",
                            "text": "package q"
                        },
                        {
                            "root": "generated",
                            "logicalPath": "p/api.ko",
                            "uri": "file:///definitely/not/on/disk/api.ko",
                            "text": "package p"
                        }
                    ]
                }
            }
        });

        let config = SourceSetConfig::from_initialization_options(Some(&options))
            .expect("valid source set")
            .expect("source-set mode");
        assert_eq!(config.roots, ["generated", "main"]);
        assert_eq!(config.sources[0].root, "generated");
        assert_eq!(config.sources[0].logical_path, "p/api.ko");
        assert_eq!(config.sources[0].text, "package p");
        assert_eq!(
            config.sources[1].uri.as_str(),
            "untitled:/does-not-exist/use.ko"
        );
    }

    #[test]
    fn source_set_initialization_preserves_legacy_when_option_is_absent() {
        assert!(
            SourceSetConfig::from_initialization_options(None)
                .expect("legacy options")
                .is_none()
        );
        assert!(
            SourceSetConfig::from_initialization_options(Some(&json!({
                "other": true,
                "koven": { "sibling": 1 }
            })))
            .expect("unrelated options")
            .is_none()
        );
    }

    #[test]
    fn source_set_initialization_rejects_schema_version_shape_and_unknown_fields() {
        for source_set in [
            json!(null),
            json!({"schema": "wrong", "version": 1, "roots": ["main"], "sources": []}),
            json!({"schema": "koven.lsp.source-set", "version": 2, "roots": ["main"], "sources": []}),
            json!({"schema": "koven.lsp.source-set", "version": 1, "roots": ["main"], "sources": [], "extra": true}),
        ] {
            let options = json!({"koven": {"sourceSet": source_set}});
            assert!(
                SourceSetConfig::from_initialization_options(Some(&options)).is_err(),
                "must reject {options}"
            );
        }
    }

    #[test]
    fn source_set_initialization_rejects_invalid_roots_keys_paths_and_uris() {
        let valid_source = json!({
            "root": "main",
            "logicalPath": "p/main.ko",
            "uri": "file:///workspace/main.ko",
            "text": "package p"
        });
        let cases = [
            (json!([]), json!([valid_source.clone()])),
            (json!(["main", "main"]), json!([])),
            (
                json!(["main"]),
                json!([{
                    "root": "other",
                    "logicalPath": "p/main.ko",
                    "uri": "file:///workspace/main.ko",
                    "text": "package p"
                }]),
            ),
            (
                json!(["main"]),
                json!([{
                    "root": "main",
                    "logicalPath": "../main.ko",
                    "uri": "file:///workspace/main.ko",
                    "text": "package p"
                }]),
            ),
            (
                json!(["main"]),
                json!([{
                    "root": "main",
                    "logicalPath": "p/main.ko",
                    "uri": "relative/main.ko",
                    "text": "package p"
                }]),
            ),
            (
                json!(["main"]),
                json!([{
                    "root": "main",
                    "logicalPath": "bad-segment/main.ko",
                    "uri": "file:///workspace/main.ko",
                    "text": "package p"
                }]),
            ),
        ];
        for (roots, sources) in cases {
            let options = json!({
                "koven": {"sourceSet": {
                    "schema": "koven.lsp.source-set",
                    "version": 1,
                    "roots": roots,
                    "sources": sources
                }}
            });
            assert!(
                SourceSetConfig::from_initialization_options(Some(&options)).is_err(),
                "must reject {options}"
            );
        }
    }

    #[test]
    fn source_set_initialization_rejects_duplicate_keys_and_uris() {
        let first = json!({
            "root": "main",
            "logicalPath": "p/one.ko",
            "uri": "file:///workspace/one.ko",
            "text": ""
        });
        for duplicate in [
            json!({
                "root": "main",
                "logicalPath": "p/one.ko",
                "uri": "file:///workspace/two.ko",
                "text": ""
            }),
            json!({
                "root": "main",
                "logicalPath": "p/two.ko",
                "uri": "file:///workspace/one.ko",
                "text": ""
            }),
        ] {
            let options = json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["main"],
                "sources": [first.clone(), duplicate]
            }}});
            assert!(SourceSetConfig::from_initialization_options(Some(&options)).is_err());
        }
    }
}
