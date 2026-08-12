//! 源码身份、字节范围与展示位置的统一基础设施。

use std::{
    error::Error,
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SOURCE_MAP_ID: AtomicU64 = AtomicU64::new(1);

/// 一份源码在所属 [`SourceMap`] 中的身份。
///
/// ID 只在分配它的 source map 内有意义；内部 owner identity 会拒绝来自其他 map 的同下标
/// ID，并从稳定 `Debug` 表示中隐藏。ID 不等同于文件系统路径，也不能作为稳定输出的排序
/// 依据。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourceId {
    map_id: u64,
    index: usize,
}

impl fmt::Debug for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // map identity 只用于防止跨 map 串源，不进入可能被快照保存的稳定表示。
        formatter
            .debug_tuple("SourceId")
            .field(&self.index)
            .finish()
    }
}

/// 关联源码的半开字节范围 `[start, end)`。
///
/// 行列位置不存储在 span 中，而是由 [`SourceMap::position`] 在展示边界统一计算。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    source_id: SourceId,
    start: usize,
    end: usize,
}

impl Span {
    /// 返回该范围所属的源码 ID。
    #[must_use]
    pub const fn source_id(self) -> SourceId {
        self.source_id
    }

    /// 返回包含端的字节偏移。
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// 返回不包含端的字节偏移。
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    /// 返回范围的字节长度。
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// 返回该范围是否为空。
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// 面向用户展示的 1-based 源码行列位置。
///
/// 列按行首到目标字节偏移之前的 Unicode scalar value 数量加一计算，tab 计一个 scalar。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePosition {
    line: usize,
    column: usize,
}

impl SourcePosition {
    /// 返回 1-based 行号。
    #[must_use]
    pub const fn line(self) -> usize {
        self.line
    }

    /// 返回 1-based 列号。
    #[must_use]
    pub const fn column(self) -> usize {
        self.column
    }
}

/// source / span 校验失败的具体原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// Source ID 不属于被查询的 source map。
    InvalidSourceId {
        /// 无法解析的源码 ID。
        source_id: SourceId,
    },
    /// 用户可见名称已在当前 source map 中注册。
    DuplicateSourceName {
        /// 被拒绝的重复名称。
        name: String,
    },
    /// Span 的起点位于终点之后。
    ReversedSpan {
        /// Span 携带的源码 ID。
        source_id: SourceId,
        /// 起始字节偏移。
        start: usize,
        /// 结束字节偏移。
        end: usize,
    },
    /// 单个字节偏移超出源码范围。
    OffsetOutOfBounds {
        /// 被查询的源码 ID。
        source_id: SourceId,
        /// 被拒绝的字节偏移。
        offset: usize,
        /// 源文本的字节长度。
        source_len: usize,
    },
    /// Span 的一个或两个端点超出源码范围。
    SpanOutOfBounds {
        /// Span 携带的源码 ID。
        source_id: SourceId,
        /// 起始字节偏移。
        start: usize,
        /// 结束字节偏移。
        end: usize,
        /// 源文本的字节长度。
        source_len: usize,
    },
    /// 字节偏移落在 UTF-8 多字节字符内部。
    NotCharBoundary {
        /// 被查询的源码 ID。
        source_id: SourceId,
        /// 被拒绝的字节偏移。
        offset: usize,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceId { source_id } => {
                write!(formatter, "source ID {source_id:?} does not exist")
            }
            Self::DuplicateSourceName { name } => {
                write!(formatter, "source name {name:?} is already registered")
            }
            Self::ReversedSpan {
                source_id,
                start,
                end,
            } => write!(
                formatter,
                "span {start}..{end} for source ID {source_id:?} is reversed"
            ),
            Self::OffsetOutOfBounds {
                source_id,
                offset,
                source_len,
            } => write!(
                formatter,
                "byte offset {offset} is outside source ID {source_id:?} of length {source_len}"
            ),
            Self::SpanOutOfBounds {
                source_id,
                start,
                end,
                source_len,
            } => write!(
                formatter,
                "span {start}..{end} is outside source ID {source_id:?} of length {source_len}"
            ),
            Self::NotCharBoundary { source_id, offset } => write!(
                formatter,
                "byte offset {offset} is not a UTF-8 character boundary in source ID {source_id:?}"
            ),
        }
    }
}

impl Error for SourceError {}

/// 拥有源码文本并集中提供范围与行列换算的容器。
///
/// 添加后的名称、文本和行索引保持不可变。`SourceId` 按添加顺序分配，但面向用户的稳定
/// 产物必须按源码名称与范围排序，不能依赖该顺序。
pub struct SourceMap {
    map_id: u64,
    sources: Vec<Source>,
}

impl Default for SourceMap {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceMap {
    /// 创建空 source map。
    #[must_use]
    pub fn new() -> Self {
        Self {
            map_id: next_source_map_id(),
            sources: Vec::new(),
        }
    }

    /// 添加唯一的用户可见名称和 UTF-8 源文本，并返回 map-local ID。
    ///
    /// # Errors
    ///
    /// 当名称已在当前 map 中注册时返回 [`SourceError::DuplicateSourceName`]，现有源码保持
    /// 不变。
    pub fn add_source(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<SourceId, SourceError> {
        let name = name.into();
        if self.sources.iter().any(|source| source.name == name) {
            return Err(SourceError::DuplicateSourceName { name });
        }

        let source_id = SourceId {
            map_id: self.map_id,
            index: self.sources.len(),
        };
        let text = text.into();
        let line_starts = line_starts(&text);

        self.sources.push(Source {
            name,
            text,
            line_starts,
        });

        Ok(source_id)
    }

    /// 返回源码的用户可见名称。
    ///
    /// # Errors
    ///
    /// 当 ID 不属于本 map 时返回 [`SourceError::InvalidSourceId`]。
    pub fn source_name(&self, source_id: SourceId) -> Result<&str, SourceError> {
        Ok(&self.source(source_id)?.name)
    }

    /// 返回不可变 UTF-8 源文本。
    ///
    /// # Errors
    ///
    /// 当 ID 不属于本 map 时返回 [`SourceError::InvalidSourceId`]。
    pub fn source_text(&self, source_id: SourceId) -> Result<&str, SourceError> {
        Ok(&self.source(source_id)?.text)
    }

    /// 创建一个已针对本 map 校验过的 span。
    ///
    /// # Errors
    ///
    /// 当 source ID 无效、范围逆序或越界、端点不位于 UTF-8 字符边界时，返回对应的
    /// [`SourceError`]。
    pub fn span(&self, source_id: SourceId, start: usize, end: usize) -> Result<Span, SourceError> {
        let source = self.source(source_id)?;
        if start > end {
            return Err(SourceError::ReversedSpan {
                source_id,
                start,
                end,
            });
        }

        self.validate_range(source_id, source, start, end)?;
        Ok(Span {
            source_id,
            start,
            end,
        })
    }

    /// 返回 span 对应的源码切片。
    ///
    /// # Errors
    ///
    /// 当 source ID 无效、范围越界或端点不位于 UTF-8 字符边界时，返回对应的
    /// [`SourceError`]。
    pub fn slice(&self, span: Span) -> Result<&str, SourceError> {
        let source = self.source_for_span(span)?;
        Ok(&source.text[span.start..span.end])
    }

    /// 把字节偏移转换为统一的 1-based 行列位置。
    ///
    /// `\n` 后的第一个字节开始新行；在 `\r\n` 中，`\r` 与 `\n` 的 offset 都仍属于前一
    /// 行。尾随换行后的 EOF 位于下一行第 1 列。
    ///
    /// # Errors
    ///
    /// 当 source ID 无效、offset 越界或不位于 UTF-8 字符边界时，返回对应的
    /// [`SourceError`]。
    pub fn position(
        &self,
        source_id: SourceId,
        offset: usize,
    ) -> Result<SourcePosition, SourceError> {
        let source = self.source(source_id)?;

        if offset > source.text.len() {
            return Err(SourceError::OffsetOutOfBounds {
                source_id,
                offset,
                source_len: source.text.len(),
            });
        }
        if !source.text.is_char_boundary(offset) {
            return Err(SourceError::NotCharBoundary { source_id, offset });
        }

        let line_index = source
            .line_starts
            .partition_point(|&line_start| line_start <= offset)
            .saturating_sub(1);
        let line_start = source.line_starts[line_index];
        let column = source.text[line_start..offset].chars().count() + 1;

        Ok(SourcePosition {
            line: line_index + 1,
            column,
        })
    }

    fn source(&self, source_id: SourceId) -> Result<&Source, SourceError> {
        if source_id.map_id != self.map_id {
            return Err(SourceError::InvalidSourceId { source_id });
        }

        self.sources
            .get(source_id.index)
            .ok_or(SourceError::InvalidSourceId { source_id })
    }

    fn source_for_span(&self, span: Span) -> Result<&Source, SourceError> {
        let source = self.source(span.source_id)?;

        self.validate_range(span.source_id, source, span.start, span.end)?;
        Ok(source)
    }

    fn validate_range(
        &self,
        source_id: SourceId,
        source: &Source,
        start: usize,
        end: usize,
    ) -> Result<(), SourceError> {
        if end > source.text.len() {
            return Err(SourceError::SpanOutOfBounds {
                source_id,
                start,
                end,
                source_len: source.text.len(),
            });
        }
        if !source.text.is_char_boundary(start) {
            return Err(SourceError::NotCharBoundary {
                source_id,
                offset: start,
            });
        }
        if !source.text.is_char_boundary(end) {
            return Err(SourceError::NotCharBoundary {
                source_id,
                offset: end,
            });
        }

        Ok(())
    }
}

fn next_source_map_id() -> u64 {
    NEXT_SOURCE_MAP_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("source map identity space is exhausted")
}

struct Source {
    name: String,
    text: String,
    line_starts: Vec<usize>,
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];

    for (offset, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(offset + 1);
        }
    }

    starts
}
