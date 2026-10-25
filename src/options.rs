use crate::config::Config;
use std::{env, ffi::OsString, io::IsTerminal, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compat {
    Bsd,
    Gnu,
}
impl Compat {
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Bsd
        } else {
            Self::Gnu
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Auto,
    Single,
    Columns,
    Across,
    Commas,
    Long,
    Json,
    Table,
    Tree,
}
impl Format {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(Self::Auto),
            "single" | "single-column" => Ok(Self::Single),
            "columns" | "vertical" => Ok(Self::Columns),
            "across" | "horizontal" => Ok(Self::Across),
            "commas" => Ok(Self::Commas),
            "long" | "verbose" => Ok(Self::Long),
            "json" => Ok(Self::Json),
            "table" => Ok(Self::Table),
            "tree" => Ok(Self::Tree),
            _ => Err(format!("invalid output format '{s}'")),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Auto,
    Always,
    Never,
}
impl When {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "auto" | "tty" | "if-tty" => Ok(Self::Auto),
            "always" | "yes" | "force" => Ok(Self::Always),
            "never" | "no" | "none" => Ok(Self::Never),
            _ => Err(format!("invalid color/indicator/hyperlink setting '{s}'")),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Name,
    None,
    Size,
    Time,
    Extension,
    Version,
    Width,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Time {
    Modified,
    Accessed,
    Changed,
    Birth,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follow {
    Default,
    Never,
    Arguments,
    ArgumentDirs,
    All,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indicator {
    None,
    Slash,
    FileType,
    Classify,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quote {
    Literal,
    Question,
    Escape,
    EscapeBsd,
    Octal,
    C,
    Shell,
    ShellAlways,
    ShellEscape,
    ShellEscapeAlways,
    Locale,
    CLocale,
    EscapeMaybe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    List,
    Help,
    Version,
    InitConfig,
    PrintConfig,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub compat: Compat,
    pub format: Format,
    pub long: bool,
    pub all: bool,
    pub almost_all: bool,
    pub directory: bool,
    pub recursive: bool,
    pub sort: Sort,
    pub reverse: bool,
    pub same_sort: bool,
    pub time: Time,
    pub time_style: String,
    pub follow: Follow,
    pub indicator: Indicator,
    pub color: When,
    pub color_explicit: bool,
    pub hyperlink: When,
    pub quote: Quote,
    pub inode: bool,
    pub blocks: bool,
    pub block_size: u64,
    pub size_divisor: Option<u64>,
    pub human: bool,
    pub si: bool,
    pub numeric: bool,
    pub owner: bool,
    pub group: bool,
    pub author: bool,
    pub context: bool,
    pub flags: bool,
    pub xattrs: bool,
    pub acl: bool,
    pub dataless: bool,
    pub whiteouts: bool,
    pub one_fs: bool,
    pub thousands: bool,
    pub group_dirs: bool,
    pub ignore: Vec<OsString>,
    pub hide: Vec<OsString>,
    pub ignore_backups: bool,
    pub width: usize,
    pub tabsize: usize,
    pub zero: bool,
    pub dired: bool,
    pub depth: Option<usize>,
    pub icons: bool,
    pub paths: Vec<PathBuf>,
    pub action: Action,
    pub tty: bool,
    pub config_path: Option<PathBuf>,
    pub config_explicit: bool,
    pub config_disabled: bool,
    long_from_cli: bool,
    pub column_minima: [usize; 8],
}

fn integer(s: &str, flag: &str) -> Result<usize, String> {
    s.parse()
        .map_err(|_| format!("invalid nonnegative integer '{s}' for {flag}"))
}

const LONG_OPTIONS: &[&str] = &[
    "all",
    "almost-all",
    "author",
    "block-size",
    "classify",
    "color",
    "context",
    "dereference",
    "dereference-command-line",
    "dereference-command-line-symlink-to-dir",
    "directory",
    "dired",
    "escape",
    "file-type",
    "format",
    "full-time",
    "group-directories-first",
    "help",
    "hide",
    "hide-control-chars",
    "human-readable",
    "hyperlink",
    "ignore",
    "ignore-backups",
    "indicator-style",
    "inode",
    "kibibytes",
    "literal",
    "no-group",
    "numeric-uid-gid",
    "quote-name",
    "quoting-style",
    "recursive",
    "reverse",
    "show-control-chars",
    "si",
    "size",
    "sort",
    "tabsize",
    "time",
    "time-style",
    "version",
    "width",
    "zero",
    "config",
    "no-config",
    "init-config",
    "print-config",
    "compat",
    "output",
    "json",
    "table",
    "tree",
    "depth",
    "icons",
    "no-icons",
    "one-file-system",
];

fn long_name(key: &str) -> Result<&'static str, String> {
    if let Some(name) = LONG_OPTIONS.iter().find(|name| **name == key) {
        return Ok(name);
    }
    let mut matches = LONG_OPTIONS.iter().filter(|name| name.starts_with(key));
    match (matches.next(), matches.next()) {
        (Some(name), None) => Ok(name),
        (Some(_), Some(_)) => Err(format!("option '--{key}' is ambiguous")),
        _ => Err(format!("unrecognized option '--{key}'")),
    }
}

pub fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let number = if split == 0 {
        1
    } else {
        s[..split]
            .parse::<u64>()
            .map_err(|_| format!("invalid block size '{s}'"))?
    };
    let suffix = &s[split..];
    let power = suffix
        .chars()
        .next()
        .and_then(|c| "KMGTPEZYRQ".find(c.to_ascii_uppercase()));
    let multiplier = if suffix.is_empty() || suffix == "B" {
        1
    } else if let Some(power) = power {
        let base: u64 = if suffix.ends_with('B') && !suffix.ends_with("iB") {
            1000
        } else {
            1024
        };
        let valid = suffix.len() == 1
            || suffix.len() == 2 && suffix.ends_with('B')
            || suffix.len() == 3 && suffix.ends_with("iB");
        if !valid {
            return Err(format!("invalid block size '{s}'"));
        }
        base.checked_pow(power as u32 + 1)
            .ok_or_else(|| format!("block size '{s}' is too large"))?
    } else {
        return Err(format!("invalid block size '{s}'"));
    };
    number
        .checked_mul(multiplier)
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("invalid block size '{s}'"))
}

impl Options {
    pub fn parse(args: Vec<OsString>, config: &Config) -> Result<Self, String> {
        Self::parse_inner(args, config, true)
    }
    pub fn preview(args: Vec<OsString>, config: &Config) -> Result<Self, String> {
        Self::parse_inner(args, config, false)
    }
    fn parse_inner(
        args: Vec<OsString>,
        config: &Config,
        read_environment: bool,
    ) -> Result<Self, String> {
        let tty = std::io::stdout().is_terminal();
        let mut compat = Compat::host();
        for (i, arg) in args.iter().enumerate() {
            if arg == "--" {
                break;
            }
            let value = arg
                .to_str()
                .and_then(|s| s.strip_prefix("--"))
                .and_then(|s| s.split_once('='))
                .and_then(|(k, v)| (long_name(k).ok() == Some("compat")).then_some(v))
                .or_else(|| {
                    if arg
                        .to_str()
                        .and_then(|s| s.strip_prefix("--"))
                        .and_then(|s| long_name(s).ok())
                        == Some("compat")
                    {
                        args.get(i + 1).and_then(|s| s.to_str())
                    } else {
                        None
                    }
                });
            if let Some(v) = value {
                compat = match v {
                    "bsd" | "macos" => Compat::Bsd,
                    "gnu" | "linux" => Compat::Gnu,
                    _ => return Err(format!("invalid compatibility '{v}' (use bsd or gnu)")),
                };
            }
        }
        let block_keys: &[&str] = if compat == Compat::Bsd {
            &["BLOCKSIZE"]
        } else {
            &["LS_BLOCK_SIZE", "BLOCK_SIZE", "BLOCKSIZE"]
        };
        let block_env = block_keys.iter().find_map(|key| env::var(key).ok());
        let default_block = if compat == Compat::Bsd || env::var_os("POSIXLY_CORRECT").is_some() {
            512
        } else {
            1024
        };
        let mut block_size = block_env
            .as_deref()
            .and_then(|s| parse_size(s).ok())
            .unwrap_or(default_block);
        #[cfg(target_os = "macos")]
        if compat == Compat::Bsd && read_environment {
            block_size = crate::platform::block_size();
        }
        #[cfg(not(target_os = "macos"))]
        if compat == Compat::Bsd {
            block_size = block_size.clamp(512, 1 << 30) / 512 * 512;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = read_environment;
        let format = Format::parse(&config.default_output)?;
        let mut o = Self {
            compat,
            format,
            long: matches!(format, Format::Long | Format::Table),
            all: false,
            almost_all: compat == Compat::Bsd && unsafe { libc::geteuid() } == 0,
            directory: false,
            recursive: false,
            sort: Sort::Name,
            reverse: false,
            same_sort: env::var_os("LS_SAMESORT").is_some(),
            time: Time::Modified,
            time_style: if compat == Compat::Bsd {
                "locale".into()
            } else {
                env::var("TIME_STYLE").unwrap_or_else(|_| "locale".into())
            },
            follow: Follow::Default,
            indicator: Indicator::None,
            color: When::parse(&config.color)?,
            color_explicit: false,
            hyperlink: When::Never,
            quote: if tty {
                if compat == Compat::Bsd {
                    Quote::Question
                } else {
                    Quote::ShellEscape
                }
            } else {
                Quote::Literal
            },
            inode: false,
            blocks: false,
            block_size,
            size_divisor: None,
            human: config.human_readable,
            si: false,
            numeric: false,
            owner: true,
            group: true,
            author: false,
            context: false,
            flags: false,
            xattrs: false,
            acl: false,
            dataless: false,
            whiteouts: false,
            one_fs: false,
            thousands: false,
            group_dirs: config.group_directories_first,
            ignore: vec![],
            hide: vec![],
            ignore_backups: false,
            width: terminal_width(),
            tabsize: env::var("TABSIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8),
            zero: false,
            dired: false,
            depth: config.tree_depth,
            icons: config.icons,
            paths: vec![],
            action: Action::List,
            tty,
            config_path: crate::config::default_path(),
            config_explicit: false,
            config_disabled: false,
            long_from_cli: false,
            column_minima: if compat == Compat::Bsd {
                column_minima()
            } else {
                [0; 8]
            },
        };
        if compat == Compat::Gnu
            && let Ok(s) = env::var("QUOTING_STYLE")
        {
            o.quote = quote_style(&s)?;
        }
        let mut i = 0;
        let mut end = false;
        let mut implicit_time_sort = false;
        let mut explicit_sort = false;
        let mut bsd_no_sort = false;
        let mut bsd_date_format = None;
        let mut quote_explicit = compat == Compat::Gnu && env::var_os("QUOTING_STYLE").is_some();
        let mut explicit_dot = false;
        while i < args.len() {
            let arg = &args[i];
            if end || arg == "-" || !arg.as_encoded_bytes().starts_with(b"-") {
                o.paths.push(PathBuf::from(arg));
                if compat == Compat::Bsd || env::var_os("POSIXLY_CORRECT").is_some() {
                    end = true;
                }
                i += 1;
                continue;
            }
            if arg == "--" {
                end = true;
                i += 1;
                continue;
            }
            let s = arg
                .to_str()
                .ok_or("option contains invalid UTF-8 (use -- before filenames)")?;
            if let Some(long) = s.strip_prefix("--") {
                let (key, inline) = long
                    .split_once('=')
                    .map_or((long, None), |(k, v)| (k, Some(v)));
                let key = long_name(key)?;
                if matches!(
                    key,
                    "escape"
                        | "literal"
                        | "quote-name"
                        | "hide-control-chars"
                        | "show-control-chars"
                        | "quoting-style"
                ) {
                    quote_explicit = true;
                }
                let mut value = || -> Result<String, String> {
                    if let Some(v) = inline {
                        return Ok(v.into());
                    }
                    i += 1;
                    args.get(i)
                        .and_then(|s| s.to_str())
                        .map(String::from)
                        .ok_or_else(|| format!("--{key} requires an argument"))
                };
                let takes_value = matches!(
                    key,
                    "compat"
                        | "config"
                        | "output"
                        | "format"
                        | "sort"
                        | "time"
                        | "time-style"
                        | "block-size"
                        | "width"
                        | "tabsize"
                        | "ignore"
                        | "hide"
                        | "quoting-style"
                        | "indicator-style"
                        | "depth"
                );
                let optional = matches!(key, "color" | "hyperlink" | "classify");
                if inline.is_some() && !takes_value && !optional {
                    return Err(format!("--{key} does not take an argument"));
                }
                match key {
                    "help" => o.action = Action::Help,
                    "version" => o.action = Action::Version,
                    "init-config" => o.action = Action::InitConfig,
                    "print-config" => o.action = Action::PrintConfig,
                    "compat" => {
                        value()?;
                    }
                    "config" => {
                        o.config_path = Some(PathBuf::from(value()?));
                        o.config_explicit = true;
                    }
                    "no-config" => o.config_disabled = true,
                    "json" => o.set_format(Format::Json),
                    "table" => o.set_format(Format::Table),
                    "tree" => o.set_format(Format::Tree),
                    "output" | "format" => o.set_format(Format::parse(&value()?)?),
                    "all" => {
                        o.all = true;
                        explicit_dot = true;
                    }
                    "almost-all" => {
                        o.almost_all = true;
                        explicit_dot = true;
                    }
                    "directory" => o.directory = true,
                    "recursive" => o.recursive = true,
                    "reverse" => o.reverse = true,
                    "sort" => {
                        o.sort = match value()?.as_str() {
                            "name" => Sort::Name,
                            "none" => Sort::None,
                            "size" => Sort::Size,
                            "time" => Sort::Time,
                            "extension" => Sort::Extension,
                            "version" => Sort::Version,
                            "width" => Sort::Width,
                            v => return Err(format!("invalid sort '{v}'")),
                        };
                        explicit_sort = true;
                    }
                    "time" => {
                        o.time = match value()?.as_str() {
                            "mtime" | "modification" => Time::Modified,
                            "atime" | "access" | "use" => Time::Accessed,
                            "ctime" | "status" => Time::Changed,
                            "birth" | "creation" => Time::Birth,
                            v => return Err(format!("invalid time '{v}'")),
                        }
                    }
                    "time-style" => o.time_style = value()?,
                    "full-time" => {
                        o.set_format(Format::Long);
                        o.time_style = "full-iso".into();
                    }
                    "color" => {
                        o.color = When::parse(inline.unwrap_or("always"))?;
                        o.color_explicit = true;
                    }
                    "hyperlink" => o.hyperlink = When::parse(inline.unwrap_or("always"))?,
                    "classify" => {
                        let w = When::parse(inline.unwrap_or("always"))?;
                        o.indicator = if w == When::Always || w == When::Auto && tty {
                            Indicator::Classify
                        } else {
                            Indicator::None
                        };
                    }
                    "file-type" => o.indicator = Indicator::FileType,
                    "indicator-style" => {
                        o.indicator = match value()?.as_str() {
                            "none" => Indicator::None,
                            "slash" => Indicator::Slash,
                            "file-type" => Indicator::FileType,
                            "classify" => Indicator::Classify,
                            v => return Err(format!("invalid indicator style '{v}'")),
                        }
                    }
                    "dereference" => o.follow = Follow::All,
                    "dereference-command-line" => o.follow = Follow::Arguments,
                    "dereference-command-line-symlink-to-dir" => o.follow = Follow::ArgumentDirs,
                    "human-readable" => {
                        o.human = true;
                        o.si = false;
                        o.size_divisor = None;
                    }
                    "si" => {
                        o.human = true;
                        o.si = true;
                        o.size_divisor = None;
                    }
                    "block-size" => {
                        let v = value()?;
                        o.human = matches!(v.as_str(), "human-readable" | "si");
                        o.si = v == "si";
                        if !o.human {
                            let n = parse_size(v.trim_start_matches('\''))?;
                            o.thousands = v.starts_with('\'');
                            o.block_size = n;
                            o.size_divisor = Some(n);
                        }
                    }
                    "kibibytes" => {
                        o.block_size = 1024;
                        if compat == Compat::Bsd {
                            o.human = false;
                        }
                    }
                    "inode" => o.inode = true,
                    "size" => o.blocks = true,
                    "numeric-uid-gid" => {
                        o.numeric = true;
                        o.set_format(Format::Long);
                    }
                    "no-group" => o.group = false,
                    "author" => o.author = true,
                    "context" => o.context = true,
                    "group-directories-first" => o.group_dirs = true,
                    "ignore" => o.ignore.push(value()?.into()),
                    "hide" => o.hide.push(value()?.into()),
                    "ignore-backups" => o.ignore_backups = true,
                    "escape" => o.quote = Quote::Escape,
                    "literal" => o.quote = Quote::Literal,
                    "quote-name" => o.quote = Quote::C,
                    "hide-control-chars" => o.quote = Quote::Question,
                    "show-control-chars" => o.quote = Quote::Literal,
                    "quoting-style" => o.quote = quote_style(&value()?)?,
                    "width" => o.width = integer(&value()?, "--width")?.min(1_000_000),
                    "tabsize" => o.tabsize = integer(&value()?, "--tabsize")?.min(1_000_000),
                    "zero" => {
                        quote_explicit = true;
                        o.zero = true;
                        o.set_format(Format::Single);
                        o.color = When::Never;
                        o.quote = Quote::Literal;
                    }
                    "dired" => o.dired = true,
                    "depth" => o.depth = Some(integer(&value()?, "--depth")?),
                    "icons" => o.icons = true,
                    "no-icons" => o.icons = false,
                    "one-file-system" => o.one_fs = true,
                    _ => return Err(format!("unrecognized option '--{key}'")),
                }
            } else {
                let mut chars = s[1..].char_indices().peekable();
                while let Some((offset, ch)) = chars.next() {
                    if matches!(ch, 'q' | 'b' | 'N' | 'Q')
                        || compat == Compat::Bsd && matches!(ch, 'B' | 'v' | 'w')
                    {
                        quote_explicit = true;
                    }
                    let mut value = || -> Result<String, String> {
                        if chars.peek().is_some() {
                            let v = s[offset + 2..].to_owned();
                            while chars.next().is_some() {}
                            return Ok(v);
                        }
                        i += 1;
                        args.get(i)
                            .and_then(|s| s.to_str())
                            .map(String::from)
                            .ok_or_else(|| format!("-{ch} requires an argument"))
                    };
                    match ch {
                        '1' if compat == Compat::Gnu && o.long_from_cli => {}
                        '1' => o.set_format(Format::Single),
                        'C' => o.set_format(Format::Columns),
                        'x' => o.set_format(Format::Across),
                        'm' => o.set_format(Format::Commas),
                        'l' => o.set_format(Format::Long),
                        'a' => {
                            o.all = true;
                            explicit_dot = true;
                        }
                        'A' => {
                            o.almost_all = true;
                            explicit_dot = true;
                        }
                        'd' => o.directory = true,
                        'R' => o.recursive = true,
                        'S' => {
                            o.sort = Sort::Size;
                            explicit_sort = true;
                        }
                        't' => {
                            o.sort = Sort::Time;
                            explicit_sort = true;
                        }
                        'r' => o.reverse = true,
                        'f' => {
                            bsd_no_sort = compat == Compat::Bsd;
                            o.sort = Sort::None;
                            o.all = true;
                            explicit_sort = true;
                        }
                        'u' => {
                            o.time = Time::Accessed;
                            implicit_time_sort = true;
                        }
                        'c' => {
                            o.time = Time::Changed;
                            implicit_time_sort = true;
                        }
                        'U' if compat == Compat::Bsd => o.time = Time::Birth,
                        'U' => {
                            o.sort = Sort::None;
                            explicit_sort = true;
                        }
                        'v' if compat == Compat::Bsd => o.quote = Quote::Literal,
                        'v' => {
                            o.sort = Sort::Version;
                            explicit_sort = true;
                        }
                        'X' if compat == Compat::Bsd => o.one_fs = true,
                        'X' => {
                            o.sort = Sort::Extension;
                            explicit_sort = true;
                        }
                        'H' => o.follow = Follow::Arguments,
                        'L' => o.follow = Follow::All,
                        'P' => o.follow = Follow::Never,
                        'F' => o.indicator = Indicator::Classify,
                        'p' => o.indicator = Indicator::Slash,
                        'G' if compat == Compat::Bsd => {
                            o.color = When::Auto;
                            o.color_explicit = true;
                        }
                        'G' => o.group = false,
                        'g' if compat == Compat::Bsd => o.owner = false,
                        'g' => {
                            o.set_format(Format::Long);
                            o.owner = false;
                        }
                        'o' => {
                            o.set_format(Format::Long);
                            o.group = false;
                        }
                        'n' => {
                            o.numeric = true;
                            o.set_format(Format::Long);
                        }
                        'h' => {
                            o.human = true;
                            o.si = false;
                            o.size_divisor = None;
                        }
                        'k' => {
                            o.block_size = 1024;
                            if compat == Compat::Bsd {
                                o.human = false;
                            }
                        }
                        'i' => o.inode = true,
                        's' => o.blocks = true,
                        'q' => o.quote = Quote::Question,
                        'b' => {
                            o.quote = if compat == Compat::Bsd {
                                Quote::EscapeBsd
                            } else {
                                Quote::Escape
                            }
                        }
                        'B' if compat == Compat::Bsd => o.quote = Quote::Octal,
                        'B' => o.ignore_backups = true,
                        'N' => o.quote = Quote::Literal,
                        'Q' => o.quote = Quote::C,
                        'w' if compat == Compat::Bsd => o.quote = Quote::Literal,
                        'w' => o.width = integer(&value()?, "-w")?.min(1_000_000),
                        'T' if compat == Compat::Bsd => o.time_style = "bsd-full".into(),
                        'T' => o.tabsize = integer(&value()?, "-T")?.min(1_000_000),
                        'D' if compat == Compat::Bsd => {
                            bsd_date_format = Some(format!("+{}", value()?))
                        }
                        'D' => o.dired = true,
                        'I' if compat == Compat::Bsd => {
                            if !explicit_dot {
                                o.almost_all = false;
                            }
                        }
                        'I' => o.ignore.push(value()?.into()),
                        '@' => o.xattrs = true,
                        'e' => o.acl = true,
                        'O' => o.flags = true,
                        '%' => o.dataless = true,
                        'W' => o.whiteouts = true,
                        ',' => o.thousands = true,
                        'y' => o.same_sort = true,
                        'Z' => o.context = true,
                        _ => return Err(format!("invalid option -- '{ch}'")),
                    }
                }
            }
            i += 1;
        }
        if compat == Compat::Gnu && implicit_time_sort && !o.long && !explicit_sort {
            o.sort = Sort::Time;
        }
        if bsd_no_sort {
            o.sort = Sort::None;
        }
        if let Some(format) = bsd_date_format {
            o.time_style = format;
        }
        if !quote_explicit && matches!(o.format, Format::Table | Format::Tree) {
            o.quote = Quote::Escape;
        }
        if o.zero {
            if o.dired {
                return Err("--zero is incompatible with --dired".into());
            }
            o.color = When::Never;
            o.hyperlink = When::Never;
            o.quote = Quote::Literal;
        }
        if o.dired
            && !matches!(
                o.format,
                Format::Long | Format::Auto | Format::Single | Format::Columns
            )
        {
            return Err("--dired requires a conventional listing format".into());
        }
        if o.time_style.contains('\0') {
            return Err("time format contains NUL".into());
        }
        let style = o.time_style.strip_prefix("posix-").unwrap_or(&o.time_style);
        if !matches!(
            style,
            "locale" | "iso" | "long-iso" | "full-iso" | "bsd-full"
        ) && !style.starts_with('+')
        {
            return Err(format!("invalid time style '{}'", o.time_style));
        }
        if o.depth.is_some_and(|d| d > 4096) {
            return Err("--depth cannot exceed 4096".into());
        }
        if o.format == Format::Auto {
            o.format = if tty { Format::Columns } else { Format::Single };
        }
        if o.paths.is_empty() {
            o.paths.push(PathBuf::from("."));
        }
        Ok(o)
    }
    fn set_format(&mut self, f: Format) {
        self.format = f;
        self.long = matches!(f, Format::Long | Format::Table);
        self.long_from_cli = f == Format::Long;
    }
    pub fn needs_metadata(&self) -> bool {
        self.long
            || matches!(self.format, Format::Json)
            || self.inode
            || self.blocks
            || matches!(self.sort, Sort::Size | Sort::Time)
            || self.indicator == Indicator::Classify
            || self.follow == Follow::All
            || self.flags
            || self.xattrs
            || self.acl
            || self.dataless
            || self.context
            || self.one_fs
    }
}

fn quote_style(s: &str) -> Result<Quote, String> {
    match s {
        "literal" => Ok(Quote::Literal),
        "shell" => Ok(Quote::Shell),
        "shell-always" => Ok(Quote::ShellAlways),
        "shell-escape" => Ok(Quote::ShellEscape),
        "shell-escape-always" => Ok(Quote::ShellEscapeAlways),
        "c" => Ok(Quote::C),
        "c-maybe" => Ok(Quote::EscapeMaybe),
        "escape" => Ok(Quote::Escape),
        "locale" => Ok(Quote::Locale),
        "clocale" => Ok(Quote::CLocale),
        _ => Err(format!("invalid quoting style '{s}'")),
    }
}

fn column_minima() -> [usize; 8] {
    let mut result = [0; 8];
    if let Ok(value) = env::var("LS_COLWIDTHS") {
        for (index, word) in value.split(':').take(8).enumerate() {
            let Ok(width) = word.parse::<usize>() else {
                break;
            };
            if width > 1024 {
                break;
            }
            result[index] = width;
        }
    }
    result
}

fn terminal_width() -> usize {
    if let Some(n) = env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    {
        return n.min(1_000_000);
    }
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) } == 0
        && size.ws_col > 0
    {
        size.ws_col as usize
    } else {
        80
    }
}

pub const HELP: &str = r#"qs — qist (quick list)
Native directory listings for macOS and Linux.

Usage: qs [OPTIONS] [FILE ...]

Output
  --json                 Structured JSON (raw path/name bytes included)
  --table                Aligned metadata table with a terminal-aware palette
  --tree                 Recursive directory tree
  --output=MODE          auto, single, columns, across, commas, long, json, table, tree
  -l, -1, -C, -x, -m    Long, single-column, columns, across, comma-separated
  --depth=N              Limit recursion/tree depth (0 lists the root itself)
  --color[=WHEN]         auto, always, never; respects NO_COLOR and terminal colors
  --hyperlink[=WHEN]     OSC 8 file links; auto, always, never
  --icons / --no-icons   Optional Unicode file-type icons (no patched font needed)

Selection and traversal
  -a, -A                 All entries, or all except . and ..
  -d, -R                 List directory operands themselves; recurse
  -H, -L, -P             Follow argument links; all links; no links
  --ignore=GLOB          Exclude matching names (repeatable)
  --hide=GLOB            Hide names unless -a/-A is set
  --ignore-backups       Ignore names ending in ~
  --one-file-system      Stay on the starting filesystem during recursion

Sorting and metadata
  -S, -t, -r             Sort by size, time; reverse
  --sort=KEY             name, none, size, time, extension, version, width
  --group-directories-first
  -u, -c                 Access/change timestamp
  --time=KEY             mtime, atime, ctime, birth
  --time-style=STYLE     locale, iso, long-iso, full-iso, +strftime-format
  -h, --si               Human-readable sizes (1024 or 1000)
  -i, -s, -n, -o         Inode, blocks, numeric IDs, long without group
  -F, -p, --file-type    File classification suffixes
  --block-size=SIZE      Display sizes in units such as K, MiB, MB
  --quoting-style=STYLE  literal, shell, shell-always, shell-escape,
                        shell-escape-always, c, c-maybe, escape, locale, clocale
  --zero                 NUL-terminated output for scripts

Platform compatibility (host dialect by default)
  --compat=bsd|gnu       Resolve conflicting macOS/GNU short flags
  macOS: -@ -e -O -% -W -I -y -, -D FORMAT -T -G -U -v -w -X -B
  GNU:   -B -D -g -G -I GLOB -N -Q -T N -U -v -w N -X -Z
  GNU long options are accepted in either dialect. See docs/COMPATIBILITY.md.

Configuration
  --config=PATH         Override ~/.config/qist/config.toml
  --no-config           Use built-in defaults
  --init-config         Create a documented config (never overwrite an existing file)
  --print-config        Print the loaded configuration as TOML
  --help, --version
"#;
