//! 两个普通 Resource local 的真实形成/构造/deinit/Drop 来源。
//! formed 是历史，不能代替当前可用性；不对未观察的 owner/phi/condition/Drop 数量做预言。
use super::{super::Report, ast::Anchors, unit as unit_facts};
use lang_frontend::{
    name_resolution::{SymbolKind, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, UnitDropTarget},
    type_checking::{
        CompilationUnitTypes, NominalKind, ParameterMode, UnitConstructionTarget, UnitExpressionId,
        UnitItemId, UnitStatementId, UnitTypeKind,
    },
};

pub(super) fn unit(
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    let [source] = names.names().source_units() else {
        report.check(
            "unit nested exits Guard unique source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let source_unit = source.source_unit();
    println!(
        "SPEC0331 NESTED EXITS UNIT TRANSFER expression={:?}",
        UnitExpressionId::new(source_unit, a.transfer)
    );
    for guard in &a.guards {
        // Guard 无普通 Name 读取；从公开形成历史取真实 qualified ID，再回核 AST marker。
        // 不从没有该 local 的全局 binding 表查询能力，也不构造私有 UnitSymbolId。
        let locals = owned
            .formed_owned_locals()
            .iter()
            .copied()
            .filter(|id| {
                id.source_unit() == source_unit
                    && unit_facts::binding_at(names, *id, guard.marker, SymbolKind::Variable)
            })
            .collect::<Vec<_>>();
        let local = match locals.as_slice() {
            [id] => Some(*id),
            _ => None,
        };
        let classes = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| {
                d.source_unit() == source_unit
                    && d.root() == guard.class_item
                    && d.name_span() == guard.class_marker
            })
            .collect::<Vec<_>>();
        let class = match classes.as_slice() {
            [d] => Some(d.id()),
            _ => None,
        };
        let nominals = typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| Some(d.declaration()) == class)
            .filter_map(|d| d.nominal())
            .collect::<Vec<_>>();
        let expression = UnitExpressionId::new(source_unit, guard.initializer);
        let construction = typed.construction(expression);
        report.check("unit nested exits qualified Guard Resource constructor/deinit identity",
            local.is_some() && class.is_some() && matches!(nominals.as_slice(), [nominal]
                if nominal.kind() == NominalKind::Class && nominal.fields().is_empty()
                    && Some(nominal.declaration()) == class
                    && nominal.deinit().is_some_and(|deinit| Some(deinit.owner()) == class
                        && deinit.item() == UnitItemId::new(source_unit, guard.deinit_item)
                        && deinit.body() == UnitStatementId::new(source_unit, guard.deinit_body)
                        && deinit.receiver_mode() == ParameterMode::Borrow))
                && construction.is_some_and(|c| c.expression() == expression
                    && c.arguments().is_empty() && c.instance().type_arguments().is_empty()
                    && matches!(c.target(), UnitConstructionTarget::Nominal(id) if Some(id) == class)
                    && matches!(typed.types().get(c.result_type()), Some(UnitTypeKind::Nominal { declaration, arguments })
                        if Some(*declaration) == class && arguments.is_empty())
                    && Some(c.result_type()) == typed.expression_type(expression)
                    && local.is_some_and(|id| typed.symbol_type(id) == Some(c.result_type()))
                    && typed.is_resource_type(c.result_type()) == Some(true)),
            (guard, &classes, &locals, construction, &nominals));
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(id) if Some(id) == local))
            .collect::<Vec<_>>();
        report.check(
            "unit nested exits Guard qualified formation history and actual Drop origins",
            local.is_some()
                && !drops.is_empty()
                && drops.iter().all(|drop| drop.value_origin() == guard.marker),
            (guard.marker, local, &drops, owned.formed_owned_locals()),
        );
        // Unit 不发布 single owner/condition 表；原 drop 与全 plan/cleanup 原样保全。
        println!(
            "SPEC0331 NESTED EXITS UNIT GUARD marker={:?} initializer={expression:?} construction_span={:?} deinit_span={:?} drops={drops:#?}",
            guard.marker, guard.span, guard.deinit_span
        );
    }
}
