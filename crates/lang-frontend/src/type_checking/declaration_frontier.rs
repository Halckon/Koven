//! 新声明仅完成语法交接时的统一语义门；不得按普通 owned 函数误编。
use crate::{
    parser::{FunctionForm, FunctionResultSource, Item},
    source::Span,
};

pub(super) fn unsupported_declaration(
    item: &Item,
    authorized_producer: bool,
) -> Option<(Span, &'static str)> {
    let Item::Function {
        extension_receiver,
        form,
        ..
    } = item
    else {
        return None;
    };
    if let Some(receiver) = extension_receiver {
        return Some((
            receiver.dot_span,
            "extension receiver ownership and lowering are not yet implemented",
        ));
    }
    if !authorized_producer
        && let FunctionForm::Explicit {
            result_source: Some(FunctionResultSource::Carrier(source)),
            ..
        } = form
    {
        return Some((
            source.from_span,
            "carrier producer source is not authorized",
        ));
    }
    None
}
