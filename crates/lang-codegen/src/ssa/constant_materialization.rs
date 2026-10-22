//! 只映射已验证的常量物化值；身份、类型登记、Span 和 owner 仍由入口负责。
use lang_frontend::{ownership_checking::ConstantMaterializationKind, type_checking::ConstValue};

use super::{
    lowering_support::LoweringErrorKind,
    model::{Operation, ScalarConstant, SsaTypeId},
};

pub(super) fn operation(
    kind: ConstantMaterializationKind,
    value: &ConstValue,
    ty: SsaTypeId,
) -> Result<Operation, LoweringErrorKind> {
    Ok(match (kind, value) {
        (ConstantMaterializationKind::InlineCopy, ConstValue::Boolean(value)) => {
            Operation::Constant(ScalarConstant::Boolean(*value))
        }
        (ConstantMaterializationKind::InlineCopy, ConstValue::Integer { value, .. }) => {
            Operation::Constant(ScalarConstant::Integer(*value))
        }
        (ConstantMaterializationKind::InlineCopy, ConstValue::Char(value)) => {
            Operation::Constant(ScalarConstant::Char(u32::from(*value)))
        }
        (ConstantMaterializationKind::StringTemporary, ConstValue::String(bytes)) => {
            Operation::StringLiteral {
                string: ty,
                bytes: bytes.to_vec(),
            }
        }
        _ => return Err(LoweringErrorKind::MismatchedAnalysis),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssa::{
        model::{Program, SsaTypeKind},
        unit_lower_test_support::parsed,
    };
    use lang_frontend::{
        name_resolution::resolve_names,
        ownership_checking::check_ownership,
        source::SourceMap,
        type_checking::{check_types, standard_environments},
    };

    fn materialize(ty: &str, literal: &str, ssa_type: SsaTypeKind) -> (Operation, SsaTypeId) {
        let mut sources = SourceMap::new();
        let (_, file) = parsed(
            &mut sources,
            "constant.ko",
            &format!("const val VALUE: {ty} = {literal}\nfun entry(): {ty} = VALUE"),
        );
        let (name_environment, environment) = standard_environments();
        let names = resolve_names(&sources, &file, &name_environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &file, &names, &environment).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let materializations = owned.constant_materializations().unwrap();
        assert!(materializations.matches(&typed));
        let [plan] = materializations.plans() else {
            panic!("one runtime constant use");
        };
        let mut program = Program::default();
        let module_id = program.add_module("constant");
        let module = program.module_mut(module_id).unwrap();
        module.intern_type(SsaTypeKind::Unit);
        let ty = module.intern_type(ssa_type);
        let operation = operation(plan.kind(), plan.descriptor().value(), ty).unwrap();
        (operation, ty)
    }

    #[test]
    fn boolean_plan_maps_to_scalar() {
        for (literal, expected) in [("false", false), ("true", true)] {
            let (actual, _) = materialize("Boolean", literal, SsaTypeKind::Boolean);
            assert_eq!(
                actual,
                Operation::Constant(ScalarConstant::Boolean(expected))
            );
        }
    }

    #[test]
    fn integer_plan_preserves_exact_value() {
        for (ty, literal, bits, signed, expected) in [
            ("Int", "-42", 32, true, -42),
            (
                "ULong",
                "18446744073709551615uL",
                64,
                false,
                18446744073709551615,
            ),
        ] {
            let (actual, _) = materialize(ty, literal, SsaTypeKind::Integer { bits, signed });
            assert_eq!(
                actual,
                Operation::Constant(ScalarConstant::Integer(expected))
            );
        }
    }

    #[test]
    fn char_plan_preserves_unicode_scalar() {
        for (literal, expected) in [("'\\0'", '\0'), ("'文'", '文'), ("'🙂'", '🙂')] {
            let (actual, _) = materialize("Char", literal, SsaTypeKind::Char);
            assert_eq!(
                actual,
                Operation::Constant(ScalarConstant::Char(u32::from(expected)))
            );
        }
    }

    #[test]
    fn string_plan_preserves_bytes_and_supplied_type() {
        for (literal, bytes) in [
            (r#""""#, b"".as_slice()),
            (r#""中🙂\0tail""#, "中🙂\0tail".as_bytes()),
        ] {
            let (actual, ty) = materialize("String", literal, SsaTypeKind::StringOwner);
            assert_eq!(
                actual,
                Operation::StringLiteral {
                    string: ty,
                    bytes: bytes.to_vec(),
                }
            );
        }
    }
}
