use std::fs;
use std::path::Path;

#[test]
fn package_and_release_binary_use_plural_project_name() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(repo_root.join("Cargo.toml")).unwrap();
    let release_workflow = fs::read_to_string(
        repo_root
            .join(".github")
            .join("workflows")
            .join("release.yml"),
    )
    .unwrap();

    assert!(manifest.contains(r#"name = "cheatsheets""#));
    assert!(release_workflow.contains(r"target\release\cheatsheets.exe"));
    assert!(release_workflow.contains(r"CheatSheets-$tag-windows-x64.zip"));
}
