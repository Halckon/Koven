//! SPEC-0321: ordinary-source append delivery; no constructed or mutated SSA.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        Namespace, SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, UnitValueDeliveryKind, UnitValueDeliverySource,
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    parser::{Item, NameMarker, ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypes, Copyability, ExpressionCategory, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

use super::{
    model::Origin,
    unit_lower::{constant::lower_const_owned_unit_with_entry, lower_owned_unit_with_entry},
};

struct Case {
    name: &'static str,
    text: &'static str,
    operands: &'static [(&'static str, ExpressionCategory, UnitValueDeliveryKind)],
    copyability: Copyability,
}

const P38: Case = Case {
    name: "p38",
    copyability: Copyability::MoveOnly,
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/p38_list_of_strings.ko"),
    operands: &[
        (
            "\"x\"",
            ExpressionCategory::Temporary,
            UnitValueDeliveryKind::Temporary,
        ),
        (
            "\"y\"",
            ExpressionCategory::Temporary,
            UnitValueDeliveryKind::Temporary,
        ),
    ],
};
const GROUPED: Case = Case {
    name: "grouped",
    copyability: Copyability::MoveOnly,
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/grouped.ko"),
    operands: &[(
        "(\"x\")",
        ExpressionCategory::Temporary,
        UnitValueDeliveryKind::Temporary,
    )],
};
const NAMED: Case = Case {
    name: "named-root",
    copyability: Copyability::MoveOnly,
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/named-root.ko"),
    operands: &[("x", ExpressionCategory::Place, UnitValueDeliveryKind::Move)],
};
const EMPTY: Case = Case {
    name: "empty",
    copyability: Copyability::MoveOnly,
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/empty.ko"),
    operands: &[],
};
const COPYABLE: Case = Case {
    name: "copyable",
    copyability: Copyability::Copyable,
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/copyable.ko"),
    operands: &[("x", ExpressionCategory::Place, UnitValueDeliveryKind::Copy)],
};
const RETAINED: Case = Case {
    name: "retained-owner",
    copyability: Copyability::MoveOnly,
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/retained-owner.ko"),
    operands: &[(
        "\"x\"",
        ExpressionCategory::Temporary,
        UnitValueDeliveryKind::Temporary,
    )],
};

fn stage<T, E: std::fmt::Debug>(name: &str, result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{name}: {error:?}"))
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn parse(
    sources: &mut SourceMap,
    path: &str,
    text: &str,
) -> Result<(SourceId, ParsedFile), String> {
    let source = stage("source", sources.add_source(path, text))?;
    let lexed = stage("lex", lex(sources, source))?;
    require(
        lexed.diagnostics().is_empty(),
        &format!("lex diagnostics: {:?}", lexed.diagnostics()),
    )?;
    let file = stage("parse", parse_file(sources, &lexed))?;
    require(
        file.diagnostics().is_empty(),
        &format!("parse diagnostics: {:?}", file.diagnostics()),
    )?;
    Ok((source, file))
}

fn unit_deliveries(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    source_unit: SourceUnitId,
) -> Result<(), String> {
    let appends = typed.container_appends();
    require(
        appends.len() == case.operands.len(),
        "unit exact append descriptors",
    )?;
    for (append, &(text, category, kind)) in appends.iter().zip(case.operands) {
        let operand = append.element();
        require(
            operand.source_unit() == source_unit
                && append.expression().source_unit() == source_unit
                && typed.container_append(append.expression()) == Some(*append),
            "unit append call and operand belong to the original main source",
        )?;
        let call = stage(
            "unit append call",
            file.ast()
                .expressions()
                .get(append.expression().expression()),
        )?;
        require(
            call.span().source_id() == file.source_id(),
            "unit append call span belongs to original main source",
        )?;
        let node = stage(
            "unit append operand",
            file.ast().expressions().get(operand.expression()),
        )?;
        require(
            node.span().source_id() == file.source_id() && sources.slice(node.span()) == Ok(text),
            "unit real operand span",
        )?;
        require(
            typed.expression_category(operand) == Some(category),
            "unit operand category",
        )?;
        let ty = typed.expression_type(operand).ok_or("unit operand type")?;
        require(
            typed.copyability(ty) == case.copyability,
            "unit operand copyability matches the explicit original oracle",
        )?;
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|delivery| {
                delivery.call() == append.expression() && delivery.argument() == operand
            })
            .collect::<Vec<_>>();
        require(
            deliveries.len() == 1,
            "unit unique real call/argument delivery",
        )?;
        let delivery = deliveries[0];
        require(delivery.kind() == kind, "unit exact delivery kind")?;
        match kind {
            UnitValueDeliveryKind::Temporary => {
                require(
                    delivery.source() == &UnitValueDeliverySource::Temporary(operand),
                    "unit exact temporary source",
                )?;
            }
            UnitValueDeliveryKind::Move | UnitValueDeliveryKind::Copy => {
                let place = delivery.place().ok_or("unit delivery place")?;
                require(place.is_root(), "unit real root place")?;
                let references = names
                    .names()
                    .references()
                    .iter()
                    .filter(|reference| {
                        reference.source_unit() == operand.source_unit()
                            && reference.span() == node.span()
                            && reference.namespace() == Some(Namespace::Value)
                    })
                    .collect::<Vec<_>>();
                require(references.len() == 1, "unit unique operand Value reference")?;
                require(
                    references[0].target() == &UnitReferenceTarget::Symbol(place.root()),
                    "unit delivery root equals the actual operand's resolved Symbol",
                )?;
            }
        }
    }
    Ok(())
}

fn unit(case: &Case, extra: Option<bool>) -> Result<(), String> {
    let mut sources = SourceMap::new();
    let (source, file) = parse(&mut sources, "main.ko", case.text)?;
    // The first product has only the original main.ko; the other two retain their
    // original independent source bytes and basic/constant capability separately.
    let extra_file = extra
        .map(|constants| {
            let marker = if constants {
                "const val MARKER: Int = 1\n"
            } else {
                "// independent source\n"
            };
            parse(&mut sources, "extra.ko", marker)
        })
        .transpose()?;
    let mut inputs = vec![SourceUnitInput::new("root", "main.ko", source, &file)];
    if let Some((extra_source, extra_file)) = &extra_file {
        inputs.push(SourceUnitInput::new(
            "root",
            "extra.ko",
            *extra_source,
            extra_file,
        ));
    }
    let (environment, types) = standard_environments();
    let index = stage("unit index", index_compilation_unit(&sources, &inputs))?;
    let names = stage(
        "unit names",
        resolve_compilation_unit_names(&sources, &inputs, &index, &environment),
    )?;
    require(names.diagnostics().is_empty(), "unit names diagnostics")?;
    let names = stage("unit valid names", names.validate())?;
    let raw = stage(
        "unit types",
        check_compilation_unit_types(&sources, &inputs, &names, &types),
    )?;
    require(raw.diagnostics().is_empty(), "unit type diagnostics")?;
    let complete = stage(
        "unit final",
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw),
    )?;
    require(
        complete.is_same_analysis(&raw) && std::ptr::eq(complete.types(), &raw),
        "strict completion belongs to this product's raw types",
    )?;
    let main_sources = names
        .names()
        .index()
        .source_units()
        .iter()
        .filter(|unit| unit.source_id() == source)
        .collect::<Vec<_>>();
    require(main_sources.len() == 1, "unique original main source unit")?;
    let main_source = main_sources[0];
    require(
        names.names().index().source_units().len() == inputs.len()
            && inputs.len() == if extra.is_some() { 2 } else { 1 }
            && main_source
                .key()
                .root()
                .is_some_and(|root| root.as_str() == "root")
            && main_source
                .key()
                .logical_path()
                .is_some_and(|path| path.as_str() == "main.ko"),
        "exact product input count and original root/logical path",
    )?;
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == main_source.id()
                && declaration.name_span().source_id() == source
                && file.roots().contains(&declaration.root())
                && file.ast().items().get(declaration.root()).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                        if *name == declaration.name_span())
                })
        })
        .collect::<Vec<_>>();
    require(
        entries.len() == 1,
        "unique source-qualified original main Function",
    )?;
    let declaration = entries[0];
    let entry = declaration.id();
    let entry_span = stage(
        "unit original main root",
        file.ast().items().get(declaration.root()),
    )?
    .span();
    let result = if extra == Some(true) {
        let typed = stage("unit constant types", raw.validate_constants())?;
        let owned = stage(
            "unit constant ownership",
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed),
        )?;
        require(
            owned.ownership().diagnostics().is_empty(),
            "constant ownership diagnostics",
        )?;
        require(
            owned.ownership().deferred().is_empty(),
            "constant ownership deferred",
        )?;
        let owned = stage("unit valid constant ownership", owned.validate())?;
        unit_deliveries(
            case,
            &sources,
            &file,
            &names,
            typed.types(),
            owned.ownership(),
            main_source.id(),
        )?;
        eprintln!(
            "{} unit constant: frontend and exact delivery passed",
            case.name
        );
        let view = stage(
            "unit constant owned view",
            const_owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned),
        )?;
        lower_const_owned_unit_with_entry(&view, entry)
    } else {
        let typed = stage("unit basic types", raw.validate())?;
        let owned = stage(
            "unit basic ownership",
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed),
        )?;
        require(
            owned.diagnostics().is_empty(),
            "basic ownership diagnostics",
        )?;
        require(owned.deferred().is_empty(), "basic ownership deferred")?;
        let owned = stage("unit valid basic ownership", owned.validate())?;
        unit_deliveries(
            case,
            &sources,
            &file,
            &names,
            typed.types(),
            owned.ownership(),
            main_source.id(),
        )?;
        eprintln!(
            "{} unit basic: frontend and exact delivery passed",
            case.name
        );
        let view = stage(
            "unit basic owned view",
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned),
        )?;
        lower_owned_unit_with_entry(&view, entry)
    };
    let (program, entry) = stage("unit SSA", result)?;
    require(
        program
            .module(entry.module())
            .and_then(|module| module.function(entry))
            .is_some_and(|function| function.origin == Origin::Source(entry_span)),
        "lowered entry is the original main AST root",
    )?;
    if extra.is_some() {
        stage(
            "unit LLVM",
            crate::llvm::render_verified_program_with_entry(&program, entry),
        )?;
    } else {
        stage(
            "single-source unit LLVM",
            crate::llvm::render_verified_program(&program),
        )?;
    }
    Ok(())
}

fn paired(case: &Case) {
    // Collect all three independent products: no first-column red hides either control.
    let results = [
        ("single-source-unit", unit(case, None)),
        ("unit-basic", unit(case, Some(false))),
        ("unit-constant", unit(case, Some(true))),
    ];
    for (entry, result) in &results {
        eprintln!("{} {entry}: {result:?}", case.name);
    }
    assert!(
        results.iter().all(|(_, result)| result.is_ok()),
        "{} expected-positive entries: {results:?}",
        case.name
    );
}

#[test]
fn p38_original_append_then_for_has_verified_delivery() {
    paired(&P38);
}
#[test]
fn grouped_append_operand_has_verified_delivery() {
    paired(&GROUPED);
}
#[test]
fn named_root_append_operand_has_verified_delivery() {
    paired(&NAMED);
}
#[test]
fn empty_list_iteration_remains_valid() {
    paired(&EMPTY);
}
#[test]
fn copyable_append_preserves_original_binding() {
    paired(&COPYABLE);
}
#[test]
fn append_preserves_unrelated_live_owner() {
    paired(&RETAINED);
}
