//! Shared Book Smartz presentation assets, independent of application routing.
use topcoat::{
    Result,
    asset::asset,
    font::{Font, font},
    icon::{icon, iconify},
    view::{View, component, view},
};
pub const FONT: Font = font! {
    "Book Smartz",
    @font-face {
        src: url(asset!("./local/font.woff2")) format("woff2");
        font-weight: 400;
        font-display: swap;
    }
};
iconify::include!("feather");
#[component]
pub async fn heading() -> Result<impl View> {
    Ok(view! {
        <header class="border-b border-stone-200 p-6">
            topcoat::font::link(font: FONT)
            icon(data: feather::BOOK_OPEN, label: "Book Smartz")
            <a href="/ranking">"Book Smartz"</a>
        </header>
    })
}
