use gpui::{
    Anchor, AnimationExt, App, Context, Div, ElementId, FontWeight, Global, IntoElement,
    ParentElement, Point, Rems, SharedString, SpringAnimation, SpringConfig, SpringState,
    StatefulInteractiveElement, Styled, Task, WeakEntity, Window, anchored, deferred, div,
    prelude::FluentBuilder, relative, rems,
};
use std::time::{Duration, Instant};
use theme::ThemeExt;

const SHOW_DELAY: Duration = Duration::from_millis(500);
const SKIP_DELAY: Duration = Duration::from_millis(300);

const SPRING: SpringConfig = SpringConfig::new(900., 60., 1.);
const SPRING_EPSILON: f32 = 0.01;
const GAP: Rems = rems(0.5);
const SLIDE: Rems = rems(0.375);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Side {
    #[default]
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    fn anchor(self) -> Anchor {
        match self {
            Self::Top => Anchor::BottomCenter,
            Self::Right => Anchor::LeftCenter,
            Self::Bottom => Anchor::TopCenter,
            Self::Left => Anchor::RightCenter,
        }
    }

    fn edge(self) -> Div {
        let edge = div().absolute();

        match self {
            Self::Top => edge.bottom_full().left_1_2(),
            Self::Right => edge.left_full().top_1_2(),
            Self::Bottom => edge.top_full().left_1_2(),
            Self::Left => edge.right_full().top_1_2(),
        }
    }

    fn direction(self) -> Point<f32> {
        match self {
            Self::Top => Point::new(0., -1.),
            Self::Right => Point::new(1., 0.),
            Self::Bottom => Point::new(0., 1.),
            Self::Left => Point::new(-1., 0.),
        }
    }
}

struct LastTooltip(WeakEntity<TooltipState>);

impl Global for LastTooltip {}

#[derive(Default)]
struct TooltipState {
    open: bool,
    mounted: bool,
    closed_at: Option<Instant>,
    transition: Option<Task<()>>,
}

impl TooltipState {
    fn is_warm(&self) -> bool {
        self.open || self.closed_at.is_some_and(|at| at.elapsed() < SKIP_DELAY)
    }

    fn is_group_warm(&self, cx: &Context<Self>) -> bool {
        let Some(last) = cx
            .try_global::<LastTooltip>()
            .and_then(|last| last.0.upgrade())
        else {
            return false;
        };

        if last.entity_id() == cx.entity_id() {
            self.is_warm()
        } else {
            last.read(cx).is_warm()
        }
    }

    fn set_hovered(&mut self, hovered: bool, delay: Duration, cx: &mut Context<Self>) {
        if hovered {
            self.show(delay, cx);
        } else {
            self.hide(cx);
        }
    }

    fn show(&mut self, delay: Duration, cx: &mut Context<Self>) {
        if delay.is_zero() || self.mounted || self.is_group_warm(cx) {
            self.open(cx);
            return;
        }

        self.transition = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |this, cx| this.open(cx)).ok();
        }));
    }

    fn open(&mut self, cx: &mut Context<Self>) {
        self.transition = None;
        self.open = true;
        self.mounted = true;
        let this = cx.weak_entity();
        cx.set_global(LastTooltip(this));
        cx.notify();
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.transition = None;
        if !self.mounted {
            return;
        }

        self.open = false;
        self.closed_at = Some(Instant::now());
        cx.notify();

        let settled = SPRING.settle_time(
            SpringState {
                position: 1.,
                velocity: 0.,
            },
            0.,
            SPRING_EPSILON,
        );
        self.transition = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(settled).await;
            this.update(cx, |this, cx| {
                this.mounted = false;
                cx.notify();
            })
            .ok();
        }));
    }
}

pub struct Tooltip {
    id: ElementId,
    text: SharedString,
    side: Side,
    delay: Duration,
}

impl Tooltip {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: (id.into(), "tooltip").into(),
            text: text.into(),
            side: Side::default(),
            delay: SHOW_DELAY,
        }
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn attach<E>(self, element: E, window: &mut Window, cx: &mut App) -> E
    where
        E: StatefulInteractiveElement + ParentElement + Styled + FluentBuilder,
    {
        let state = window.use_keyed_state(self.id, cx, |_, _| TooltipState::default());
        let TooltipState { open, mounted, .. } = *state.read(cx);
        let delay = self.delay;

        element
            .relative()
            .on_hover(move |hovered, _, cx| {
                state.update(cx, |state, cx| state.set_hovered(*hovered, delay, cx));
            })
            .when(mounted, |element| {
                element.child(Self::render_bubble(self.text, self.side, open, window, cx))
            })
    }

    fn render_bubble(
        text: SharedString,
        side: Side,
        open: bool,
        window: &Window,
        cx: &App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let direction = side.direction();
        let gap = GAP.to_pixels(window.rem_size());
        let slide = SLIDE.to_pixels(window.rem_size());

        let bubble = div()
            .relative()
            .whitespace_nowrap()
            .px(rems(0.625))
            .py(rems(0.375))
            .rounded(theme.radius.sm)
            .border_1()
            .border_color(theme.colors.border)
            .bg(theme.colors.surface)
            .shadow_md()
            .text_color(theme.colors.primary)
            .text_size(theme.text.sm)
            .line_height(relative(1.))
            .font_weight(FontWeight::MEDIUM)
            .child(text)
            .with_spring(
                "tooltip-spring",
                SpringAnimation::new(SPRING)
                    .to(open)
                    .from(false)
                    .with_epsilon(SPRING_EPSILON),
                move |bubble, phase| {
                    let phase = phase.0.clamp(0., 1.);
                    let distance = slide * (phase - 1.);

                    bubble
                        .opacity(phase)
                        .left(distance * direction.x)
                        .top(distance * direction.y)
                },
            );

        side.edge().child(deferred(
            anchored()
                .anchor(side.anchor())
                .offset(Point::new(gap * direction.x, gap * direction.y))
                .snap_to_window_with_margin(gap)
                .child(bubble),
        ))
    }
}
