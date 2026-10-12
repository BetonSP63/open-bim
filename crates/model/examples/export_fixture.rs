fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Specify destination JSON path")?;
    let project = open_bim_model::example_steel_frame()?;
    std::fs::write(path, project.to_json()?)?;
    Ok(())
}
