#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStringExt;
use std::{
    ffi::OsString,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qs"));
    c.args(["--no-config"]);
    clean(&mut c);
    c
}
fn clean(c: &mut Command) {
    for key in [
        "LS_COLORS",
        "LSCOLORS",
        "CLICOLOR",
        "CLICOLOR_FORCE",
        "NO_COLOR",
        "POSIXLY_CORRECT",
        "TIME_STYLE",
        "QUOTING_STYLE",
        "BLOCKSIZE",
        "BLOCK_SIZE",
        "LS_BLOCK_SIZE",
        "LS_SAMESORT",
        "COLUMNS",
        "TABSIZE",
    ] {
        c.env_remove(key);
    }
    c.env("LC_ALL", "C")
        .env("TZ", "UTC")
        .env("TERM", "xterm-256color");
}
fn fixture() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    for (name, content) in [
        ("alpha.txt", "a"),
        ("beta.rs", "123456789"),
        ("file2", "xx"),
        ("file10", "xxx"),
        (".hidden", "secret"),
        ("backup~", "backup"),
    ] {
        fs::write(d.path().join(name), content).unwrap();
    }
    fs::create_dir(d.path().join("dir")).unwrap();
    fs::write(d.path().join("dir/nested"), "hello").unwrap();
    fs::set_permissions(d.path().join("file2"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink("alpha.txt", d.path().join("link")).unwrap();
    symlink("missing", d.path().join("broken")).unwrap();
    d
}
fn output(args: &[&str], dir: &Path) -> Output {
    command().args(args).arg(dir).output().unwrap()
}
fn stdout(out: Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn native_ls_name_selection_and_sorting_parity() {
    let d = fixture();
    let cases: &[&[&str]] = &[
        &["-1"],
        &["-1a"],
        &["-1A"],
        &["-1r"],
        &["-1S"],
        &["-1i"],
        &["-1p"],
        &["-1F"],
        &["-1R"],
        &["-1d"],
        &["-1L"],
        &["-1A", "--color=never"],
    ];
    for args in cases {
        let ours = output(args, d.path());
        let mut native = Command::new("/bin/ls");
        clean(&mut native);
        let theirs = native.args(*args).arg(d.path()).output().unwrap();
        assert_eq!(ours.status.code(), theirs.status.code(), "{args:?}");
        assert_eq!(
            ours.stdout,
            theirs.stdout,
            "{args:?}\nours: {}\ntheirs: {}",
            String::from_utf8_lossy(&ours.stdout),
            String::from_utf8_lossy(&theirs.stdout)
        );
    }
}

#[test]
fn native_ls_long_format_parity() {
    let d = fixture();
    for args in [&["-ln"][..], &["-lhn"], &["-lon"], &["-lsn"], &["-lnT"]] {
        #[cfg(target_os = "linux")]
        if args.contains(&"-lnT") {
            continue;
        }
        let ours = output(args, d.path());
        let mut native = Command::new("/bin/ls");
        clean(&mut native);
        let theirs = native.args(args).arg(d.path()).output().unwrap();
        assert_eq!(
            ours.stdout,
            theirs.stdout,
            "{args:?}\nours: {}\ntheirs: {}",
            String::from_utf8_lossy(&ours.stdout),
            String::from_utf8_lossy(&theirs.stdout)
        );
    }
}

#[test]
fn json_metadata_and_recursive_children() {
    let d = fixture();
    let raw = stdout(output(&["--json", "-R"], d.path()));
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["errors"].as_array().unwrap().len(), 0);
    let entries = doc["roots"][0]["entries"].as_array().unwrap();
    let alpha = entries.iter().find(|e| e["name"] == "alpha.txt").unwrap();
    assert_eq!(alpha["metadata"]["size"], 1);
    assert_eq!(alpha["type"], "file");
    let dir = entries.iter().find(|e| e["name"] == "dir").unwrap();
    assert_eq!(dir["entries"][0]["name"], "nested");
    let broken = entries.iter().find(|e| e["name"] == "broken").unwrap();
    assert_eq!(broken["broken_symlink"], true);
}

#[test]
fn json_all_does_not_follow_dot_entries() {
    let d = fixture();
    let raw = stdout(output(&["--json", "-aR"], d.path()));
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(doc["errors"].as_array().unwrap().is_empty());
    assert!(
        doc["roots"][0]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == ".")
    );
}

#[test]
#[cfg(target_os = "linux")] // APFS rejects invalid UTF-8 names with EILSEQ.
fn json_preserves_invalid_utf8_names() {
    let d = fixture();
    let bytes = b"bad-\xff".to_vec();
    fs::write(d.path().join(OsString::from_vec(bytes.clone())), b"x").unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(&stdout(output(&["--json"], d.path()))).unwrap();
    assert!(
        doc["roots"][0]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name_bytes"] == serde_json::json!(bytes))
    );
    let raw = output(&["-1"], d.path());
    assert!(raw.stdout.windows(5).any(|w| w == b"bad-\xff"));
}

#[test]
fn zero_separators_and_double_dash_operand() {
    let d = tempfile::tempdir().unwrap();
    for name in ["a\nb", "--json", "tab\tfile"] {
        fs::write(d.path().join(name), "").unwrap();
    }
    let out = output(&["--zero", "--color=always"], d.path());
    assert!(out.status.success());
    assert!(!out.stdout.contains(&0x1b));
    let names: Vec<_> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&b"a\nb".as_slice()));
    let out = command()
        .current_dir(d.path())
        .args(["--", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.stdout, b"--json\n");
}

#[test]
fn tree_depth_and_cycles() {
    let d = fixture();
    let tree = stdout(output(&["--tree", "--depth=1"], d.path()));
    assert!(tree.contains("├──"));
    assert!(tree.contains("dir"));
    assert!(!tree.contains("nested"));
    symlink("..", d.path().join("dir/back")).unwrap();
    let out = output(&["--tree", "-L"], d.path());
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("[cycle]"));
    let out = output(&["--json", "-RL"], d.path());
    assert!(!out.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!doc["errors"].as_array().unwrap().is_empty());
}

#[test]
fn glob_filters_natural_order_and_directory_groups() {
    let d = fixture();
    let text = stdout(output(
        &[
            "--compat=gnu",
            "-1v",
            "--ignore=*.txt",
            "--hide=*.rs",
            "--ignore-backups",
            "--group-directories-first",
        ],
        d.path(),
    ));
    assert!(text.starts_with("dir\n"));
    assert!(!text.contains("alpha.txt"));
    assert!(!text.contains("beta.rs"));
    assert!(!text.contains("backup~"));
    assert!(text.find("file2\n") < text.find("file10\n"));
}

#[test]
fn color_respects_no_color_and_explicit_override() {
    let d = fixture();
    let out = command()
        .env("NO_COLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .arg(d.path())
        .output()
        .unwrap();
    assert!(!out.stdout.contains(&0x1b));
    let out = command()
        .env("NO_COLOR", "1")
        .env("LS_COLORS", "di=35:*.rs=31")
        .args(["--color=always", "-1"])
        .arg(d.path())
        .output()
        .unwrap();
    assert!(out.stdout.windows(5).any(|w| w == b"\x1b[35m"));
    assert!(out.stdout.windows(5).any(|w| w == b"\x1b[31m"));
    let out = output(&["--json", "--color=always"], d.path());
    assert!(!out.stdout.contains(&0x1b));
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap();
}

#[test]
fn config_defaults_and_cli_precedence() {
    let d = fixture();
    let cfg = d.path().join("config.toml");
    fs::write(&cfg, "default_output='json'\ncolor='never'\n").unwrap();
    let out = command()
        .arg("--config")
        .arg(&cfg)
        .arg(d.path())
        .output()
        .unwrap();
    // --no-config deliberately overrides an explicit config.
    assert!(!out.stdout.starts_with(b"{"));
    let mut c = Command::new(env!("CARGO_BIN_EXE_qs"));
    clean(&mut c);
    let out = c.arg("--config").arg(&cfg).arg(d.path()).output().unwrap();
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_qs"));
    clean(&mut c);
    let out = c
        .arg("--config")
        .arg(&cfg)
        .arg("-1")
        .arg(d.path())
        .output()
        .unwrap();
    assert!(!out.stdout.starts_with(b"{"));
}

#[test]
fn config_creation_never_overwrites() {
    let d = tempfile::tempdir().unwrap();
    let cfg = d.path().join("qist/config.toml");
    let first = command()
        .arg("--config")
        .arg(&cfg)
        .arg("--init-config")
        .output()
        .unwrap();
    assert!(first.status.success());
    let bytes = fs::read(&cfg).unwrap();
    let second = command()
        .arg("--config")
        .arg(&cfg)
        .arg("--init-config")
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert_eq!(bytes, fs::read(&cfg).unwrap());
}

#[test]
fn table_and_missing_operand_json() {
    let d = fixture();
    let table = stdout(output(&["--table"], d.path()));
    assert!(table.contains("Permissions"));
    assert!(table.contains("Name"));
    assert!(table.contains("alpha.txt"));
    assert_eq!(
        table.matches("Permissions").count(),
        1,
        "directory table must have exactly one header"
    );
    let out = output(&["--json"], &d.path().join("missing"));
    assert!(!out.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!doc["errors"].as_array().unwrap().is_empty());
}

#[test]
fn dired_offsets_point_at_names() {
    let d = fixture();
    let raw = stdout(output(&["--compat=gnu", "-ln", "--dired"], d.path()));
    let offsets = raw
        .lines()
        .find(|line| line.starts_with("//DIRED//"))
        .unwrap()
        .split_whitespace()
        .skip(1)
        .map(|n| n.parse::<usize>().unwrap())
        .collect::<Vec<_>>();
    assert!(!offsets.is_empty());
    for pair in offsets.chunks_exact(2) {
        let name = &raw[pair[0]..pair[1]];
        assert!(
            d.path().join(name).symlink_metadata().is_ok(),
            "offset points to {name:?}"
        );
    }
}

#[test]
fn arguments_with_non_utf8_paths_and_stacked_flags() {
    let d = fixture();
    #[cfg(target_os = "linux")]
    let name = OsString::from_vec(b"dir-\xff".to_vec());
    #[cfg(target_os = "macos")]
    let name = OsString::from("dir-文件");
    let bad = d.path().join(name);
    fs::create_dir(&bad).unwrap();
    fs::write(bad.join("item"), "").unwrap();
    assert_eq!(output(&["-1"], &bad).stdout, b"item\n");
    #[cfg(target_os = "macos")]
    assert!(stdout(output(&["-l1"], d.path())).starts_with("alpha.txt\n"));
    #[cfg(target_os = "linux")]
    assert!(stdout(output(&["-l1"], d.path())).starts_with("total "));
    assert!(stdout(output(&["-1l"], d.path())).starts_with("total "));
}

#[test]
fn errors_and_help_are_actionable() {
    let out = command().arg("--not-an-option").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(if cfg!(target_os = "macos") { 1 } else { 2 })
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("--help"));
    let out = command()
        .args(["--config=/does/not/exist", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = command().args(["--zero", "--dired"]).output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn option_arguments_that_look_like_actions_stay_arguments() {
    let d = fixture();
    let out = output(&["--ignore", "--help"], d.path());
    assert!(out.status.success());
    assert!(!out.stdout.starts_with("qs —".as_bytes()));
    let out = output(&["--ignore", "--config"], d.path());
    assert!(out.status.success());
    let out = command()
        .args(["--invalid", "--init-config"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn native_filename_quoting_parity() {
    let d = tempfile::tempdir().unwrap();
    for name in [
        "a b",
        "a\nb",
        "a\tb",
        "back\\slash",
        "apost'rophe",
        "double\"quote",
        "unicode-文件",
    ] {
        fs::write(d.path().join(name), "").unwrap();
    }
    #[cfg(target_os = "macos")]
    let flags = ["-1b", "-1B", "-1q", "-1v", "-1w"];
    #[cfg(target_os = "linux")]
    let flags = ["-1b", "-1q", "-1N", "-1Q", "--quoting-style=shell-always"];
    for flag in flags {
        let ours = output(&[flag], d.path());
        let mut native = Command::new("/bin/ls");
        clean(&mut native);
        let theirs = native.arg(flag).arg(d.path()).output().unwrap();
        assert_eq!(
            ours.stdout,
            theirs.stdout,
            "{flag}\nours: {}\ntheirs: {}",
            String::from_utf8_lossy(&ours.stdout),
            String::from_utf8_lossy(&theirs.stdout)
        );
    }
}

#[test]
fn machine_output_handles_multiple_roots_empty_directories_and_depth_zero() {
    let d = fixture();
    let empty = tempfile::tempdir().unwrap();
    let out = command()
        .arg("--json")
        .arg(d.path())
        .arg(empty.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["roots"].as_array().unwrap().len(), 2);
    let empty_node = doc["roots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == empty.path().to_string_lossy().as_ref())
        .unwrap();
    assert!(empty_node["entries"].as_array().unwrap().is_empty());
    let doc: serde_json::Value =
        serde_json::from_str(&stdout(output(&["--json", "--depth=0"], d.path()))).unwrap();
    assert!(doc["roots"][0].get("entries").is_none());
}

#[test]
fn new_human_modes_escape_control_characters() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("a\nb\x1b[31m"), "").unwrap();
    for mode in ["--tree", "--table"] {
        let text = stdout(output(&[mode], d.path()));
        assert!(text.contains("a\\nb\\033[31m"));
        assert!(!text.contains('\x1b'));
    }
}

#[test]
fn bsd_date_and_sort_option_precedence_matches_native() {
    #[cfg(target_os = "macos")]
    {
        let d = fixture();
        for args in [
            &["-1u"][..],
            &["-1c"],
            &["-1fS"],
            &["-ln", "-D", "%Y", "-T"],
            &["-lnF"],
            &["-1s"],
        ] {
            let ours = output(args, d.path());
            let mut native = Command::new("/bin/ls");
            clean(&mut native);
            let theirs = native.args(args).arg(d.path()).output().unwrap();
            assert_eq!(
                ours.stdout,
                theirs.stdout,
                "{args:?}\nours: {}\ntheirs: {}",
                String::from_utf8_lossy(&ours.stdout),
                String::from_utf8_lossy(&theirs.stdout)
            );
        }
    }
}

#[test]
fn security_context_is_visible_in_short_output() {
    let d = fixture();
    let text = stdout(output(&["--compat=gnu", "-1Z"], d.path()));
    #[cfg(target_os = "macos")]
    assert!(text.lines().all(|line| line.starts_with("? ")));
    #[cfg(target_os = "linux")]
    {
        let mut native = Command::new("/bin/ls");
        clean(&mut native);
        let expected = native.arg("-1Z").arg(d.path()).output().unwrap();
        assert_eq!(text.as_bytes(), expected.stdout);
    }
}

#[test]
fn long_options_accept_unambiguous_prefixes() {
    let d = fixture();
    let doc: serde_json::Value =
        serde_json::from_str(&stdout(output(&["--out=json", "--colo=never"], d.path()))).unwrap();
    assert_eq!(doc["schema_version"], 1);
    assert!(!output(&["--ti"], d.path()).status.success());
}

#[test]
fn shell_escape_round_trips_names_through_a_shell() {
    let d = tempfile::tempdir().unwrap();
    let name = "a b\nquote'\\\"";
    fs::write(d.path().join(name), "").unwrap();
    let quoted = stdout(output(&["--quoting-style=shell-escape-always"], d.path()));
    let out = Command::new("/bin/bash")
        .args([
            "-c",
            &format!("printf '%s' {}", quoted.trim_end_matches('\n')),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, name.as_bytes());
}

#[test]
fn version_sort_handles_compound_extensions_and_prereleases() {
    let d = tempfile::tempdir().unwrap();
    let names = [
        "hello-8.2.txt",
        "hello-8.txt",
        "hello-8.10.tar.gz",
        "hello-8.2~rc.tar.xz",
        "hello-10.tar.gz",
    ];
    for name in names {
        fs::write(d.path().join(name), "").unwrap();
    }
    let sorted = stdout(output(&["--sort=version"], d.path()));
    assert_eq!(
        sorted,
        "hello-8.txt\nhello-8.2~rc.tar.xz\nhello-8.2.txt\nhello-8.10.tar.gz\nhello-10.tar.gz\n"
    );
}

#[test]
fn extended_attributes_are_read_from_the_real_filesystem() {
    let d = fixture();
    let file = d.path().join("alpha.txt");
    let path = std::ffi::CString::new(file.as_os_str().as_encoded_bytes()).unwrap();
    #[cfg(target_os = "macos")]
    let (key, result) = ("com.qist.fixture", unsafe {
        libc::setxattr(
            path.as_ptr(),
            c"com.qist.fixture".as_ptr(),
            b"test".as_ptr().cast(),
            4,
            0,
            0,
        )
    });
    #[cfg(target_os = "linux")]
    let (key, result) = ("user.qist.fixture", unsafe {
        libc::setxattr(
            path.as_ptr(),
            c"user.qist.fixture".as_ptr(),
            b"test".as_ptr().cast(),
            4,
            0,
        )
    });
    assert_eq!(
        result,
        0,
        "test filesystem must support xattrs: {}",
        std::io::Error::last_os_error()
    );
    let text = stdout(output(&["-l@"], &file));
    assert!(text.contains(key));
    let doc: serde_json::Value =
        serde_json::from_str(&stdout(output(&["--json", "-@"], &file))).unwrap();
    assert!(
        doc["roots"][0]["extended_attributes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["name"] == key && x["size"] == 4)
    );
}
