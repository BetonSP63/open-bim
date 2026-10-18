//! Malformed-input corpus: ≥1000 deterministic mutations per format of real
//! project, IFC and DXF files. Every input must yield Ok or Err — never a
//! panic, abort or unbounded allocation.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

const PER_FORMAT: usize = 1200;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Truncation, byte flips, deletions, duplicated slices, hostile tokens.
fn mutate(seed: &[u8], rng: &mut Rng, tokens: &[&[u8]]) -> Vec<u8> {
    let mut data = seed.to_vec();
    let rounds = 1 + rng.below(4);
    for _ in 0..rounds {
        let len = data.len().max(1);
        match rng.below(9) {
            0 => data.truncate(rng.below(len)),
            1 => {
                for _ in 0..1 + rng.below(16) {
                    let i = rng.below(len);
                    if i < data.len() {
                        data[i] = rng.next() as u8;
                    }
                }
            }
            2 => {
                let a = rng.below(len);
                let b = (a + rng.below(256)).min(data.len());
                data.drain(a.min(b)..b);
            }
            3 => {
                let a = rng.below(len);
                let b = (a + rng.below(512)).min(data.len());
                let slice = data[a.min(b)..b].to_vec();
                let at = rng.below(data.len().max(1));
                data.splice(at..at, slice);
            }
            4 => {
                let token = tokens[rng.below(tokens.len())];
                let at = rng.below(data.len().max(1)).min(data.len());
                data.splice(at..at, token.iter().copied());
            }
            5 => {
                // Deep nesting.
                let depth = 1 + rng.below(5000);
                let at = rng.below(data.len().max(1)).min(data.len());
                let open: Vec<u8> = std::iter::repeat_n(b'[', depth)
                    .chain(std::iter::repeat_n(b'(', depth))
                    .collect();
                data.splice(at..at, open);
            }
            6 => data = (0..rng.below(4096)).map(|_| rng.next() as u8).collect(),
            7 => {
                // Huge numeric literal.
                let at = rng.below(data.len().max(1)).min(data.len());
                let digits = format!("1{}", "9".repeat(rng.below(400)));
                data.splice(at..at, digits.into_bytes());
            }
            _ => {
                // Replace a number-looking byte run with extreme values.
                let replacement: &[u8] =
                    [&b"1e308"[..], b"-1e308", b"NaN", b"inf", b"0", b"-0.0"][rng.below(6)];
                if let Some(i) = data.iter().position(|b| b.is_ascii_digit()) {
                    let start = (i + rng.below(data.len() - i)).min(data.len() - 1);
                    data.splice(
                        start..(start + 1).min(data.len()),
                        replacement.iter().copied(),
                    );
                }
            }
        }
    }
    data
}

fn corpus(
    name: &str,
    seed: Vec<u8>,
    tokens: &[&[u8]],
    check: impl Fn(&[u8]) -> bool,
) -> serde_json::Value {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ name.len() as u64);
    let (mut ok, mut err) = (0usize, 0usize);
    let samples = fixtures().join("malformed").join(name);
    std::fs::create_dir_all(&samples).unwrap();
    for i in 0..PER_FORMAT {
        let input = mutate(&seed, &mut rng, tokens);
        if i % 100 == 0 {
            std::fs::write(samples.join(format!("{i:04}.bin")), &input).unwrap();
        }
        let started = std::time::Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| check(&input)));
        assert!(result.is_ok(), "{name} input {i} panicked");
        assert!(
            started.elapsed().as_secs() < 30,
            "{name} input {i} too slow"
        );
        if result.unwrap() { ok += 1 } else { err += 1 }
    }
    serde_json::json!({"inputs": PER_FORMAT, "accepted": ok, "rejected": err, "panics": 0})
}

#[test]
fn malformed_projects_ifc_and_dxf_never_panic() {
    let project = std::fs::read(fixtures().join("steel_release/source.obim")).unwrap();
    let ifc = std::fs::read(fixtures().join("ifc/connections.ifc")).unwrap();
    let dxf = std::fs::read(fixtures().join("exports/beam-a3.dxf")).unwrap();
    // Legacy v0 envelope carries no checksum, so mutations reach model
    // deserialisation and validation instead of stopping at the digest.
    let value: serde_json::Value = serde_json::from_slice(&project).unwrap();
    let legacy = serde_json::to_vec(&serde_json::json!({
        "magic": "open-bim-project", "format_version": 0, "project": value["project"],
    }))
    .unwrap();
    let summary = serde_json::json!({
        "project_v0_validation": corpus("project-v0", legacy, &[b"{", b"}", b"-1", b"1e308", b"\"elements\":{}", b"null", b"[[[[", b"\"id\":0"], |b| {
            open_bim_persistence::decode(b).is_ok()
        }),
        "project": corpus("project", project, &[b"{", b"}", b"\"elements\":", b"null", b"\"\\uD800\"", b"[]", b"18446744073709551616"], |b| {
            open_bim_persistence::decode(b).is_ok()
        }),
        "ifc": corpus("ifc", ifc, &[b"#1=", b"IFCBEAM(", b"\\X2\\D800\\X0\\", b"'", b"$", b"(((", b";", b"#999999999999999999999"], |b| {
            String::from_utf8(b.to_vec()).ok().is_some_and(|t| open_bim_ifc::import(&t).is_ok())
        }),
        "dxf": corpus("dxf", dxf, &[b"  0\nSECTION\n", b"999999\n", b"\\U+D800", b"  10\nnan\n", b"  0\nEOF\n"], |b| {
            String::from_utf8(b.to_vec()).ok().is_some_and(|t| open_bim_drawing_export::dxf::read(&t).is_ok())
        }),
    });
    std::fs::write(
        fixtures().join("malformed/summary.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .unwrap();
}

fn leaves(v: &serde_json::Value, path: Vec<String>, out: &mut Vec<Vec<String>>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                let mut p = path.clone();
                p.push(k.clone());
                out.push(p.clone());
                leaves(x, p, out);
            }
        }
        serde_json::Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                let mut p = path.clone();
                p.push(i.to_string());
                out.push(p.clone());
                leaves(x, p, out);
            }
        }
        _ => {}
    }
}

fn at<'a>(v: &'a mut serde_json::Value, path: &[String]) -> Option<&'a mut serde_json::Value> {
    path.iter().try_fold(v, |v, k| match v {
        serde_json::Value::Object(m) => m.get_mut(k),
        serde_json::Value::Array(a) => k.parse::<usize>().ok().and_then(|i| a.get_mut(i)),
        _ => None,
    })
}

/// Structured mutations of the legacy (checksum-free) project: every input
/// is valid JSON, so model deserialisation and validation are exercised.
#[test]
fn structurally_mutated_projects_are_rejected_without_panic() {
    let bytes = std::fs::read(fixtures().join("steel_release/source.obim")).unwrap();
    let mut base: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    base["format_version"] = 0.into();
    base.as_object_mut().unwrap().remove("payload_sha256");
    let mut paths = Vec::new();
    leaves(&base, Vec::new(), &mut paths);
    let mut rng = Rng(42);
    let extremes = [
        serde_json::json!(-1.0),
        serde_json::json!(0.0),
        serde_json::json!(1e300),
        serde_json::json!(-1e300),
        serde_json::json!(1e-300),
        serde_json::json!(18446744073709551615u64),
        serde_json::json!(""),
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::json!(7),
    ];
    let (mut ok, mut err) = (0, 0);
    for i in 0..PER_FORMAT {
        let mut v = base.clone();
        for _ in 0..1 + rng.below(3) {
            let path = &paths[rng.below(paths.len())];
            if rng.below(4) == 0 {
                // Delete the key or element.
                if let Some((last, parent)) = path.split_last()
                    && let Some(p) = at(&mut v, parent)
                {
                    match p {
                        serde_json::Value::Object(m) => {
                            m.remove(last);
                        }
                        serde_json::Value::Array(a) => {
                            if let Ok(i) = last.parse::<usize>()
                                && i < a.len()
                            {
                                a.remove(i);
                            }
                        }
                        _ => {}
                    }
                }
            } else if let Some(slot) = at(&mut v, path) {
                *slot = extremes[rng.below(extremes.len())].clone();
            }
        }
        let input = serde_json::to_vec(&v).unwrap();
        // The first accepted projects also go through geometry (net masses)
        // and lists; drilled-body meshing is slow in debug builds.
        let geometry = ok < 3;
        let result = catch_unwind(AssertUnwindSafe(|| {
            match open_bim_persistence::decode(&input) {
                Ok(project) => {
                    if geometry {
                        let _ = open_bim_reports::generate(&project);
                    }
                    true
                }
                Err(_) => false,
            }
        }));
        assert!(result.is_ok(), "structured input {i} panicked");
        if result.unwrap() { ok += 1 } else { err += 1 }
    }
    assert!(
        err > PER_FORMAT / 2,
        "mutations should mostly be rejected: {ok}/{err}"
    );
    std::fs::write(
        fixtures().join("malformed/structured-summary.json"),
        serde_json::to_string_pretty(&serde_json::json!({"inputs": PER_FORMAT, "accepted": ok, "rejected": err, "panics": 0})).unwrap(),
    )
    .unwrap();
}

#[test]
fn dxf_reader_reads_back_exports() {
    let text = std::fs::read_to_string(fixtures().join("exports/beam-a3.dxf")).unwrap();
    let s = open_bim_drawing_export::dxf::read(&text).unwrap();
    assert_eq!(s.insunits, Some(4));
    assert!(s.layers.iter().any(|l| l == "OB_VISIBLE"));
    assert!(s.texts.iter().any(|t| t.contains("Вид спереди")));
    assert!(s.entities.values().sum::<usize>() > 10);
}
