//! 全文格式化只适配现有 frontend 结果与 LSP UTF-16 编辑范围。

use lang_frontend::{
    formatting::{FormattingError, format_source},
    source::{SourceId, SourceMap},
};
use lsp_types::TextEdit;

use crate::position_adapter::span_range;

/// 借用同轮源码；用户语法错误不返回部分编辑，也不修改当前文档。
pub(crate) fn edits(
    sources: &SourceMap,
    source_id: SourceId,
) -> Result<Option<Vec<TextEdit>>, String> {
    let original = sources
        .source_text(source_id)
        .map_err(|error| error.to_string())?;
    let formatted = match format_source(sources, source_id) {
        Ok(text) => text,
        Err(FormattingError::Diagnostics(_)) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if formatted == original {
        return Ok(Some(Vec::new()));
    }
    let span = sources
        .span(source_id, 0, original.len())
        .map_err(|error| error.to_string())?;
    let range = span_range(sources, span).map_err(|error| error.to_string())?;
    Ok(Some(vec![TextEdit::new(range, formatted)]))
}
