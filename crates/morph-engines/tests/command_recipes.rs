//! S19: command recipes run a local command, report its stderr on failure, and can't
//! write outside their output.

use morph_core::recipe::RecipeFile;
use morph_core::{Job, Registry, Runner};
use morph_engines::command::CommandOp;

#[cfg(windows)]
const SHELL: [&str; 2] = ["cmd", "/c"];
#[cfg(not(windows))]
const SHELL: [&str; 2] = ["sh", "-c"];

/// A recipe running one shell command line.
fn recipe(name: &str, output: &str, line: &str) -> RecipeFile {
    let text = format!(
        "name = {name:?}\nring = \"tools\"\naccepts = [\"*\"]\noutput = {output:?}\nrun = [{:?}, {:?}, {line:?}]\n",
        SHELL[0], SHELL[1]
    );
    RecipeFile::parse(&text).unwrap()
}

fn run(r: &RecipeFile, input: &std::path::Path) -> morph_core::JobReport {
    let mut reg = Registry::new();
    reg.register(CommandOp::new(r).unwrap());
    Runner::new(&reg)
        .run(&Job::new(&r.id(), vec![input.to_path_buf()]), |_| {})
        .unwrap()
}

fn input(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("notes.txt");
    std::fs::write(&p, "hello").unwrap();
    p
}

#[test]
fn runs_the_command_and_commits_the_output() {
    let d = tempfile::tempdir().unwrap();
    let src = input(d.path());
    #[cfg(windows)]
    let line = "copy \"{input}\" \"{output}\" >NUL";
    #[cfg(not(windows))]
    let line = "cp \"{input}\" \"{output}\"";
    let r = recipe("Duplicate", "{stem} Copy.{ext}", line);
    let first = run(&r, &src);
    assert!(first.ok(), "{:?}", first.errors);
    assert_eq!(first.outputs[0], d.path().join("notes Copy.txt"));
    assert_eq!(std::fs::read_to_string(&first.outputs[0]).unwrap(), "hello");
    // Same name again: numbered, never replaced; no staging folders left behind.
    let second = run(&r, &src);
    assert_eq!(second.outputs[0], d.path().join("notes Copy 2.txt"));
    let names: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
}

#[test]
fn failures_report_stderr() {
    let d = tempfile::tempdir().unwrap();
    let src = input(d.path());
    #[cfg(windows)]
    let line = "echo the converter exploded 1>&2 & exit 3 & rem {output}";
    #[cfg(not(windows))]
    let line = "echo the converter exploded >&2; exit 3 # {output}";
    let report = run(&recipe("Broken", "{stem}.out", line), &src);
    assert!(report.outputs.is_empty());
    let err = &report.errors[0].1;
    assert!(err.contains("exit code 3"), "{err}");
    assert!(err.contains("the converter exploded"), "{err}");
}

#[test]
fn cannot_write_outside_the_output() {
    if !morph_engines::command::confined() {
        eprintln!("this kernel can't confine writes (no Landlock) — skipping");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = input(d.path());
    let evil = d.path().join("evil.txt");
    #[cfg(windows)]
    let line = "echo pwned> \"{dir}\\evil.txt\" & copy \"{input}\" \"{output}\" >NUL";
    #[cfg(not(windows))]
    let line = "echo pwned > \"{dir}/evil.txt\"; cp \"{input}\" \"{output}\"";
    let report = run(&recipe("Sneaky", "{stem} Safe.txt", line), &src);
    assert!(!evil.exists(), "the command wrote outside its output");
    // It still produced its real output.
    assert!(report.ok(), "{:?}", report.errors);
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "hello");
}

#[test]
fn output_templates_cannot_be_paths() {
    for bad in ["../{stem}.txt", "sub/{stem}.txt", "C:\\\\x\\\\{stem}.txt"] {
        let r = recipe("Path", bad, "true {output}");
        assert!(CommandOp::new(&r).is_err(), "{bad} accepted");
    }
}

#[test]
fn documented_example_parses_with_params() {
    let r = RecipeFile::parse(
        r#"
name    = "Thumbnail"
ring    = "tools"
accepts = ["image/*"]
output  = "{stem} Thumb.webp"
run     = ["magick", "{input}", "-thumbnail", "{size}x{size}", "{output}"]

[[param]]
key     = "size"
label   = "Size"
type    = "int"
min     = 16
max     = 4096
default = 512
"#,
    )
    .unwrap();
    let op = CommandOp::new(&r).unwrap();
    use morph_core::Operation;
    let spec = op.spec();
    assert_eq!(spec.id, "recipe.thumbnail");
    assert_eq!(spec.params[0].key, "size");
    assert_eq!(spec.params[0].default, serde_json::json!(512));
    assert!(matches!(&spec.output, morph_core::OutputKind::Ext(e) if e == "webp"));
}
