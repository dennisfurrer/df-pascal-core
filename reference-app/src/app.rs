//! Root component: top bar, market rail, routes and status bar.

use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::path;

use crate::icons::{Glyph, Icon};
use crate::pages::{Funds, Keys, MarketPage, Markets, Portfolio, Profile, Settings};
use crate::state::{AppState, MarketList};
use crate::ui::{Connect, Logo, Rail, StatusBar, Toasts};

#[component]
pub fn App() -> impl IntoView {
    let app = AppState::new();
    provide_context(app);
    MarketList::provide(app);
    view! {
        <Router>
            <div class="shell">
                <div class="banner">
                    <span>"This is a reference frontend for the df-pascal-core Rust SDK."</span>
                    <a href="https://app.pascal.trade" target="_blank" rel="noopener">"Open the official Pascal app"<Glyph icon=Icon::ArrowUpRight size=14 /></a>
                </div>
                <header class="top">
                    <A href="/" attr:class="brand" attr:aria-label="Markets">
                        <Logo />
                        <span>"Pascal"</span>
                    </A>
                    <nav class="nav" aria-label="Pages">
                        <A href="/" exact=true>"Markets"</A>
                        <A href="/portfolio">"Portfolio"</A>
                        <A href="/funds">"Funds"</A>
                        <A href="/keys">"Trading keys"</A>
                        <A href="/profile">"Profile"</A>
                        <A href="/settings">"Settings"</A>
                    </nav>
                    <div class="who"><Connect /></div>
                </header>
                <div class="body">
                    <Rail />
                    <main class="main">
                        <Routes fallback=|| view! { <p class="empty">"Nothing here."</p> }>
                            <Route path=path!("/") view=Markets />
                            <Route path=path!("/market/:symbol") view=MarketPage />
                            <Route path=path!("/portfolio") view=Portfolio />
                            <Route path=path!("/funds") view=Funds />
                            <Route path=path!("/keys") view=Keys />
                            <Route path=path!("/profile") view=Profile />
                            <Route path=path!("/settings") view=Settings />
                        </Routes>
                    </main>
                </div>
                <StatusBar />
            </div>
            <Toasts />
        </Router>
    }
}
