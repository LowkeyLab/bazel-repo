//! Throwaway library consumer for the Topcoat Bazel gate.
use topcoat::{
    Result,
    asset::asset,
    font::{Font, font},
    icon::{icon, iconify},
    view::{View, component, view},
};

pub const FIXTURE_FONT: Font = font! {
    "Fixture Roboto",
    @font-face {
        src: url(asset!("./fixture_assets/fixture.woff2")) format("woff2");
        font-weight: 400;
        font-display: swap;
    }
};

iconify::include!("feather");

#[component]
pub async fn library_panel() -> Result<impl View> {
    Ok(view! {
        <section class="text-emerald-700">
            topcoat::font::link(font: FIXTURE_FONT)
            <h1>"Topcoat compatibility fixture"</h1>
            icon(data: feather::FEATHER, label: "Fixture feather")
            <a href=(asset!("./library.txt"))>"Library asset"</a>
        </section>
    })
}
