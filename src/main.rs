#![cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(unused))]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("qist currently supports macOS and Linux");

mod color;
mod config;
mod fs;
mod options;
mod platform;
mod quote;
mod render;
mod sort;

use crate::{
    color::Palette,
    fs::{Entry, Kind},
    options::{Action, Compat, Format, Options},
    render::Renderer,
};
use std::{
    collections::HashSet,
    ffi::OsString,
    io::{self, BufWriter, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
    process::ExitCode,
};

fn main() -> ExitCode {
    // Match Unix utilities when a downstream reader closes the pipe.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    // Locale is process-global; initialize once before any filesystem/render work.
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
    }
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("qs: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> io::Result<u8> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // Parse actions before loading config, while still respecting option values
    // (for example --ignore --help treats --help as a pattern).
    let preliminary = match Options::preview(args.clone(), &config::Config::default()) {
        Ok(options) => options,
        Err(error) => return cli_error(error),
    };
    if preliminary.action == Action::Help {
        print!("{}", options::HELP);
        return Ok(0);
    }
    if preliminary.action == Action::Version {
        println!("qs {} (qist — quick list)", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    let (config_path, explicit, disabled) = (
        preliminary.config_path,
        preliminary.config_explicit,
        preliminary.config_disabled,
    );
    if preliminary.action == Action::InitConfig {
        let Some(path) = config_path else {
            return cli_error("cannot locate config directory; set HOME or XDG_CONFIG_HOME".into());
        };
        match config::init(path) {
            Ok(path) => {
                println!("Created {}", path.display());
                return Ok(0);
            }
            Err(e) => return cli_error(e.to_string()),
        }
    }
    let config = if disabled {
        config::Config::default()
    } else {
        match config::load(config_path, explicit) {
            Ok(v) => v,
            Err(e) => return cli_error(e),
        }
    };
    let o = match Options::parse(args, &config) {
        Ok(v) => v,
        Err(e) => return cli_error(e),
    };
    if o.action == Action::PrintConfig {
        print!(
            "{}",
            toml::to_string_pretty(&config).map_err(io::Error::other)?
        );
        return Ok(0);
    }
    let palette = Palette::new(&o, config.colors);
    let out = BufWriter::with_capacity(64 * 1024, io::stdout().lock());
    let mut renderer = Renderer::new(out, &o, palette);
    let mut status = Status::default();
    let mut operands: Vec<_> = o
        .paths
        .iter()
        .map(|p| fs::operand(p, &o, renderer.palette.enabled))
        .collect();
    for e in &operands {
        if let Some(error) = &e.error {
            status.error(&e.path, error, if o.compat == Compat::Gnu { 2 } else { 1 });
        }
    }
    if o.format == Format::Json {
        json(&mut renderer, operands, &mut status)?;
    } else if o.format == Format::Tree && !o.directory {
        sort_entries(&mut operands, &o, &mut status);
        for (index, root) in operands.into_iter().enumerate() {
            if index > 0 {
                renderer.newline()?;
            }
            renderer.tree_root(&root)?;
            if root.error.is_none() && root.kind == Kind::Directory {
                tree(&mut renderer, root, &mut status)?;
            }
        }
    } else {
        let (mut dirs, mut files): (Vec<_>, Vec<_>) = operands
            .into_iter()
            .filter(|e| e.error.is_none())
            .partition(|e| e.kind == Kind::Directory && !o.directory);
        sort_entries(&mut files, &o, &mut status);
        sort_entries(&mut dirs, &o, &mut status);
        if !files.is_empty() {
            renderer.listing(&files, false)?;
        }
        let mut emitted = !files.is_empty();
        // A stack preserves pre-order ls -R traversal without consuming the call
        // stack. Ancestor IDs detect cycles while permitting independent aliases.
        let mut stack: Vec<_> = dirs
            .into_iter()
            .rev()
            .map(|entry| Task {
                root_device: entry.stat.as_ref().map(|s| s.device),
                entry,
                depth: 0,
                ancestors: HashSet::new(),
            })
            .collect();
        while let Some(mut task) = stack.pop() {
            if o.paths.len() > 1 || task.depth > 0 || o.recursive && o.compat == Compat::Gnu {
                renderer.header(&task.entry.path, emitted)?;
            }
            emitted = true;
            if o.dataless && task.entry.dataless() {
                continue;
            }
            if o.depth.is_some_and(|max| task.depth >= max) {
                continue;
            }
            if let Some(id) = task.entry.identity()
                && !task.ancestors.insert(id)
            {
                status.error(
                    &task.entry.path,
                    "not listing already-listed directory (symlink cycle)",
                    1,
                );
                continue;
            }
            match fs::read_dir(&task.entry.path, &o, renderer.palette.enabled) {
                Ok(mut entries) => {
                    sort_entries(&mut entries, &o, &mut status);
                    report_entries(&entries, &mut status);
                    if o.compat == Compat::Bsd {
                        entries.retain(|e| e.error.is_none());
                    }
                    if o.blocks && !o.long {
                        renderer.total(&entries)?;
                    }
                    renderer.listing(&entries, true)?;
                    if o.recursive {
                        for child in entries.into_iter().rev().filter(|e| descend(e, &task, &o)) {
                            stack.push(Task {
                                entry: child,
                                depth: task.depth + 1,
                                ancestors: task.ancestors.clone(),
                                root_device: task.root_device,
                            });
                        }
                    }
                }
                Err(error) => status.error(&task.entry.path, &error.to_string(), 1),
            }
        }
    }
    renderer.finish()?;
    Ok(status.code)
}

fn cli_error(message: String) -> io::Result<u8> {
    eprintln!("qs: {message}\nTry 'qs --help' for usage.");
    Ok(if Compat::host() == Compat::Bsd { 1 } else { 2 })
}

#[derive(Default)]
struct Status {
    code: u8,
    errors: Vec<serde_json::Value>,
}
impl Status {
    fn error(&mut self, path: &Path, message: &str, code: u8) {
        let escaped = quote::name(path.as_os_str(), options::Quote::Escape);
        eprintln!("qs: {}: {message}", String::from_utf8_lossy(&escaped));
        self.code = self.code.max(code);
        self.errors.push(serde_json::json!({"path":path.to_string_lossy(),"path_bytes":path.as_os_str().as_bytes(),"message":message}));
    }
}
struct Task {
    entry: Entry,
    depth: usize,
    ancestors: HashSet<(u64, u64)>,
    root_device: Option<u64>,
}
fn descend(e: &Entry, parent: &Task, o: &Options) -> bool {
    e.kind == Kind::Directory
        && e.name != "."
        && e.name != ".."
        && e.error.is_none()
        && (!o.one_fs || e.stat.as_ref().map(|s| s.device) == parent.root_device)
        && (!o.dataless || !e.dataless())
}
fn report_entries(entries: &[Entry], status: &mut Status) {
    for entry in entries {
        if let Some(error) = &entry.error {
            status.error(&entry.path, error, 1);
        }
    }
}
fn sort_entries(entries: &mut [Entry], o: &Options, status: &mut Status) {
    if sort::entries(entries, o) {
        let path = entries
            .first()
            .and_then(|e| e.path.parent())
            .unwrap_or(Path::new("."));
        status.error(
            path,
            "filename cannot be collated in the active locale (Illegal byte sequence)",
            1,
        );
    }
}

fn tree<W: Write>(r: &mut Renderer<'_, W>, root: Entry, status: &mut Status) -> io::Result<()> {
    let o = r.o;
    if o.depth == Some(0) || o.dataless && root.dataless() {
        return Ok(());
    }
    let root_device = root.stat.as_ref().map(|s| s.device);
    let mut ancestors = HashSet::new();
    if let Some(id) = root.identity() {
        ancestors.insert(id);
    }
    type TreeTask = (Entry, String, bool, usize, HashSet<(u64, u64)>);
    let mut stack: Vec<TreeTask> = vec![];
    let mut directories = 0;
    let mut files = 0;
    match fs::read_dir(&root.path, o, r.palette.enabled) {
        Ok(mut entries) => {
            sort_entries(&mut entries, o, status);
            report_entries(&entries, status);
            entries.retain(|e| e.name != "." && e.name != "..");
            let count = entries.len();
            for (index, entry) in entries.into_iter().enumerate().rev() {
                stack.push((
                    entry,
                    String::new(),
                    index + 1 == count,
                    1,
                    ancestors.clone(),
                ));
            }
        }
        Err(error) => {
            status.error(&root.path, &error.to_string(), 1);
            return Ok(());
        }
    }
    while let Some((entry, prefix, last, depth, mut ancestors)) = stack.pop() {
        if entry.kind == Kind::Directory {
            directories += 1;
        } else {
            files += 1;
        }
        let cycle = entry.identity().is_some_and(|id| ancestors.contains(&id));
        let crossing = o.one_fs && entry.stat.as_ref().map(|s| s.device) != root_device;
        let annotation = if cycle {
            Some("cycle")
        } else if o.dataless && entry.dataless() {
            Some("dataless")
        } else if crossing {
            Some("different filesystem")
        } else {
            None
        };
        r.tree_entry(&entry, &prefix, last, annotation)?;
        if cycle {
            status.error(&entry.path, "symlink cycle", 1);
            continue;
        }
        if entry.kind != Kind::Directory
            || entry.error.is_some()
            || crossing
            || o.dataless && entry.dataless()
            || o.depth.is_some_and(|max| depth >= max)
        {
            continue;
        }
        if let Some(id) = entry.identity() {
            ancestors.insert(id);
        }
        match fs::read_dir(&entry.path, o, r.palette.enabled) {
            Ok(mut entries) => {
                sort_entries(&mut entries, o, status);
                report_entries(&entries, status);
                entries.retain(|e| e.name != "." && e.name != "..");
                let count = entries.len();
                let next = format!("{prefix}{}", if last { "    " } else { "│   " });
                for (index, entry) in entries.into_iter().enumerate().rev() {
                    stack.push((
                        entry,
                        next.clone(),
                        index + 1 == count,
                        depth + 1,
                        ancestors.clone(),
                    ));
                }
            }
            Err(error) => status.error(&entry.path, &error.to_string(), 1),
        }
    }
    r.newline()?;
    r.emit(
        format!(
            "{directories} {}, {files} {}\n",
            if directories == 1 {
                "directory"
            } else {
                "directories"
            },
            if files == 1 { "file" } else { "files" }
        )
        .as_bytes(),
    )
}

fn json_value(entry: &Entry) -> serde_json::Value {
    serde_json::json!({
        "name":entry.name.to_string_lossy(),"name_bytes":entry.name.as_bytes(),
        "path":entry.path.to_string_lossy(),"path_bytes":entry.path.as_os_str().as_bytes(),
        "type":entry.kind,"inode":entry.inode,"permissions":fs::permissions(entry),
        "metadata":entry.stat,"symlink_target":entry.link.as_ref().map(|p|p.to_string_lossy()),
        "symlink_target_bytes":entry.link.as_ref().map(|p|p.as_os_str().as_bytes()),
        "broken_symlink":entry.broken,"error":entry.error,
        "extended_attributes":entry.xattrs.iter().map(|(name,size)|serde_json::json!({"name":name.to_string_lossy(),"name_bytes":name.as_bytes(),"size":size})).collect::<Vec<_>>(),
        "acl":entry.acl,"security_context":entry.context,
    })
}

enum JsonTask {
    Node(Box<Task>, bool),
    End,
    Comma,
}
fn json<W: Write>(
    r: &mut Renderer<'_, W>,
    mut roots: Vec<Entry>,
    status: &mut Status,
) -> io::Result<()> {
    sort_entries(&mut roots, r.o, status);
    r.emit(b"{\"schema_version\":1,\"roots\":[")?;
    let mut stack = vec![];
    for (index, entry) in roots.into_iter().enumerate().rev() {
        let device = entry.stat.as_ref().map(|s| s.device);
        stack.push(JsonTask::Node(
            Box::new(Task {
                entry,
                depth: 0,
                ancestors: HashSet::new(),
                root_device: device,
            }),
            index > 0,
        ));
    }
    while let Some(task) = stack.pop() {
        match task {
            JsonTask::End => r.emit(b"]}")?,
            JsonTask::Comma => r.emit(b",")?,
            JsonTask::Node(mut task, comma) => {
                if comma {
                    r.emit(b",")?;
                }
                let value = json_value(&task.entry);
                let bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
                r.emit(&bytes[..bytes.len() - 1])?;
                let dot = task.depth > 0 && (task.entry.name == "." || task.entry.name == "..");
                let cycle = !dot
                    && task.entry.kind == Kind::Directory
                    && task
                        .entry
                        .identity()
                        .is_some_and(|id| task.ancestors.contains(&id));
                let can_read = task.entry.kind == Kind::Directory
                    && !r.o.directory
                    && task.entry.error.is_none()
                    && (task.depth == 0 || r.o.recursive)
                    && !cycle
                    && !dot
                    && (!r.o.dataless || !task.entry.dataless())
                    && r.o.depth.is_none_or(|max| task.depth < max)
                    && (!r.o.one_fs
                        || task.entry.stat.as_ref().map(|s| s.device) == task.root_device);
                if cycle {
                    status.error(&task.entry.path, "symlink cycle", 1);
                    r.emit(b",\"cycle\":true")?;
                }
                if can_read {
                    if let Some(id) = task.entry.identity() {
                        task.ancestors.insert(id);
                    }
                    match fs::read_dir(&task.entry.path, r.o, false) {
                        Ok(mut entries) => {
                            sort_entries(&mut entries, r.o, status);
                            report_entries(&entries, status);
                            r.emit(b",\"entries\":[")?;
                            stack.push(JsonTask::End);
                            for (index, entry) in entries.into_iter().enumerate().rev() {
                                // . and .. are listed but never traversed, even with -aR.
                                let dot = entry.name == "." || entry.name == "..";
                                let mut ancestors = task.ancestors.clone();
                                if dot && let Some(id) = entry.identity() {
                                    ancestors.insert(id);
                                }
                                stack.push(JsonTask::Node(
                                    Box::new(Task {
                                        entry,
                                        depth: task.depth + 1,
                                        ancestors,
                                        root_device: task.root_device,
                                    }),
                                    false,
                                ));
                                if index > 0 {
                                    stack.push(JsonTask::Comma);
                                }
                            }
                        }
                        Err(error) => {
                            status.error(&task.entry.path, &error.to_string(), 1);
                            r.emit(b",\"entries\":[],\"read_error\":")?;
                            r.emit(
                                &serde_json::to_vec(&error.to_string())
                                    .map_err(io::Error::other)?,
                            )?;
                            r.emit(b"}")?;
                        }
                    }
                } else {
                    r.emit(b"}")?;
                }
            }
        }
    }
    r.emit(b"],\"errors\":")?;
    r.emit(&serde_json::to_vec(&status.errors).map_err(io::Error::other)?)?;
    r.emit(b"}\n")
}
