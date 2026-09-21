use crate::{Icon, Size, WithSize};
use core::panic;
use gpui::{
    App, ElementId, FocusHandle, FontWeight, Hsla, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Refineable, RenderOnce, SharedString, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, actions, div, prelude::FluentBuilder, relative, rems,
};
use std::rc::Rc;
use theme::ThemeExt;

const CONTEXT: &str = "sidebar";

actions!(sidebar, [SelectPrev, SelectNext]);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrev, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
    ]);
}

pub trait SidebarValue: Clone + PartialEq + 'static {}
impl<T: Clone + PartialEq + 'static> SidebarValue for T {}

#[derive(Clone, Copy)]
struct Palette {
    hover: Hsla,
    active_bg: Hsla,
    active_fg: Hsla,
    muted_fg: Hsla,
    ring: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Main,
    Content,
}

type ChangeHandler<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;
type Matcher<T> = Rc<dyn Fn(&T, &T) -> bool>;

struct ItemContext<T> {
    palette: Palette,
    focused: bool,
    active: bool,
    focus_handle: FocusHandle,
    on_change: Option<ChangeHandler<T>>,
}

fn refocus(focus_handle: &FocusHandle, window: &mut Window, cx: &mut App) {
    let focus_handle = focus_handle.clone();
    window.defer(cx, move |window, cx| focus_handle.focus(window, cx));
}

#[derive(IntoElement)]
pub struct SidebarItem<T: SidebarValue> {
    id: ElementId,
    icon: Option<Icon>,
    label: Option<SharedString>,
    value: Option<T>,
    context: Option<ItemContext<T>>,
}

impl<T: SidebarValue> SidebarItem<T> {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            icon: None,
            label: None,
            value: None,
            context: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn value(mut self, value: T) -> Self {
        self.value = Some(value);
        self
    }

    fn with_context(mut self, context: ItemContext<T>) -> Self {
        self.context = Some(context);
        self
    }
}

impl<T: SidebarValue> RenderOnce for SidebarItem<T> {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ItemContext {
            palette,
            focused,
            active,
            focus_handle,
            on_change,
        } = self
            .context
            .expect("SidebarItem must be rendered as part of a Sidebar");

        let theme = cx.theme();
        let has_label = self.label.is_some();

        let fg = if active {
            palette.active_fg
        } else {
            palette.muted_fg
        };

        let item = div()
            .id(self.id)
            .cursor_pointer()
            .flex()
            .items_center()
            .rounded(theme.radius.md)
            .border_1()
            .text_color(fg)
            .text_size(theme.text.md)
            .line_height(relative(1.))
            .font_weight(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .hover(move |div| div.bg(palette.hover))
            .when(active, |div| div.bg(palette.active_bg))
            .when(active && focused, |div| div.border_color(palette.ring))
            .when_else(
                has_label,
                |div| div.h(rems(2.5)).px(rems(0.875)).w_full().gap(rems(0.625)),
                |div| div.p(rems(0.625)).size(rems(2.75)).justify_center(),
            )
            .when_some(self.icon, |el, icon| {
                let size = if has_label { Size::Medium } else { Size::Large };

                el.child(icon.size(size).color(fg))
            })
            .when_some(self.label, |el, label| el.child(div().child(label)));

        match (self.value, on_change) {
            (Some(value), Some(on_change)) => item.on_click(move |_, window, cx| {
                on_change(&value, window, cx);
                refocus(&focus_handle, window, cx);
            }),
            _ => item,
        }
    }
}

#[derive(IntoElement)]
pub struct Sidebar<T: SidebarValue> {
    id: ElementId,
    style: StyleRefinement,
    kind: Kind,
    items: Vec<SidebarItem<T>>,
    bottom_items: Vec<SidebarItem<T>>,
    value: Option<T>,
    on_change: Option<ChangeHandler<T>>,
    matches: Option<Matcher<T>>,
}

impl<T: SidebarValue> Sidebar<T> {
    #[track_caller]
    pub fn main() -> Self {
        Self::build(Kind::Main)
    }

    #[track_caller]
    pub fn new() -> Self {
        Self::build(Kind::Content)
    }

    #[track_caller]
    fn build(kind: Kind) -> Self {
        Self {
            id: (
                ElementId::CodeLocation(*panic::Location::caller()),
                "sidebar",
            )
                .into(),
            style: StyleRefinement::default(),
            kind,
            items: Vec::new(),
            bottom_items: Vec::new(),
            value: None,
            on_change: None,
            matches: None,
        }
    }

    pub fn item(mut self, item: SidebarItem<T>) -> Self {
        self.items.push(item);
        self
    }

    pub fn bottom_item(mut self, item: SidebarItem<T>) -> Self {
        self.bottom_items.push(item);
        self
    }

    pub fn value(mut self, value: T) -> Self {
        self.value = Some(value);
        self
    }

    pub fn on_change(mut self, handler: impl Fn(&T, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    pub fn matches(mut self, matches: impl Fn(&T, &T) -> bool + 'static) -> Self {
        self.matches = Some(Rc::new(matches));
        self
    }

    fn all_items(&self) -> impl Iterator<Item = &SidebarItem<T>> {
        self.items.iter().chain(&self.bottom_items)
    }

    fn palette(&self, cx: &App) -> Palette {
        let colors = &cx.theme().colors;

        match self.kind {
            Kind::Main => Palette {
                hover: colors.sidebar.hover,
                active_bg: colors.sidebar.surface,
                active_fg: colors.sidebar.primary,
                muted_fg: colors.sidebar.secondary,
                ring: colors.sidebar.primary,
            },
            Kind::Content => Palette {
                hover: colors.hover,
                active_bg: colors.surface,
                active_fg: colors.primary,
                muted_fg: colors.secondary,
                ring: colors.primary,
            },
        }
    }

    fn has_label(&self) -> bool {
        self.all_items().any(|item| item.label.is_some())
    }

    fn neighbors(&self, is_active: &Matcher<T>) -> (Option<T>, Option<T>) {
        let values = || self.all_items().filter_map(|item| item.value.as_ref());
        let count = values().count();

        if count == 0 {
            return (None, None);
        }

        let active_ix = self
            .value
            .as_ref()
            .and_then(|current| values().position(|value| is_active(value, current)));

        let at = |offset: isize| {
            let ix = match active_ix {
                Some(ix) => (ix as isize + offset).rem_euclid(count as isize) as usize,
                None if offset > 0 => 0,
                None => count - 1,
            };
            values().nth(ix).cloned()
        };

        (at(-1), at(1))
    }
}

impl<T: SidebarValue> Default for Sidebar<T> {
    #[track_caller]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: SidebarValue> Styled for Sidebar<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: SidebarValue> RenderOnce for Sidebar<T> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();

        let focused = focus_handle.is_focused(window) && window.last_input_was_keyboard();
        let palette = self.palette(cx);
        let has_label = self.has_label();

        let is_active: Matcher<T> = self.matches.take().unwrap_or_else(|| Rc::new(T::eq) as _);
        let (prev, next) = self.neighbors(&is_active);

        let current = self.value.take();

        let on_change = self.on_change.take().map(|handler| {
            let current = current.clone();

            Rc::new(move |value: &T, window: &mut Window, cx: &mut App| {
                if current.as_ref() != Some(value) {
                    handler(value, window, cx);
                }
            }) as ChangeHandler<T>
        });

        let prepare = |item: SidebarItem<T>| {
            let active = item
                .value
                .as_ref()
                .zip(current.as_ref())
                .is_some_and(|(value, current)| is_active(value, current));

            item.with_context(ItemContext {
                palette,
                focused,
                active,
                focus_handle: focus_handle.clone(),
                on_change: on_change.clone(),
            })
        };

        let group = |items: Vec<SidebarItem<T>>| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .w_full()
                .gap(rems(0.625))
                .children(items.into_iter().map(&prepare))
        };

        let on_key = |value: Option<T>| {
            let focus_handle = focus_handle.clone();
            let on_change = on_change.clone();

            move |window: &mut Window, cx: &mut App| {
                let (Some(value), Some(on_change)) = (&value, &on_change) else {
                    return;
                };
                on_change(value, window, cx);
                refocus(&focus_handle, window, cx);
            }
        };

        let mut sidebar = div()
            .id(self.id)
            .key_context(CONTEXT)
            .track_focus(&focus_handle)
            .on_action({
                let on_key = on_key(prev);
                move |_: &SelectPrev, window, cx| on_key(window, cx)
            })
            .on_action({
                let on_key = on_key(next);
                move |_: &SelectNext, window, cx| on_key(window, cx)
            })
            .flex()
            .flex_col()
            .justify_between()
            .when(has_label, |div| div.min_w(rems(13.)))
            .child(group(self.items))
            .child(group(self.bottom_items));

        sidebar.style().refine(&self.style);
        sidebar
    }
}
