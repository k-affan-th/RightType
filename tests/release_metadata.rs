//! Cargo.toml is the single source of the version; these catch the places that
//! still have to spell it out by hand drifting away from it.

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn read(path: &str) -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/").to_owned() + path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn installer_fallback_version_matches_cargo() {
    let iss = read("packaging/RightType.iss");
    assert!(
        iss.contains(&format!("#define MyAppVersion \"{VERSION}\"")),
        "packaging/RightType.iss fallback MyAppVersion is not {VERSION}"
    );
}

#[test]
fn readme_names_the_current_artifacts() {
    let readme = read("README.md");
    for artifact in [
        format!("RightType-{VERSION}-setup.exe"),
        format!("RightType-{VERSION}-x64.zip"),
    ] {
        assert!(
            readme.contains(&artifact),
            "README.md does not mention {artifact}"
        );
    }
}

#[test]
fn changelog_has_an_entry_for_this_version() {
    let changelog = read("CHANGELOG.md");
    assert!(
        changelog.contains(&format!("## [{VERSION}]")),
        "CHANGELOG.md has no `## [{VERSION}]` section"
    );
}
