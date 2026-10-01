use core::panic;
use gpui::{
    App, Div, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Refineable, RenderOnce, SharedString, StatefulInteractiveElement, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder, relative, rems,
};
use theme::ThemeExt;

#[derive(IntoElement)]
pub struct FieldLabel {
    id: ElementId,
    style: StyleRefinement,
    text: SharedString,
    focus_target: Option<FocusHandle>,
}

impl FieldLabel {
    #[track_caller]
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            id: (
                ElementId::CodeLocation(*panic::Location::caller()),
                "field_label",
            )
                .into(),
            style: StyleRefinement::default(),
            text: text.into(),
            focus_target: None,
        }
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    pub fn focus_target(mut self, focus_handle: impl Into<Option<FocusHandle>>) -> Self {
        self.focus_target = focus_handle.into();
        self
    }
}

impl Styled for FieldLabel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for FieldLabel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let mut label = div()
            .id(self.id)
            .text_size(theme.text.md)
            .line_height(relative(1.))
            .font_weight(FontWeight::NORMAL)
            .text_color(theme.colors.secondary)
            .when_some(self.focus_target, |label, focus_handle| {
                label.on_click(move |_, window, cx| focus_handle.focus(window, cx))
            })
            .child(self.text);

        label.style().refine(&self.style);
        label
    }
}

#[derive(Default)]
pub struct FieldProps {
    label: Option<SharedString>,
}

impl FieldProps {
    pub(crate) fn wrap<E: IntoElement + Styled>(
        self,
        id: impl Into<ElementId>,
        mut control: E,
        focus_target: Option<FocusHandle>,
    ) -> Div {
        let id = id.into();
        let width = control.style().max_size.width;

        div()
            .when_some(width, |field, width| field.max_w(width))
            .when_some(self.label, |field, label| {
                field.child(
                    FieldLabel::new(label)
                        .id((id, "label"))
                        .focus_target(focus_target)
                        .mb(rems(0.375)),
                )
            })
            .child(control)
    }
}

pub trait WithField: Sized {
    #[doc(hidden)]
    fn field_props(&mut self) -> &mut FieldProps;

    fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.field_props().label = Some(label.into());
        self
    }
}
