//! Files, privacy, network, system and QR tools.

use std::{
    fs,
    path::{Path, PathBuf},
};

use saqviro_core::registry::Registry;
use saqviro_jobs::{Config, ErrorKind, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, OutputFile, Params};
use saqviro_tools::{network, privacy::inspect, qr, system};
use serde_json::{Value, json};

const KEY_FINGERPRINT: &str = "SHA256:txKuDfzxlh7/rj5NhA5XASXzJSehcwA4ZlArMTFW3CI";
const CERT_FINGERPRINT: &str =
    "21:F0:03:82:23:0A:AD:E0:46:72:2A:28:3D:3A:C5:EE:B2:D9:4A:6F:08:C8:B9:B2:82:12:F7:E8:19:62:7D:B2";

fn registry() -> Registry {
    fn collect(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.file_name().is_some_and(|n| n == "tool.yaml") {
                out.push((path.display().to_string(), fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut files = Vec::new();
    collect(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools"), &mut files);
    Registry::from_sources(files.iter().map(|(p, s)| (p.as_str(), s.as_str()))).unwrap()
}

fn run_input(tool_id: &str, input: JobInput, params: Value) -> JobSnapshot {
    let registry = registry();
    let tool = registry.get(tool_id).unwrap();
    let params = tool.resolve_params(params.as_object().unwrap()).unwrap();
    let exec = saqviro_tools::executor(tool).unwrap();
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    let jobs = JobManager::new(config, |_: &JobSnapshot| {}).unwrap();
    jobs.wait(jobs.submit(tool_id, exec, input, params)).unwrap()
}

fn run(tool_id: &str, files: &[PathBuf], params: Value) -> JobSnapshot {
    run_input(tool_id, JobInput::Files(files.to_vec()), params)
}

fn text(done: &JobSnapshot) -> String {
    match &done.output {
        Some(JobOutput::Text(text)) => text.clone(),
        other => panic!("expected text, got {other:?} / {:?}", done.error),
    }
}

fn file(done: &JobSnapshot) -> OutputFile {
    match &done.output {
        Some(JobOutput::Files(files)) => files[0].clone(),
        other => panic!("expected files, got {other:?} / {:?}", done.error),
    }
}

/// a.txt, b.txt with the same content, sub/c.bin, sub/empty.txt, void/
fn folder(root: &Path) -> PathBuf {
    let dir = root.join("data");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::create_dir_all(dir.join("void")).unwrap();
    fs::write(dir.join("a.txt"), "same").unwrap();
    fs::write(dir.join("b.txt"), "same").unwrap();
    fs::write(dir.join("sub/c.bin"), vec![1; 2048]).unwrap();
    fs::write(dir.join("sub/empty.txt"), "").unwrap();
    dir
}

#[test]
fn draws_a_tree() {
    let root = tempfile::tempdir().unwrap();
    let done = run("files.tree", &[folder(root.path())], json!({}));
    assert_eq!(
        text(&done),
        "data/\n├── a.txt\n├── b.txt\n├── sub/\n│   ├── c.bin\n│   └── empty.txt\n└── void/\n"
    );
    let done = run("files.tree", &[root.path().join("data")], json!({ "max_depth": 1 }));
    assert!(!text(&done).contains("c.bin"));
}

#[test]
fn hides_hidden_files_unless_asked() {
    let root = tempfile::tempdir().unwrap();
    let dir = folder(root.path());
    fs::write(dir.join(".secret"), "x").unwrap();
    assert!(!text(&run("files.tree", std::slice::from_ref(&dir), json!({}))).contains(".secret"));
    assert!(text(&run("files.tree", &[dir], json!({ "show_hidden": true }))).contains(".secret"));
}

#[test]
fn sums_up_a_folder() {
    let root = tempfile::tempdir().unwrap();
    let out = text(&run("files.folder-size", &[folder(root.path())], json!({})));
    assert!(out.starts_with("data: 2.0 KB in 4 files\n"), "{out}");
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[2].ends_with("sub"), "the largest comes first: {out}");
}

#[test]
fn finds_large_files() {
    let root = tempfile::tempdir().unwrap();
    let dir = folder(root.path());
    let out = text(&run(
        "files.large",
        std::slice::from_ref(&dir),
        json!({ "min_mb": 0, "top": 1 }),
    ));
    assert!(out.contains("c.bin") && !out.contains("a.txt"), "{out}");
    assert_eq!(
        text(&run("files.large", &[dir], json!({ "min_mb": 1 }))),
        "No files that large"
    );
}

#[test]
fn finds_duplicates_by_content() {
    let root = tempfile::tempdir().unwrap();
    let dir = folder(root.path());
    fs::write(dir.join("same-size.txt"), "diff").unwrap();
    let out = text(&run("files.duplicates", &[dir], json!({})));
    assert!(out.starts_with("1 sets of duplicates, 4 B could be freed"), "{out}");
    assert!(
        out.contains("a.txt") && out.contains("b.txt") && !out.contains("same-size"),
        "{out}"
    );
}

#[test]
fn says_when_nothing_is_duplicated() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("one");
    fs::write(&path, "x").unwrap();
    assert_eq!(
        text(&run("files.duplicates", &[path], json!({}))),
        "No duplicates found"
    );
}

#[test]
fn finds_what_is_empty() {
    let root = tempfile::tempdir().unwrap();
    let out = text(&run("files.empty", &[folder(root.path())], json!({})));
    assert!(out.contains("Empty folders: 1") && out.contains("void"), "{out}");
    assert!(out.contains("Empty files: 1") && out.contains("empty.txt"), "{out}");
}

#[test]
fn a_folder_checksum_ignores_where_the_folder_is() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (folder(one.path()), folder(two.path()));
    let hash = |dir: &PathBuf| text(&run("files.folder-checksum", std::slice::from_ref(dir), json!({})));
    assert_eq!(hash(&a), hash(&b));

    fs::rename(b.join("a.txt"), b.join("renamed.txt")).unwrap();
    assert_ne!(hash(&a), hash(&b), "a name is part of the folder");
    fs::rename(b.join("renamed.txt"), b.join("a.txt")).unwrap();
    fs::write(b.join("a.txt"), "other").unwrap();
    assert_ne!(hash(&a), hash(&b), "content is part of the folder");
}

#[cfg(unix)]
#[test]
fn links_are_not_followed() {
    let root = tempfile::tempdir().unwrap();
    let dir = folder(root.path());
    let outside = root.path().join("outside.txt");
    fs::write(&outside, "same").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("link.txt")).unwrap();
    let out = text(&run("files.duplicates", &[dir], json!({})));
    assert!(!out.contains("link.txt"), "{out}");
}

#[test]
fn tar_archives_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let dir = folder(root.path());
    let done = run("files.tar.create", std::slice::from_ref(&dir), json!({}));
    let archive = file(&done).path;
    assert_eq!(archive.file_name().unwrap(), "data.tar.gz");

    let elsewhere = tempfile::tempdir().unwrap();
    let copy = elsewhere.path().join("data.tar.gz");
    fs::copy(&archive, &copy).unwrap();
    let done = run("files.tar.extract", &[copy], json!({}));
    let out = file(&done).path;
    assert_eq!(out.file_name().unwrap(), "data");
    assert_eq!(fs::read_to_string(out.join("data/a.txt")).unwrap(), "same");
    assert_eq!(fs::read(out.join("data/sub/c.bin")).unwrap().len(), 2048);
    assert!(out.join("data/void").is_dir());
}

/// A tar with one file whose name is written straight into the header.
fn raw_tar(name: &str, kind: u8) -> Vec<u8> {
    let mut block = vec![0u8; 512];
    block[..name.len()].copy_from_slice(name.as_bytes());
    block[100..107].copy_from_slice(b"0000644");
    block[108..115].copy_from_slice(b"0000000");
    block[116..123].copy_from_slice(b"0000000");
    block[124..135].copy_from_slice(b"00000000005");
    block[136..147].copy_from_slice(b"00000000000");
    block[156] = kind;
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    block[148..156].copy_from_slice(b"        ");
    let sum: u32 = block.iter().map(|b| u32::from(*b)).sum();
    block[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
    block.extend(b"hello");
    block.resize(1024, 0);
    block.extend([0; 1024]);
    block
}

#[test]
fn a_tar_cannot_write_outside_its_folder() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("evil.tar");
    fs::write(&source, raw_tar("../escaped.txt", b'0')).unwrap();
    let done = run("files.tar.extract", &[source], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
    assert!(!root.path().join("escaped.txt").exists());
    assert!(!root.path().join("evil").exists(), "a failed job leaves no folder");
}

#[test]
fn a_tar_cannot_plant_links() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("links.tar");
    fs::write(&source, raw_tar("link", b'2')).unwrap();
    let done = run("files.tar.extract", &[source], json!({}));
    let out = file(&done);
    assert_eq!(out.note.as_deref(), Some("1 links were skipped."));
    assert!(fs::symlink_metadata(out.path.join("link")).is_err());
}

#[test]
fn a_damaged_tar_is_explained() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("a.tgz");
    fs::write(&source, [0x1F, 0x8B, 8, 0, 1, 2, 3]).unwrap();
    let done = run("files.tar.extract", &[source], json!({}));
    assert_eq!(done.status, JobStatus::Failed);
}

#[test]
fn encodes_files_as_base64() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.png");
    fs::write(&path, "hi").unwrap();
    assert_eq!(
        text(&run("files.base64", std::slice::from_ref(&path), json!({}))),
        "aGk="
    );
    assert_eq!(
        text(&run("files.base64", &[path], json!({ "data_uri": true }))),
        "data:image/png;base64,aGk="
    );
}

#[test]
fn encrypts_and_decrypts() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.txt");
    fs::write(&path, "the secret text").unwrap();

    let done = run(
        "privacy.encrypt",
        std::slice::from_ref(&path),
        json!({ "passphrase": "correct horse" }),
    );
    let locked = file(&done).path;
    assert_eq!(locked.file_name().unwrap(), "note.txt.age");
    let bytes = fs::read(&locked).unwrap();
    assert!(bytes.starts_with(b"age-encryption.org/v1"));
    assert!(!bytes.windows(6).any(|w| w == b"secret"));

    let done = run(
        "privacy.decrypt",
        std::slice::from_ref(&locked),
        json!({ "passphrase": "wrong" }),
    );
    assert!(done.error.unwrap().message.contains("wrong passphrase"));

    fs::remove_file(&path).unwrap();
    let done = run("privacy.decrypt", &[locked], json!({ "passphrase": "correct horse" }));
    let open = file(&done).path;
    assert_eq!(open.file_name().unwrap(), "note.txt");
    assert_eq!(fs::read_to_string(open).unwrap(), "the secret text");
}

#[test]
fn a_changed_encrypted_file_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.txt");
    fs::write(&path, "the secret text").unwrap();
    let done = run(
        "privacy.encrypt",
        std::slice::from_ref(&path),
        json!({ "passphrase": "pw" }),
    );
    let locked = file(&done).path;
    let mut bytes = fs::read(&locked).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&locked, bytes).unwrap();
    fs::remove_file(path).unwrap();

    let done = run("privacy.decrypt", &[locked], json!({ "passphrase": "pw" }));
    assert_eq!(done.error.unwrap().kind, ErrorKind::UnsupportedFormat);
    assert!(!root.path().join("note.txt").exists(), "nothing half-decrypted is left");
}

#[test]
fn a_passphrase_is_never_in_the_error() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("not-age.age");
    fs::write(&path, "plain").unwrap();
    let done = run("privacy.decrypt", &[path], json!({ "passphrase": "hunter2secret" }));
    let error = done.error.unwrap();
    assert!(error.message.contains("not an age file"), "{}", error.message);
    assert!(!error.message.contains("hunter2secret"));
}

#[test]
fn rates_passwords() {
    let none = Params::new();
    let weak = inspect::password_strength("password", &none).unwrap();
    assert!(weak.starts_with("Strength: very weak"), "{weak}");
    let strong = inspect::password_strength("vT8#qLz!4mXp@9Rk2wNe", &none).unwrap();
    assert!(strong.starts_with("Strength: very strong"), "{strong}");
    assert!(inspect::password_strength("", &none).is_err());
}

#[test]
fn reads_a_certificate() {
    let pem = include_str!("fixtures/self-signed.pem");
    let out = inspect::certificate(pem, &Params::new()).unwrap();
    assert!(out.contains("CN=saqviro.test"), "{out}");
    assert!(out.contains("O=Saqviro"), "{out}");
    assert!(out.contains(CERT_FINGERPRINT), "{out}");
    assert!(out.contains("saqviro.test"), "{out}");
    assert!(inspect::certificate("nonsense!", &Params::new()).is_err());
}

#[test]
fn matches_ssh_keygen() {
    let key = include_str!("fixtures/ed25519.pub");
    let out = inspect::ssh_fingerprint(key, &Params::new()).unwrap();
    assert_eq!(out, format!("{KEY_FINGERPRINT} me@host (ssh-ed25519)"));
}

#[test]
fn refuses_a_private_key() {
    let error = inspect::ssh_fingerprint("-----BEGIN OPENSSH PRIVATE KEY-----\nabc", &Params::new()).unwrap_err();
    assert!(error.contains("private key"));
    assert!(inspect::ssh_fingerprint("nonsense", &Params::new()).is_err());
}

#[test]
fn calculates_networks() {
    let none = Params::new();
    let out = network::cidr("192.168.1.10/24", &none).unwrap();
    assert_eq!(
        out,
        "Network: 192.168.1.0/24\nMask: 255.255.255.0\nWildcard: 0.0.0.255\nBroadcast: 192.168.1.255\n\
         Hosts: 192.168.1.1 – 192.168.1.254\nUsable hosts: 254\nAddresses: 256\nType: private"
    );
    assert_eq!(
        network::cidr("10.0.0.0/255.255.0.0", &none)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
        "Network: 10.0.0.0/16"
    );
    assert!(
        network::cidr("8.8.8.8", &none)
            .unwrap()
            .contains("Addresses: 1\nType: public")
    );
    assert!(network::cidr("10.0.0.0/31", &none).unwrap().contains("Addresses: 2"));
    assert!(
        network::cidr("0.0.0.0/0", &none)
            .unwrap()
            .contains("Addresses: 4294967296")
    );
    let v6 = network::cidr("2001:db8::1/64", &none).unwrap();
    assert!(
        v6.starts_with("Network: 2001:db8::/64\nFirst: 2001:db8::\nLast: 2001:db8::ffff:ffff:ffff:ffff"),
        "{v6}"
    );
    assert!(network::cidr("::/0", &none).unwrap().contains("2^128"));
}

#[test]
fn refuses_what_is_not_a_network() {
    let none = Params::new();
    assert!(network::cidr("nope", &none).is_err());
    assert!(network::cidr("10.0.0.0/33", &none).is_err());
    assert!(network::cidr("10.0.0.0/255.0.255.0", &none).is_err());
    assert!(network::cidr("::1/129", &none).is_err());
}

#[test]
fn describes_this_computer() {
    let out = system::info("", &Params::new()).unwrap();
    assert!(out.contains("Architecture:") && out.contains("Memory:"), "{out}");
}

#[test]
fn lists_the_path() {
    let p = json!({ "only_path": true });
    let out = system::environment("", p.as_object().unwrap()).unwrap();
    assert!(!out.contains("PATH="));
    assert!(system::environment("", &Params::new()).unwrap().contains("PATH="));
}

#[test]
fn writes_the_wifi_format() {
    let p = |v: Value| v.as_object().unwrap().clone();
    let text = qr::wifi_text(&p(
        json!({ "network": "My;Net", "password": "a\"b", "security": "WPA" }),
    ))
    .unwrap();
    assert_eq!(text, r#"WIFI:S:My\;Net;T:WPA;P:a\"b;;"#);
    let open = qr::wifi_text(&p(json!({ "network": "Cafe", "security": "none", "hidden": true }))).unwrap();
    assert_eq!(open, "WIFI:S:Cafe;T:nopass;H:true;;");
    assert!(qr::wifi_text(&p(json!({ "network": "x", "security": "WPA" }))).is_err());
    assert!(qr::wifi_text(&Params::new()).is_err());
}

#[test]
fn a_wifi_code_can_be_read_back() {
    let done = run_input(
        "qr.wifi",
        JobInput::Text(String::new()),
        json!({ "network": "Home", "password": "pw" }),
    );
    let Some(JobOutput::Image(picture)) = done.output else {
        panic!("{:?}", done.error)
    };
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(picture.data).unwrap();
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!(qr::read_codes(&image), ["WIFI:S:Home;T:WPA;P:pw;;"]);
}

#[test]
fn writes_vcards() {
    let p = json!({ "name": "Ann, Lee", "phone": "+1 555", "email": "a@b.c" });
    let text = qr::vcard_text(p.as_object().unwrap()).unwrap();
    assert!(
        text.starts_with("BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ann\\, Lee\r\n"),
        "{text}"
    );
    assert!(text.contains("TEL:+1 555\r\nEMAIL:a@b.c\r\nEND:VCARD"), "{text}");
    assert!(qr::vcard_text(&Params::new()).is_err());
}

#[test]
fn draws_barcodes() {
    let picture =
        |text: &str, format: &str| run_input("qr.barcode", JobInput::Text(text.into()), json!({ "format": format }));
    for (text, format) in [
        ("HELLO-123", "code128"),
        ("HELLO", "code39"),
        ("5901234123457", "ean13"),
    ] {
        let done = picture(text, format);
        let Some(JobOutput::Image(image)) = done.output else {
            panic!("{format}: {:?}", done.error)
        };
        assert_eq!(image.ext, "png");
    }
    assert_eq!(picture("abc", "ean13").error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn two_files_of_one_name_both_fit_in_a_tar() {
    let root = tempfile::tempdir().unwrap();
    let (a, b) = (root.path().join("d1/f.txt"), root.path().join("d2/f.txt"));
    fs::create_dir_all(a.parent().unwrap()).unwrap();
    fs::create_dir_all(b.parent().unwrap()).unwrap();
    fs::write(&a, "one").unwrap();
    fs::write(&b, "two").unwrap();
    let done = run("files.tar.create", &[a, b], json!({}));
    let archive = file(&done).path;
    let done = run("files.tar.extract", &[archive], json!({}));
    let out = file(&done).path;
    assert_eq!(fs::read_to_string(out.join("f.txt")).unwrap(), "one");
    assert_eq!(fs::read_to_string(out.join("f (2).txt")).unwrap(), "two");
}

#[test]
fn a_tar_that_never_ends_is_stopped() {
    use saqviro_tools::files::{archive::Limits, tarball};
    use std::io::Read;
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("out");
    fs::create_dir(&target).unwrap();
    // A long name that claims 3 GB, and more bytes than the limit allows.
    let mut header = raw_tar("././@LongLink", b'L');
    header.truncate(512);
    header[124..135].copy_from_slice(b"26000000000");
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    header[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
    let limits = Limits {
        entries: 10,
        total_bytes: 1024,
        ratio: 1,
        ratio_floor: 1024,
    };
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    let jobs = JobManager::new(config, |_: &JobSnapshot| {}).unwrap();
    let exec: std::sync::Arc<dyn saqviro_jobs::Executor> =
        std::sync::Arc::new(move |ctx: &saqviro_jobs::JobContext, _: JobInput, _: &Params| {
            let stream = std::io::Cursor::new(header.clone()).chain(std::io::repeat(b'a').take(200 * 1024 * 1024));
            tarball::unpack(ctx, stream, 10, &target, limits).map(|_| JobOutput::Text(String::new()))
        });
    let done = jobs
        .wait(jobs.submit("t", exec, JobInput::Text(String::new()), Params::new()))
        .unwrap();
    assert_eq!(done.status, JobStatus::Failed);
}
