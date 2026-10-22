//! Sample plugin «Ряд колонн»: columns evenly spaced between two picked
//! points. Shows the plugin protocol: the request comes as JSON on stdin,
//! the answer — macro steps — goes to stdout.
use serde_json::{Value, json};
use std::io::Read;

fn point(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    let p = [
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2)?.as_f64()?,
    ];
    Some(p)
}

fn answer(request: &Value) -> Result<Value, String> {
    let inputs = &request["inputs"];
    let attributes = &request["attributes"];
    let start = point(&inputs["start"]).ok_or("нет первой точки")?;
    let end = point(&inputs["end"]).ok_or("нет второй точки")?;
    let count = attributes["count"].as_f64().ok_or("нет количества")?;
    if count.fract() != 0.0 || !(2.0..=200.0).contains(&count) {
        return Err("Количество колонн: целое от 2 до 200".into());
    }
    let height = attributes["height"].as_f64().ok_or("нет высоты")?;
    if height <= 0.0 {
        return Err("Высота колонны должна быть положительной".into());
    }
    let n = count as usize;
    let steps: Vec<Value> = (0..n)
        .map(|i| {
            let t = i as f64 / (n - 1) as f64;
            let base: Vec<f64> = (0..3).map(|k| start[k] + t * (end[k] - start[k])).collect();
            let top = vec![base[0], base[1], base[2] + height];
            json!({ "insert_member": {
                "kind": "column",
                "name": "Колонна",
                "profile": attributes["profile"],
                "material": attributes["material"],
                "start": base,
                "end": top
            } })
        })
        .collect();
    Ok(json!({ "steps": steps }))
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
