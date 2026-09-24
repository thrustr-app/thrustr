use domain::game::GameId;
use gpui::{App, Global, SharedString};
use std::{any::Any, collections::VecDeque, mem::replace, rc::Rc};
use ui::{Icon, Sidebar, SidebarItem};

const MAX_HISTORY: usize = 20;

pub fn init(cx: &mut App) {
    cx.set_global(Navigator::new(Page::Home));
}

/// A navigable node in the page tree.
pub trait NavNode: Into<Page> + Clone {
    fn label(&self) -> &'static str;
    fn icon(&self) -> Icon;
    fn is_parent_of(&self, other: &Self) -> bool;
}

/// Binds a sidebar to the navigator.
pub trait NavSidebar<T> {
    fn nav(self, current: T) -> Self;
}

impl<T: NavNode + PartialEq + 'static> NavSidebar<T> for Sidebar<T> {
    fn nav(self, current: T) -> Self {
        self.value(current)
            .matches(T::is_parent_of)
            .on_change(|page: &T, _, cx| cx.navigate(page.clone()))
    }
}

/// Builds a sidebar item that navigates to `page` and reflects its active state.
pub fn nav_item<T: NavNode + PartialEq + 'static>(page: T) -> SidebarItem<T> {
    SidebarItem::new(page.label()).icon(page.icon()).value(page)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Home,
    Library,
    Collections,
    Game(GameId),
    Settings(Option<SettingsPage>),
}

impl NavNode for Page {
    fn label(&self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Library => "Library",
            Self::Collections => "Collections",
            Self::Game(_) => "",
            Self::Settings(None) => "Settings",
            Self::Settings(Some(sub)) => sub.label(),
        }
    }

    fn icon(&self) -> Icon {
        match self {
            Self::Home => Icon::home(),
            Self::Library => Icon::library(),
            Self::Collections => Icon::collections(),
            Self::Game(_) => Icon::library(),
            Self::Settings(_) => Icon::settings(),
        }
    }

    fn is_parent_of(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Library, Self::Game(_)) => true,
            (Self::Settings(None), Self::Settings(_)) => true,
            (Self::Settings(Some(a)), Self::Settings(Some(b))) => a.is_parent_of(b),
            _ => self == other,
        }
    }
}

impl Page {
    fn resolve(self) -> Self {
        match self {
            Self::Settings(None) => SettingsPage::Storefronts(None).into(),
            other => other,
        }
    }
}

impl From<SettingsPage> for Page {
    fn from(sub: SettingsPage) -> Self {
        Self::Settings(Some(sub))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsPage {
    Storefronts(Option<SharedString>),
    Plugins(Option<SharedString>),
    Appearance,
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self::Storefronts(None)
    }
}

impl NavNode for SettingsPage {
    fn label(&self) -> &'static str {
        match self {
            Self::Storefronts(_) => "Storefronts",
            Self::Plugins(_) => "Plugins",
            Self::Appearance => "Appearance",
        }
    }

    fn icon(&self) -> Icon {
        match self {
            Self::Storefronts(_) => Icon::storefront(),
            Self::Plugins(_) => Icon::plugin(),
            Self::Appearance => Icon::appearance(),
        }
    }

    fn is_parent_of(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Storefronts(None), Self::Storefronts(_)) => true,
            (Self::Plugins(None), Self::Plugins(_)) => true,
            _ => self == other,
        }
    }
}

#[derive(Clone)]
pub struct RouteState(Rc<dyn Any>);

impl RouteState {
    pub(crate) fn new<T: 'static>(state: T) -> Self {
        Self(Rc::new(state))
    }

    pub(crate) fn downcast<T: Clone + 'static>(&self) -> Option<T> {
        self.0.downcast_ref().cloned()
    }
}

type StateSaver = Box<dyn Fn(&Page, &App) -> Option<RouteState>>;

struct Entry {
    page: Page,
    state: Option<RouteState>,
}

impl Entry {
    fn new(page: Page) -> Self {
        Self { page, state: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transition {
    /// Returns to the previous history entry.
    Back,
    /// Replaces the current page, leaving the history untouched.
    Replace,
    /// Saves the current page onto the history.
    Push,
}

pub struct Navigator {
    current: Entry,
    history: VecDeque<Entry>,
    save_state: Option<StateSaver>,
}

impl Global for Navigator {}

impl Navigator {
    fn new(initial: Page) -> Self {
        Self {
            current: Entry::new(initial),
            history: VecDeque::new(),
            save_state: None,
        }
    }

    pub fn current_page(&self) -> Page {
        self.current.page.clone()
    }

    pub(crate) fn current_state(&self) -> Option<RouteState> {
        self.current.state.clone()
    }

    pub(crate) fn set_state_saver(
        &mut self,
        saver: impl Fn(&Page, &App) -> Option<RouteState> + 'static,
    ) {
        self.save_state = Some(Box::new(saver));
    }

    fn transition(&self, next: &Page) -> Option<Transition> {
        if *next == self.current.page {
            None
        } else if !next.is_parent_of(&self.current.page) {
            Some(Transition::Push)
        } else if self.history.back().is_some_and(|entry| entry.page == *next) {
            Some(Transition::Back)
        } else {
            Some(Transition::Replace)
        }
    }

    fn push(&mut self, next: Page, state: Option<RouteState>) {
        let previous = replace(&mut self.current, Entry::new(next));
        self.history.push_back(Entry {
            page: previous.page,
            state,
        });

        if self.history.len() > MAX_HISTORY {
            self.history.pop_front();
        }
    }

    fn replace(&mut self, next: Page) {
        self.current = Entry::new(next);
    }

    fn navigate_back(&mut self) {
        if let Some(previous) = self.history.pop_back() {
            self.current = previous;
        }
    }
}

pub trait NavigatorExt {
    fn navigator(&self) -> &Navigator;
    fn navigate(&mut self, page: impl Into<Page>);
    fn navigate_back(&mut self);
}

impl NavigatorExt for App {
    fn navigator(&self) -> &Navigator {
        self.global::<Navigator>()
    }

    fn navigate(&mut self, page: impl Into<Page>) {
        let next = page.into().resolve();

        self.defer(move |cx| {
            let navigator = cx.navigator();
            match navigator.transition(&next) {
                None => {}
                Some(Transition::Push) => {
                    let state = navigator
                        .save_state
                        .as_ref()
                        .and_then(|save| save(&next, cx));
                    cx.global_mut::<Navigator>().push(next, state);
                }
                Some(Transition::Replace) => cx.global_mut::<Navigator>().replace(next),
                Some(Transition::Back) => cx.global_mut::<Navigator>().navigate_back(),
            }
        });
    }

    fn navigate_back(&mut self) {
        self.defer(|cx| {
            if cx.navigator().history.is_empty() {
                return;
            }

            cx.global_mut::<Navigator>().navigate_back();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: u64) -> Page {
        Page::Game(GameId::from(id))
    }

    fn navigator(pages: &[Page]) -> Navigator {
        let (first, rest) = pages.split_first().unwrap();
        let mut navigator = Navigator::new(first.clone());
        for page in rest {
            let state = RouteState::new(navigator.current_page());
            navigator.push(page.clone(), Some(state));
        }
        navigator
    }

    #[track_caller]
    fn check_transition(pages: &[Page], next: Page, expected: Option<Transition>) {
        assert_eq!(navigator(pages).transition(&next), expected);
    }

    #[test]
    fn navigating_to_the_current_page_does_nothing() {
        check_transition(&[Page::Home, Page::Library], Page::Library, None);
    }

    #[test]
    fn navigating_to_an_unrelated_page_pushes() {
        check_transition(&[Page::Home], Page::Library, Some(Transition::Push));
        check_transition(
            &[Page::Library, game(1)],
            Page::Home,
            Some(Transition::Push),
        );
    }

    #[test]
    fn navigating_to_a_child_pushes() {
        check_transition(&[Page::Library], game(1), Some(Transition::Push));
    }

    #[test]
    fn navigating_to_the_previous_parent_goes_back() {
        check_transition(
            &[Page::Library, game(1)],
            Page::Library,
            Some(Transition::Back),
        );
    }

    #[test]
    fn navigating_to_another_parent_replaces() {
        check_transition(
            &[Page::Home, game(1)],
            Page::Library,
            Some(Transition::Replace),
        );
    }

    #[test]
    fn going_back_restores_the_saved_state() {
        let mut navigator = navigator(&[Page::Home, Page::Library, game(1)]);

        navigator.navigate_back();

        assert_eq!(navigator.current_page(), Page::Library);
        let state = navigator.current_state().and_then(|state| state.downcast());
        assert_eq!(state, Some(Page::Library));
    }

    #[test]
    fn replacing_keeps_the_history() {
        let mut navigator = navigator(&[Page::Home, game(1)]);

        navigator.replace(Page::Library);
        navigator.navigate_back();

        assert_eq!(navigator.current_page(), Page::Home);
    }

    #[test]
    fn history_is_capped() {
        let pages: Vec<_> = (0..MAX_HISTORY as u64 + 5).map(game).collect();
        let navigator = navigator(&pages);

        assert_eq!(navigator.history.len(), MAX_HISTORY);
        assert_eq!(
            navigator.history.front().map(|entry| &entry.page),
            Some(&game(4))
        );
    }
}
