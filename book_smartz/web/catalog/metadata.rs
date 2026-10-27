//! Validated provider metadata and bounded live search, independent of reader selections.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use book_smartz_domain::OpenLibraryWorkId;

pub(super) struct Provider {
    pub(super) client: reqwest::Client,
    pub(super) endpoint: url::Url,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogWork {
    pub work_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub(super) year: Option<i32>,
    pub cover: Option<u64>,
}
impl CatalogWork {
    pub(super) fn valid(&self) -> bool {
        self.work_id.len() <= 64
            && OpenLibraryWorkId::try_from(self.work_id.as_str()).is_ok()
            && valid_text(&self.title, 1024)
            && self.authors.len() <= 20
            && self.authors.iter().all(|author| valid_text(author, 256))
            && self.year.is_none_or(|year| (-5000..=10000).contains(&year))
            && self
                .cover
                .is_none_or(|cover| cover > 0 && i64::try_from(cover).is_ok())
    }
    pub fn source(&self) -> String {
        format!("https://openlibrary.org/works/{}", self.work_id)
    }
    pub fn cover_url(&self) -> Option<String> {
        self.cover
            .map(|id| format!("https://covers.openlibrary.org/b/id/{id}-M.jpg?default=false"))
    }
    pub fn author_label(&self) -> String {
        if self.authors.is_empty() {
            "Author unavailable".into()
        } else {
            self.authors.join(", ")
        }
    }
}
pub(super) fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= maximum
        && !value.chars().any(char::is_control)
}
impl Provider {
    pub(super) async fn search(
        &self,
        query: &str,
        page: u32,
    ) -> std::result::Result<SearchPage, Failure> {
        let page = page.to_string();
        let mut endpoint = self.endpoint.clone();
        endpoint.query_pairs_mut().extend_pairs([
            ("q", query),
            ("page", page.as_str()),
            ("limit", "20"),
            ("fields", "key,title,author_name,first_publish_year,cover_i"),
        ]);
        let mut response = self
            .client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| transport(&error))?;
        if response.status().as_u16() == 429 {
            return Err(Failure::Limited);
        }
        if !response.status().is_success() {
            return Err(Failure::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|size| size > 1_048_576)
        {
            return Err(Failure::Oversized);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| transport(&error))? {
            if bytes.len() + chunk.len() > 1_048_576 {
                return Err(Failure::Oversized);
            }
            bytes.extend_from_slice(&chunk);
        }
        let raw: ProviderPage = serde_json::from_slice(&bytes).map_err(|_| Failure::Malformed)?;
        if raw.docs.len() > 20 {
            return Err(Failure::Malformed);
        }
        let num_found = match (raw.num_found, raw.num_found_camel_case) {
            (Some(count), None) | (None, Some(count)) => count,
            (Some(count), Some(other)) if count == other => count,
            _ => return Err(Failure::Malformed),
        };
        let mut seen = HashSet::new();
        let mut works = Vec::new();
        for doc in raw.docs {
            let Ok(doc) = serde_json::from_value::<ProviderWork>(doc) else {
                continue;
            };
            let id = doc
                .key
                .strip_prefix("/works/")
                .unwrap_or(&doc.key)
                .to_owned();
            let work = CatalogWork {
                work_id: id,
                title: doc.title,
                authors: doc.author_name,
                year: doc.first_publish_year,
                cover: doc.cover_i,
            };
            if work.valid() && seen.insert(work.work_id.clone()) {
                works.push(work);
            }
        }
        Ok(SearchPage {
            works,
            more: raw.start.checked_add(20).is_some_and(|end| end < num_found),
            no_matches: num_found == 0,
        })
    }
}
#[derive(Deserialize)]
struct ProviderPage {
    num_found: Option<u64>,
    #[serde(rename = "numFound")]
    num_found_camel_case: Option<u64>,
    start: u64,
    docs: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
struct ProviderWork {
    key: String,
    title: String,
    #[serde(default)]
    author_name: Vec<String>,
    first_publish_year: Option<i32>,
    cover_i: Option<u64>,
}
pub(super) struct SearchPage {
    pub(super) works: Vec<CatalogWork>,
    pub(super) more: bool,
    pub(super) no_matches: bool,
}
pub(super) enum Failure {
    Limited,
    Unavailable,
    Timeout,
    Malformed,
    Oversized,
}
impl Failure {
    pub(super) const fn message(&self) -> &'static str {
        match self {
            Self::Limited => "Search rate limit reached. Please retry shortly.",
            Self::Unavailable => "Search is unavailable. Please retry.",
            Self::Timeout => "Search timed out. Please retry.",
            Self::Malformed => "Search returned a malformed response. Please retry.",
            Self::Oversized => "Search response was too large. Please retry.",
        }
    }
}
fn transport(error: &reqwest::Error) -> Failure {
    if error.is_timeout() {
        Failure::Timeout
    } else {
        Failure::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use googletest::{assert_that, matchers::eq};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::{Failure, Provider};

    #[googletest::test]
    #[tokio::test]
    async fn provider_count_spellings_preserve_pagination_and_empty_results() -> anyhow::Result<()>
    {
        for (counts, page, more, empty) in [
            (json!({"numFound": 42}), 2, true, false),
            (json!({"num_found": 42}), 2, true, false),
            (json!({"numFound": 42, "num_found": 42}), 2, true, false),
            (json!({"numFound": 0, "num_found": 0}), 1, false, true),
            (json!({"numFound": 20, "num_found": 20}), 1, false, false),
        ] {
            let provider = MockServer::start().await;
            let mut response = counts;
            response["start"] = json!((page - 1) * 20);
            response["docs"] = json!([]);
            Mock::given(method("GET"))
                .and(path("/search.json"))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .mount(&provider)
                .await;
            let catalog = Provider {
                client: reqwest::Client::builder().no_proxy().build()?,
                endpoint: format!("{}/search.json", provider.uri()).parse()?,
            };
            let found = catalog
                .search("count compatibility", page)
                .await
                .ok()
                .context("Valid provider counts must return a search page")?;
            assert_that!(found.more, eq(more));
            assert_that!(found.no_matches, eq(empty));
        }
        Ok(())
    }

    #[googletest::test]
    #[tokio::test]
    async fn missing_or_conflicting_provider_counts_reject_the_response() -> anyhow::Result<()> {
        for mut response in [json!({}), json!({"numFound": 42, "num_found": 43})] {
            let provider = MockServer::start().await;
            response["start"] = json!(0);
            response["docs"] = json!([]);
            Mock::given(method("GET"))
                .and(path("/search.json"))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .mount(&provider)
                .await;
            let catalog = Provider {
                client: reqwest::Client::builder().no_proxy().build()?,
                endpoint: format!("{}/search.json", provider.uri()).parse()?,
            };
            assert_that!(
                matches!(
                    catalog.search("count compatibility", 1).await,
                    Err(Failure::Malformed)
                ),
                eq(true)
            );
        }
        Ok(())
    }
}
