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

/// Lazy interpretation for one retained correlation. Construction never decodes payloads.
pub(super) struct SingleAction<'a> {
    facts: Vec<&'a Value>,
}

pub(super) enum Recovery<'a> {
    Missing,
    Saved,
    Rejected(String),
    Prepared {
        original: Box<Intent>,
        event: &'a Value,
        document: Box<CloudEventDocument>,
    },
    Pending(Box<Intent>),
}

impl<'a> SingleAction<'a> {
    pub(super) fn new(history: &'a [ApplicationEvent], action: ActionId) -> Self {
        Self {
            facts: history
                .iter()
                .map(ApplicationEvent::envelope)
                .filter(|event| event["correlationid"] == action.text())
                .collect(),
        }
    }

    fn first(&self, wanted: Stage) -> Option<&'a Value> {
        self.facts
            .iter()
            .copied()
            .find(|event| stage(event) == Some(wanted))
    }

    pub(super) fn intent(&self) -> anyhow::Result<Option<Intent>> {
        self.first(Stage::Intent)
            .map(|event| serde_json::from_value(event["data"].clone()).map_err(Into::into))
            .transpose()
    }

    pub(super) fn registration_prepared(&self) -> Option<&'a Value> {
        self.first(Stage::RegistrationPrepared)
    }

    pub(super) fn first_fact_id(&self) -> String {
        self.facts[0]["id"].as_str().unwrap().to_owned()
    }

    fn rejection(&self) -> Option<String> {
        self.first(Stage::Rejected)
            .and_then(|event| event["data"]["reason"].as_str())
            .map(str::to_owned)
    }

    pub(super) fn has_ranking_decision(&self) -> bool {
        self.first(Stage::RankingPrepared).is_some() || self.rejection().is_some()
    }

    pub(super) fn recovery(&self, ranking: &[CloudEventDocument]) -> anyhow::Result<Recovery<'a>> {
        let Some(original) = self.intent()? else {
            return Ok(Recovery::Missing);
        };
        let prepared = self.first(Stage::RankingPrepared);
        if let Some(expected) = prepared {
            for document in ranking {
                if document.event().metadata().id.as_uuid().to_string()
                    == expected["data"]["document"]["id"].as_str().unwrap_or("")
                {
                    if matches_document(document, &expected["data"]["document"])? {
                        return Ok(Recovery::Saved);
                    }
                    break;
                }
            }
        }
        if let Some(reason) = self.rejection() {
            return Ok(Recovery::Rejected(reason));
        }
        if let Some(event) = prepared {
            let document =
                book_smartz_domain::decode_event(&event["data"]["document"].to_string())?;
            anyhow::ensure!(
                !ranking
                    .iter()
                    .any(|d| d.event().metadata().id == document.event().metadata().id),
                "conflicting durable evidence"
            );
            return Ok(Recovery::Prepared {
                original: Box::new(original),
                event,
                document: Box::new(document),
            });
        }
        Ok(Recovery::Pending(Box::new(original)))
    }
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

    #[googletest::test]
    fn single_action_saved_precedes_rejection_and_requires_a_correlated_intent()
    -> anyhow::Result<()> {
        let expected = document();
        let ranking = [book_smartz_domain::decode_event(&expected.to_string())?];
        for reason in [json!("rejected"), json!(false)] {
            let history = [
                fact(1, 1, "preference.intent", intent(1)),
                fact(2, 1, "add.rankingprepared", json!({"document": expected})),
                fact(3, 1, "preference.rejected", json!({"reason": reason})),
                fact(4, 2, "add.rejected", json!({"reason": false})),
            ];
            let evidence = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)));
            assert_that!(
                matches!(evidence.recovery(&ranking)?, super::Recovery::Saved),
                eq(true)
            );
            let no_intent =
                super::SingleAction::new(&history[1..], super::ActionId(Uuid::from_u128(1)));
            assert_that!(
                matches!(no_intent.recovery(&ranking)?, super::Recovery::Missing),
                eq(true)
            );
        }
        Ok(())
    }

    #[googletest::test]
    fn single_action_first_rejection_controls_prepared_validation() -> anyhow::Result<()> {
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
            let evidence = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)));
            let super::Recovery::Rejected(reason) = evidence.recovery(&[])? else {
                anyhow::bail!("expected rejection");
            };
            assert_that!(reason, eq("first rejection"));
            let mut malformed_first = history.clone();
            malformed_first.swap(2, 3);
            let evidence =
                super::SingleAction::new(&malformed_first, super::ActionId(Uuid::from_u128(1)));
            assert_that!(evidence.has_ranking_decision(), eq(true));
            assert_that!(evidence.recovery(&[]).is_err(), eq(true));
        }
        let history = [
            fact(1, 1, "add.intent", intent(1)),
            fact(2, 1, "add.rejected", json!({"reason": false})),
            fact(3, 1, "preference.rejected", json!({"reason": "later"})),
        ];
        let evidence = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)));
        assert_that!(evidence.has_ranking_decision(), eq(false));
        assert_that!(
            matches!(evidence.recovery(&[])?, super::Recovery::Pending(_)),
            eq(true)
        );
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
        let evidence = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)));
        let super::Recovery::Prepared {
            original,
            event,
            document: retained,
        } = evidence.recovery(&[])?
        else {
            anyhow::bail!("expected prepared action");
        };
        assert_that!(original.event, eq(Uuid::from_u128(99)));
        assert_that!(event["id"], eq(&json!(Uuid::from_u128(2))));
        assert_that!(
            serde_json::from_str::<Value>(&book_smartz_domain::encode_document(&retained)?)?,
            eq(&expected)
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
            let evidence = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)));
            assert_that!(
                matches!(evidence.recovery(&ranking), Ok(super::Recovery::Saved)),
                eq(false)
            );
        }
        let mut conflict = expected.clone();
        conflict["retainedextension"] = json!("different");
        let history = [
            history[0].clone(),
            fact(2, 1, "add.rankingprepared", json!({"document": conflict})),
        ];
        let error = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)))
            .recovery(&ranking)
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
        let evidence = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)));
        assert_that!(
            evidence.registration_prepared().unwrap()["data"]["bookId"],
            eq(&json!(Uuid::from_u128(80)))
        );
        assert_that!(
            evidence.first_fact_id(),
            eq(&Uuid::from_u128(1).to_string())
        );
        assert_that!(evidence.intent().is_err(), eq(true));
        assert_that!(evidence.recovery(&[]).is_err(), eq(true));
        Ok(())
    }

    #[googletest::test]
    fn single_action_uses_first_correlated_intent_without_validating_other_actions()
    -> anyhow::Result<()> {
        let history = [
            fact(1, 2, "add.intent", json!({})),
            fact(2, 1, "preference.intent", intent(9)),
            fact(3, 1, "add.intent", json!({})),
        ];
        let evidence = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(1)));
        let original = evidence.intent()?.unwrap();
        assert_that!(original.action.text(), eq(&Uuid::from_u128(9).to_string()));
        assert_that!(original.event, eq(Uuid::from_u128(99)));
        assert_that!(original.revision, eq(0));
        assert_that!(original.time.to_rfc3339(), eq("2026-01-01T12:00:00+00:00"));
        assert_that!(
            evidence.first_fact_id(),
            eq(&Uuid::from_u128(2).to_string())
        );
        let missing = super::SingleAction::new(&history, super::ActionId(Uuid::from_u128(3)));
        assert_that!(
            matches!(missing.recovery(&[])?, super::Recovery::Missing),
            eq(true)
        );
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
