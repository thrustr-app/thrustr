use crate::FieldLabel;
use core::panic;
use gpui::{
    Animation, AnimationExt, App, ElementId, Hsla, IntoElement, ParentElement, Refineable, Rems,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div, ease_out_quint,
    prelude::FluentBuilder, relative, rems,
};
use smallvec::SmallVec;
use std::time::Duration;
use theme::ThemeExt;

const HEIGHT: Rems = rems(0.5);
const CAP: Rems = rems(0.25);
const FILL_DURATION: Duration = Duration::from_millis(600);

#[derive(IntoElement)]
pub struct Meter {
    id: ElementId,
    style: StyleRefinement,
    total: f64,
    segments: SmallVec<[(f64, Hsla); 2]>,
    label: Option<SharedString>,
    value_label: Option<SharedString>,
}

impl Meter {
    #[track_caller]
    pub fn new(total: impl Into<f64>) -> Self {
        Self {
            id: (ElementId::CodeLocation(*panic::Location::caller()), "meter").into(),
            style: StyleRefinement::default(),
            total: total.into(),
            segments: SmallVec::new(),
            label: None,
            value_label: None,
        }
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    pub fn segment(mut self, value: impl Into<f64>, color: Hsla) -> Self {
        self.segments.push((value.into(), color));
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn value_label(mut self, value_label: impl Into<SharedString>) -> Self {
        self.value_label = Some(value_label.into());
        self
    }

    fn fraction(&self, value: f64) -> f32 {
        if self.total <= 0. {
            return 0.;
        }
        (value / self.total).clamp(0., 1.) as f32
    }
}

impl Styled for Meter {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Meter {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let radius = theme.radius.pill;

        let visible = self
            .segments
            .iter()
            .enumerate()
            .filter(|(_, (value, _))| *value > 0.)
            .collect::<Vec<_>>();
        let last = visible.len().saturating_sub(1);

        let segments = visible
            .iter()
            .enumerate()
            .map(|(position, &(ix, &(value, color)))| {
                let fraction = self.fraction(value);
                let (first, last) = (position == 0, position == last);
                div()
                    .h_full()
                    .min_w(if first && last { HEIGHT } else { CAP })
                    .when(first, |segment| segment.rounded_l(radius))
                    .when(last, |segment| segment.rounded_r(radius))
                    .bg(color)
                    .with_animation(
                        (self.id.clone(), SharedString::from(format!("segment-{ix}"))),
                        Animation::new(FILL_DURATION).with_easing(ease_out_quint()),
                        move |segment, delta| segment.w(relative(fraction * delta)),
                    )
            })
            .collect::<Vec<_>>();

        let bar = div()
            .flex()
            .w_full()
            .h(HEIGHT)
            .rounded(radius)
            .bg(theme.colors.surface)
            .children(segments);

        let has_header = self.label.is_some() || self.value_label.is_some();
        let header = has_header.then(|| {
            div()
                .flex()
                .items_center()
                .justify_end()
                .when(self.label.is_some(), |header| header.justify_between())
                .gap(rems(1.))
                .children(
                    self.label
                        .map(|label| FieldLabel::new(label).id((self.id.clone(), "label"))),
                )
                .children(self.value_label.map(|value_label| {
                    div()
                        .text_size(theme.text.sm)
                        .line_height(relative(1.))
                        .text_color(theme.colors.secondary)
                        .child(value_label)
                }))
        });

        let mut meter = div()
            .flex()
            .flex_col()
            .gap(rems(0.625))
            .children(header)
            .child(bar);

        meter.style().refine(&self.style);
        meter
    }
}
