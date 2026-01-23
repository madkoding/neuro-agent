use neuro::config::AppConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Try to load the config.json file
    let config = AppConfig::from_file("config.json")?;
    println!("✅ Config loaded successfully!");
    println!("Fast model: {} via {}", config.fast_model.model, config.fast_model.provider);
    println!("Heavy model: {} via {}", config.heavy_model.model, config.heavy_model.provider);
    println!("Experimental features: {:?}", config.experimental);
    println!("Min Ollama version: {:?}", config.min_ollama_version);
    Ok(())
}