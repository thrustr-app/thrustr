use super::{Route, cover_path};
use crate::{adapters::ColorExt, context::SpawnTaskExt, globals::GameServiceExt};
use domain::game::GameId;
use gpui::{
    AnyElement, BoxShadow, Context, Entity, FontWeight, Hsla, ImageSource, IntoElement, ObjectFit,
    ParentElement, Rems, Render, Resource, RetainAllImageCache, SharedString, Styled, StyledImage,
    Task, Window, black, div, img, linear_color_stop, linear_gradient, px, relative, rems,
};
use std::{path::Path, sync::Arc};
use theme::ThemeExt;
use tracing::error;
use ui::{Button, Icon, WithSize, WithVariant};

// FIXME: gpui gradients only take two stops, so this is a workaround to simulate
// a multi-stop gradient by stacking layers
const HEADER_GRADIENT_LAYERS: usize = 3;
const COVER_HEIGHT: Rems = rems(12.);
const COVER_PLACEHOLDER_ASPECT_RATIO: f32 = 2. / 3.;

pub struct Game {
    _id: GameId,
    name: SharedString,
    summary: Option<SharedString>,
    cover_path: Option<Arc<Path>>,
    accent: Option<Hsla>,
    image_cache: Entity<RetainAllImageCache>,
    _load_task: Task<()>,
}

impl Game {
    pub fn new(id: GameId, cx: &mut Context<Self>) -> Self {
        let game_service = cx.game_service();
        let load_task = cx.spawn_and_update(
            async move { game_service.get(id) },
            |game, result, _| match result {
                Ok(Some(loaded)) => {
                    game.name = loaded.name.into();
                    game.summary = loaded.summary.map(Into::into);
                    if let Some(cover) = loaded.cover {
                        game.cover_path = cover_path(&cover.hash);
                        game.accent = cover.accent.map(|a| a.to_gpui_hsla());
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    error!("failed to load game: {e:#}");
                }
            },
        );

        Self {
            _id: id,
            name: SharedString::default(),
            summary: None,
            cover_path: None,
            accent: None,
            image_cache: RetainAllImageCache::new(cx),
            _load_task: load_task,
        }
    }

    fn render_cover(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let shadow = vec![BoxShadow::new(px(0.), px(4.), black().opacity(0.3)).blur_radius(px(5.))];

        let Some(path) = self.cover_path.clone() else {
            return div()
                .flex_shrink_0()
                .h(COVER_HEIGHT)
                .aspect_ratio(COVER_PLACEHOLDER_ASPECT_RATIO)
                .rounded(theme.radius.md)
                .bg(theme.colors.surface)
                .shadow(shadow)
                .into_any_element();
        };

        img(ImageSource::Resource(Resource::Path(path)))
            .flex_shrink_0()
            .h(COVER_HEIGHT)
            .aspect_ratio(COVER_PLACEHOLDER_ASPECT_RATIO)
            .object_fit(ObjectFit::Cover)
            .rounded(theme.radius.md)
            .bg(theme.colors.surface)
            .shadow(shadow)
            .into_any_element()
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.colors.background;
        let radius = theme.radius.lg;
        let fade = (0..HEADER_GRADIENT_LAYERS).map(|_| {
            div()
                .absolute()
                .inset_0()
                .rounded(radius)
                .bg(linear_gradient(
                    0.,
                    linear_color_stop(background, 0.),
                    linear_color_stop(background.opacity(0.), 1.),
                ))
        });

        div()
            .relative()
            .flex_shrink_0()
            .h(rems(22.))
            .p(rems(1.5))
            .flex()
            .items_end()
            .justify_between()
            .gap(rems(1.5))
            .rounded(radius)
            .bg(self.accent.unwrap_or(theme.colors.surface))
            .children(fade)
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(rems(1.5))
                    .child(self.render_cover(cx))
                    .child(
                        div()
                            .min_w_0()
                            .line_clamp(2)
                            .text_ellipsis()
                            .text_size(rems(2.5))
                            .line_height(relative(1.2))
                            .font_weight(FontWeight::BLACK)
                            .text_color(theme.colors.primary)
                            .child(self.name.clone()),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(rems(0.5))
                    .child(
                        Button::new("download")
                            .size_xl()
                            .variant_accent()
                            .shadow(vec![
                                BoxShadow::new(px(0.), px(0.), theme.colors.accent.opacity(0.35))
                                    .blur_radius(px(6.)),
                            ])
                            .with_icon(Icon::download())
                            .child("Download"),
                    )
                    .child(
                        Button::icon("menu", Icon::menu())
                            .size_xl()
                            .variant_outline(),
                    ),
            )
    }
}

impl Route for Game {
    const TOPBAR: bool = false;
}

impl Render for Game {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .image_cache(self.image_cache.clone())
            .pt(rems(2.))
            .flex()
            .flex_col()
            .child(self.render_header(cx))
    }
}
