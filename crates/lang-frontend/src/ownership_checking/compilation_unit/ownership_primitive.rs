//! 封闭 producer 的 root commit 必须与既有 source-qualified loan/delivery 契约一致。
use super::{
    CompilationUnitOwnership, UnitCallArgumentOwnershipKind, UnitLoanTarget, UnitValueDeliveryKind,
};
use crate::{
    ownership_checking::{LoanKind, OwnershipPrimitiveValueTransfer},
    type_checking::OwnershipPrimitiveKind,
};

impl CompilationUnitOwnership {
    pub(super) fn ownership_primitives_are_valid(&self) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        self.ownership_primitives.iter().all(|plan| {
            let descriptor = plan.descriptor();
            let call = descriptor.expression();
            let count = if descriptor.kind() == OwnershipPrimitiveKind::Replace {
                1
            } else {
                2
            };
            if !seen.insert(call)
                || plan.places().len() != count
                || descriptor
                    .operands()
                    .iter()
                    .any(|operand| operand.source_unit() != call.source_unit())
            {
                return false;
            }
            for (index, place) in plan.places().iter().enumerate() {
                if !place.is_root()
                    || place.root().source_unit() != call.source_unit()
                    || self
                        .bindings
                        .iter()
                        .any(|binding| binding.symbol() == place.root())
                    || plan.places()[..index]
                        .iter()
                        .any(|other| place.overlaps(other))
                {
                    return false;
                }
                let argument = descriptor.operands()[index];
                let loans = self
                    .loans
                    .iter()
                    .filter(|loan| loan.call() == call && loan.argument() == argument)
                    .collect::<Vec<_>>();
                if loans.len() != 1
                    || loans[0].kind() != LoanKind::Exclusive
                    || loans[0].target() != &UnitLoanTarget::Place(place.clone())
                {
                    return false;
                }
            }
            let contracts = self
                .call_argument_contracts
                .iter()
                .filter(|contract| contract.call() == call)
                .collect::<Vec<_>>();
            if contracts.len() != 2 {
                return false;
            }
            for (index, argument) in descriptor.operands().iter().enumerate() {
                let expected = if index < count {
                    UnitCallArgumentOwnershipKind::ExclusiveLoan
                } else {
                    UnitCallArgumentOwnershipKind::Value
                };
                if !contracts.iter().any(|contract| {
                    contract.argument() == *argument
                        && contract.parameter_index() == index
                        && contract.parameter_type() == descriptor.value_type()
                        && contract.kind() == expected
                }) {
                    return false;
                }
            }
            if descriptor.kind() == OwnershipPrimitiveKind::Swap {
                return plan.new_value_transfer().is_none();
            }
            let deliveries = self
                .value_deliveries
                .iter()
                .filter(|delivery| {
                    delivery.call() == call && delivery.argument() == descriptor.operands()[1]
                })
                .collect::<Vec<_>>();
            deliveries.len() == 1
                && plan.new_value_transfer()
                    == Some(match deliveries[0].kind() {
                        UnitValueDeliveryKind::Copy => OwnershipPrimitiveValueTransfer::Copy,
                        UnitValueDeliveryKind::Move => OwnershipPrimitiveValueTransfer::Move,
                        UnitValueDeliveryKind::Temporary => {
                            OwnershipPrimitiveValueTransfer::Temporary
                        }
                    })
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

    fn checked() -> CompilationUnitOwnership {
        let mut sources = SourceMap::new();
        let source = sources.add_source("root.ko", "fun run(): Unit {\nvar a = 1\nvar b = 2\nval old = replace(&a, b)\nswap(&a, &b)\n}").unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "root.ko", source, &file)];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
    }

    #[test]
    fn root_primitive_validation_rejects_missing_or_inconsistent_facts() {
        let valid = checked();
        assert_eq!(valid.ownership_primitives.len(), 2);
        assert!(valid.clone().validate().is_ok());
        type Mutation = fn(&mut CompilationUnitOwnership);
        let mutations: &[Mutation] = &[
            |owned| {
                owned
                    .ownership_primitives
                    .push(owned.ownership_primitives[0].clone())
            },
            |owned| owned.ownership_primitives[0].places.clear(),
            |owned| {
                owned.ownership_primitives[1].places[1] =
                    owned.ownership_primitives[1].places[0].clone()
            },
            |owned| owned.ownership_primitives[0].new_value_transfer = None,
            |owned| {
                owned.ownership_primitives[0].new_value_transfer =
                    Some(OwnershipPrimitiveValueTransfer::Move)
            },
            |owned| {
                owned.ownership_primitives[1].new_value_transfer =
                    Some(OwnershipPrimitiveValueTransfer::Copy)
            },
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
}
