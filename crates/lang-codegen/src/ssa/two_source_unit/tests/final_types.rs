use super::*;
use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};

#[test]
fn final_types_reject_unreachable_member_in_complete_source_set() {
    for placement in 0..3 {
        let separate_file = placement == 2;
        let bad = "fun bad():Unit {val a=5;a.fooBar()}";
        let mut sources = SourceMap::new();
        let main = sources
            .add_source(
                "main.ko",
                source(false, if placement == 1 { bad } else { "" }),
            )
            .unwrap();
        let extra = sources
            .add_source("extra.ko", if separate_file { bad } else { "" })
            .unwrap();
        let parsed_main = parse_file(&sources, &lex(&sources, main).unwrap()).unwrap();
        let parsed_extra = parse_file(&sources, &lex(&sources, extra).unwrap()).unwrap();
        assert!(parsed_main.diagnostics().is_empty());
        assert!(parsed_extra.diagnostics().is_empty());
        let inputs = [
            SourceUnitInput::new("root", "main.ko", main, &parsed_main),
            SourceUnitInput::new("root", "extra.ko", extra, &parsed_extra),
        ];
        let (env, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "main")
            .unwrap()
            .id();
        if placement == 0 {
            let unit =
                prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry).unwrap();
            super::super::super::verify::verify_program(&unit.program).unwrap();
            let units =
                prepare_two_source_units(&sources, &inputs, &names, &types, &typed, &[entry])
                    .unwrap();
            assert!(units.parts(entry).is_some());
            super::super::super::verify::verify_program(&units.program).unwrap();
            continue;
        }
        let Err(FinalTypeAdmissionError::Diagnostics(expected)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, typed.types())
        else {
            panic!("final types must reject unreachable member");
        };
        assert_eq!(expected.len(), 1);
        assert_eq!(expected[0].code().to_string(), "L0080");
        assert_eq!(
            expected[0].primary_span().source_id(),
            if separate_file { extra } else { main }
        );
        assert_eq!(sources.slice(expected[0].primary_span()).unwrap(), "fooBar");
        let Err(TwoSourceUnitError::FinalTypes(FinalTypeAdmissionError::Diagnostics(actual))) =
            prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry)
        else {
            panic!("invalid full Unit must not issue a native capability");
        };
        assert_eq!(actual, expected);
        let Err(TwoSourceUnitError::FinalTypes(FinalTypeAdmissionError::Diagnostics(actual))) =
            prepare_two_source_units(&sources, &inputs, &names, &types, &typed, &[entry])
        else {
            panic!("multi-entry preparation must preserve full Unit rejection");
        };
        assert_eq!(actual, expected);
    }
}
