use gpui::{
    Bounds, Context, Corner, FocusHandle, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Window, actions, black, canvas, div, fill, point, px, red, size,
};

const WIDTH: f32 = 640.;
const HEIGHT: f32 = 480.;
const CONTEXT: &str = "emulator";

actions!(emulator, [Up, Down, Left, Right]);

pub struct Root {
    x: f32,
    y: f32,
    focus_handle: FocusHandle,
}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new("up", Up, Some(CONTEXT)),
            KeyBinding::new("down", Down, Some(CONTEXT)),
            KeyBinding::new("left", Left, Some(CONTEXT)),
            KeyBinding::new("right", Right, Some(CONTEXT)),
        ]);

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);

        Self {
            x: 0.,
            y: 0.,
            focus_handle,
        }
    }

    pub fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.y -= 1.;
        cx.notify();
    }

    pub fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.y += 1.;
        cx.notify();
    }

    pub fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.x -= 1.;
        cx.notify();
    }

    pub fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.x += 1.;
        cx.notify();
    }
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let x = self.x;
        let y = self.y;

        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .bg(black())
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .child(canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    window.paint_quad(fill(
                        Bounds::from_corner_and_size(
                            Corner::TopLeft,
                            point(px(x), px(y)),
                            size(px(2.), px(2.)),
                        ),
                        red(),
                    ));
                },
            ))
    }
}
