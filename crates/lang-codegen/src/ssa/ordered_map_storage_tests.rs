//! Ordinary source to verified LLVM layout observations; no native or IR mutation.

use super::model::SsaTypeKind;

#[test]
fn ordered_map_headers_preserve_shared_six_field_storage() {
    let sources = [
        r#"fun entry(): Unit {
 var values = mutableMapOf<Int, Int>()
 values.put(9, 90)
 values.put(1, 10)
 values.put(9, 91)
 values.remove(9)
 values.put(9, 92)
 println(values.size)
 val empty = mapOf<Int, Int>()
 println(empty.size)
}"#,
        r#"fun entry(): Unit {
 var values = mutableMapOf<String, String>()
 values.put("second".clone(), "old".clone())
 values.put("first".clone(), "value".clone())
 values.put("second".clone(), "new".clone())
 values.remove("first")
 values.put("first".clone(), "again".clone())
 println(values.size)
 val empty = mapOf<String, String>()
 println(empty.size)
}"#,
    ];
    let mut observations = Vec::new();
    for (source_index, source) in sources.iter().enumerate() {
        for unit in [false, true] {
            let program = super::owned_map_borrow_tests::lower(source, unit);
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            assert_eq!(program.modules.len(), 1);
            let maps: Vec<_> = program.modules[0]
                .types
                .iter()
                .enumerate()
                .filter_map(|(index, kind)| {
                    matches!(kind, SsaTypeKind::MapContainer { .. }).then_some(index)
                })
                .collect();
            assert_eq!(
                maps.len(),
                2,
                "Map and MutableMap must remain distinct SSA types"
            );
            let headers: Vec<Vec<String>> = maps
                .into_iter()
                .map(|index| {
                    let prefix = format!("%koven.map.t{index} = type {{ ");
                    let matches: Vec<_> = llvm
                        .lines()
                        .filter_map(|line| line.strip_prefix(&prefix))
                        .collect();
                    assert_eq!(matches.len(), 1, "{prefix}\n{llvm}");
                    matches[0]
                        .strip_suffix(" }")
                        .unwrap()
                        .split(", ")
                        .map(str::to_owned)
                        .collect()
                })
                .collect();
            println!("ORDERED MAP HEADER source={source_index} unit={unit}: {headers:?}");
            observations.push(headers);
        }
    }
    assert_eq!(observations.len(), 4);
    for headers in observations {
        assert_eq!(
            headers[0], headers[1],
            "same K/V must share physical storage"
        );
        for fields in headers {
            assert_eq!(
                fields.len(),
                6,
                "buffer,size,capacity,tombstones,first,last"
            );
            assert_eq!(fields[0], "ptr");
            assert!(fields[1].starts_with('i'));
            assert!(fields[1..].iter().all(|field| field == &fields[1]));
        }
    }
}
