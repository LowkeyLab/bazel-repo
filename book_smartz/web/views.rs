//! Shared presentation accepts display data, without application orchestration.
use topcoat::{
    Result,
    asset::asset,
    font::{Font, font},
    icon::{icon, iconify},
    view::{View, component, view},
};
pub const FONT: Font = font! {
    "Source Sans 3",
    @font-face {
        src: url(asset!("./fonts/source-sans-3.ttf")) format("truetype");
        font-weight: 200 900;
        font-display: swap;
    }
};
pub const HEADING_FONT: Font = font! {
    "Literata",
    @font-face {
        src: url(asset!("./fonts/literata.ttf")) format("truetype");
        font-weight: 200 900;
        font-display: swap;
    }
};
#[derive(Clone)]
pub struct BookDisplay {
    pub title: String,
    pub author: String,
    pub cover: Option<String>,
    pub source: String,
}
iconify::include!("feather");
#[component]
pub async fn heading() -> Result<impl View> {
    Ok(view! {
        <header class="library-header">
            <a class="brand" href="/ranking">
                "Book Smartz"
                icon(data: feather::BOOK_OPEN, label: "Your private library")
            </a>
            <nav aria-label="Library">
                <a href="/ranking">"Find a book"</a>
                <a href="/ranking#saved-ranking">"Your ranking"</a>
            </nav>
        </header>
    })
}
#[component]
pub async fn cover(url: Option<String>, compact: bool) -> Result<impl View> {
    let width = if compact { "48" } else { "160" };
    let height = if compact { "72" } else { "240" };
    Ok(view! {
        <div class=(if compact { "book-cover compact" } else { "book-cover" })>
            <img
                src=(asset!("./cover-placeholder.svg"))
                alt="Cover unavailable"
                width=(width)
                height=(height)
            >
            if let Some(cover) = url {
                <object
                    data=(cover)
                    type="image/jpeg"
                    aria-label="Representative book cover"
                >
                    <img
                        src=(asset!("./cover-placeholder.svg"))
                        alt="Cover unavailable"
                        width=(width)
                        height=(height)
                    >
                </object>
            }
        </div>
    })
}
#[component]
pub async fn book(display: BookDisplay, compact: bool) -> Result<impl View> {
    Ok(view! {
        <article class=(if compact { "book-summary compact" } else { "book-summary" })>
            cover(url: display.cover, compact: compact)
            <div class="book-copy">
                <a class="book-title" href=(display.source)>(display.title)</a>
                <p class="book-author">(display.author)</p>
            </div>
        </article>
    })
}
#[component]
pub async fn feedback(message: String) -> Result<impl View> {
    Ok(view! {
        <p
            class="inline-feedback"
            role="status"
            tabindex="-1"
            data-response-focus="error"
        >
            (message)
        </p>
    })
}

#[component]
pub async fn assets() -> Result<impl View> {
    Ok(view! {
        topcoat::font::link(font: FONT)
        topcoat::font::link(font: HEADING_FONT)
        <link
            rel="license"
            href=(asset!("./fonts/literata-OFL.txt"))
            title="Literata font license"
        >
        <link
            rel="license"
            href=(asset!("./fonts/source-sans-3-OFL.txt"))
            title="Source Sans 3 font license"
        >
    })
}

#[component]
pub async fn submission_status() -> Result<impl View> {
    Ok(view! {
        <p
            class="submission-status"
            role="status"
            aria-live="polite"
            aria-atomic="true"
            data-submitting=""
        ></p>
    })
}
