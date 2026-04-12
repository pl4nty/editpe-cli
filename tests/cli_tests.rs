use editpe::Image;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static UNIQUE_ID: AtomicU64 = AtomicU64::new(0);

const AVATAR_ICO: &[u8] = &[
    0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x20, 0x00, 0x46,
    0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00, 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01,
    0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42,
    0x60, 0x82,
];

fn fixture_exe() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("assets")
        .join("smallbin-large.exe")
}

fn temp_dir() -> PathBuf {
    let suffix = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("editpe-cli-tests-{ts}-{suffix}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup_editpe_exe() -> (PathBuf, PathBuf) {
    let dir = temp_dir();
    let exe = dir.join("editpe.exe");
    fs::copy(fixture_exe(), &exe).unwrap();
    (dir, exe)
}

fn run_ok(exe: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_editpe"))
        .arg(exe)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn cli_set_and_get_version_string() {
    let (dir, exe) = setup_editpe_exe();
    run_ok(&exe, &["--set-version-string", "Comments", "This is an exe"]);
    let got = run_ok(&exe, &["--get-version-string", "Comments"]);
    assert_eq!(got, "This is an exe");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cli_set_and_get_file_and_product_version() {
    let (dir, exe) = setup_editpe_exe();
    run_ok(
        &exe,
        &[
            "--set-file-version",
            "10.7",
            "--set-product-version",
            "1.2.3",
        ],
    );
    let file = run_ok(&exe, &["--get-version-string", "FileVersion"]);
    let product = run_ok(&exe, &["--get-version-string", "ProductVersion"]);
    assert_eq!(file, "10.7.0.0");
    assert_eq!(product, "1.2.3.0");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cli_set_icon_and_version_in_one_command() {
    let (dir, exe) = setup_editpe_exe();
    let icon = dir.join("avatar.ico");
    fs::write(&icon, AVATAR_ICO).unwrap();
    run_ok(
        &exe,
        &[
            "--set-icon",
            icon.to_str().unwrap(),
            "--set-file-version",
            "10.7",
        ],
    );
    let image = Image::parse_file(exe).unwrap();
    let icon_data = image
        .resource_directory()
        .unwrap()
        .get_main_icon()
        .unwrap()
        .unwrap();
    assert!(!icon_data.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cli_set_and_get_resource_string() {
    let (dir, exe) = setup_editpe_exe();
    run_ok(&exe, &["--set-resource-string", "11", "hello world"]);
    let got = run_ok(&exe, &["--get-resource-string", "11"]);
    assert_eq!(got, "hello world");
    fs::remove_dir_all(dir).unwrap();
}
