//! source-qualified 候选合同及源码签名构造。
use super::*;
use crate::{name_resolution::ExternalSymbolId, type_checking::EnvironmentFunctionEffect};

/// Receiver-derived inputs kept in the canonical table before candidate discovery.
#[derive(Clone)]
pub(super) enum MemberOwner {
    Direct {
        declaration: DeclarationId,
        arguments: Vec<UnitTypeId>,
    },
    Inherited {
        receiver: UnitTypeId,
        template: UnitTypeId,
    },
}

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
    pub(super) member_owner: Option<MemberOwner>,
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
            member_owner: None,
            cross_thread_parameters: BTreeSet::new(),
            aborts: false,
            prints_line: false,
        }
    }
}

impl BodyChecker<'_> {
    pub(super) fn external_candidate(
        &mut self,
        external: ExternalSymbolId,
    ) -> Option<CallCandidate> {
        let ExternalTypeBinding::Function(signature) = self.environment.binding(external)?.clone()
        else {
            return None;
        };
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| MappedParameter {
                name: None,
                mode: parameter.mode,
                ty: self.normalize_environment_type(&parameter.ty),
                span: None,
            })
            .collect();
        Some(CallCandidate {
            target: UnitCallTarget::External(external),
            declaration_span: None,
            type_parameters: Vec::new(),
            move_only: false,
            parameters,
            return_type: self.normalize_environment_type(&signature.return_type),
            result_source: crate::type_checking::CallableResultSource::Owned,
            instance_arguments: Vec::new(),
            receiver: None,
            member_owner: None,
            cross_thread_parameters: signature
                .effects
                .iter()
                .filter_map(|effect| match effect {
                    EnvironmentFunctionEffect::CrossThreadTransfer { parameter } => {
                        Some(*parameter)
                    }
                    EnvironmentFunctionEffect::Abort | EnvironmentFunctionEffect::PrintLine => None,
                })
                .collect(),
            aborts: signature
                .effects
                .contains(&EnvironmentFunctionEffect::Abort),
            prints_line: signature
                .effects
                .contains(&EnvironmentFunctionEffect::PrintLine),
        })
    }
}
