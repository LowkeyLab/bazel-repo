//! Bounded live discovery, deployment-shared admission and reader-bound selections.
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        content::Form,
        error::{bad_request, forbidden},
        page,
    },
    view::{View, view},
};

use book_smartz_domain::{OpenLibraryWorkId, ReaderId};

use crate::diagnostic_events::{OperationOutcome, Reason, Stage};
use crate::{Application, authorize};

mod metadata;

pub use metadata::migrate as migrate_cache;
use metadata::{Cache, Failure, Provider, admit, valid_text};
pub use metadata::{CatalogWork, LookupFailure};

pub struct Catalog {
    provider: Provider,
    cache: Cache,
    signing_key: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    iss: String,
    aud: String,
    reader: uuid::Uuid,
    iat: i64,
    exp: i64,
    work: CatalogWork,
}

#[derive(Serialize, Deserialize)]
struct Offer {
    reader: uuid::Uuid,
    action: crate::actions::ActionId,
    selection: String,
    revision: u64,
    aud: String,
}
impl Catalog {
    /// Looks up metadata without changing a reader's saved state.
    ///
    /// # Errors
    /// Returns a bounded failure when neither cache nor provider supplies usable metadata.
    #[allow(
        dead_code,
        reason = "This application API intentionally has no reader route."
    )]
    pub async fn lookup(
        &self,
        reader: ReaderId,
        key: &OpenLibraryWorkId,
    ) -> std::result::Result<CatalogWork, LookupFailure> {
        self.cache.lookup(&self.provider, reader, key).await
    }

    pub fn offer(
        &self,
        reader: ReaderId,
        action: crate::actions::ActionId,
        selection: &str,
        revision: u64,
    ) -> anyhow::Result<String> {
        Ok(encode(
            &Header::new(Algorithm::HS256),
            &Offer {
                reader: *reader.as_uuid(),
                action,
                selection: selection.to_owned(),
                revision,
                aud: "book-smartz-offer".into(),
            },
            &EncodingKey::from_secret(&self.signing_key),
        )?)
    }
    pub fn verify_offer(
        &self,
        reader: ReaderId,
        action: crate::actions::ActionId,
        selection: &str,
        revision: u64,
        token: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(token.len() <= 131_072, "invalid offer");
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        validation.required_spec_claims.clear();
        validation.set_audience(&["book-smartz-offer"]);
        let offer = decode::<Offer>(
            token,
            &DecodingKey::from_secret(&self.signing_key),
            &validation,
        )?
        .claims;
        anyhow::ensure!(
            offer.reader == *reader.as_uuid()
                && offer.action == action
                && offer.selection == selection
                && offer.revision == revision,
            "invalid offer"
        );
        Ok(())
    }

    pub fn from_environment(pool: PgPool) -> anyhow::Result<Self> {
        let endpoint = url::Url::parse(
            &std::env::var("OPEN_LIBRARY_SEARCH_ENDPOINT")
                .unwrap_or_else(|_| "https://openlibrary.org/search.json".into()),
        )?;
        anyhow::ensure!(
            endpoint.scheme() == "https"
                || (endpoint.scheme() == "http"
                    && matches!(endpoint.host_str(), Some("127.0.0.1" | "[::1]"))),
            "catalog endpoint must use HTTPS or explicit loopback"
        );
        anyhow::ensure!(
            endpoint.username().is_empty()
                && endpoint.password().is_none()
                && endpoint.fragment().is_none()
                && endpoint.query().is_none(),
            "invalid catalog endpoint"
        );
        let contact = std::env::var("BOOK_SMARTZ_MAINTAINER_CONTACT")?;
        anyhow::ensure!(valid_text(&contact, 256), "maintainer contact is required");
        let signing_key = std::env::var("BOOK_SMARTZ_SELECTION_KEY")?.into_bytes();
        anyhow::ensure!(
            signing_key.len() >= 32,
            "selection key must contain at least 32 bytes"
        );
        Ok(Self {
            cache: Cache::new(pool),
            signing_key,
            provider: Provider::new(endpoint, &contact)?,
        })
    }
    fn sign(&self, reader: ReaderId, work: CatalogWork) -> jsonwebtoken::errors::Result<String> {
        let now = chrono::Utc::now().timestamp();
        encode(
            &Header::new(Algorithm::HS256),
            &Selection {
                iss: "book-smartz-selection".into(),
                aud: "book-smartz-add".into(),
                reader: *reader.as_uuid(),
                iat: now,
                exp: now + 900,
                work,
            },
            &EncodingKey::from_secret(&self.signing_key),
        )
    }
    /// Verifies the complete retained selection locally; never consults the provider.
    pub fn verify(&self, reader: ReaderId, token: &str) -> anyhow::Result<CatalogWork> {
        anyhow::ensure!(token.len() <= 65536, "invalid selection");
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        validation.set_issuer(&["book-smartz-selection"]);
        validation.set_audience(&["book-smartz-add"]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let selection = decode::<Selection>(
            token,
            &DecodingKey::from_secret(&self.signing_key),
            &validation,
        )?
        .claims;
        let now = chrono::Utc::now().timestamp();
        anyhow::ensure!(
            selection.reader == *reader.as_uuid()
                && selection.iat <= now
                && selection.exp > now
                && selection.exp.checked_sub(selection.iat) == Some(900)
                && selection.work.valid(),
            "invalid selection"
        );
        Ok(selection.work)
    }
    async fn discover(
        &self,
        pool: &PgPool,
        diagnostic: &mut crate::diagnostics::Operation,
        reader: ReaderId,
        query: &str,
        page: u32,
    ) -> Result<Discovery> {
        diagnostic.stage(Stage::Admission);
        let admitted = match admit(pool, reader).await {
            Ok(admitted) => admitted,
            Err(error) => {
                diagnostic.database_failure(&error);
                return Err(error.into());
            }
        };
        let result = if admitted {
            diagnostic.stage(Stage::Provider);
            self.provider.search(query, page).await
        } else {
            diagnostic.finish(OperationOutcome::Rejected, Reason::AdmissionLimited);
            Err(Failure::Limited)
        };
        let mut results = Vec::new();
        let mut message = None;
        let mut more = false;
        match result {
            Ok(found) => {
                diagnostic.stage(Stage::Selection);
                more = found.more && page < 1_000_000;
                if found.works.is_empty() {
                    message = Some(if found.no_matches {
                        "No matches found."
                    } else {
                        "No selectable works returned."
                    });
                }
                for work in self.cache.retain(found.works).await {
                    let token = match self.sign(reader, work.clone()) {
                        Ok(token) => token,
                        Err(error) => {
                            diagnostic.finish(
                                OperationOutcome::DependencyFailure,
                                Reason::SelectionPreparation,
                            );
                            return Err(error.into());
                        }
                    };
                    results.push((work, token));
                }
                diagnostic.finish(OperationOutcome::Confirmed, Reason::None);
            }
            Err(error) => {
                if admitted {
                    diagnostic.finish(
                        if matches!(error, Failure::Limited) {
                            OperationOutcome::Rejected
                        } else {
                            OperationOutcome::DependencyFailure
                        },
                        match error {
                            Failure::Limited => Reason::ProviderLimited,
                            Failure::Unavailable => Reason::Unavailable,
                            Failure::Timeout => Reason::Timeout,
                            Failure::Malformed => Reason::Malformed,
                            Failure::Oversized => Reason::Oversized,
                        },
                    );
                }
                message = Some(error.message());
            }
        }
        Ok(Discovery {
            results,
            message,
            more,
        })
    }
}
struct Discovery {
    results: Vec<(CatalogWork, String)>,
    message: Option<&'static str>,
    more: bool,
}
#[derive(Deserialize)]
pub struct SearchInput {
    csrf: String,
    query: String,
    page: u32,
}
#[page(POST "/search")]
pub async fn search(cx: &Cx, Form(input): Form<SearchInput>) -> Result<impl View> {
    let session = authorize(cx).await?;
    let app: &Application = app_context(cx);
    crate::require_post(cx, &session, &input.csrf)?;
    let mut diagnostic = app.diagnostics.catalog_search();
    if !valid_text(&input.query, 256) || !(1..=1_000_000).contains(&input.page) {
        diagnostic.finish(OperationOutcome::Rejected, Reason::InvalidInput);
        return Err(bad_request("Enter 1–256 search characters and a valid page").into());
    }
    let Discovery {
        results,
        message,
        more,
    } = app
        .catalog
        .discover(
            &app.pool,
            &mut diagnostic,
            session.reader,
            &input.query,
            input.page,
        )
        .await?;
    let next_page = input.page + 1;
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                book_smartz_views::assets()
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Search · Book Smartz"</title>
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./submitting.js"))></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="library-workspace">
                    <section class="active-task" aria-label="Book ranking workspace">
                        <h1 tabindex="-1" data-response-focus="task">"Search books"</h1>
                        <a href="/ranking">"Your saved ranking"</a>
                        if let Some(message) = message {
                            book_smartz_views::feedback(
                                message: message.to_owned(),
                                kind: book_smartz_views::FeedbackKind::Error
                            )
                        }
                        <form method="post" action="/search">
                            <input type="hidden" name="csrf" value=(&session.csrf)>
                            <label>
                                "Title or author"
                                <input
                                    name="query"
                                    maxlength="256"
                                    required=""
                                    value=(&input.query)
                                >
                            </label>
                            <input
                                type="hidden"
                                name="page"
                                value=(input.page.to_string())
                            >
                            <button type="submit">"Retry search"</button>
                        </form>
                        <ul class="search-results" aria-label="Search results">
                            for (work, token) in results {
                                result_card(
                                    work: work,
                                    token: token,
                                    csrf: session.csrf.clone()
                                )
                            }
                        </ul>
                        if input.page > 1 {
                            <form method="post" action="/search">
                                <input type="hidden" name="csrf" value=(&session.csrf)>
                                <input type="hidden" name="query" value=(&input.query)>
                                <input
                                    type="hidden"
                                    name="page"
                                    value=((input.page - 1).to_string())
                                >
                                <button type="submit">"Previous page"</button>
                            </form>
                        }
                        if more {
                            <form method="post" action="/search">
                                <input type="hidden" name="csrf" value=(&session.csrf)>
                                <input type="hidden" name="query" value=(&input.query)>
                                <input
                                    type="hidden"
                                    name="page"
                                    value=(next_page.to_string())
                                >
                                <button type="submit">"Next page"</button>
                            </form>
                        }
                    </section>
                    crate::saved_ranking(
                        reader: session.reader,
                        completion: None,
                        diagnostic: None
                    )
                </main>
            </body>
        </html>
    })
}
#[derive(Deserialize)]
pub struct SelectInput {
    csrf: String,
    selection: String,
}
#[page(POST "/select")]
pub async fn select(cx: &Cx, Form(input): Form<SelectInput>) -> Result<impl View> {
    let session = authorize(cx).await?;
    crate::require_post(cx, &session, &input.csrf)?;
    let app: &Application = app_context(cx);
    let work = app
        .catalog
        .verify(session.reader, &input.selection)
        .map_err(|_| forbidden())?;
    let ranking = app
        .store
        .load_ranking(
            session.reader,
            book_smartz_storage::OperationContext::default(),
        )
        .await?;
    let revision = ranking.projection().revision();
    let blocked = ranking.projection().pending().is_some()
        || crate::actions::checking(app, session.reader)
            .await
            .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
    let gate = if blocked {
        Some(
            "Finish your unfinished comparison or check your earlier save before adding another book.",
        )
    } else {
        None
    };
    let action = crate::actions::ActionId::new();
    let offer = app
        .catalog
        .offer(session.reader, action, &input.selection, revision)
        .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                book_smartz_views::assets()
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Selected work · Book Smartz"</title>
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./submitting.js"))></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="library-workspace">
                    <section class="active-task" aria-label="Book ranking workspace">
                        <h1 tabindex="-1" data-response-focus="task">
                            "Selected work"
                        </h1>
                        book_smartz_views::book(
                            display: presentation(&work),
                            compact: false
                        )
                        <p>"This selection expires 15 minutes after search."</p>
                        if let Some(message) = gate {
                            book_smartz_views::feedback(
                                message: message.to_owned(),
                                kind: book_smartz_views::FeedbackKind::Error
                            )
                        }
                        <form method="post" action="/add" data-ranking-action="">
                            <input type="hidden" name="csrf" value=(&session.csrf)>
                            <input
                                type="hidden"
                                name="selection"
                                value=(&input.selection)
                            >
                            <input type="hidden" name="action" value=(action.text())>
                            <input
                                type="hidden"
                                name="revision"
                                value=(revision.to_string())
                            >
                            <input type="hidden" name="offer" value=(offer)>
                            <button type="submit" disabled=(blocked)>
                                "Add to your ranking"
                            </button>
                            book_smartz_views::submission_status()
                        </form>
                        <a href="/ranking">"Return to your ranking"</a>
                    </section>
                    crate::saved_ranking(
                        reader: session.reader,
                        completion: None,
                        diagnostic: None
                    )
                </main>
            </body>
        </html>
    })
}

#[topcoat::view::component]
async fn result_card(work: CatalogWork, token: String, csrf: String) -> Result<impl View> {
    Ok(view! {
        <li>
            book_smartz_views::book(display: presentation(&work), compact: false)
            if let Some(year) = work.year {
                <p>(year.to_string())</p>
            }
            <form method="post" action="/select">
                <input type="hidden" name="csrf" value=(csrf)>
                <input type="hidden" name="selection" value=(token)>
                <button type="submit">"Select this work"</button>
            </form>
        </li>
    })
}

fn presentation(work: &CatalogWork) -> book_smartz_views::BookDisplay {
    book_smartz_views::BookDisplay {
        title: work.title.clone(),
        author: work.author_label(),
        cover: work.cover_url(),
        source: work.source(),
    }
}
