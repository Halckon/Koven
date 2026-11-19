//! 字段提交必须有精确 receiver、exclusive loan 与新值交付证明。
use std::{cell::OnceCell, collections::BTreeMap};

use super::{
    CompilationUnitOwnership, UnitCallArgumentOwnershipKind, UnitLoanTarget, UnitValueDeliveryKind,
};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        LoanKind, OwnershipPrimitiveValueTransfer, UnitFieldReplaceOwnershipPlan,
    },
    source::Span,
    type_checking::{
        CompilationUnitTypes, NominalKind, OwnershipPrimitiveKind, ParameterMode,
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitExpressionId, UnitTypeId,
        UnitTypeKind,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FieldReplacementType {
    call: UnitExpressionId,
    root: UnitSymbolId,
    owner_type: UnitTypeId,
    field: UnitSymbolId,
    value_type: UnitTypeId,
    inout_receiver: Option<(UnitExpressionId, Span)>,
}

impl FieldReplacementType {
    fn from_plan(plan: &UnitFieldReplaceOwnershipPlan) -> Option<Self> {
        let place = plan.place();
        let root = place.root();
        let field = match (plan.inout_receiver(), place.fields()) {
            (Some(_), []) => root,
            (None, [field]) => *field,
            _ => return None,
        };
        Some(Self {
            call: plan.descriptor().expression(),
            root,
            owner_type: plan.owner_type(),
            field,
            value_type: plan.descriptor().value_type(),
            inout_receiver: plan.inout_receiver(),
        })
    }

    fn matches_types(
        &self,
        typed: &CompilationUnitTypes,
        inout_receivers: &OnceCell<BTreeMap<(UnitTypeId, usize), Span>>,
    ) -> bool {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = typed.types().get(self.owner_type)
        else {
            return false;
        };
        let Some(owner) = typed
            .signatures()
            .declaration(*declaration)
            .and_then(|owner| owner.nominal())
        else {
            return false;
        };
        if owner.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !owner
                .fields()
                .iter()
                .any(|field| field.symbol() == self.field && field.ty() == self.value_type)
        {
            return false;
        }
        let Some((projection, receiver_span)) = self.inout_receiver else {
            return typed.symbol_type(self.root) == Some(self.owner_type);
        };
        let Some(projection) = typed.aggregate_projection(projection) else {
            return false;
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return false;
        };
        projection.expression().source_unit() == self.call.source_unit()
            && receiver.source_unit() == self.call.source_unit()
            && projection.kind() == UnitAggregateProjectionKind::Field
            && projection.field() == self.field
            && projection.ty() == self.value_type
            && typed.expression_type(receiver) == Some(self.owner_type)
            && inout_receivers
                .get_or_init(|| {
                    typed
                        .signatures()
                        .declarations()
                        .iter()
                        .filter_map(|owner| owner.nominal())
                        .flat_map(|owner| owner.members())
                        .filter_map(|member| member.receiver())
                        .filter(|receiver| receiver.mode() == ParameterMode::Inout)
                        .map(|receiver| {
                            let span = receiver.declaration_span();
                            ((receiver.ty(), span.start()), span)
                        })
                        .collect()
                })
                .get(&(self.owner_type, receiver_span.start()))
                == Some(&receiver_span)
    }
}

impl CompilationUnitOwnership {
    /// 精确冻结 producer 已核对的字段与所有者种类，验证时拒绝变更。
    pub(super) fn field_replacement_types(
        typed: &CompilationUnitTypes,
        plans: &[UnitFieldReplaceOwnershipPlan],
    ) -> BTreeMap<UnitExpressionId, FieldReplacementType> {
        let inout_receivers = OnceCell::new();
        plans
            .iter()
            .filter_map(|plan| {
                let proof = FieldReplacementType::from_plan(plan)?;
                (typed.ownership_primitive(proof.call) == Some(*plan.descriptor())
                    && proof.matches_types(typed, &inout_receivers))
                .then_some((proof.call, proof))
            })
            .collect()
    }

    pub(super) fn field_replacements_are_valid(&self) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        self.field_replacements.iter().all(|plan| {
            let descriptor = plan.descriptor();
            let call = descriptor.expression();
            let place = plan.place();
            if !seen.insert(call)
                || descriptor.kind() != OwnershipPrimitiveKind::Replace
                || place.element().is_some()
                || place.root().source_unit() != call.source_unit()
                || descriptor
                    .operands()
                    .iter()
                    .any(|argument| argument.source_unit() != call.source_unit())
                || self.ownership_primitive(call).is_some()
                || self
                    .bindings
                    .iter()
                    .any(|binding| binding.symbol() == place.root())
                || FieldReplacementType::from_plan(plan)
                    .is_none_or(|proof| self.field_replacement_types.get(&call) != Some(&proof))
            {
                return false;
            }
            let loans = self
                .loans
                .iter()
                .filter(|loan| loan.call() == call && loan.argument() == descriptor.operands()[0])
                .collect::<Vec<_>>();
            if loans.len() != 1
                || loans[0].kind() != LoanKind::Exclusive
                || loans[0].target() != &UnitLoanTarget::Place(place.clone())
            {
                return false;
            }
            let contracts = self
                .call_argument_contracts
                .iter()
                .filter(|contract| contract.call() == call)
                .collect::<Vec<_>>();
            if contracts.len() != 2
                || !descriptor
                    .operands()
                    .iter()
                    .enumerate()
                    .all(|(index, argument)| {
                        contracts.iter().any(|contract| {
                            contract.argument() == *argument
                                && contract.parameter_index() == index
                                && contract.parameter_type() == descriptor.value_type()
                                && contract.kind()
                                    == if index == 0 {
                                        UnitCallArgumentOwnershipKind::ExclusiveLoan
                                    } else {
                                        UnitCallArgumentOwnershipKind::Value
                                    }
                        })
                    })
            {
                return false;
            }
            let deliveries = self
                .value_deliveries
                .iter()
                .filter(|delivery| {
                    delivery.call() == call && delivery.argument() == descriptor.operands()[1]
                })
                .collect::<Vec<_>>();
            deliveries.len() == 1
                && !deliveries[0]
                    .place()
                    .is_some_and(|source| source.overlaps(place))
                && plan.new_value_transfer()
                    == match deliveries[0].kind() {
                        UnitValueDeliveryKind::Copy => OwnershipPrimitiveValueTransfer::Copy,
                        UnitValueDeliveryKind::Move => OwnershipPrimitiveValueTransfer::Move,
                        UnitValueDeliveryKind::Temporary => {
                            OwnershipPrimitiveValueTransfer::Temporary
                        }
                    }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::check_compilation_unit_ownership,
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };

    fn checked_source(text: &str) -> (CompilationUnitOwnership, UnitSymbolId) {
        let mut sources = SourceMap::new();
        let source = sources.add_source("field.ko", text).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "field.ko", source, &file)];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let source_names = &names.names().source_units()[0];
        let fixed = source_names
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "fixed")
            .unwrap()
            .id();
        let fixed = UnitSymbolId::new(source_names.source_unit(), fixed);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        (
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap(),
            fixed,
        )
    }

    fn checked() -> (CompilationUnitOwnership, UnitSymbolId) {
        checked_source(
            "class Holder(var state: Int, val fixed: Int)\nfun run(): Unit {\nval holder = Holder(1, 2)\nval other = Holder(3, 4)\nval new = 2\nreplace(&holder.state, new)\nreplace(&other.state, new)\n}",
        )
    }

    #[test]
    fn receiver_field_replace_validation_rejects_another_methods_projection() {
        let (valid, _) = checked_source(
            "class Holder(var state: Int, val fixed: Int) {\n\
                inout fun first(): Int { return replace(&this.state, 2) }\n\
                inout fun second(): Int { return replace(&this.state, 3) }\n\
             }",
        );
        let [first, second] = valid.field_replacements.as_slice() else {
            panic!("two valid commits with the same field identity");
        };
        assert_eq!(first.place(), second.place());
        let (first_projection, first_receiver) = first.inout_receiver().unwrap();
        let (_, second_receiver) = second.inout_receiver().unwrap();
        assert_ne!(first_receiver, second_receiver);
        assert!(valid.clone().validate().is_ok());

        let mut wrong_method = valid.clone();
        wrong_method.field_replacements[0].inout_receiver =
            Some((first_projection, second_receiver));
        assert!(wrong_method.validate().is_err());

        let mut another_projection = valid.clone();
        another_projection.field_replacements[0].inout_receiver = second.inout_receiver();
        assert!(another_projection.validate().is_err());
    }

    #[test]
    fn field_replace_validation_rejects_missing_or_inconsistent_facts() {
        let (valid, _) = checked();
        assert_eq!(valid.field_replacements.len(), 2);
        assert!(valid.clone().validate().is_ok());
        type Mutation = fn(&mut CompilationUnitOwnership);
        let mutations: &[Mutation] = &[
            |owned| {
                owned
                    .field_replacements
                    .push(owned.field_replacements[0].clone())
            },
            |owned| {
                owned.field_replacements[0].place = super::super::UnitOwnershipPlace::new(
                    owned.field_replacements[0].place.root(),
                    Vec::new(),
                )
            },
            |owned| {
                let root = owned.field_replacements[0].place.root();
                owned.field_replacements[0].place =
                    super::super::UnitOwnershipPlace::new(root, vec![root]);
            },
            |owned| {
                let field = owned.field_replacements[0].place.fields()[0];
                owned.field_replacements[0].place =
                    super::super::UnitOwnershipPlace::new(field, vec![field]);
            },
            |owned| {
                owned.field_replacements[0].owner_type =
                    owned.field_replacements[0].descriptor.value_type()
            },
            |owned| {
                owned.field_replacements[0].new_value_transfer =
                    OwnershipPrimitiveValueTransfer::Move
            },
            |owned| owned.field_replacement_types.clear(),
            |owned| owned.loans.clear(),
            |owned| owned.value_deliveries.clear(),
            |owned| owned.call_argument_contracts.clear(),
        ];
        for mutation in mutations {
            let mut damaged = valid.clone();
            mutation(&mut damaged);
            assert!(damaged.validate().is_err());
        }
    }

    #[test]
    fn field_replace_validation_rejects_paired_loan_target_substitution() {
        let (valid, fixed) = checked();
        assert!(valid.clone().validate().is_ok());
        for place in [
            super::super::UnitOwnershipPlace::new(
                valid.field_replacements[0].place.root(),
                vec![fixed],
            ),
            valid.field_replacements[1].place.clone(),
        ] {
            let mut damaged = valid.clone();
            let descriptor = damaged.field_replacements[0].descriptor;
            damaged.field_replacements[0].place = place.clone();
            let loan = damaged
                .loans
                .iter_mut()
                .find(|loan| {
                    loan.call() == descriptor.expression()
                        && loan.argument() == descriptor.operands()[0]
                })
                .unwrap();
            loan.target = UnitLoanTarget::Place(place);
            assert!(
                damaged.validate().is_err(),
                "a matching loan cannot authorize a different field or receiver"
            );
        }
    }
}
