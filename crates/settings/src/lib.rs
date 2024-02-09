use directories::ProjectDirs;
use std::collections::HashMap;
use std::path::PathBuf;
use twelf::{config, Layer};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("config error: {0}")]
    ConfigError(#[from] twelf::Error),
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("env error: {0}")]
    EnvError(#[from] std::env::VarError),
}

// Need to name it explicitly to avoid conflict with std::Result
// which gets used by the `#[config]` macro below.
pub type SettingsResult<T> = std::result::Result<T, Error>;

#[config]
#[derive(Debug)]
pub struct Settings {
    /// The address to listen on
    pub server_hostname: String,
    /// API endpoint for the server
    pub api_endpoint: String,
    /// configured storage backends available on device
    pub profiles: HashMap<String, HashMap<String, String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            server_hostname: "http://127.0.0.1:80".to_string(),
            api_endpoint: "http://127.0.0.1:8080".to_string(),
            profiles: HashMap::new(),
        }
    }
}

impl Settings {
    pub fn load_from_env_and_file(config_path: Option<PathBuf>) -> SettingsResult<Self> {
        let config_file = match config_path {
            Some(path) => create_config_folder(&path)?,
            None => {
                if let Some(proj_dirs) = ProjectDirs::from("com", "aks", "terraphim") {
                    let config_dir = proj_dirs.config_dir();
                    create_config_folder(&config_dir.to_path_buf())?
                } else {
                    create_config_folder(&PathBuf::from(".config/"))?
                }
            }
        };
        println!("Reading config_file: {:?}", config_file);

        Ok(Settings::with_layers(&[
            Layer::Toml(config_file),
            Layer::Env(Some(String::from("TERRAPHIM_"))),
        ])?)
    }
}

fn create_config_folder(path: &PathBuf) -> Result<PathBuf, std::io::Error> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    let filename = path.join("settings.toml");
    // let default_config = include_str!("../default/settings.toml");
    if filename.exists() {
        log::warn!("File exists");
        log::warn!("{:?}", filename);
    } else {
        log::warn!("File does not exist");
        let default_config_path =
            std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        let default_config_path = format!("{}/default/settings.toml", default_config_path);
        println!("Default config path: {:?}", default_config_path);
        println!("Writing default config to: {:?}", filename);
        std::fs::copy(default_config_path, &filename)?;
    }
    Ok(filename)
}
/// To run test with logs and variables use:
/// RUST_LOG="info,warn" TERRAPHIM_API_ENDPOINT="test_endpoint" TERRAPHIM_PROFILE_S3_REGION="us-west-1" cargo test -- --nocapture
#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test]
    fn test_env_variable() {
        let env_vars = vec![
            ("TERRAPHIM_PROFILE_S3_REGION", "us-west-1"),
            ("TERRAPHIM_PROFILE_S3_ENABLE_VIRTUAL_HOST_STYLE", "on"),
        ];
        for (k, v) in &env_vars {
            std::env::set_var(k, v);
        }
        let config = Settings::load_from_env_and_file(None);
        println!("{:?}", config);

        assert_eq!(
            config
                .unwrap()
                .profiles
                .get("s3")
                .unwrap()
                .get("region")
                .unwrap(),
            &String::from("us-west-1")
        );
        for (k, _) in &env_vars {
            std::env::remove_var(k);
        }
    }
}
