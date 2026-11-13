//! One App-authenticated remote history, bound by private payload digests.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    Result,
    model::{AppIdentity, Comment, Event, Identity, IssueKey},
    require,
};

pub const PREFIX: &str = "software-factory-history:v1\n";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: Identity,
    pub issue: IssueKey,
    pub sequence: usize,
    pub previous_digest: Option<String>,
    pub event: Event,
}
impl Record {
    ///
    /// # Errors
    ///
    /// Returns an error if the record cannot be serialized as JSON.
    pub fn body(&self) -> Result<String> {
        Ok(format!("{PREFIX}{}", serde_json::to_string(self)?))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Anchor {
    pub comment: String,
    pub record: Identity,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingRecord {
    pub issue: IssueKey,
    pub record: Identity,
    pub body: String,
    pub attempted: bool,
    pub comment: Option<String>,
}
#[must_use]
pub fn digest(body: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(body.as_bytes())
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 0x0f)]),
            ]
        })
        .collect()
}
/// Complete reads are required by the GitHub boundary. Extra structured records are not silently adopted.
///
/// # Errors
///
/// Returns an error for invalid structured records, wrong targets or authors,
/// duplicate identities, broken continuity, changed anchors, or unproven pending history.
pub fn validate(
    comments: &[Comment],
    issue: &IssueKey,
    app: &AppIdentity,
    anchors: &[Anchor],
    pending: Option<&PendingRecord>,
) -> Result<Vec<Record>> {
    if let Some(pending) = pending
        && let Some(id) = &pending.comment
    {
        require(
            comments.iter().any(|comment| {
                &comment.id == id
                    && comment.issue == pending.issue
                    && comment.app.as_ref() == Some(app)
                    && comment.body == pending.body
            }),
            "known pending history missing or modified",
        )?;
    }
    let mut records: Vec<(Record, String)> = Vec::new();
    let mut comment_ids = BTreeSet::new();
    let mut record_ids = BTreeSet::new();
    for comment in comments {
        require(
            &comment.issue == issue && comment_ids.insert(&comment.id),
            "wrong history target or duplicate comment",
        )?;
        if !comment.body.starts_with(PREFIX) {
            require(
                !anchors.iter().any(|a| a.comment == comment.id),
                "anchored history marker edited",
            )?;
            continue;
        }
        require(
            comment.app.as_ref() == Some(app),
            "forged history author or App",
        )?;
        let record: Record = serde_json::from_str(&comment.body[PREFIX.len()..])?;
        let position = records.len();
        require(
            &record.issue == issue
                && record.sequence == position
                && record_ids.insert(record.id.clone()),
            "contradictory history identity or order",
        )?;
        let previous = records.last().map(|(_, body)| digest(body));
        require(
            record.previous_digest == previous,
            "broken history continuity",
        )?;
        if let Some(anchor) = anchors.get(position) {
            require(
                anchor.comment == comment.id
                    && anchor.record == record.id
                    && anchor.digest == digest(&comment.body),
                "edited or replaced anchored history",
            )?;
        } else {
            let pending =
                pending.ok_or_else(|| crate::Error::Stop("unanchored remote history".into()))?;
            require(
                position == anchors.len()
                    && pending.issue == *issue
                    && pending.record == record.id
                    && pending.body == comment.body
                    && pending.comment.as_ref().is_none_or(|id| id == &comment.id),
                "unexpected pending history",
            )?;
        }
        records.push((record, comment.body.clone()));
    }
    require(
        records.len() >= anchors.len()
            && records.len() <= anchors.len() + usize::from(pending.is_some()),
        "missing or contradictory history segment",
    )?;
    Ok(records.into_iter().map(|(r, _)| r).collect())
}
