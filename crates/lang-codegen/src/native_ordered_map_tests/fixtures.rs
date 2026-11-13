//! Independent contract snapshots. Sources are separate ordinary .ko fixtures.

#[derive(Clone, Copy, Debug)]
pub(super) enum Key {
    Int(i32),
    String(&'static str),
}

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub(super) key: Key,
    pub(super) value: i32,
    pub(super) slot: Option<u64>,
}

#[derive(Debug)]
pub(super) struct Snapshot {
    pub(super) step: i32,
    pub(super) capacity: u64,
    pub(super) tombstones: u64,
    pub(super) entries: Vec<Entry>,
}

pub(super) struct Fixture {
    pub(super) name: &'static str,
    pub(super) source: &'static str,
    pub(super) readonly: bool,
    pub(super) string_keys: bool,
    pub(super) snapshots: Vec<Snapshot>,
}

impl Fixture {
    pub(super) fn stdout(&self) -> String {
        format!(
            "{}values\ndone\n",
            "checkpoint\n".repeat(self.snapshots.len())
        )
    }

    pub(super) fn stderr(&self) -> String {
        let mut output = String::new();
        for snapshot in &self.snapshots {
            output.push_str(&format!("snapshot {} ok\n", snapshot.step));
        }
        output.push_str(&format!("complete {}\n", self.snapshots.len()));
        output
    }
}

fn rows(
    step: i32,
    capacity: u64,
    tombstones: u64,
    values: &[(i32, i32)],
    strings: bool,
) -> Snapshot {
    // A fixed literal vocabulary for these manually specified contract sequences.
    // This does not read source operations, bucket order, or production hash/links.
    const TEXT: [&str; 50] = [
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31",
        "32", "33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46",
        "47", "48", "99",
    ];
    Snapshot {
        step,
        capacity,
        tombstones,
        entries: values
            .iter()
            .map(|&(key, value)| Entry {
                key: if strings {
                    Key::String(TEXT[if key == 99 { 49 } else { key as usize }])
                } else {
                    Key::Int(key)
                },
                value,
                slot: None,
            })
            .collect(),
    }
}

fn fixture(
    name: &'static str,
    source: &'static str,
    readonly: bool,
    strings: bool,
    snapshots: Vec<Snapshot>,
) -> Fixture {
    Fixture {
        name,
        source,
        readonly,
        string_keys: strings,
        snapshots,
    }
}

fn empty(strings: bool) -> Vec<Snapshot> {
    vec![
        rows(0, 16, 0, &[], strings),
        rows(1, 16, 0, &[(9, 90)], strings),
        rows(2, 16, 1, &[], strings),
        rows(3, 16, 1, &[], strings),
        rows(4, 16, 0, &[(9, 91)], strings),
    ]
}

fn endpoints(strings: bool) -> Vec<Snapshot> {
    vec![
        rows(0, 16, 0, &[], strings),
        rows(1, 16, 0, &[(9, 90)], strings),
        rows(2, 16, 0, &[(9, 90), (1, 10)], strings),
        rows(3, 16, 0, &[(9, 90), (1, 10), (7, 70)], strings),
        rows(4, 16, 0, &[(9, 90), (1, 10), (7, 70), (3, 30)], strings),
        rows(5, 16, 1, &[(1, 10), (7, 70), (3, 30)], strings),
        rows(6, 16, 2, &[(1, 10), (3, 30)], strings),
        rows(7, 16, 3, &[(1, 10)], strings),
        rows(8, 16, 4, &[], strings),
        rows(9, 16, 4, &[], strings),
    ]
}

fn overwrite(strings: bool) -> Vec<Snapshot> {
    vec![
        rows(0, 16, 0, &[], strings),
        rows(1, 16, 0, &[(9, 90)], strings),
        rows(2, 16, 0, &[(9, 90), (1, 10)], strings),
        rows(3, 16, 0, &[(9, 91), (1, 10)], strings),
        rows(4, 16, 1, &[(1, 10)], strings),
        rows(5, 16, 0, &[(1, 10), (9, 92)], strings),
    ]
}

const GROW_ORDER: [(i32, i32); 11] = [
    (9, 90),
    (1, 10),
    (7, 70),
    (3, 30),
    (11, 110),
    (5, 50),
    (0, 0),
    (13, 130),
    (2, 20),
    (15, 150),
    (4, 40),
];
const GROW_OVERWRITE: [(i32, i32); 11] = [
    (9, 999),
    (1, 10),
    (7, 70),
    (3, 30),
    (11, 110),
    (5, 50),
    (0, 0),
    (13, 130),
    (2, 20),
    (15, 150),
    (4, 40),
];

fn growth(strings: bool) -> Vec<Snapshot> {
    // The source is independently written. These are explicit contractual prefixes.
    let mut result = (0..=11)
        .map(|length| rows(length as i32, 16, 0, &GROW_ORDER[..length], strings))
        .collect::<Vec<_>>();
    result.push(rows(12, 32, 0, &GROW_OVERWRITE, strings));
    result
}

const REHASH_ORDER: [(i32, i32); 49] = [
    (48, 480),
    (47, 470),
    (46, 460),
    (45, 450),
    (44, 440),
    (43, 430),
    (42, 420),
    (41, 410),
    (40, 400),
    (39, 390),
    (38, 380),
    (37, 370),
    (36, 360),
    (35, 350),
    (34, 340),
    (33, 330),
    (32, 320),
    (31, 310),
    (30, 300),
    (29, 290),
    (28, 280),
    (27, 270),
    (26, 260),
    (25, 250),
    (24, 240),
    (23, 230),
    (22, 220),
    (21, 210),
    (20, 200),
    (19, 190),
    (18, 180),
    (17, 170),
    (16, 160),
    (15, 150),
    (14, 140),
    (13, 130),
    (12, 120),
    (11, 110),
    (10, 100),
    (9, 90),
    (8, 80),
    (7, 70),
    (6, 60),
    (5, 50),
    (4, 40),
    (3, 30),
    (2, 20),
    (1, 10),
    (0, 0),
];

fn rehash(strings: bool) -> Vec<Snapshot> {
    (0..=49)
        .map(|length| {
            let capacity = match length {
                0..=11 => 16,
                12..=23 => 32,
                24..=47 => 64,
                48..=49 => 128,
                _ => unreachable!(),
            };
            rows(length as i32, capacity, 0, &REHASH_ORDER[..length], strings)
        })
        .collect()
}

fn empty_growth(strings: bool) -> Vec<Snapshot> {
    let mut result = (0..=11)
        .map(|length| rows(length as i32, 16, 0, &GROW_ORDER[..length], strings))
        .collect::<Vec<_>>();
    // Removal source names the same eleven distinct keys in their original order.
    // The contract's independently registered surviving suffixes end at [].
    for removed in 1..=11 {
        result.push(rows(
            (11 + removed) as i32,
            16,
            removed as u64,
            &GROW_ORDER[removed..],
            strings,
        ));
    }
    result.push(rows(23, 32, 0, &[(99, 990)], strings));
    result
}

fn collisions(strings: bool) -> Vec<Snapshot> {
    let mut result = vec![
        rows(0, 16, 0, &[], false),
        rows(1, 16, 0, &[(0, 100)], false),
        rows(2, 16, 0, &[(0, 100), (5, 105)], false),
        rows(3, 16, 0, &[(0, 100), (5, 105), (10, 110)], false),
        rows(4, 16, 1, &[(5, 105), (10, 110)], false),
        rows(5, 16, 1, &[(5, 205), (10, 110)], false),
        rows(6, 16, 0, &[(5, 205), (10, 110), (0, 200)], false),
    ];
    for snapshot in &mut result {
        for entry in &mut snapshot.entries {
            // Independently calculated initial collisions: Int 0/5/10 -> 0;
            // String a/q/A -> 12. Their linear slots stay fixed at cap16.
            let Key::Int(key) = entry.key else {
                unreachable!()
            };
            let (slot, text) = match key {
                0 => (0, "a"),
                5 => (1, "q"),
                10 => (2, "A"),
                _ => unreachable!(),
            };
            entry.slot = Some(slot + if strings { 12 } else { 0 });
            if strings {
                entry.key = Key::String(text);
            }
        }
    }
    result
}

pub(super) fn empty_and_endpoints() -> Vec<Fixture> {
    vec![
        fixture(
            "int_readonly.ko",
            include_str!("fixtures/int_readonly.ko"),
            true,
            false,
            vec![rows(0, 16, 0, &[], false)],
        ),
        fixture(
            "int_empty.ko",
            include_str!("fixtures/int_empty.ko"),
            false,
            false,
            empty(false),
        ),
        fixture(
            "int_endpoints.ko",
            include_str!("fixtures/int_endpoints.ko"),
            false,
            false,
            endpoints(false),
        ),
    ]
}

pub(super) fn overwrite_reinsert() -> Fixture {
    fixture(
        "int_overwrite.ko",
        include_str!("fixtures/int_overwrite.ko"),
        false,
        false,
        overwrite(false),
    )
}

pub(super) fn collision() -> Fixture {
    fixture(
        "int_collision.ko",
        include_str!("fixtures/int_collision.ko"),
        false,
        false,
        collisions(false),
    )
}

pub(super) fn overwrite_growth() -> Fixture {
    fixture(
        "int_growth.ko",
        include_str!("fixtures/int_growth.ko"),
        false,
        false,
        growth(false),
    )
}

pub(super) fn multiple_rehashes() -> Fixture {
    fixture(
        "int_rehash.ko",
        include_str!("fixtures/int_rehash.ko"),
        false,
        false,
        rehash(false),
    )
}

pub(super) fn empty_tombstone_growth() -> Fixture {
    fixture(
        "int_empty_growth.ko",
        include_str!("fixtures/int_empty_growth.ko"),
        false,
        false,
        empty_growth(false),
    )
}

fn empty_key() -> Vec<Snapshot> {
    let order: &[&[(&str, i32)]] = &[
        &[],
        &[("", 10)],
        &[("", 10), ("a", 20)],
        &[("", 10), ("a", 20), ("aa", 30)],
        &[("", 11), ("a", 20), ("aa", 30)],
        &[("", 11), ("a", 20), ("aa", 31)],
        &[("a", 20), ("aa", 31)],
        &[("a", 20), ("aa", 31), ("", 12)],
    ];
    order
        .iter()
        .enumerate()
        .map(|(step, entries)| Snapshot {
            step: step as i32,
            capacity: 16,
            tombstones: u64::from(step == 6),
            entries: entries
                .iter()
                .map(|&(key, value)| Entry {
                    key: Key::String(key),
                    value,
                    slot: None,
                })
                .collect(),
        })
        .collect()
}

pub(super) fn string_cases() -> Vec<Fixture> {
    vec![
        fixture(
            "string_readonly.ko",
            include_str!("fixtures/string_readonly.ko"),
            true,
            true,
            vec![rows(0, 16, 0, &[], true)],
        ),
        fixture(
            "string_empty.ko",
            include_str!("fixtures/string_empty.ko"),
            false,
            true,
            empty(true),
        ),
        fixture(
            "string_endpoints.ko",
            include_str!("fixtures/string_endpoints.ko"),
            false,
            true,
            endpoints(true),
        ),
        fixture(
            "string_overwrite.ko",
            include_str!("fixtures/string_overwrite.ko"),
            false,
            true,
            overwrite(true),
        ),
        fixture(
            "string_collision.ko",
            include_str!("fixtures/string_collision.ko"),
            false,
            true,
            collisions(true),
        ),
        fixture(
            "string_growth.ko",
            include_str!("fixtures/string_growth.ko"),
            false,
            true,
            growth(true),
        ),
        fixture(
            "string_rehash.ko",
            include_str!("fixtures/string_rehash.ko"),
            false,
            true,
            rehash(true),
        ),
        fixture(
            "string_empty_growth.ko",
            include_str!("fixtures/string_empty_growth.ko"),
            false,
            true,
            empty_growth(true),
        ),
        fixture(
            "string_empty_key.ko",
            include_str!("fixtures/string_empty_key.ko"),
            false,
            true,
            empty_key(),
        ),
    ]
}
