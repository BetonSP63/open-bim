fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Specify destination .obim path")?;
    let project = open_bim_model::example_steel_frame()?;
    open_bim_persistence::save_atomic(path, &project)?;
    Ok(())
}
