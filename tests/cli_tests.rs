use editpe::Image;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static UNIQUE_ID: AtomicU64 = AtomicU64::new(0);

fn fixture_exe() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("assets")
        .join("smallbin-large.exe")
}

fn fixture_icon() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("assets")
        .join("avatar.ico")
}

fn temp_dir() -> PathBuf {
    let suffix = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("editpe-cli-tests-{ts}-{suffix}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    dir: PathBuf,
    exe: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn setup_editpe_exe() -> Fixture {
    let dir = temp_dir();
    let exe = dir.join("editpe.exe");
    fs::copy(fixture_exe(), &exe).unwrap();
    Fixture { dir, exe }
}

fn run_ok(exe: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_editpe")).arg(exe).args(args).output().unwrap();
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
    let fixture = setup_editpe_exe();
    run_ok(
        &fixture.exe,
        &["--set-version-string", "Comments", "This is an exe"],
    );
    let got = run_ok(&fixture.exe, &["--get-version-string", "Comments"]);
    assert_eq!(got, "This is an exe");
}

#[test]
fn cli_set_and_get_file_and_product_version() {
    let fixture = setup_editpe_exe();
    run_ok(
        &fixture.exe,
        &[
            "--set-file-version",
            "10.7",
            "--set-product-version",
            "1.2.3",
        ],
    );
    let file = run_ok(&fixture.exe, &["--get-version-string", "FileVersion"]);
    let product = run_ok(&fixture.exe, &["--get-version-string", "ProductVersion"]);
    assert_eq!(file, "10.7.0.0");
    assert_eq!(product, "1.2.3.0");
}

#[test]
fn cli_set_icon_and_version_in_one_command() {
    let fixture = setup_editpe_exe();
    let icon = fixture.dir.join("avatar.ico");
    fs::copy(fixture_icon(), &icon).unwrap();
    run_ok(
        &fixture.exe,
        &[
            "--set-icon",
            icon.to_str().unwrap(),
            "--set-file-version",
            "10.7",
        ],
    );
    let image = Image::parse_file(&fixture.exe).unwrap();
    let icon_data = image.resource_directory().unwrap().get_main_icon().unwrap().unwrap();
    assert!(!icon_data.is_empty());
}

#[test]
fn cli_set_and_get_resource_string() {
    let fixture = setup_editpe_exe();
    run_ok(&fixture.exe, &["--set-resource-string", "11", "hello world"]);
    let got = run_ok(&fixture.exe, &["--get-resource-string", "11"]);
    assert_eq!(got, "hello world");
}
