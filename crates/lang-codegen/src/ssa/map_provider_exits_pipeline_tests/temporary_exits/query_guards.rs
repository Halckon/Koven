//! 两个独立 Resource local 共用真实 Guard 类型与析构；形成不冒充当前 owner。
use super::super::ast::{Anchors, Input};
use super::{Failure, failure, need, one, unit_bridge};
use lang_frontend::{
    name_resolution::{
        Namespace, SourceUnitId, SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::CompilationUnitOwnership,
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, NominalKind, ParameterMode,
        UnitConstructionTarget, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeKind,
    },
};

pub(super) fn unit(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
) -> Result<[UnitSymbolId; 2], Failure> {
    let mut locals = Vec::new();
    let mut classes = Vec::new();
    let mut types = Vec::new();
    for guard in &a.guards {
        // 普通 Guard 无 Name 读取；只从真实 formed 表取 qualified ID，再回核 names marker。
        let local = one(
            owned.formed_owned_locals().iter().copied().filter(|id| {
                id.source_unit() == unit
                    && unit_bridge::binding_at(names, *id, guard.marker, SymbolKind::Variable)
                        .is_ok()
            }),
            "temporary exits FE unique actual qualified Guard formation",
        )?;
        unit_bridge::binding_at(names, local, guard.marker, SymbolKind::Variable)?;
        let class = one(
            names.names().index().declarations().iter().filter(|d| {
                d.source_unit() == unit
                    && d.root() == guard.class_item
                    && d.name_span() == guard.class_marker
                    && d.kind() == SymbolKind::Classifier
                    && d.namespace() == Namespace::Type
            }),
            "temporary exits FE unique actual qualified Guard declaration",
        )?
        .id();
        let nominal = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter(|d| d.declaration() == class)
                .filter_map(|d| d.nominal()),
            "temporary exits FE unique qualified Guard nominal",
        )?;
        let expression = UnitExpressionId::new(unit, guard.initializer);
        let construction = typed.construction(expression).ok_or_else(|| {
            failure(
                "temporary exits FE actual qualified Guard construction",
                guard,
            )
        })?;
        let deinit = nominal
            .deinit()
            .ok_or_else(|| failure("temporary exits FE actual qualified Guard deinit", nominal))?;
        need(
            nominal.declaration() == class
                && nominal.kind() == NominalKind::Class
                && nominal.fields().is_empty()
                && nominal.type_parameters().is_empty()
                && deinit.owner() == class
                && deinit.item() == UnitItemId::new(unit, guard.deinit_item)
                && deinit.body() == UnitStatementId::new(unit, guard.deinit_body)
                && deinit.receiver_mode() == ParameterMode::Borrow
                && deinit.receiver_type() == construction.result_type()
                && construction.expression() == expression
                && construction.arguments().is_empty()
                && construction.instance().type_arguments().is_empty()
                && construction.target() == UnitConstructionTarget::Nominal(class)
                && matches!(typed.types().get(construction.result_type()),
                Some(UnitTypeKind::Nominal { declaration, arguments }) if *declaration == class && arguments.is_empty())
                && typed.expression_type(expression) == Some(construction.result_type())
                && typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
                && typed.symbol_type(local) == Some(construction.result_type())
                && typed.is_resource_type(construction.result_type()) == Some(true),
            "temporary exits FE qualified Guard Resource formation, nominal and real deinit",
            (guard, local, nominal, construction, deinit),
        )?;
        unit_bridge::callee(f, names, guard.initializer, class, Namespace::Type)?;
        locals.push(local);
        classes.push(class);
        types.push(construction.result_type());
    }
    need(
        locals.len() == 2
            && locals[0] != locals[1]
            && classes[0] == classes[1]
            && types[0] == types[1],
        "temporary exits FE qualified Guard locals distinct and same class/type",
        (&locals, classes, types),
    )?;
    locals
        .try_into()
        .map_err(|v| failure("temporary exits FE two actual qualified Guard locals", v))
}
