//! Sample plugin «Оголовок колонны»: a square plate on the top of a picked
//! column. A dependent plugin: when the column moves or changes, the
//! program runs again with the same input and the plate follows.
use open_bim_model::{ObjectId, Project};
use serde_json::{Value, json};
use std::io::Read;

fn answer(request: &Value) -> Result<Value, String> {
    let attributes = &request["attributes"];
    let id = request["inputs"]["column"].as_u64().ok_or("нет колонны")?;
    let model: Project =
        serde_json::from_value(request["model"].clone()).map_err(|e| format!("модель: {e}"))?;
    let element = model
        .elements
        .get(&ObjectId(id))
        .ok_or(format!("нет детали {id}"))?;
    let (start, end, _) = model
        .member_reference_line(&element.shape)
        .ok_or("деталь — не колонна")?;
    let top = if end.0[2] >= start.0[2] { end } else { start };
    let size = attributes["size"].as_f64().ok_or("нет размера")?;
    let thickness = attributes["thickness"].as_f64().ok_or("нет толщины")?;
    if size <= 0.0 || thickness <= 0.0 {
        return Err("Размер и толщина должны быть положительными".into());
    }
    let h = size / 2.0;
    let [x, y, z] = top.0;
    // Counter-clockwise from above: the plate goes up from the column top.
    let outline = [
        [x - h, y - h, z],
        [x + h, y - h, z],
        [x + h, y + h, z],
        [x - h, y + h, z],
    ];
    Ok(json!({ "steps": [ { "insert_plate": {
        "name": "Оголовок",
        "material": attributes["material"],
        "thickness": thickness,
        "outline": outline
    } } ] }))
}

fn main() {
    let mut text = String::new();
    let reply = match std::io::stdin().read_to_string(&mut text) {
        Ok(_) => match serde_json::from_str::<Value>(&text) {
            Ok(request) => answer(&request).unwrap_or_else(|e| json!({ "error": e })),
            Err(e) => json!({ "error": format!("неверный запрос: {e}") }),
        },
        Err(e) => json!({ "error": e.to_string() }),
    };
    println!("{reply}");
}
