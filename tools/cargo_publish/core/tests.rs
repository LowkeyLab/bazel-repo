use googletest::prelude::*;

use crate::PackagePath;

#[gtest]
fn package_paths_reject_traversal_and_preserve_canonical_relative_names() {
    for path in [
        "",
        "/src/lib.rs",
        "../outside",
        "src/../outside",
        "src//lib.rs",
        "./src",
        "src\\lib.rs",
        "src/",
        "src/\0bad",
    ] {
        assert_that!(PackagePath::new(path), err(anything()));
    }
    assert_that!(
        PackagePath::new("src/nested/lib.rs").unwrap().as_str(),
        eq("src/nested/lib.rs")
    );
}

#[gtest]
fn sha256_and_paths_preserve_json_string_contracts_without_bypassing_validation() {
    use crate::{Sha256, digest_bytes};
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_that!(digest_bytes(b"abc").as_str(), eq(expected));
    let expected_json = format!("\"{expected}\"");
    assert_that!(
        serde_json::to_string(&digest_bytes(b"abc"))
            .unwrap()
            .as_str(),
        eq(expected_json.as_str())
    );
    assert_that!(
        serde_json::from_str::<Sha256>("\"bad-hash\""),
        err(anything())
    );
    assert_that!(
        serde_json::from_str::<PackagePath>("\"../escape\""),
        err(anything())
    );
    assert_that!(
        serde_json::to_string(&PackagePath::new("src/lib.rs").unwrap()).unwrap(),
        eq("\"src/lib.rs\"")
    );
}

#[gtest]
fn genuine_generation_contract_roundtrips_without_changing_the_public_schema() {
    use crate::GenerationContract;
    let source = include_str!("testdata/generation-contract.json");
    let expected: serde_json::Value = serde_json::from_str(source).unwrap();
    let contract: GenerationContract = serde_json::from_str(source).unwrap();
    assert_that!(contract.validate(), ok(anything()));
    assert_that!(contract.package.name.as_str(), eq("publishing-fixture"));
    assert_that!(
        contract
            .selection
            .selected_targets()
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        elements_are![eq("lib:fixture_library")]
    );
    assert_that!(serde_json::to_value(&contract).unwrap(), eq(&expected));
}

#[gtest]
fn sparse_analysis_input_and_typed_file_io_preserve_the_accepted_contract() {
    use crate::{AnalysisSpec, digest_file, read_json, write_json};
    let scratch = tempfile::tempdir().unwrap();
    let input = scratch.path().join("analysis.json");
    std::fs::write(&input, r#"{
      "schema_version":1,"target":"x86_64-unknown-linux-gnu",
      "package":{"name":"fixture","version":"0.1.0","edition":"2024","description":"Fixture","license":"MIT","repository":"https://example.com/repo","rust-version":"1.96","readme":"README.md"},
      "library":{"label":"//fixture:lib","crate_name":"fixture","crate_type":"rlib","edition":"2024","path":"src/lib.rs","dependencies":[]},
      "tests":[{"label":"//fixture:test","dependencies":[]}],
      "inputs":[{"path":"src/lib.rs","source":"/fixture/lib.rs"}],
      "dependency_closure":[{"label":"@dependency//:dep","crate_name":"dep","external":true,"version":null,"manifest":null,"features":[]}]
    }"#).unwrap();
    let analysis: AnalysisSpec = read_json(&input).unwrap();
    assert_that!(analysis.tests[0].label.as_str(), eq("//fixture:test"));
    assert_that!(analysis.tests[0].dependencies.is_empty(), is_true());
    assert_that!(analysis.dependency_closure[0].manifest.is_none(), is_true());
    let output = scratch.path().join("roundtrip.json");
    write_json(&output, &analysis).unwrap();
    assert_that!(read_json::<AnalysisSpec>(&output).unwrap(), eq(&analysis));
    let bytes = scratch.path().join("source");
    std::fs::write(&bytes, b"abc").unwrap();
    assert_that!(
        digest_file(&bytes).unwrap().as_str(),
        eq("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_that!(
        read_json::<AnalysisSpec>(scratch.path().join("missing.json"))
            .unwrap_err()
            .to_string(),
        contains_substring("missing.json")
    );
}

#[gtest]
fn contract_rejects_reserved_duplicate_and_file_directory_input_conflicts() {
    use crate::GenerationContract;
    let source = include_str!("testdata/generation-contract.json");
    let original: GenerationContract = serde_json::from_str(source).unwrap();
    for path in [
        "Cargo.toml",
        "Cargo.toml.orig",
        "Cargo.lock",
        ".cargo_vcs_info.json",
        "src/lib.rs",
        "src",
    ] {
        let mut contract = original.clone();
        let mut input = contract.inputs[0].clone();
        input.path = PackagePath::new(path).unwrap();
        contract.inputs.push(input);
        assert_that!(
            contract.validate(),
            err(anything()),
            "invalid declared input path: {path}"
        );
    }
    let mut missing_identity = original.clone();
    missing_identity
        .selection
        .library
        .as_mut()
        .unwrap()
        .crate_name
        .clear();
    assert_that!(missing_identity.validate(), err(anything()));
}
