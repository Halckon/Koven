//! 字段提交必须有独立的 owned receiver/type、exclusive loan 与新值交付证明。
use super::{
    CompilationUnitOwnership, UnitCallArgumentOwnershipKind, UnitLoanTarget, UnitValueDeliveryKind,
};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        LoanKind, OwnershipPrimitiveValueTransfer, UnitFieldReplaceOwnershipPlan,
    },
    type_checking::{
        CompilationUnitTypes, NominalKind, OwnershipPrimitiveKind, UnitExpressionId, UnitTypeId,
        UnitTypeKind,
    },
};

impl CompilationUnitOwnership {
    /// 保存原调用已获准的精确 target，而非同类型 class 的任意字段集合。
    /// producer 已检查 var、local owner 与 projection；此处再用 typed descriptor/type 交叉核对。
    pub(super) fn field_replacement_types(
        typed: &CompilationUnitTypes,
        plans: &[UnitFieldReplaceOwnershipPlan],
    ) -> Vec<(
        UnitExpressionId,
        UnitSymbolId,
        UnitTypeId,
        UnitSymbolId,
        UnitTypeId,
    )> {
        let mut types = Vec::new();
        for plan in plans {
            let call = plan.descriptor().expression();
            if typed.ownership_primitive(call) != Some(*plan.descriptor()) {
                continue;
            }
            let [selected_field] = plan.place().fields() else {
                continue;
            };
            let root = plan.place().root();
            let Some(owner_type) = typed.symbol_type(root) else {
                continue;
            };
            let Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) = typed.types().get(owner_type)
            else {
                continue;
            };
            let Some(owner) = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|owner| owner.nominal())
            else {
                continue;
            };
            if owner.kind() != NominalKind::Class || !arguments.is_empty() {
                continue;
            }
            if let Some(field) = owner.fields().iter().find(|field| {
                field.symbol() == *selected_field && field.ty() == plan.descriptor().value_type()
            }) {
                types.push((call, root, owner_type, field.symbol(), field.ty()));
            }
        }
        types.sort();
        types.dedup();
        types
    }

    pub(super) fn field_replacements_are_valid(&self) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        self.field_replacements.iter().all(|plan| {
            let descriptor = plan.descriptor();
            let call = descriptor.expression();
            let place = plan.place();
            if !seen.insert(call)
                || descriptor.kind() != OwnershipPrimitiveKind::Replace
                || place.fields().len() != 1
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
                || !self.field_replacement_types.contains(&(
                    call,
                    place.root(),
                    plan.owner_type(),
                    place.fields()[0],
                    descriptor.value_type(),
                ))
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

    fn checked() -> (CompilationUnitOwnership, UnitSymbolId) {
        let mut sources = SourceMap::new();
        let source = sources.add_source("field.ko", "class Holder(var state: Int, val fixed: Int)\nfun run(): Unit {\nval holder = Holder(1, 2)\nval other = Holder(3, 4)\nval new = 2\nreplace(&holder.state, new)\nreplace(&other.state, new)\n}").unwrap();
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
