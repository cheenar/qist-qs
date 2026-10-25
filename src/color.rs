use crate::{
    config::Colors,
    fs::{Entry, Kind},
    options::{Compat, Options, When},
};
use std::{collections::HashMap, env, os::unix::ffi::OsStrExt};

pub struct Palette {
    pub enabled: bool,
    pub colors: Colors,
    rules: HashMap<Vec<u8>, String>,
    suffixes: Vec<(Vec<u8>, String)>,
    bsd: bool,
    bsd_rules: bool,
}
impl Palette {
    pub fn new(o: &Options, colors: Colors) -> Self {
        let enabled = match o.color {
            When::Never => false,
            When::Always => true,
            When::Auto => {
                env::var_os("NO_COLOR").is_none()
                    && env::var("TERM").is_ok_and(|s| s != "dumb")
                    && (o.tty || env::var_os("CLICOLOR_FORCE").is_some_and(|s| s != "0"))
            }
        };
        let mut result = Self {
            enabled,
            colors,
            rules: HashMap::new(),
            suffixes: vec![],
            bsd: o.compat == Compat::Bsd,
            bsd_rules: false,
        };
        let bsd_colors = env::var("LSCOLORS").ok();
        if let Some(value) =
            env::var_os("LS_COLORS").filter(|_| o.compat != Compat::Bsd || bsd_colors.is_none())
        {
            for rule in value.as_bytes().split(|b| *b == b':') {
                if let Some(index) = rule.iter().position(|b| *b == b'=') {
                    let key = &rule[..index];
                    let value = String::from_utf8_lossy(&rule[index + 1..]).to_string();
                    // Only SGR numeric parameters are accepted, never terminal commands.
                    if !value.bytes().all(|b| b.is_ascii_digit() || b == b';') {
                        continue;
                    }
                    if let Some(suffix) = key.strip_prefix(b"*") {
                        result.suffixes.push((suffix.to_vec(), value));
                    } else {
                        result.rules.insert(key.to_vec(), value);
                    }
                }
            }
        } else if let Some(value) = bsd_colors {
            result.bsd_rules = true;
            let defaults = b"exfxcxdxbxegedabagacadah";
            for (slot, key) in [
                "di", "ln", "so", "pi", "ex", "bd", "cd", "su", "sg", "tw", "ow", "dl",
            ]
            .iter()
            .enumerate()
            {
                let mut codes = vec![];
                for index in 0..2 {
                    let ch = value
                        .as_bytes()
                        .get(slot * 2 + index)
                        .copied()
                        .unwrap_or(defaults[slot * 2 + index]);
                    if ch.is_ascii_uppercase() {
                        codes.push("1".to_owned());
                    }
                    let lower = ch.to_ascii_lowercase();
                    if (b'a'..=b'h').contains(&lower) {
                        codes.push((if index == 0 { 30 } else { 40 } + lower - b'a').to_string());
                    }
                }
                result
                    .rules
                    .insert(key.as_bytes().to_vec(), codes.join(";"));
            }
        }
        result
    }
    pub fn wrap(&self, code: &str, raw: &[u8]) -> Vec<u8> {
        if !self.enabled || code.is_empty() {
            return raw.to_vec();
        }
        let mut result = format!("\x1b[{code}m").into_bytes();
        result.extend_from_slice(raw);
        result.extend_from_slice(b"\x1b[0m");
        result
    }
    pub fn file(&self, e: &Entry, raw: &[u8]) -> Vec<u8> {
        if !self.enabled {
            return raw.to_vec();
        }
        let (key, default) = match e.kind {
            _ if e.dataless() && self.bsd_rules => ("dl", &self.colors.file),
            Kind::Directory => {
                let mode = e.stat.as_ref().map_or(0, |s| s.mode);
                if mode & 0o1002 == 0o1002 {
                    ("tw", &self.colors.directory)
                } else if mode & 0o002 != 0 {
                    ("ow", &self.colors.directory)
                } else if mode & 0o1000 != 0 {
                    ("st", &self.colors.directory)
                } else {
                    ("di", &self.colors.directory)
                }
            }
            Kind::Symlink if e.broken && !self.bsd_rules => ("or", &self.colors.broken_symlink),
            Kind::Symlink => ("ln", &self.colors.symlink),
            Kind::Socket => ("so", &self.colors.symlink),
            Kind::Fifo => ("pi", &self.colors.size),
            Kind::BlockDevice => ("bd", &self.colors.size),
            Kind::CharDevice => ("cd", &self.colors.size),
            _ if (!self.bsd || e.executable())
                && e.stat.as_ref().is_some_and(|s| s.mode & 0o4000 != 0) =>
            {
                ("su", &self.colors.executable)
            }
            _ if (!self.bsd || e.executable())
                && e.stat.as_ref().is_some_and(|s| s.mode & 0o2000 != 0) =>
            {
                ("sg", &self.colors.executable)
            }
            _ if e.executable() => ("ex", &self.colors.executable),
            _ => ("fi", &self.colors.file),
        };
        if key == "fi" {
            for (suffix, color) in self.suffixes.iter().rev() {
                if e.name.as_bytes().ends_with(suffix) {
                    return self.wrap(color, raw);
                }
            }
        }
        self.wrap(
            self.rules
                .get(key.as_bytes())
                .map_or(default.as_str(), String::as_str),
            raw,
        )
    }
}
