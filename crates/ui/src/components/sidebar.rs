use gpui::{
    AbsoluteLength, App, ClickEvent, ElementId, FocusHandle, Hsla, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Refineable, RenderOnce, SharedString, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, actions, div, prelude::FluentBuilder, rems, svg,
    transparent_black,
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

/// Which theme tokens a [`Sidebar`] renders with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarPalette {
    /// The `sidebar_*` theme tokens, for the application rail.
    #[default]
    Sidebar,
    /// The regular theme tokens, for sidebars embedded in content areas.
    Content,
}

#[derive(Clone, Copy)]
struct Palette {
    hover: Hsla,
    active_bg: Hsla,
    active_fg: Hsla,
    muted_fg: Hsla,
    ring: Hsla,
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A single navigation entry of a [`Sidebar`]. Icon-only when no label is set.
pub struct SidebarItem {
    id: ElementId,
    icon: Option<SharedString>,
    label: Option<SharedString>,
    active: bool,
    on_click: Option<ClickHandler>,
}

impl SidebarItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            icon: None,
            label: None,
            active: false,
            on_click: None,
        }
    }

    pub fn icon(mut self, path: impl Into<SharedString>) -> Self {
        self.icon = Some(path.into());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    fn render(
        self,
        palette: Palette,
        radius: AbsoluteLength,
        focused: bool,
        focus_handle: FocusHandle,
    ) -> impl IntoElement {
        let fg = if self.active {
            palette.active_fg
        } else {
            palette.muted_fg
        };

        div()
            .id(self.id)
            .cursor_pointer()
            .flex()
            .items_center()
            .rounded(radius)
            .border_1()
            .border_color(transparent_black())
            .bg(transparent_black())
            .text_color(fg)
            .hover(move |div| div.bg(palette.hover))
            .when(self.active, |div| div.bg(palette.active_bg))
            .when(self.active && focused, |div| div.border_color(palette.ring))
            .when_some(self.on_click, |div, handler| {
                div.on_click(move |event, window, cx| {
                    handler(event, window, cx);

                    let focus_handle = focus_handle.clone();
                    window.defer(cx, move |window, cx| focus_handle.focus(window, cx));
                })
            })
            .when_else(
                self.label.is_some(),
                |div| div.py(rems(0.625)).px(rems(1.25)).w_full().gap(rems(0.75)),
                |div| div.p(rems(0.625)).justify_center(),
            )
            .when_some(self.icon, |div, path| {
                div.child(
                    svg()
                        .flex_shrink_0()
                        .path(path)
                        .text_color(fg)
                        .size(rems(1.5)),
                )
            })
            .when_some(self.label, |el, label| el.child(div().child(label)))
    }
}

#[derive(IntoElement)]
pub struct Sidebar {
    id: ElementId,
    style: StyleRefinement,
    palette: SidebarPalette,
    items: Vec<SidebarItem>,
    bottom_items: Vec<SidebarItem>,
    wrap: bool,
}

impl Sidebar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: (id.into(), "sidebar").into(),
            style: StyleRefinement::default(),
            palette: SidebarPalette::default(),
            items: Vec::new(),
            bottom_items: Vec::new(),
            wrap: true,
        }
    }

    pub fn palette(mut self, palette: SidebarPalette) -> Self {
        self.palette = palette;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn item(mut self, item: SidebarItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn bottom_item(mut self, item: SidebarItem) -> Self {
        self.bottom_items.push(item);
        self
    }
}

impl Styled for Sidebar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Sidebar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();

        if !focus_handle.tab_stop {
            focus_handle = focus_handle.tab_stop(true);
        }

        let focused = focus_handle.is_focused(window);
        let theme = cx.theme();
        let radius = theme.radius.full;
        let colors = &theme.colors;

        let palette = match self.palette {
            SidebarPalette::Sidebar => Palette {
                hover: colors.sidebar_hover,
                active_bg: colors.sidebar_surface,
                active_fg: colors.sidebar_primary,
                muted_fg: colors.sidebar_secondary,
                ring: colors.accent,
            },
            SidebarPalette::Content => Palette {
                hover: colors.hover,
                active_bg: colors.surface,
                active_fg: colors.primary,
                muted_fg: colors.secondary,
                ring: colors.accent,
            },
        };

        let all_items = || self.items.iter().chain(self.bottom_items.iter());
        let active_ix = all_items().position(|item| item.active);
        let handlers: Vec<Option<ClickHandler>> =
            all_items().map(|item| item.on_click.clone()).collect();

        let select = Rc::new({
            let focus_handle = focus_handle.clone();
            let wrap = self.wrap;
            move |delta: isize, window: &mut Window, cx: &mut App| {
                let count = handlers.len();
                if count == 0 {
                    return;
                }

                let target = match active_ix {
                    None if delta > 0 => 0,
                    None => count - 1,
                    Some(current) => {
                        let next = current as isize + delta;
                        if next < 0 {
                            if !wrap {
                                return;
                            }
                            count - 1
                        } else if next as usize >= count {
                            if !wrap {
                                return;
                            }
                            0
                        } else {
                            next as usize
                        }
                    }
                };

                if let Some(handler) = &handlers[target] {
                    handler(&ClickEvent::default(), window, cx);
                }

                let focus_handle = focus_handle.clone();
                window.defer(cx, move |window, cx| focus_handle.focus(window, cx));
            }
        });

        let group =
            |items: Vec<SidebarItem>, focus_handle: FocusHandle| {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(rems(0.75))
                    .children(items.into_iter().map(move |item| {
                        item.render(palette, radius, focused, focus_handle.clone())
                    }))
            };

        let mut sidebar = div()
            .id(self.id)
            .key_context(CONTEXT)
            .track_focus(&focus_handle)
            .on_action({
                let select = select.clone();
                move |_: &SelectPrev, window, cx| select(-1, window, cx)
            })
            .on_action(move |_: &SelectNext, window, cx| select(1, window, cx))
            .flex()
            .flex_col()
            .justify_between()
            .child(group(self.items, focus_handle.clone()))
            .child(group(self.bottom_items, focus_handle));

        sidebar.style().refine(&self.style);
        sidebar
    }
}
