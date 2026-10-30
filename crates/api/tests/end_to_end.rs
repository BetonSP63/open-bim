//! End-to-end run of the whole chain without the interface: a macro for the
//! `open-bim-batch` command builds or opens a model, makes drawings and
//! writes every output (PDF, DXF, CSV lists, IFC, NC). The checks read only
//! the files on disk, the way a fabricator or another program would, and
//! compare them with each other and with independently computed numbers.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/steel_release/source.obim"
);
const EXPECTED: &str = include_str!("../../../tests/fixtures/steel_release/expected.json");

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("open-bim-e2e-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run the batch command on a macro written to `dir`; returns the step report.
fn run_batch(dir: &Path, name: &str, script: serde_json::Value) -> serde_json::Value {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&script).unwrap()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_open-bim-batch"))
        .arg(&path)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{name}: {stderr}");
    assert!(stderr.is_empty(), "{name}: {stderr}");
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Every output of the chain into `out/`, then save.
fn export_steps(out: &str, save: &str) -> Vec<serde_json::Value> {
    serde_json::from_value(serde_json::json!([
        { "export_pdf": { "path": format!("{out}/комплект.pdf") } },
        { "export_dxf": { "dir": format!("{out}/dxf") } },
        { "export_reports": format!("{out}/ведомости") },
        { "export_ifc": format!("{out}/model.ifc") },
        { "export_nc": format!("{out}/nc") },
        { "save": save }
    ]))
    .unwrap()
}

fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut all = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                all.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    all
}

fn names(dir: &Path, ext: &str) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(ext))
        .collect()
}

/// Rows of a CSV list: BOM, `;`, CRLF, quoted fields, decimal comma.
fn csv(path: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(path).unwrap();
    let text = text.strip_prefix('\u{feff}').expect("UTF-8 BOM");
    assert!(text.ends_with("\r\n"), "{}: CRLF rows", path.display());
    let mut rows = Vec::new();
    let (mut row, mut cell, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ';' if !quoted => row.push(std::mem::take(&mut cell)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c => cell.push(c),
        }
    }
    rows
}

fn number(cell: &str) -> f64 {
    cell.replace(',', ".")
        .parse()
        .unwrap_or_else(|_| panic!("number: {cell}"))
}

/// Part list rows by position (without the header and the total), and the total.
struct PartList {
    rows: BTreeMap<String, Vec<String>>,
    quantity: usize,
    mass: f64,
}

fn part_list(path: &Path) -> PartList {
    let rows = csv(path);
    assert_eq!(rows[0][0], "Позиция");
    let col = |name: &str| rows[0].iter().position(|h| h == name).unwrap();
    let (qty, total) = (col("Кол-во"), col("Масса общ., кг"));
    let last = rows.last().unwrap();
    assert_eq!(last[0], "Итого");
    let body = &rows[1..rows.len() - 1];
    let summed: f64 = body.iter().map(|r| number(&r[total])).sum();
    let list = PartList {
        rows: body.iter().map(|r| (r[0].clone(), r.clone())).collect(),
        quantity: last[qty].parse().unwrap(),
        mass: number(&last[total]),
    };
    assert_eq!(list.rows.len(), body.len(), "positions are unique");
    assert_eq!(
        list.quantity,
        body.iter()
            .map(|r| r[qty].parse::<usize>().unwrap())
            .sum::<usize>()
    );
    // Rows are rounded to grams; their sum stays within a few grams.
    assert!((summed - list.mass).abs() < 0.001 * body.len() as f64);
    list
}

/// An `.nc1` file as a reader sees it: the 24 header lines and the blocks.
struct Nc {
    mark: String,
    quantity: usize,
    length: f64,
    blocks: BTreeSet<String>,
    holes: usize,
}

fn nc(text: &str) -> Nc {
    assert!(text.is_ascii(), "DSTV files are ASCII");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "ST");
    assert_eq!(*lines.last().unwrap(), "EN");
    let mut header = Vec::new();
    let mut i = 1;
    while header.len() < 24 {
        if !lines[i].starts_with("**") {
            header.push(lines[i].trim());
        }
        i += 1;
    }
    let blocks: BTreeSet<String> = lines[i..]
        .iter()
        .filter(|l| l.len() == 2 && l.chars().all(|c| c.is_ascii_uppercase()))
        .map(|l| l.to_string())
        .collect();
    let mut holes = 0;
    let mut in_bo = false;
    for line in &lines[i..] {
        if line.len() == 2 && !line.starts_with(' ') {
            in_bo = *line == "BO";
        } else if in_bo && !line.starts_with("**") {
            holes += 1;
        }
    }
    Nc {
        mark: header[3].to_string(),
        quantity: header[5].parse().unwrap(),
        length: header[8].parse().unwrap(),
        blocks,
        holes,
    }
}

/// Pages of a PDF: page objects, not the page tree.
fn pdf_pages(bytes: &[u8]) -> usize {
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.trim_ascii_end().ends_with(b"%%EOF"));
    let text = String::from_utf8_lossy(bytes);
    text.matches("/Type /Page")
        .count()
        .saturating_sub(text.matches("/Type /Pages").count())
}

fn ifc_entities(text: &str) -> BTreeMap<String, usize> {
    let mut count = BTreeMap::new();
    for line in text.lines() {
        if let Some((_, rest)) = line.split_once('=')
            && let Some((entity, _)) = rest.split_once('(')
            && line.starts_with('#')
        {
            *count.entry(entity.trim().to_string()).or_default() += 1;
        }
    }
    count
}

/// Checks shared by both models; returns the part list and NC files by mark.
fn check_outputs(out: &Path, parts: usize, drawings: usize) -> (PartList, BTreeMap<String, Nc>) {
    // Drawings: one PDF page and one DXF per drawing.
    let pdf = std::fs::read(out.join("комплект.pdf")).unwrap();
    assert_eq!(pdf_pages(&pdf), drawings, "pages in the PDF set");
    let dxf = names(&out.join("dxf"), ".dxf");
    assert_eq!(dxf.len(), drawings, "{dxf:?}");
    for name in &dxf {
        let text = std::fs::read_to_string(out.join("dxf").join(name)).unwrap();
        assert!(text.contains("ENTITIES"), "{name}");
        assert!(text.trim_end().ends_with("EOF"), "{name}");
    }

    // Lists.
    let list = part_list(&out.join("ведомости/parts.csv"));
    assert_eq!(list.quantity, parts, "parts in the part list");
    let assemblies = csv(&out.join("ведомости/assemblies.csv"));
    assert!(assemblies.len() >= 2, "assembly list has rows");
    let fasteners = csv(&out.join("ведомости/fasteners.csv"));
    assert_eq!(fasteners[0][0], "Тип");

    // IFC: one product per part, read back by the importer.
    let ifc = std::fs::read_to_string(out.join("model.ifc")).unwrap();
    assert!(ifc.starts_with("ISO-10303-21;"));
    assert!(ifc.contains("FILE_SCHEMA(('IFC4'))"));
    assert!(ifc.trim_end().ends_with("END-ISO-10303-21;"));
    let entities = ifc_entities(&ifc);
    let products: usize = ["IFCBEAM", "IFCCOLUMN", "IFCPLATE", "IFCMEMBER"]
        .iter()
        .map(|e| entities.get(*e).copied().unwrap_or(0))
        .sum();
    assert_eq!(products, parts, "{entities:?}");
    let (imported, report) = open_bim_ifc::import(&ifc).unwrap();
    assert_eq!(imported.elements.len(), parts, "{}", report.summary());

    // NC: one file per position, with the quantity and length of the list.
    let nc_dir = out.join("nc");
    let nc_files: BTreeMap<String, Nc> = names(&nc_dir, ".nc1")
        .into_iter()
        .map(|n| {
            let file = nc(&std::fs::read_to_string(nc_dir.join(&n)).unwrap());
            assert_eq!(n, format!("{}.nc1", file.mark));
            (file.mark.clone(), file)
        })
        .collect();
    assert_eq!(
        nc_files.keys().cloned().collect::<BTreeSet<_>>(),
        list.rows
            .keys()
            .map(|p| open_bim_nc::ascii(p))
            .collect::<BTreeSet<_>>(),
        "one NC file per position of the part list"
    );
    let header = csv(&out.join("ведомости/parts.csv")).remove(0);
    let col = |name: &str| header.iter().position(|h| h == name).unwrap();
    for (position, row) in &list.rows {
        let file = &nc_files[&open_bim_nc::ascii(position)];
        assert_eq!(file.quantity.to_string(), row[col("Кол-во")], "{position}");
        if !row[col("Сечение")].starts_with('—') {
            assert!(
                (file.length - number(&row[col("Длина, мм")])).abs() < 0.5,
                "{position}: NC {} vs list {}",
                file.length,
                row[col("Длина, мм")]
            );
        }
    }
    (list, nc_files)
}

#[test]
fn release_model_goes_from_file_to_shop_outputs() {
    let dir = temp("release");
    std::fs::copy(SOURCE, dir.join("model.obim")).unwrap();
    let expected: serde_json::Value = serde_json::from_str(EXPECTED).unwrap();
    let parts = expected["parts"].as_u64().unwrap() as usize;

    let mut steps: Vec<serde_json::Value> = serde_json::from_value(serde_json::json!([
        "number",
        { "create_drawings": {} },
        { "commit": "Чертежи" },
        { "issue_drawings": [] },
        { "commit": "Выпуск" }
    ]))
    .unwrap();
    steps.extend(export_steps("out", "saved.obim"));
    let report = run_batch(
        &dir,
        "release.json",
        serde_json::json!({ "open": "model.obim", "steps": steps }),
    );
    assert_eq!(report["staged"], 0);
    assert_eq!(report["parts"], parts);

    let saved = open_bim_persistence::load(dir.join("saved.obim")).unwrap();
    let drawings = saved.drawings.len();
    assert!(drawings > parts / 4, "wizard made the set: {drawings}");
    assert!(saved.drawings.values().all(|d| d.issued_revision.is_some()));
    let (list, nc_files) = check_outputs(&dir.join("out"), parts, drawings);

    // Numbers computed outside the program for this model.
    let mass = expected["total_mass_kg"].as_f64().unwrap();
    assert!((list.mass - mass).abs() < 0.0005, "{} vs {mass}", list.mass);
    let bolts: usize = csv(&dir.join("out/ведомости/fasteners.csv"))
        .iter()
        .skip(1)
        .filter(|r| r[0].starts_with("Болт"))
        .map(|r| r[2].parse::<usize>().unwrap())
        .sum();
    assert_eq!(bolts as u64, expected["bolts"].as_u64().unwrap());
    // Every hole of the model reaches a shop file.
    let holes: usize = nc_files.values().map(|f| f.holes * f.quantity).sum();
    let modelled = saved
        .details
        .values()
        .filter(|d| matches!(d.kind, open_bim_model::DetailKind::Hole { .. }))
        .count();
    assert!(holes > 0 && nc_files.values().any(|f| f.blocks.contains("BO")));
    assert_eq!(holes, modelled, "holes in NC files vs holes in the model");

    // Reopened from disk, the same model gives the same files.
    let again = run_batch(
        &dir,
        "reopen.json",
        serde_json::json!({ "open": "saved.obim", "steps": export_steps("again", "saved.obim") }),
    );
    assert_eq!(again["revision"], report["revision"]);
    let (first, second) = (files(&dir.join("out")), files(&dir.join("again")));
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        second.keys().collect::<Vec<_>>()
    );
    for (name, bytes) in &first {
        assert!(second[name] == *bytes, "{name} differs after reopening");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A small frame typed as a macro: two frames of two columns and a beam,
/// a base plate under each column.
fn frame_steps() -> Vec<serde_json::Value> {
    let column = |x: f64| {
        serde_json::json!({ "insert_member": {
            "kind": "column", "name": "Колонна", "profile": "30К1 (Р 57837)", "material": "С255",
            "start": [x, 0, 0], "end": [x, 0, 3600] } })
    };
    let base = |x: f64| {
        serde_json::json!({ "insert_plate": {
            "name": "Плита", "material": "С255", "thickness": 20,
            "outline": [[x - 200., -200., -20.], [x + 200., -200., -20.],
                        [x + 200., 200., -20.], [x - 200., 200., -20.]] } })
    };
    serde_json::from_value(serde_json::json!([
        column(0.),
        column(6000.),
        { "insert_member": {
            "name": "Балка", "profile": "30Б1 (Р 57837)", "material": "С255",
            "start": [0, 0, 3600], "end": [6000, 0, 3600] } },
        base(0.),
        base(6000.),
        { "select": {} },
        { "copy": { "vector": [0, 6000, 0] } },
        { "commit": "Рамы" },
        "number",
        { "create_drawings": {} },
        { "commit": "Чертежи" }
    ]))
    .unwrap()
}

#[test]
fn typed_frame_and_its_change_reach_every_output() {
    let dir = temp("frame");
    let mut steps = frame_steps();
    steps.extend(export_steps("v1", "frame.obim"));
    let report = run_batch(&dir, "frame.json", serde_json::json!({ "steps": steps }));
    assert_eq!(report["parts"], 10);
    let saved = open_bim_persistence::load(dir.join("frame.obim")).unwrap();
    let drawings = saved.drawings.len();
    let (list, nc_files) = check_outputs(&dir.join("v1"), 10, drawings);
    // Equal parts share a position: columns, beams, plates.
    assert_eq!(list.rows.len(), 3, "{:?}", list.rows.keys());
    assert_eq!(nc_files.len(), 3);
    let beam = list
        .rows
        .values()
        .find(|r| r[1] == "Балка")
        .unwrap()
        .clone();
    assert_eq!(beam[4], "2");

    // Lengthen one beam by 500 mm: drawings go out of date, the update
    // brings them back, and the lists and NC files follow the model.
    let session = open_bim_api::Session::open(dir.join("frame.obim")).unwrap();
    let beams = session
        .query(&open_bim_api::Query {
            name: Some("Балка".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(beams.len(), 2);
    let mut member = session.select_member(beams[1]).unwrap();
    member.end.0[0] += 500.;
    let mut steps = vec![serde_json::json!({ "modify_member": member })];
    steps.extend(
        serde_json::from_value::<Vec<serde_json::Value>>(serde_json::json!([
            "number",
            { "commit": "Балка длиннее" },
            { "update_drawings": [] },
            { "commit": "Обновление чертежей" }
        ]))
        .unwrap(),
    );
    steps.extend(export_steps("v2", "frame.obim"));
    run_batch(
        &dir,
        "change.json",
        serde_json::json!({ "open": "frame.obim", "steps": steps }),
    );
    let changed = open_bim_persistence::load(dir.join("frame.obim")).unwrap();
    assert!(changed.drawings.values().all(|d| {
        open_bim_drawings::status(&changed, d) == open_bim_drawings::Status::UpToDate
    }));
    let (list2, nc2) = check_outputs(&dir.join("v2"), 10, changed.drawings.len());
    assert!(list2.mass > list.mass);
    // Stored numbering: the changed beam gets the next free position, the
    // other one keeps its position; nothing else is renumbered.
    assert_eq!(list2.rows.len(), 4, "{:?}", list2.rows.keys());
    assert_eq!(list2.rows[&beam[0]][4], "1");
    assert_eq!(list2.rows[&beam[0]][5], beam[5]);
    let beam2 = list2
        .rows
        .values()
        .find(|r| !list.rows.contains_key(&r[0]))
        .unwrap();
    assert_eq!((beam2[1].as_str(), beam2[4].as_str()), ("Балка", "1"));
    assert_eq!(number(&beam2[5]), number(&beam[5]) + 500.);
    let (old, new) = (open_bim_nc::ascii(&beam[0]), open_bim_nc::ascii(&beam2[0]));
    assert_eq!(nc2[&old].quantity, 1);
    assert_eq!(nc2[&new].length, nc_files[&old].length + 500.);
    for (position, row) in &list.rows {
        if *position == beam[0] {
            continue;
        }
        assert_eq!(&list2.rows[position], row, "position {position} kept");
        let name = format!("{}.nc1", open_bim_nc::ascii(position));
        assert_eq!(
            std::fs::read(dir.join("v1/nc").join(&name)).unwrap(),
            std::fs::read(dir.join("v2/nc").join(&name)).unwrap(),
            "{name}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
