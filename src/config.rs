use std::fs::File;
use std::io::Read;
use toml;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub sleep: u64,
    pub job: Vec<JenkinsConfig>,
}

#[derive(Deserialize, Debug)]
pub struct JenkinsConfig {
    pub server: String,
    pub id: String,
    pub user: String,
    pub token: String,
    pub notify: Vec<String>,
}

impl Config {
    pub fn from_file(file_name: &str) -> Result<Config, String> {
        let mut contents = String::new();
        File::open(file_name)
            .and_then(|mut file| file.read_to_string(&mut contents))
            .map_err(|err| err.to_string())?;
        Config::from_string(&contents)
    }

    pub fn from_string(contents: &str) -> Result<Config, String> {
        toml::from_str(contents).map_err(|err| err.to_string())
    }
}
