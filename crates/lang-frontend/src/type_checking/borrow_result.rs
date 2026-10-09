use super::ParameterMode;
use crate::{
    parser::{BorrowReturnSource, FunctionForm, NameMarker, ParameterModeMarker, ValueParameter},
    source::{SourceError, SourceMap, Span},
};

/// 普通借用结果的唯一签名来源；不构造一等借用类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowReturnOrigin {
    /// 源码声明顺序的非 owning 参数。
    Parameter(usize),
    /// 当前实例 callable 的非 owning receiver。
    Receiver,
}

/// Phase 2 验证的普通借用结果合同；实际返回 origin 仍由 Phase 3 检查。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowReturnContract {
    origin: BorrowReturnOrigin,
    marker_span: Span,
    source_span: Span,
}

impl BorrowReturnContract {
    /// 返回唯一参数/receiver 来源。
    pub const fn origin(self) -> BorrowReturnOrigin {
        self.origin
    }
    /// 返回声明的真实 borrow marker。
    pub const fn marker_span(self) -> Span {
        self.marker_span
    }
    /// 返回声明的来源范围。
    pub const fn source_span(self) -> Span {
        self.source_span
    }
}

#[derive(Debug)]
pub(crate) struct BorrowReturnIssue {
    pub(crate) span: Span,
    pub(crate) message: &'static str,
}

pub(crate) fn resolve_borrow_return(
    sources: &SourceMap,
    form: FunctionForm,
    parameters: &[ValueParameter],
    receiver: Option<ParameterMode>,
) -> Result<Result<Option<BorrowReturnContract>, BorrowReturnIssue>, SourceError> {
    let FunctionForm::Explicit {
        borrow_return: Some(syntax),
        ..
    } = form
    else {
        return Ok(Ok(None));
    };
    let (origin, source_span) = match syntax.source {
        BorrowReturnSource::Receiver(span)
            if matches!(receiver, Some(ParameterMode::Borrow | ParameterMode::Inout)) =>
        {
            (BorrowReturnOrigin::Receiver, span)
        }
        BorrowReturnSource::Receiver(span) => {
            return Ok(Err(BorrowReturnIssue {
                span,
                message: "borrow result requires a non-owning receiver source",
            }));
        }
        BorrowReturnSource::Parameter(NameMarker::Present(span)) => {
            let name = sources.slice(span)?;
            let mut source = None;
            for (index, parameter) in parameters.iter().enumerate() {
                if let NameMarker::Present(parameter_name) = parameter.name
                    && sources.slice(parameter_name)? == name
                {
                    if matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))) {
                        return Ok(Err(BorrowReturnIssue {
                            span,
                            message: "owned parameter cannot be a borrow result source",
                        }));
                    }
                    source = Some(index);
                    break;
                }
            }
            let Some(index) = source else {
                return Ok(Err(BorrowReturnIssue {
                    span,
                    message: "borrow result source must name one parameter or this",
                }));
            };
            (BorrowReturnOrigin::Parameter(index), span)
        }
        BorrowReturnSource::Parameter(NameMarker::Missing(span) | NameMarker::Error(span)) => {
            return Ok(Err(BorrowReturnIssue {
                span,
                message: "borrow result requires a unique source",
            }));
        }
    };
    Ok(Ok(Some(BorrowReturnContract {
        origin,
        marker_span: syntax.borrow_span,
        source_span,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lexer::lex,
        parser::{Item, parse_file},
    };

    fn resolved(
        text: &str,
        receiver: Option<ParameterMode>,
    ) -> (SourceMap, Option<BorrowReturnContract>) {
        let mut sources = SourceMap::new();
        let source = sources.add_source("signature.ko", text).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(file.diagnostics().is_empty());
        let (form, parameters) = file
            .ast()
            .items()
            .iter()
            .find_map(|(_, node)| match node.payload() {
                Item::Function {
                    form, parameters, ..
                } => Some((*form, parameters)),
                _ => None,
            })
            .unwrap();
        let contract = resolve_borrow_return(&sources, form, parameters, receiver)
            .unwrap()
            .unwrap();
        (sources, contract)
    }

    #[test]
    fn unique_parameter_origin_uses_declaration_order_and_real_source_spans() {
        let (sources, contract) = resolved(
            "fun view(aux: Int, source: String): borrow String from source = source",
            None,
        );
        let contract = contract.unwrap();
        assert_eq!(contract.origin(), BorrowReturnOrigin::Parameter(1));
        assert_eq!(sources.slice(contract.marker_span()).unwrap(), "borrow");
        assert_eq!(sources.slice(contract.source_span()).unwrap(), "source");
        assert_eq!(contract.source_span().start(), 55);
    }

    #[test]
    fn receiver_contract_accepts_only_non_owning_modes() {
        for mode in [ParameterMode::Borrow, ParameterMode::Inout] {
            let (sources, contract) = resolved(
                "class Record { fun view(): borrow String from this }",
                Some(mode),
            );
            let contract = contract.unwrap();
            assert_eq!(contract.origin(), BorrowReturnOrigin::Receiver);
            assert_eq!(sources.slice(contract.source_span()).unwrap(), "this");
        }
    }

    #[test]
    fn default_owned_return_does_not_create_a_borrow_contract() {
        assert!(
            resolved("fun view(source: String): String = source", None)
                .1
                .is_none()
        );
        assert!(resolved("fun view() {}", None).1.is_none());
    }
}
