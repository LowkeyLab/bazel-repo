use std::collections::BTreeSet;

use coverage_reports::build_event::{ArtifactPolicy, BuildEvents};
use googletest::{assert_that, gtest, matchers::eq};
use serde_json::json;

#[gtest]
fn nested_repeated_artifacts_preserve_configuration_and_order() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("baseline"), b"lines")?;
    let artifact = json!({"name":"baseline", "length":"5", "digest":"5ea44c3961f16643e614435496b16115aa6d75458b5cc3fd5398aae291f3126b"});
    let events = [
        json!({"id":{"namedSet":{"id":"leaf"}},"namedSetOfFiles":{"files":[artifact]}}),
        json!({"id":{"namedSet":{"id":"parent"}},"namedSetOfFiles":{"fileSets":[{"id":"leaf"},{"id":"leaf"}]}}),
        json!({"id":{"targetCompleted":{"label":"//:root","configuration":{"id":"one"}}},"completed":{"success":true,"outputGroup":[{"name":"go_coverage_baselines","fileSets":[{"id":"parent"}]}]}}),
        json!({"id":{"targetCompleted":{"label":"//:root","aspect":"aspect"}},"completed":{"success":false}}),
        json!({"id":{"targetCompleted":{"label":"//:root","configuration":{"id":"two"}}},"completed":{"success":true,"outputGroup":[{"name":"go_coverage_baselines","fileSets":[{"id":"leaf"}]}]}}),
    ];
    let events = BuildEvents::parse(
        &events
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )?;
    let roots = BTreeSet::from(["//:root".to_owned()]);
    let selected = events.baselines(
        &roots,
        "go_coverage_baselines",
        directory.path(),
        "Go",
        ArtifactPolicy::WorkspaceIntegrity,
    )?;
    assert_that!(selected.len(), eq(2));
    assert_that!(
        selected[0].identity.configuration,
        eq(&Some(json!({"id":"one"})))
    );
    assert_that!(
        selected[1].identity.configuration,
        eq(&Some(json!({"id":"two"})))
    );
    assert_that!(
        selected[0].paths,
        eq(&vec![directory.path().join("baseline"); 2])
    );
    Ok(())
}

#[gtest]
fn malformed_length_cannot_validate_an_empty_artifact() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("empty"), b"")?;
    let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    for length in [
        json!("invalid"),
        json!(null),
        json!({}),
        json!([]),
        json!(true),
    ] {
        let text = event_text(&[
            json!({"id":{"namedSet":{"id":"files"}},"namedSetOfFiles":{"files":[{"name":"empty","length":length,"digest":digest}]}}),
            json!({"id":{"targetCompleted":{"label":"//:root"}},"completed":{"success":true,"outputGroup":[{"name":"baselines","fileSets":[{"id":"files"}]}]}}),
        ]);
        let events = BuildEvents::parse(&text)?;
        assert_that!(
            events
                .baselines(
                    &BTreeSet::from(["//:root".to_owned()]),
                    "baselines",
                    directory.path(),
                    "Go",
                    ArtifactPolicy::WorkspaceIntegrity
                )
                .is_err(),
            eq(true)
        );
    }
    Ok(())
}

fn event_text(events: &[serde_json::Value]) -> String {
    events
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[gtest]
fn required_baselines_reject_missing_cyclic_and_incomplete_evidence() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let roots = BTreeSet::from(["//:root".to_owned()]);
    for fault in [
        "missing-root",
        "failed-root",
        "missing-group",
        "incomplete",
        "cycle",
        "missing-set",
    ] {
        let mut events = vec![
            json!({"id":{"namedSet":{"id":"files"}},"namedSetOfFiles":{"fileSets":[{"id":"files"}]}}),
            json!({"id":{"targetCompleted":{"label":"//:root"}},"completed":{"success":true,"outputGroup":[{"name":"baselines","fileSets":[{"id":"files"}]}]}}),
        ];
        match fault {
            "missing-root" => {
                events.pop();
            }
            "failed-root" => events[1]["completed"]["success"] = json!(false),
            "missing-group" => events[1]["completed"]["outputGroup"] = json!([]),
            "incomplete" => events[1]["completed"]["outputGroup"][0]["incomplete"] = json!(true),
            "missing-set" => events[0]["namedSetOfFiles"]["fileSets"][0]["id"] = json!("missing"),
            _ => (),
        }
        let events = BuildEvents::parse(&event_text(&events))?;
        assert_that!(
            events
                .baselines(
                    &roots,
                    "baselines",
                    directory.path(),
                    "Go",
                    ArtifactPolicy::WorkspaceIntegrity
                )
                .is_err(),
            eq(true)
        );
    }
    assert_that!(BuildEvents::parse("{bad json}").is_err(), eq(true));
    assert_that!(
        BuildEvents::parse(r#"{"namedSetOfFiles":{}}"#).is_err(),
        eq(true)
    );
    Ok(())
}

#[gtest]
fn unchecked_workspace_policy_preserves_lexical_paths_without_integrity_checks()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let text = event_text(&[
        json!({"id":{"namedSet":{"id":"files"}},"namedSetOfFiles":{"files":[{"pathPrefix":["missing",".."],"name":"baseline","uri":"bytestream://ignored"}]}}),
        json!({"id":{"targetCompleted":{"label":"//:root"}},"completed":{"success":true,"outputGroup":[{"name":"baselines","fileSets":[{"id":"files"}]}]}}),
    ]);
    let events = BuildEvents::parse(&text)?;
    let roots = BTreeSet::from(["//:root".to_owned()]);
    let result = events.baselines(
        &roots,
        "baselines",
        directory.path(),
        "Go",
        ArtifactPolicy::WorkspaceUnchecked,
    )?;
    assert_that!(
        result[0].paths,
        eq(&vec![directory.path().join("missing/../baseline")])
    );
    assert_that!(
        events
            .baselines(
                &roots,
                "baselines",
                directory.path(),
                "Go",
                ArtifactPolicy::WorkspaceIntegrity
            )
            .is_err(),
        eq(true)
    );
    Ok(())
}
