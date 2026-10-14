//! SPEC-0321: ordinary-source append delivery; no constructed or mutated SSA.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, UnitValueDeliveryKind, UnitValueDeliverySource,
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypes, Copyability, ExpressionCategory, check_compilation_unit_types,
        check_types, complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    unit_lower::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry},
};

struct Case {
    name: &'static str,
    text: &'static str,
    operands: &'static [(&'static str, ExpressionCategory, UnitValueDeliveryKind)],
}

const P38: Case = Case {
    name: "p38",
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
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/grouped.ko"),
    operands: &[(
        "(\"x\")",
        ExpressionCategory::Temporary,
        UnitValueDeliveryKind::Temporary,
    )],
};
const NAMED: Case = Case {
    name: "named-root",
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/named-root.ko"),
    operands: &[("x", ExpressionCategory::Place, UnitValueDeliveryKind::Move)],
};
const EMPTY: Case = Case {
    name: "empty",
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/empty.ko"),
    operands: &[],
};
const COPYABLE: Case = Case {
    name: "copyable",
    text: include_str!("../../tests/fixtures/mutable-list-delivery-0321/copyable.ko"),
    operands: &[("x", ExpressionCategory::Place, UnitValueDeliveryKind::Copy)],
};
const RETAINED: Case = Case {
    name: "retained-owner",
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
    let file = stage("parse", parse_file(sources, &lexed))?;
    require(
        file.diagnostics().is_empty(),
        &format!("parse diagnostics: {:?}", file.diagnostics()),
    )?;
    Ok((source, file))
}

fn single(case: &Case) -> Result<(), String> {
    let mut sources = SourceMap::new();
    let (_, file) = parse(&mut sources, "main.ko", case.text)?;
    let (environment, types) = standard_environments();
    let names = stage("single names", resolve_names(&sources, &file, &environment))?;
    let typed = stage("single types", check_types(&sources, &file, &names, &types))?;
    let _final = stage(
        "single final",
        complete_file_types(&sources, &file, &names, &types, &typed),
    )?;
    let owned = stage(
        "single ownership",
        check_ownership(&sources, &file, &names, &typed),
    )?;
    require(
        owned.diagnostics().is_empty(),
        &format!("single ownership diagnostics: {:?}", owned.diagnostics()),
    )?;
    require(
        owned.deferred().is_empty(),
        &format!("single ownership deferred: {:?}", owned.deferred()),
    )?;
    let appends = typed.container_appends();
    require(
        appends.len() == case.operands.len(),
        "single exact append descriptors",
    )?;
    for (append, &(text, category, _)) in appends.iter().zip(case.operands) {
        let node = stage(
            "single append operand",
            file.ast().expressions().get(append.element()),
        )?;
        require(
            sources.slice(node.span()) == Ok(text),
            "single real operand span",
        )?;
        require(
            typed.expression_category(append.element()) == Some(category),
            "single operand category",
        )?;
        let ty = typed
            .expression_type(append.element())
            .ok_or("single operand type")?;
        let expected = if case.name == "copyable" {
            Copyability::Copyable
        } else {
            Copyability::MoveOnly
        };
        require(
            typed.copyability(ty) == Some(expected),
            "single operand copyability",
        )?;
    }
    eprintln!(
        "{} single: frontend and exact append identity passed",
        case.name
    );
    let program = stage(
        "single SSA",
        lower_scalar_file(&sources, &file, &names, &typed, &owned),
    )?;
    stage(
        "single LLVM",
        crate::llvm::render_verified_program(&program),
    )?;
    Ok(())
}

fn unit_deliveries(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), String> {
    let appends = typed.container_appends();
    require(
        appends.len() == case.operands.len(),
        "unit exact append descriptors",
    )?;
    for (append, &(text, category, kind)) in appends.iter().zip(case.operands) {
        let operand = append.element();
        let node = stage(
            "unit append operand",
            file.ast().expressions().get(operand.expression()),
        )?;
        require(
            sources.slice(node.span()) == Ok(text),
            "unit real operand span",
        )?;
        require(
            typed.expression_category(operand) == Some(category),
            "unit operand category",
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
                require(
                    delivery.place().is_some_and(|place| place.is_root()),
                    "unit real root place",
                )?;
            }
        }
    }
    Ok(())
}

fn unit(case: &Case, constants: bool) -> Result<(), String> {
    let mut sources = SourceMap::new();
    let (source, file) = parse(&mut sources, "main.ko", case.text)?;
    // The original program is byte-identical; a second source selects the constant route.
    let marker = if constants {
        "const val MARKER: Int = 1\n"
    } else {
        "// independent source\n"
    };
    let (extra_source, extra) = parse(&mut sources, "extra.ko", marker)?;
    let inputs = [
        SourceUnitInput::new("root", "main.ko", source, &file),
        SourceUnitInput::new("root", "extra.ko", extra_source, &extra),
    ];
    let (environment, types) = standard_environments();
    let index = stage("unit index", index_compilation_unit(&sources, &inputs))?;
    let names = stage(
        "unit names",
        resolve_compilation_unit_names(&sources, &inputs, &index, &environment),
    )?;
    let names = stage("unit valid names", names.validate())?;
    let raw = stage(
        "unit types",
        check_compilation_unit_types(&sources, &inputs, &names, &types),
    )?;
    let _final = stage(
        "unit final",
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw),
    )?;
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "main")
        .ok_or("unit entry")?
        .id();
    let result = if constants {
        let typed = stage("unit constant types", raw.validate_constants())?;
        let owned = stage(
            "unit constant ownership",
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed),
        )?;
        let owned = stage("unit valid constant ownership", owned.validate())?;
        unit_deliveries(case, &sources, &file, typed.types(), owned.ownership())?;
        eprintln!(
            "{} unit constant: frontend and exact delivery passed",
            case.name
        );
        lower_constant_unit_with_entry(&sources, &inputs, &names, &types, &typed, &owned, entry)
    } else {
        let typed = stage("unit basic types", raw.validate())?;
        let owned = stage(
            "unit basic ownership",
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed),
        )?;
        let owned = stage("unit valid basic ownership", owned.validate())?;
        unit_deliveries(case, &sources, &file, typed.types(), owned.ownership())?;
        eprintln!(
            "{} unit basic: frontend and exact delivery passed",
            case.name
        );
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &types, &typed, &owned, entry)
    };
    let (program, entry) = stage("unit SSA", result)?;
    stage(
        "unit LLVM",
        crate::llvm::render_verified_program_with_entry(&program, entry),
    )?;
    Ok(())
}

fn paired(case: &Case) {
    // Collect every entry before asserting: a single-file red cannot hide either unit control.
    let results = [
        ("single", single(case)),
        ("unit-basic", unit(case, false)),
        ("unit-constant", unit(case, true)),
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
