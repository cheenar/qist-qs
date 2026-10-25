use serde::{Deserialize, Serialize};
use std::{env, fs, io, path::PathBuf};

pub const EXAMPLE: &str = include_str!("../config.example.toml");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub default_output: String,
    pub color: String,
    pub icons: bool,
    pub human_readable: bool,
    pub group_directories_first: bool,
    pub tree_depth: Option<usize>,
    pub colors: Colors,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub directory: String,
    pub symlink: String,
    pub executable: String,
    pub file: String,
    pub broken_symlink: String,
    pub header: String,
    pub muted: String,
    pub size: String,
    pub date: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            directory: "1;34".into(),
            symlink: "36".into(),
            executable: "32".into(),
            file: String::new(),
            broken_symlink: "1;31".into(),
            header: "1".into(),
            muted: "2".into(),
            size: "33".into(),
            date: "2".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_output: "auto".into(),
            color: "auto".into(),
            icons: false,
            human_readable: false,
            group_directories_first: false,
            tree_depth: None,
            colors: Colors::default(),
        }
    }
}

pub fn default_path() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|p| p.join("qist/config.toml"))
}

pub fn load(path: Option<PathBuf>, explicit: bool) -> Result<Config, String> {
    let Some(path) = path else {
        return Ok(Config::default());
    };
    match fs::read_to_string(&path) {
        Ok(text) => {
            let config: Config =
                toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            for code in [
                &config.colors.directory,
                &config.colors.symlink,
                &config.colors.executable,
                &config.colors.file,
                &config.colors.broken_symlink,
                &config.colors.header,
                &config.colors.muted,
                &config.colors.size,
                &config.colors.date,
            ] {
                if !code.bytes().all(|b| b.is_ascii_digit() || b == b';') {
                    return Err(format!(
                        "{}: colors must be ANSI SGR numbers (for example '1;34')",
                        path.display()
                    ));
                }
            }
            Ok(config)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound && !explicit => Ok(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn init(path: PathBuf) -> io::Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    use io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(EXAMPLE.as_bytes())?;
    Ok(path)
}
