//! Read-only checks of LLVM generated from the ordinary dynamic Range fixtures.
use std::collections::BTreeMap;

pub(super) fn assert_half_open_cfg(llvm: &str, expected: usize) {
    let mut count = 0;
    for function in llvm.split("define ").skip(1) {
        let body = function.split_once("\n}").expect("defined function body").0;
        let mut blocks: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut current = None;
        for line in body.lines() {
            if !line.starts_with(' ')
                && let Some((label, _)) = line.split_once(':')
            {
                current = Some(label);
            } else if let Some(label) = current {
                blocks.entry(label).or_default().push(line.trim());
            }
        }
        let mut previous_ready = None;
        // Walk textual instruction order, not the map's label ordering.
        for line in body.lines().map(str::trim) {
            let Some((predicate, operands)) = line.split_once(" = icmp slt ") else {
                continue;
            };
            if !predicate.ends_with(".nonempty") {
                continue;
            }
            count += 1;
            let (width, operands) = operands.split_once(' ').unwrap();
            assert!(matches!(width, "i32" | "i64"));
            let (_, bound) = operands.split_once(", ").unwrap();
            let (origin, instructions) = blocks
                .iter()
                .find(|(_, lines)| lines.contains(&line))
                .unwrap();
            if let Some(ready) = previous_ready {
                assert_eq!(
                    *origin, ready,
                    "next constructor continues in prior ready block"
                );
            }
            let prefix = format!("br i1 {predicate}, label %");
            let branch = instructions
                .iter()
                .find_map(|line| line.strip_prefix(&prefix))
                .unwrap();
            let (present, absent) = branch.split_once(", label %").unwrap();
            let present_lines = &blocks[present];
            let absent_lines = &blocks[absent];
            let suffix = format!(" = sub {width} {bound}, 1");
            let last = present_lines
                .iter()
                .find_map(|line| line.strip_suffix(&suffix))
                .unwrap();
            assert_eq!(
                present_lines
                    .iter()
                    .filter(|line| line.contains(" = sub "))
                    .count(),
                1
            );
            assert!(!absent_lines.iter().any(|line| line.contains(" = sub ")));
            assert!(!instructions.iter().any(|line| line.contains(&suffix)));
            let ready = present_lines
                .last()
                .unwrap()
                .strip_prefix("br label %")
                .unwrap();
            assert_eq!(absent_lines.last().unwrap(), &format!("br label %{ready}"));
            let phi = format!(" = phi {width} [ {last}, %{present} ], [ 0, %{absent} ]");
            assert_eq!(
                blocks[ready]
                    .iter()
                    .filter(|line| line.contains(&phi))
                    .count(),
                1
            );
            previous_ready = Some(ready);
        }
    }
    assert_eq!(count, expected);
}
