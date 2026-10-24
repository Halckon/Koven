//! 两个普通 Resource local 的真实形成/构造/deinit/Drop 来源。
//! formed 是历史，不能代替当前可用性；不对未观察的 owner/phi/condition/Drop 数量做预言。
use super::{
    super::Report,
    ast::{Anchors, Input},
    unit as unit_facts,
};
use lang_frontend::{
    name_resolution::{Namespace, SymbolKind, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, UnitDropTarget},
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
    report: &mut Report,
) {
    let [source] = names.names().source_units() else {
        report.check(
            "unit temporary exits Guard unique source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let source_unit = source.source_unit();
    let mut local_ids = Vec::new();
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
                    && d.kind() == SymbolKind::Classifier
                    && d.namespace() == Namespace::Type
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
        report.check("unit temporary exits qualified Guard Resource constructor/deinit identity",
            local.is_some() && class.is_some_and(|id| super::calls_unit::callee(f, names, guard.initializer, id, Namespace::Type))
                && matches!(nominals.as_slice(), [nominal]
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
                    && typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
                    && local.is_some_and(|id| typed.symbol_type(id) == Some(c.result_type()))
                    && typed.is_resource_type(c.result_type()) == Some(true)),
            (guard, &classes, &locals, construction, &nominals));
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(id) if Some(id) == local))
            .collect::<Vec<_>>();
        report.check(
            "unit temporary exits Guard qualified formation history and actual Drop origins",
            local.is_some()
                && !drops.is_empty()
                && drops.iter().all(|drop| drop.value_origin() == guard.marker),
            (guard.marker, local, &drops, owned.formed_owned_locals()),
        );
        if let Some(local) = local {
            local_ids.push(local);
        }
        // Unit 不发布 single owner/condition 表；原 drop 与全 plan/cleanup 原样保全。
        println!(
            "SPEC0331 TEMPORARY EXITS UNIT GUARD marker={:?} initializer={expression:?} construction_span={:?} deinit_span={:?} drops={drops:#?}",
            guard.marker, guard.span, guard.deinit_span
        );
    }
    report.check("unit temporary exits two distinct qualified Guard locals share the real class/deinit",
        matches!(local_ids.as_slice(), [left, right] if left != right && left.source_unit() == source_unit && right.source_unit() == source_unit)
            && a.guards[0].class_item == a.guards[1].class_item
            && a.guards[0].deinit_item == a.guards[1].deinit_item,
        (&local_ids, &a.guards));
}
