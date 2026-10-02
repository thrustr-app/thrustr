use crate::{
    FieldProps, FocusProps, Icon, Size, Tab, TabPrev, WithField, WithFocus, WithScrollbar, WithSize,
};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Bounds, ClickEvent, Context, ElementId, Entity,
    FocusHandle, FontWeight, InteractiveElement, IntoElement, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, Refineable, Rems, RenderOnce,
    ScrollHandle, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Subscription,
    Window, actions, anchored, canvas, deferred, div, ease_out_quint, point,
    prelude::FluentBuilder, px, relative, rems,
};
use std::{rc::Rc, time::Duration};
use theme::ThemeExt;

const CONTEXT: &str = "select";

const BORDER: Pixels = px(1.);
const MENU_GAP: Rems = rems(0.25);
const MENU_PADDING: Rems = rems(0.25);
const MENU_MAX_HEIGHT: Rems = rems(16.);
const MENU_SLIDE: Pixels = px(6.);
const MENU_OPEN_DURATION: Duration = Duration::from_millis(150);

actions!(
    select,
    [
        SelectPrev,
        SelectNext,
        SelectFirst,
        SelectLast,
        SelectPrevPage,
        SelectNextPage,
        Confirm,
        Cancel
    ]
);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrev, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(CONTEXT)),
        KeyBinding::new("pageup", SelectPrevPage, Some(CONTEXT)),
        KeyBinding::new("pagedown", SelectNextPage, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("space", Confirm, Some(CONTEXT)),
        KeyBinding::new("alt-up", Confirm, Some(CONTEXT)),
        KeyBinding::new("alt-down", Confirm, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
    ]);
}

type ItemRenderer<T> = Box<dyn Fn(&T, &mut Window, &mut App) -> AnyElement>;
type ChangeHandler<T> = Box<dyn Fn(&T, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Select<T: PartialEq + 'static> {
    id: ElementId,
    style: StyleRefinement,
    size: Size,
    items: Vec<T>,
    value: Option<T>,
    placeholder: Option<SharedString>,
    render_item: ItemRenderer<T>,
    on_change: Option<ChangeHandler<T>>,
    focus: FocusProps,
    field: FieldProps,
    loading: bool,
    disabled: bool,
}

impl<T: PartialEq + 'static> Select<T> {
    pub fn new<E: IntoElement>(
        id: impl Into<ElementId>,
        render_item: impl Fn(&T, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        Self {
            id: (id.into(), "select").into(),
            style: StyleRefinement::default(),
            size: Size::default(),
            items: Vec::new(),
            value: None,
            placeholder: None,
            render_item: Box::new(move |item, window, cx| {
                render_item(item, window, cx).into_any_element()
            }),
            on_change: None,
            focus: FocusProps::default(),
            field: FieldProps::default(),
            loading: false,
            disabled: false,
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = T>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn value(mut self, value: impl Into<Option<T>>) -> Self {
        self.value = value.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn on_change(mut self, handler: impl Fn(&T, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Box::new(handler));
        self
    }

    pub fn loading(mut self) -> Self {
        self.loading = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

fn height(size: Size) -> Rems {
    match size {
        Size::Small => rems(2.),
        Size::Medium => rems(2.375),
        Size::Large => rems(2.625),
        Size::ExtraLarge => rems(2.875),
    }
}

fn item_padding_y(size: Size) -> Rems {
    match size {
        Size::Small => rems(0.25),
        Size::Medium => rems(0.375),
        Size::Large => rems(0.5),
        Size::ExtraLarge => rems(0.625),
    }
}

fn padding(size: Size) -> Rems {
    match size {
        Size::Small => rems(0.625),
        Size::Medium => rems(0.75),
        Size::Large => rems(1.),
        Size::ExtraLarge => rems(1.25),
    }
}

impl<T: PartialEq + 'static> Styled for Select<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: PartialEq + 'static> WithSize for Select<T> {
    fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl<T: PartialEq + 'static> WithFocus for Select<T> {
    fn focus_props(&mut self) -> &mut FocusProps {
        &mut self.focus
    }
}

impl<T: PartialEq + 'static> WithField for Select<T> {
    fn field_props(&mut self) -> &mut FieldProps {
        &mut self.field
    }
}

impl<T: PartialEq + 'static> RenderOnce for Select<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            SelectState::new(self.focus.auto_focus, window, cx)
        });

        let interactive = !self.disabled && !self.loading;
        if !interactive {
            state.update(cx, |state, cx| state.close(cx));
        }

        let focus_handle = self.focus.configure(state.read(cx).focus_handle.clone());
        let highlighted = state.read(cx).highlighted;

        let selected = self
            .value
            .as_ref()
            .and_then(|value| self.items.iter().position(|item| item == value));
        let menu = Menu {
            state,
            options: Rc::new(Options {
                items: self.items,
                selected,
                on_change: self.on_change,
            }),
        };

        let theme = cx.theme();

        let content = match selected {
            Some(ix) => (self.render_item)(&menu.options.items[ix], window, cx),
            None => div()
                .text_color(theme.colors.field.placeholder)
                .children(self.placeholder)
                .into_any_element(),
        };

        let indicator = if self.loading {
            Icon::loader().size(self.size).spin()
        } else {
            Icon::chevrons().size_sm()
        }
        .color(theme.colors.secondary);

        let border_color = self.field.border_color(theme.colors.field.border, &theme);
        let focus_border_color = self.field.border_color(theme.colors.field.focus, &theme);

        let mut trigger = div()
            .id(self.id.clone())
            .relative()
            .flex()
            .items_center()
            .gap(rems(0.5))
            .h(height(self.size))
            .px(padding(self.size))
            .rounded(theme.radius.md)
            .border_1()
            .border_color(border_color)
            .bg(theme.colors.field.background)
            .text_color(theme.colors.primary)
            .text_size(theme.text.md)
            .line_height(relative(1.1))
            .font_weight(FontWeight::NORMAL)
            .focus(|trigger| trigger.border_color(focus_border_color))
            .when(highlighted.is_some(), |trigger| {
                trigger.border_color(focus_border_color)
            })
            .when(self.disabled, |trigger| trigger.opacity(0.6))
            .when(interactive, |trigger| {
                trigger
                    .key_context(CONTEXT)
                    .track_focus(&focus_handle)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        menu.listener(|menu, _: &MouseDownEvent, _, cx| menu.toggle(cx)),
                    )
                    .on_action(menu.listener(|menu, _: &SelectPrev, window, cx| {
                        menu.step(Step::Prev, window, cx)
                    }))
                    .on_action(menu.listener(|menu, _: &SelectNext, window, cx| {
                        menu.step(Step::Next, window, cx)
                    }))
                    .on_action(menu.listener(|menu, _: &SelectFirst, window, cx| {
                        menu.step(Step::First, window, cx)
                    }))
                    .on_action(menu.listener(|menu, _: &SelectLast, window, cx| {
                        menu.step(Step::Last, window, cx)
                    }))
                    .on_action(menu.listener(|menu, _: &SelectPrevPage, window, cx| {
                        menu.step(Step::PrevPage, window, cx)
                    }))
                    .on_action(menu.listener(|menu, _: &SelectNextPage, window, cx| {
                        menu.step(Step::NextPage, window, cx)
                    }))
                    .on_action(
                        menu.listener(|menu, _: &Confirm, window, cx| menu.confirm(window, cx)),
                    )
                    .when(highlighted.is_some(), |trigger| {
                        trigger
                            .on_action(menu.listener(|menu, _: &Cancel, _, cx| menu.close(cx)))
                            .on_action(menu.listener(|menu, _: &Tab, window, cx| {
                                menu.choose_and_tab(window, cx)
                            }))
                            .on_action(menu.listener(|menu, _: &TabPrev, window, cx| {
                                menu.choose_and_tab(window, cx)
                            }))
                    })
                    .child(menu.track_trigger_bounds())
            })
            .child(div().flex_1().min_w_0().truncate().child(content))
            .child(indicator)
            .when_some(highlighted, |trigger, highlighted| {
                trigger.child(menu.render(highlighted, &self.render_item, self.size, window, cx))
            });

        trigger.style().refine(&self.style);

        let container = self.field.wrap(
            self.id.clone(),
            trigger,
            interactive.then(|| focus_handle.clone()),
        );

        self.focus
            .attach_reveal(container, &focus_handle, (self.id, "reveal"), window, cx)
    }
}

struct Options<T> {
    items: Vec<T>,
    selected: Option<usize>,
    on_change: Option<ChangeHandler<T>>,
}

impl<T> Options<T> {
    fn select(&self, ix: usize, window: &mut Window, cx: &mut App) {
        if self.selected == Some(ix) {
            return;
        }
        if let Some(on_change) = &self.on_change {
            on_change(&self.items[ix], window, cx);
        }
    }
}

struct SelectState {
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    trigger_bounds: Bounds<Pixels>,
    highlighted: Option<usize>,
    _blur: Subscription,
}

impl SelectState {
    fn new(auto_focus: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        if auto_focus {
            focus_handle.focus(window, cx);
        }
        let _blur = cx.on_blur(&focus_handle, window, |_, window, cx| {
            cx.defer_in(window, |this, _, cx| this.close(cx));
        });

        Self {
            focus_handle,
            scroll_handle: ScrollHandle::new(),
            trigger_bounds: Bounds::default(),
            highlighted: None,
            _blur,
        }
    }

    fn is_open(&self) -> bool {
        self.highlighted.is_some()
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if self.highlighted.take().is_some() {
            cx.notify();
        }
    }

    fn highlight(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.highlighted.is_some_and(|current| current != ix) {
            self.highlighted = Some(ix);
            cx.notify();
        }
    }

    fn reveal(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.highlighted != Some(ix) {
            self.highlighted = Some(ix);
            cx.notify();
        }
        self.scroll_handle.scroll_to_item(ix);
    }

    fn page_size(&self, ix: usize) -> usize {
        let viewport = self.scroll_handle.bounds().size.height;
        self.scroll_handle
            .bounds_for_item(ix)
            .map_or(1, |item| (viewport / item.size.height) as usize)
            .max(1)
    }

    fn set_trigger_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let resized = bounds.size.width != self.trigger_bounds.size.width;
        self.trigger_bounds = bounds;
        // The menu takes its width from the trigger.
        if resized && self.is_open() {
            cx.notify();
        }
    }
}

struct Menu<T> {
    state: Entity<SelectState>,
    options: Rc<Options<T>>,
}

impl<T> Clone for Menu<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            options: self.options.clone(),
        }
    }
}

impl<T: 'static> Menu<T> {
    fn listener<E: ?Sized>(
        &self,
        f: impl Fn(&Self, &E, &mut Window, &mut App) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let this = self.clone();
        move |event, window, cx| f(&this, event, window, cx)
    }

    fn toggle(&self, cx: &mut App) {
        if self.options.items.is_empty() {
            return;
        }
        let selected = self.options.selected.unwrap_or(0);
        self.state.update(cx, |state, cx| match state.is_open() {
            true => state.close(cx),
            false => state.reveal(selected, cx),
        });
    }

    fn step(&self, step: Step, window: &mut Window, cx: &mut App) {
        let count = self.options.items.len();
        if count == 0 {
            return;
        }

        let state = self.state.read(cx);
        match state.highlighted {
            Some(ix) => {
                let target = step.target(Some(ix), count, state.page_size(ix));
                self.state.update(cx, |state, cx| state.reveal(target, cx));
            }
            None => {
                let target = step.target(self.options.selected, count, count);
                self.options.select(target, window, cx);
            }
        }
    }

    fn confirm(&self, window: &mut Window, cx: &mut App) {
        match self.state.read(cx).is_open() {
            true => self.choose_highlighted(window, cx),
            false => self.toggle(cx),
        }
    }

    fn choose_highlighted(&self, window: &mut Window, cx: &mut App) {
        let Some(ix) = self.state.read(cx).highlighted else {
            return;
        };
        self.close(cx);
        self.options.select(ix, window, cx);
    }

    fn choose_and_tab(&self, window: &mut Window, cx: &mut App) {
        self.choose_highlighted(window, cx);
        cx.propagate();
    }

    fn highlight(&self, ix: usize, cx: &mut App) {
        self.state.update(cx, |state, cx| state.highlight(ix, cx));
    }

    fn close(&self, cx: &mut App) {
        self.state.update(cx, |state, cx| state.close(cx));
    }

    fn dismiss(&self, position: Point<Pixels>, cx: &mut App) {
        self.state.update(cx, |state, cx| {
            if !state.trigger_bounds.contains(&position) {
                state.close(cx);
            }
        });
    }

    fn track_trigger_bounds(&self) -> impl IntoElement {
        let state = self.state.clone();
        canvas(
            move |bounds, _, cx| {
                // The canvas covers the padding box, so grow it over the border.
                let bounds = bounds.dilate(BORDER);
                state.update(cx, |state, cx| state.set_trigger_bounds(bounds, cx));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    fn render(
        &self,
        highlighted: usize,
        render_item: &ItemRenderer<T>,
        size: Size,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (width, scroll_handle) = {
            let state = self.state.read(cx);
            (state.trigger_bounds.size.width, state.scroll_handle.clone())
        };
        let gap = MENU_GAP.to_pixels(window.rem_size());
        let item_padding_x = rems(padding(size).0 - MENU_PADDING.0);

        let items = self.options.items.iter().enumerate().map(|(ix, item)| {
            div()
                .flex()
                .items_center()
                .gap(rems(0.5))
                .px(item_padding_x)
                .py(item_padding_y(size))
                .rounded(theme.radius.sm)
                .cursor_pointer()
                .when(ix == highlighted, |item| {
                    item.bg(theme.colors.popover.highlight)
                })
                .on_mouse_move(
                    self.listener(move |menu, _: &MouseMoveEvent, _, cx| menu.highlight(ix, cx)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    self.listener(move |menu, _: &MouseDownEvent, _, cx| menu.highlight(ix, cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(render_item(item, window, cx)),
                )
                .child(
                    div()
                        .flex_none()
                        .size(Icon::length(Size::Small))
                        .when(self.options.selected == Some(ix), |slot| {
                            slot.child(Icon::check().size_sm().color(theme.colors.accent))
                        }),
                )
        });

        let menu =
            div()
                .relative()
                .w(width)
                .occlude()
                .rounded(theme.radius.md)
                .border_1()
                .border_color(theme.colors.popover.border)
                .bg(theme.colors.popover.background)
                .shadow_md()
                .on_mouse_down_out(self.listener(|menu, event: &MouseDownEvent, _, cx| {
                    menu.dismiss(event.position, cx)
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .max_h(MENU_MAX_HEIGHT)
                        .p(MENU_PADDING)
                        .children(items)
                        .overflow_y_scrollbar()
                        .handle(&scroll_handle)
                        .on_click(self.listener(|menu, _: &ClickEvent, window, cx| {
                            menu.choose_highlighted(window, cx)
                        })),
                )
                .with_animation(
                    "menu",
                    Animation::new(MENU_OPEN_DURATION).with_easing(ease_out_quint()),
                    |menu, delta| menu.opacity(delta).top(MENU_SLIDE * (delta - 1.)),
                );

        div().absolute().top_full().left(-BORDER).child(deferred(
            anchored()
                .offset(point(px(0.), BORDER + gap))
                .snap_to_window_with_margin(gap)
                .child(menu),
        ))
    }
}

#[derive(Debug, Clone, Copy)]
enum Step {
    Prev,
    Next,
    First,
    Last,
    PrevPage,
    NextPage,
}

impl Step {
    fn target(self, current: Option<usize>, count: usize, page: usize) -> usize {
        let last = count - 1;
        let current = current.map(|ix| ix.min(last));

        match self {
            Self::First => 0,
            Self::Last => last,
            Self::Prev => current.map_or(last, |ix| ix.saturating_sub(1)),
            Self::Next => current.map_or(0, |ix| (ix + 1).min(last)),
            Self::PrevPage => current.map_or(last, |ix| ix.saturating_sub(page)),
            Self::NextPage => current.map_or(0, |ix| ix.saturating_add(page).min(last)),
        }
    }
}
