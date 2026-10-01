use std::collections::BTreeSet;

use crate::type_checking::{
    BuiltinType, ExpressionCategory, OwnershipPrimitiveKind, ParameterMode, UnitCallTarget,
    UnitExpressionId, UnitTypeId, UnitTypeKind,
};

use super::{CompilationUnitTypes, checker::unit_assignable};

/// Phase 2 的 source-qualified 原语结构；不代表 Phase 3 的原子 commit 权限。
///
/// 非正常返回的 operand 不删除此事实，后继仍须按源码顺序求值调用前缀。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitOwnershipPrimitiveDescriptor {
    expression: UnitExpressionId,
    kind: OwnershipPrimitiveKind,
    value_type: UnitTypeId,
    operands: [UnitExpressionId; 2],
}

impl UnitOwnershipPrimitiveDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        kind: OwnershipPrimitiveKind,
        value_type: UnitTypeId,
        operands: [UnitExpressionId; 2],
    ) -> Self {
        Self {
            expression,
            kind,
            value_type,
            operands,
        }
    }

    /// 返回带 source identity 的原语调用。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回 compiler-bound 原语身份。
    #[must_use]
    pub const fn kind(self) -> OwnershipPrimitiveKind {
        self.kind
    }

    /// 返回被置换值的 unit-global 类型 `T`。
    #[must_use]
    pub const fn value_type(self) -> UnitTypeId {
        self.value_type
    }

    /// 返回源码顺序的两个 source-qualified 实参。
    #[must_use]
    pub const fn operands(self) -> [UnitExpressionId; 2] {
        self.operands
    }
}

impl CompilationUnitTypes {
    /// 只交叉核对已有 typed facts；精确 AST operand 顺序由 producer 绑定，后继读取 AST 时复核。
    pub(super) fn ownership_primitives_are_valid(&self) -> bool {
        let mut expressions = BTreeSet::new();
        self.ownership_primitives.iter().all(|fact| {
            if !expressions.insert(fact.expression)
                || !self.primitive_type_is_complete(fact.value_type)
                || fact
                    .operands
                    .iter()
                    .any(|operand| operand.source_unit() != fact.expression.source_unit())
            {
                return false;
            }
            let mut calls = self
                .calls
                .iter()
                .filter(|call| call.expression() == fact.expression);
            let Some(call) = calls.next() else {
                return false;
            };
            let expected_result = match fact.kind {
                OwnershipPrimitiveKind::Replace => fact.value_type,
                OwnershipPrimitiveKind::Swap => {
                    let Some(unit) = self.types().builtin(BuiltinType::Unit) else {
                        return false;
                    };
                    unit
                }
            };
            if calls.next().is_some()
                || !matches!(call.target(), UnitCallTarget::External(_))
                || call.instance().type_arguments() != [fact.value_type]
                || call.return_type() != expected_result
                || self.expression_type(fact.expression) != Some(expected_result)
                || self.expression_category(fact.expression) != Some(ExpressionCategory::Temporary)
                || call.receiver().is_some()
                || call.aborts()
                || call.prints_line()
                || call.arguments().len() != 2
            {
                return false;
            }
            for (index, operand) in fact.operands.iter().enumerate() {
                let Some(operand_type) = self.expression_type(*operand) else {
                    return false;
                };
                let inout = index == 0 || fact.kind == OwnershipPrimitiveKind::Swap;
                let argument = call.arguments()[index];
                if !self.primitive_type_is_complete(operand_type)
                    || !unit_assignable(self.types(), operand_type, fact.value_type)
                    || (inout && !unit_assignable(self.types(), fact.value_type, operand_type))
                    || argument.argument_index() != index
                    || argument.parameter_index() != index
                    || argument.parameter_type() != fact.value_type
                    || argument.mode()
                        != if inout {
                            ParameterMode::Inout
                        } else {
                            ParameterMode::Value
                        }
                    || argument.crosses_thread()
                    || self.expression_category(*operand) != Some(argument.category())
                    || (inout && argument.category() != ExpressionCategory::Place)
                {
                    return false;
                }
            }
            true
        })
    }

    fn primitive_type_is_complete(&self, root: UnitTypeId) -> bool {
        let mut pending = vec![root];
        let mut visited = BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match self.types().get(ty) {
                None
                | Some(
                    UnitTypeKind::Error
                    | UnitTypeKind::Deferred(_)
                    | UnitTypeKind::IntegerLiteral(_),
                ) => return false,
                Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner)) => {
                    pending.push(*inner)
                }
                Some(
                    UnitTypeKind::Nominal { arguments, .. }
                    | UnitTypeKind::Intrinsic { arguments, .. },
                ) => pending.extend(arguments),
                Some(UnitTypeKind::Function {
                    parameters,
                    return_type,
                    ..
                }) => {
                    pending.push(*return_type);
                    pending.extend(parameters.iter().map(|parameter| parameter.ty()));
                }
                Some(UnitTypeKind::EnumCase { root, .. }) => pending.push(*root),
                _ => {}
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::parse_file,
        source::SourceMap,
        type_checking::{
            BuiltinType, CompilationUnitTypes, ExpressionCategory, OwnershipPrimitiveKind,
            ParameterMode, UnitCallTarget, check_compilation_unit_types, standard_environments,
        },
    };

    fn checked(with_constant: bool) -> CompilationUnitTypes {
        let mut sources = SourceMap::new();
        let text =
            "fun run(): Unit {\nvar a = 1\nvar b = 2\nval old = replace(&a, 3)\nswap(&a, &b)\n}";
        let text = if with_constant {
            format!("const val ANSWER: Int = 1\n{text}")
        } else {
            text.to_owned()
        };
        let first = sources.add_source("a.ko", text).unwrap();
        let second = sources.add_source("b.ko", "fun other(): Unit {\nvar a = 1\nvar b = 2\nval old = replace(&a, 3)\nswap(&a, &b)\n}").unwrap();
        let first_file = parse_file(&sources, &lex(&sources, first).unwrap()).unwrap();
        let second_file = parse_file(&sources, &lex(&sources, second).unwrap()).unwrap();
        assert!(first_file.diagnostics().is_empty());
        assert!(second_file.diagnostics().is_empty());
        let inputs = [
            SourceUnitInput::new("root", "a.ko", first, &first_file),
            SourceUnitInput::new("root", "b.ko", second, &second_file),
        ];
        let (names, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.ownership_primitives().len(), 4);
        typed
    }

    #[test]
    fn primitive_validation_rejects_inconsistent_source_type_operand_and_call_facts() {
        let valid = checked(false);
        assert!(valid.clone().validate().is_ok());
        type Mutation = fn(&mut CompilationUnitTypes);
        let mutations: &[(&str, Mutation)] = &[
            ("duplicate primitive", |typed| {
                typed
                    .ownership_primitives
                    .push(typed.ownership_primitives[0])
            }),
            ("missing call", |typed| {
                let expression = typed.ownership_primitives[0].expression;
                typed.calls.retain(|call| call.expression != expression);
            }),
            ("duplicate call", |typed| {
                let call = typed
                    .call(typed.ownership_primitives[0].expression)
                    .unwrap()
                    .clone();
                typed.calls.push(call);
            }),
            ("ordinary target", |typed| {
                typed.calls[0].instance.target = UnitCallTarget::FunctionValue;
            }),
            ("wrong kind", |typed| {
                typed.ownership_primitives[0].kind = OwnershipPrimitiveKind::Swap;
            }),
            ("wrong value type", |typed| {
                typed.ownership_primitives[0].value_type =
                    typed.types().builtin(BuiltinType::Boolean).unwrap();
            }),
            ("foreign operand source", |typed| {
                typed.ownership_primitives[0].operands[0] =
                    typed.ownership_primitives[2].operands[0];
            }),
            ("temporary first operand", |typed| {
                typed.ownership_primitives[0].operands[0] =
                    typed.ownership_primitives[0].operands[1];
            }),
            ("wrong argument type", |typed| {
                typed.calls[0].arguments[0].parameter_type =
                    typed.types().builtin(BuiltinType::Boolean).unwrap();
            }),
            ("wrong argument mode", |typed| {
                typed.calls[0].arguments[0].mode = ParameterMode::Value;
            }),
            ("wrong argument mapping", |typed| {
                typed.calls[0].arguments[0].argument_index = 1;
            }),
            ("wrong result type", |typed| {
                typed.calls[0].return_type = typed.types().builtin(BuiltinType::Unit).unwrap();
            }),
            ("wrong operand category", |typed| {
                typed.expression_categories.insert(
                    typed.ownership_primitives[0].operands[0],
                    ExpressionCategory::Temporary,
                );
            }),
            ("missing operand type", |typed| {
                typed
                    .expression_types
                    .remove(&typed.ownership_primitives[0].operands[0]);
            }),
        ];
        for (label, mutate) in mutations {
            let mut invalid = valid.clone();
            mutate(&mut invalid);
            assert!(invalid.validate().is_err(), "{label}");
        }
    }

    #[test]
    fn constant_enabled_validation_also_checks_primitive_structure() {
        let valid = checked(true);
        assert!(valid.clone().validate_constants().is_ok());
        let mut invalid = valid;
        invalid.ownership_primitives[0].kind = OwnershipPrimitiveKind::Swap;
        assert!(invalid.validate_constants().is_err());
    }
}
