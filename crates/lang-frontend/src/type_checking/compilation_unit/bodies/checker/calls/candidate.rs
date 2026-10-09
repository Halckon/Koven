//! source-qualified 候选合同及源码签名构造。
use super::*;

#[derive(Clone)]
pub(super) struct CallCandidate {
    pub(super) target: UnitCallTarget,
    pub(super) declaration_span: Option<crate::source::Span>,
    pub(super) type_parameters: Vec<crate::name_resolution::UnitSymbolId>,
    pub(super) move_only: bool,
    pub(super) parameters: Vec<MappedParameter<UnitTypeId>>,
    pub(super) return_type: UnitTypeId,
    pub(super) result_source: crate::type_checking::CallableResultSource,
    pub(super) instance_arguments: Vec<UnitTypeId>,
    pub(super) receiver: Option<(ParameterMode, UnitTypeId)>,
    pub(super) owner_substitutions: BTreeMap<crate::name_resolution::UnitSymbolId, UnitTypeId>,
    pub(super) cross_thread_parameters: BTreeSet<usize>,
    pub(super) aborts: bool,
    pub(super) prints_line: bool,
}

impl CallCandidate {
    pub(super) fn from_signature(
        declaration: DeclarationId,
        callable: &UnitCallableSignature,
    ) -> Self {
        Self::from_source(UnitCallTarget::Declaration(declaration), callable)
    }

    pub(super) fn from_source(target: UnitCallTarget, callable: &UnitCallableSignature) -> Self {
        Self {
            target,
            declaration_span: Some(callable.name_span()),
            type_parameters: callable.type_parameters().to_vec(),
            move_only: false,
            parameters: callable
                .parameters()
                .iter()
                .map(|parameter| MappedParameter {
                    name: parameter.name().map(str::to_owned),
                    mode: parameter.mode(),
                    ty: parameter.ty(),
                    span: Some(parameter.span()),
                })
                .collect(),
            return_type: callable.return_type(),
            result_source: callable.result_source(),
            instance_arguments: Vec::new(),
            receiver: callable
                .receiver()
                .map(|receiver| (receiver.mode(), receiver.ty())),
            owner_substitutions: BTreeMap::new(),
            cross_thread_parameters: BTreeSet::new(),
            aborts: false,
            prints_line: false,
        }
    }
}
