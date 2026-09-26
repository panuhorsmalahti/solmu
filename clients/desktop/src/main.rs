fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = dotenvy::dotenv() { if !error.not_found() { return Err("Could not load .env".into()); } }
    solmu_desktop::application(solmu_client::Api::from_env()).run()?;
    Ok(())
}
