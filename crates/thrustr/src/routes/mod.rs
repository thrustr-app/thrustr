use crate::navigation::{Page, RouteState};
use config::paths;
use domain::game::GameId;
use gpui::{
    AnyElement, AnyView, App, AppContext, Context, EmptyView, Entity, Rems, Render, Window, rems,
};
use std::{path::Path, sync::Arc};

mod collections;
mod game;
mod home;
mod library;
mod root;
mod settings;

pub use collections::*;
pub use game::*;
pub use home::*;
pub use library::*;
pub use root::Root;
pub use settings::*;

pub const ROUTE_PADDING: Rems = rems(3.);

pub(crate) fn cover_path(hash: &str) -> Option<Arc<Path>> {
    paths::artwork_path(hash, "webp").ok().map(Into::into)
}

pub trait Route: Render {
    const PADDING: Rems = ROUTE_PADDING;
    const TOPBAR: bool = true;

    /// Arguments for building the route, taken from the `Page`.
    type Args;
    /// State kept in the history when navigating and handed back when returning.
    type State: Clone + Default + 'static;

    fn build(
        args: Self::Args,
        state: Self::State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self;

    fn header(&self, _this: &Entity<Self>, _cx: &App) -> Option<AnyElement> {
        None
    }

    /// Called when navigating from this route to `next`.
    fn save_state(&self, _next: &Page, _cx: &App) -> Option<Self::State> {
        None
    }
}

impl Route for EmptyView {
    type Args = ();
    type State = ();

    fn build(_: (), _: (), _: &mut Window, _: &mut Context<Self>) -> Self {
        EmptyView
    }
}

pub trait RouteHandle {
    fn view(&self) -> AnyView;
    fn padding(&self) -> Rems;
    fn has_topbar(&self) -> bool;
    fn render_header(&self, cx: &App) -> Option<AnyElement>;
    fn save_state(&self, next: &Page, cx: &App) -> Option<RouteState>;
}

impl<T: Route> RouteHandle for Entity<T> {
    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn padding(&self) -> Rems {
        T::PADDING
    }

    fn has_topbar(&self) -> bool {
        T::TOPBAR
    }

    fn render_header(&self, cx: &App) -> Option<AnyElement> {
        self.read(cx).header(self, cx)
    }

    fn save_state(&self, next: &Page, cx: &App) -> Option<RouteState> {
        self.read(cx).save_state(next, cx).map(RouteState::new)
    }
}

fn mount<R: Route>(
    args: R::Args,
    state: Option<RouteState>,
    window: &mut Window,
    cx: &mut App,
) -> Box<dyn RouteHandle> {
    let state = state
        .and_then(|state| state.downcast::<R::State>())
        .unwrap_or_default();
    Box::new(cx.new(|cx| R::build(args, state, window, cx)))
}

/// Determines whether two pages reuse the same route view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Home,
    Library,
    Collections,
    Game(GameId),
    Settings,
}

impl Page {
    fn build_view(
        &self,
        state: Option<RouteState>,
        window: &mut Window,
        cx: &mut App,
    ) -> Box<dyn RouteHandle> {
        match self {
            Self::Home => mount::<Home>((), state, window, cx),
            Self::Library => mount::<Library>((), state, window, cx),
            Self::Collections => mount::<Collections>((), state, window, cx),
            Self::Game(id) => mount::<Game>(*id, state, window, cx),
            Self::Settings(Some(sub)) => mount::<Settings>(sub.clone(), state, window, cx),
            Self::Settings(None) => mount::<EmptyView>((), state, window, cx),
        }
    }

    fn section(&self) -> Section {
        match self {
            Self::Home => Section::Home,
            Self::Library => Section::Library,
            Self::Collections => Section::Collections,
            Self::Game(id) => Section::Game(*id),
            Self::Settings(_) => Section::Settings,
        }
    }
}
