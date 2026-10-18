//! Read-only interpretation of supplied original-action evidence.
//!
//! These observations do not authorize retries or provide a shared transaction snapshot.
use std::collections::{HashMap, HashSet};

use serde_json::Value;
use uuid::Uuid;

use book_smartz_domain::{CloudEventDocument, encode_document};
use book_smartz_storage::ApplicationEvent;

use super::{ActionId, Intent};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Intent,
    Rejected,
    RankingPrepared,
    RegistrationPrepared,
}

pub(super) fn stage(event: &Value) -> Option<Stage> {
    match event["type"].as_str() {
        Some("booksmartz.add.intent.v1" | "booksmartz.preference.intent.v1") => Some(Stage::Intent),
        Some("booksmartz.add.rejected.v1" | "booksmartz.preference.rejected.v1") => {
            Some(Stage::Rejected)
        }
        Some("booksmartz.add.rankingprepared.v1" | "booksmartz.preference.rankingprepared.v1") => {
            Some(Stage::RankingPrepared)
        }
        Some("booksmartz.add.registrationprepared.v1") => Some(Stage::RegistrationPrepared),
        _ => None,
    }
}

pub(super) fn matches_document(
    actual: &CloudEventDocument,
    expected: &Value,
) -> anyhow::Result<bool> {
    Ok(serde_json::from_str::<Value>(&encode_document(actual)?)? == *expected)
}

pub(super) fn facts(history: &[ApplicationEvent], action: ActionId) -> Vec<&Value> {
    history
        .iter()
        .map(ApplicationEvent::envelope)
        .filter(|event| event["correlationid"] == action.text())
        .collect()
}
pub(super) fn intent(
    history: &[ApplicationEvent],
    action: ActionId,
) -> anyhow::Result<Option<Intent>> {
    facts(history, action)
        .iter()
        .find(|e| stage(e) == Some(Stage::Intent))
        .map(|e| serde_json::from_value(e["data"].clone()).map_err(Into::into))
        .transpose()
}
pub(super) fn prepared(history: &[ApplicationEvent], action: ActionId) -> Option<&Value> {
    facts(history, action)
        .into_iter()
        .find(|e| stage(e) == Some(Stage::RankingPrepared))
}
pub(super) fn saved(
    history: &[ApplicationEvent],
    ranking: &[CloudEventDocument],
    action: ActionId,
) -> anyhow::Result<bool> {
    let Some(expected) = prepared(history, action) else {
        return Ok(false);
    };
    for doc in ranking {
        if doc.event().metadata().id.as_uuid().to_string()
            == expected["data"]["document"]["id"].as_str().unwrap_or("")
        {
            return matches_document(doc, &expected["data"]["document"]);
        }
    }
    Ok(false)
}
pub(super) fn rejected(history: &[ApplicationEvent], action: ActionId) -> Option<String> {
    facts(history, action)
        .into_iter()
        .find(|e| stage(e) == Some(Stage::Rejected))
        .and_then(|e| e["data"]["reason"].as_str())
        .map(str::to_owned)
}
pub(super) fn registration_prepared(
    history: &[ApplicationEvent],
    action: ActionId,
) -> Option<&Value> {
    facts(history, action)
        .into_iter()
        .find(|event| stage(event) == Some(Stage::RegistrationPrepared))
}

pub(super) fn ranking_document(
    event: &Value,
    ranking: &[CloudEventDocument],
) -> anyhow::Result<CloudEventDocument> {
    let document = book_smartz_domain::decode_event(&event["data"]["document"].to_string())?;
    anyhow::ensure!(
        !ranking
            .iter()
            .any(|d| d.event().metadata().id == document.event().metadata().id),
        "conflicting durable evidence"
    );
    Ok(document)
}

/// A read only reconstruction. Absence is unresolved, never a rollback verdict.
pub(super) fn unresolved(
    history: &[ApplicationEvent],
    ranking: &[CloudEventDocument],
) -> anyhow::Result<Vec<ActionId>> {
    let mut intents = Vec::new();
    let mut rejections = HashSet::new();
    let mut preparations = HashMap::new();
    for event in history {
        let envelope = event.envelope();
        match stage(envelope) {
            Some(Stage::Intent) => {
                let original: Intent = serde_json::from_value(envelope["data"].clone())?;
                intents.push(original.action);
            }
            Some(Stage::Rejected) => {
                anyhow::ensure!(envelope["data"]["reason"].is_string(), "Invalid rejection");
                rejections.insert(serde_json::from_value::<ActionId>(
                    envelope["correlationid"].clone(),
                )?);
            }
            Some(Stage::RankingPrepared) => {
                let action: ActionId = serde_json::from_value(envelope["correlationid"].clone())?;
                preparations
                    .entry(action)
                    .or_insert_with(|| &envelope["data"]["document"]);
            }
            _ => {}
        }
    }
    let documents: HashMap<_, _> = ranking
        .iter()
        .map(|document| (*document.event().metadata().id.as_uuid(), document))
        .collect();
    let mut checking = Vec::new();
    for action in intents {
        if rejections.contains(&action) {
            continue;
        }
        let mut confirmed = false;
        if let Some(expected) = preparations.get(&action) {
            let id: Uuid = serde_json::from_value(expected["id"].clone())?;
            if let Some(actual) = documents.get(&id) {
                confirmed = matches_document(actual, expected)?;
            }
        }
        if !confirmed {
            checking.push(action);
        }
    }
    Ok(checking)
}
#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use book_smartz_domain::ReaderId;
    use book_smartz_storage::ApplicationEvent;

    use super::unresolved;

    fn intent(action: u128) -> Value {
        json!({
            "action": Uuid::from_u128(action), "selection": "retained selection", "work": null,
            "revision": 0, "event": Uuid::from_u128(99), "time": "2026-01-01T12:00:00Z"
        })
    }

    fn fact(sequence: u64, action: u128, kind: &str, data: Value) -> ApplicationEvent {
        let reader = ReaderId::new(Uuid::from_u128(100));
        ApplicationEvent::decode(
            reader,
            &json!({
                "specversion": "1.0", "id": Uuid::from_u128(u128::from(sequence)),
                "source": format!("urn:book-smartz:application:{}", reader.as_uuid()),
                "type": format!("booksmartz.{kind}.v1"),
                "time": "2026-01-01T12:00:00Z", "datacontenttype": "application/json",
                "sequence": format!("{sequence:020}"),
                "correlationid": Uuid::from_u128(action), "data": data
            })
            .to_string(),
        )
        .unwrap()
    }

    fn action() -> super::ActionId {
        super::ActionId(Uuid::from_u128(1))
    }

    #[googletest::test]
    fn single_action_uses_first_correlated_intent_without_validating_other_actions()
    -> anyhow::Result<()> {
        let history = [
            fact(1, 2, "add.intent", json!({})),
            fact(2, 1, "preference.intent", intent(9)),
            fact(3, 1, "add.intent", json!({})),
        ];
        let original = super::intent(&history, action())?.unwrap();
        assert_that!(original.action.text(), eq(&Uuid::from_u128(9).to_string()));
        assert_that!(original.event, eq(Uuid::from_u128(99)));
        assert_that!(original.revision, eq(0));
        assert_that!(original.time.to_rfc3339(), eq("2026-01-01T12:00:00+00:00"));
        assert_that!(
            super::facts(&history, action())[0]["id"],
            eq(&json!(Uuid::from_u128(2)))
        );
        assert_that!(
            super::intent(&history, super::ActionId(Uuid::from_u128(3)))?.is_none(),
            eq(true)
        );
        Ok(())
    }

    #[googletest::test]
    fn single_action_saved_evidence_is_independent_of_rejection_validation() -> anyhow::Result<()> {
        let expected = document();
        let ranking = [book_smartz_domain::decode_event(&expected.to_string())?];
        for reason in [json!("rejected"), json!(false)] {
            let history = [
                fact(1, 1, "preference.intent", intent(1)),
                fact(2, 1, "add.rankingprepared", json!({"document": expected})),
                fact(3, 1, "preference.rejected", json!({"reason": reason})),
                fact(4, 2, "add.rejected", json!({"reason": false})),
            ];
            assert_that!(super::intent(&history, action())?.is_some(), eq(true));
            assert_that!(super::saved(&history, &ranking, action())?, eq(true));
            // Preparation alone doesn't supply execution's required correlated intent.
            assert_that!(super::intent(&history[1..], action())?.is_none(), eq(true));
        }
        assert_that!(super::saved(&[], &ranking, action())?, eq(false));
        Ok(())
    }

    #[googletest::test]
    fn single_action_first_rejection_does_not_validate_or_replace_preparation() -> anyhow::Result<()>
    {
        for bad_document in [json!({}), json!({"id": "invalid"})] {
            let history = [
                fact(1, 1, "add.intent", intent(1)),
                fact(
                    2,
                    1,
                    "preference.rankingprepared",
                    json!({"document": bad_document}),
                ),
                fact(3, 1, "add.rejected", json!({"reason": "first rejection"})),
                fact(4, 1, "preference.rejected", json!({"reason": false})),
            ];
            assert_that!(super::saved(&history, &[], action())?, eq(false));
            assert_that!(
                super::rejected(&history, action()).as_deref(),
                eq(Some("first rejection"))
            );
            assert_that!(
                super::ranking_document(super::prepared(&history, action()).unwrap(), &[]).is_err(),
                eq(true)
            );
            let mut malformed_first = history.clone();
            malformed_first.swap(2, 3);
            assert_that!(
                super::rejected(&malformed_first, action()).is_none(),
                eq(true)
            );
        }
        Ok(())
    }

    #[googletest::test]
    fn single_action_preparation_preserves_first_envelope_and_detects_conflict()
    -> anyhow::Result<()> {
        let expected = document();
        let ranking = [book_smartz_domain::decode_event(&expected.to_string())?];
        let history = [
            fact(1, 1, "add.intent", intent(1)),
            fact(
                2,
                1,
                "preference.rankingprepared",
                json!({"document": expected}),
            ),
            fact(3, 1, "add.rankingprepared", json!({"document": {}})),
        ];
        let prepared = super::prepared(&history, action()).unwrap();
        assert_that!(prepared["id"], eq(&json!(Uuid::from_u128(2))));
        let retained = super::ranking_document(prepared, &[])?;
        assert_that!(
            serde_json::from_str::<Value>(&book_smartz_domain::encode_document(&retained)?)?,
            eq(&expected)
        );
        let mut unrelated = expected.clone();
        unrelated["id"] = json!(Uuid::from_u128(71));
        let unrelated = [book_smartz_domain::decode_event(&unrelated.to_string())?];
        assert_that!(super::saved(&history, &unrelated, action())?, eq(false));
        assert_that!(
            super::ranking_document(prepared, &unrelated).is_ok(),
            eq(true)
        );
        for field in [
            "traceparent",
            "tracestate",
            "retainedextension",
            "subject",
            "time",
            "data",
        ] {
            let mut conflict = expected.clone();
            conflict[field] = json!("different");
            let history = [
                history[0].clone(),
                fact(2, 1, "add.rankingprepared", json!({"document": conflict})),
            ];
            assert_that!(super::saved(&history, &ranking, action())?, eq(false));
        }
        let mut conflict = expected.clone();
        conflict["retainedextension"] = json!("different");
        let history = [
            history[0].clone(),
            fact(2, 1, "add.rankingprepared", json!({"document": conflict})),
        ];
        let error = super::ranking_document(super::prepared(&history, action()).unwrap(), &ranking)
            .err()
            .unwrap();
        assert_that!(error.to_string(), eq("conflicting durable evidence"));
        Ok(())
    }

    #[googletest::test]
    fn single_action_first_intent_validation_and_registration_association_are_lazy()
    -> anyhow::Result<()> {
        let history = [
            fact(1, 1, "add.unknown", json!({})),
            fact(
                2,
                2,
                "add.registrationprepared",
                json!({"bookId": "unrelated"}),
            ),
            fact(
                3,
                1,
                "preference.registrationprepared",
                json!({"bookId": "wrong family"}),
            ),
            fact(
                4,
                1,
                "add.registrationprepared",
                json!({"bookId": Uuid::from_u128(80)}),
            ),
            fact(5, 1, "add.registrationprepared", json!({})),
            fact(6, 1, "preference.intent", json!({})),
            fact(7, 1, "add.intent", intent(1)),
        ];
        assert_that!(
            super::registration_prepared(&history, action()).unwrap()["data"]["bookId"],
            eq(&json!(Uuid::from_u128(80)))
        );
        assert_that!(
            super::facts(&history, action())[0]["id"],
            eq(&json!(Uuid::from_u128(1)))
        );
        assert_that!(super::intent(&history, action()).is_err(), eq(true));
        Ok(())
    }

    #[googletest::test]
    fn unresolved_keeps_intent_order_duplicates_and_payload_identity() -> anyhow::Result<()> {
        let history = [
            fact(1, 20, "add.intent", intent(2)),
            fact(2, 1, "preference.intent", intent(1)),
            fact(3, 20, "add.intent", intent(2)),
            fact(4, 1, "add.unknown", json!({})),
        ];
        let actions: Vec<_> = unresolved(&history, &[])?
            .into_iter()
            .map(|a| a.text())
            .collect();
        assert_that!(
            actions,
            eq(&vec![
                Uuid::from_u128(2).to_string(),
                Uuid::from_u128(1).to_string(),
                Uuid::from_u128(2).to_string()
            ])
        );
        Ok(())
    }

    fn document() -> Value {
        json!({
            "specversion": "1.0", "id": Uuid::from_u128(70),
            "source": format!("urn:uuid:{}", Uuid::from_u128(100)),
            "type": "bookranking.placement.started.v1",
            "subject": format!("books/{}", Uuid::from_u128(80)),
            "time": "2026-01-01T12:00:00Z", "datacontenttype": "application/json",
            "sequence": "00000000000000000001",
            "traceparent": "retained-trace", "tracestate": "retained-state",
            "retainedextension": "retained-value",
            "data": {"readerId": Uuid::from_u128(100), "candidateBookId": Uuid::from_u128(80), "sequence": "1"}
        })
    }

    #[googletest::test]
    fn only_the_complete_first_preparation_confirms_a_save() -> anyhow::Result<()> {
        let expected = document();
        let ranking = [book_smartz_domain::decode_event(&expected.to_string())?];
        let initial = fact(1, 1, "add.intent", intent(1));
        let saved = fact(
            2,
            1,
            "preference.rankingprepared",
            json!({"document": expected}),
        );
        let malformed = fact(3, 1, "add.rankingprepared", json!({"document": {}}));
        // Both stage families associate by retained correlation, and later preparations cannot replace the first.
        assert_that!(
            unresolved(
                &[initial.clone(), saved.clone(), malformed.clone()],
                &ranking
            )?
            .len(),
            eq(0)
        );
        assert_that!(
            unresolved(&[initial.clone(), malformed, saved.clone()], &ranking).is_err(),
            eq(true)
        );
        assert_that!(unresolved(&[initial.clone(), saved], &[])?.len(), eq(1));
        // Same ID, same projected decision, different retained context: still unresolved.
        for field in [
            "traceparent",
            "tracestate",
            "retainedextension",
            "time",
            "subject",
            "data",
        ] {
            let mut conflicting = expected.clone();
            conflicting[field] = json!("different");
            let prepared = fact(
                2,
                1,
                "add.rankingprepared",
                json!({"document": conflicting}),
            );
            assert_that!(
                unresolved(&[initial.clone(), prepared], &ranking)?.len(),
                eq(1)
            );
        }
        let mut missing_context = expected.clone();
        missing_context
            .as_object_mut()
            .unwrap()
            .remove("traceparent");
        let prepared = fact(
            2,
            1,
            "add.rankingprepared",
            json!({"document": missing_context}),
        );
        assert_that!(unresolved(&[initial, prepared], &ranking)?.len(), eq(1));
        Ok(())
    }

    #[googletest::test]
    fn rejection_excludes_saved_and_unresolved_actions_before_document_validation()
    -> anyhow::Result<()> {
        let expected = document();
        let ranking = [book_smartz_domain::decode_event(&expected.to_string())?];
        let history = [
            fact(1, 1, "add.intent", intent(1)),
            fact(2, 2, "preference.intent", intent(2)),
            fact(
                3,
                1,
                "preference.rankingprepared",
                json!({"document": expected}),
            ),
            fact(
                4,
                2,
                "add.rankingprepared",
                json!({"document": {"id": "invalid"}}),
            ),
            fact(
                5,
                1,
                "add.rejected",
                json!({"reason": "saved and rejected"}),
            ),
            fact(6, 2, "preference.rejected", json!({"reason": "rejected"})),
        ];
        assert_that!(unresolved(&history, &ranking)?.len(), eq(0));
        Ok(())
    }

    #[googletest::test]
    fn recognized_intents_and_rejections_validate_across_the_entire_history() -> anyhow::Result<()>
    {
        for malformed in [
            fact(2, 2, "preference.intent", json!({})),
            fact(2, 2, "preference.rejected", json!({"reason": false})),
            fact(2, 1, "add.intent", json!({})),
        ] {
            assert_that!(
                unresolved(&[fact(1, 1, "add.intent", intent(1)), malformed], &[]).is_err(),
                eq(true)
            );
        }
        // A later bad rejection still fails, even if an earlier rejection excluded the action.
        let rejected = fact(2, 1, "add.rejected", json!({"reason": "first"}));
        let malformed = fact(3, 1, "preference.rejected", json!({"reason": false}));
        let error = unresolved(
            &[fact(1, 1, "add.intent", intent(1)), rejected, malformed],
            &[],
        )
        .err()
        .unwrap();
        assert_that!(error.to_string(), eq("Invalid rejection"));
        Ok(())
    }

    #[googletest::test]
    fn prepared_identity_validation_waits_for_an_unrejected_intent() -> anyhow::Result<()> {
        for document in [json!({}), json!({"id": "invalid"})] {
            let unrelated = fact(
                2,
                2,
                "preference.rankingprepared",
                json!({"document": document}),
            );
            assert_that!(
                unresolved(
                    &[fact(1, 1, "add.intent", intent(1)), unrelated.clone()],
                    &[]
                )?
                .len(),
                eq(1)
            );
            assert_that!(
                unresolved(&[fact(1, 2, "add.intent", intent(2)), unrelated], &[]).is_err(),
                eq(true)
            );
        }
        // History validation completes before any prepared document identity is decoded.
        let history = [
            fact(1, 1, "add.intent", intent(1)),
            fact(2, 1, "add.rankingprepared", json!({"document": {}})),
            fact(3, 2, "add.rejected", json!({"reason": false})),
        ];
        let error = unresolved(&history, &[]).err().unwrap();
        assert_that!(error.to_string(), eq("Invalid rejection"));
        Ok(())
    }
}
