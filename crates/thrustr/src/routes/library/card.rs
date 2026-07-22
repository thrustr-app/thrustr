use super::{
    CARD_ASPECT_RATIO, CARD_ICON_SIZE_REM, CARD_INNER_GAP_REM, CARD_PADDING_REM,
    CARD_TEXT_SIZE_REM, CARD_TITLE_HEIGHT_REM, CARD_WIDTH,
};
use crate::{
    globals::ArtworkServiceExt,
    navigation::{NavigatorExt, Page},
};
use config::paths;
use domain::{
    artwork::Color,
    game::{GameId, GameListItem},
};
use gpui::{
    App, FontWeight, Hsla, Image, ImageSource, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, RenderOnce, Resource, SharedString, StatefulInteractiveElement, Styled,
    StyledImage, Window, div, img, prelude::FluentBuilder, rems, rgb, transparent_black,
};
use std::{collections::HashMap, path::Path, sync::Arc};
use theme::ThemeExt;

pub(super) fn cover_path(hash: &str) -> Arc<Path> {
    paths::artwork_path(hash, "webp").into()
}

pub(super) fn accent_hsla(color: Color) -> Hsla {
    rgb(color.into()).into()
}

#[derive(Clone)]
pub(super) struct GameEntry {
    pub id: GameId,
    pub element_id: SharedString,
    pub name: SharedString,
    pub cover_url: Option<SharedString>,
    pub cover_path: Option<Arc<Path>>,
    pub accent_color: Option<Hsla>,
    pub source_icon: Option<Arc<Image>>,
}

impl GameEntry {
    pub(super) fn from_list_item(item: GameListItem, icons: &HashMap<String, Arc<Image>>) -> Self {
        let (cover_path, accent_color) = match item.cover {
            Some(art) => (
                Some(cover_path(&art.hash)),
                art.accent_color.map(accent_hsla),
            ),
            None => (None, None),
        };
        Self {
            id: item.id,
            element_id: item.id.to_string().into(),
            name: item.name.into(),
            cover_url: item.cover_url.map(Into::into),
            source_icon: icons.get(&item.source_id).cloned(),
            cover_path,
            accent_color,
        }
    }
}

#[derive(IntoElement)]
pub(super) struct GameCard {
    game: Option<GameEntry>,
    filler: bool,
    selected: bool,
}

impl GameCard {
    pub(super) fn new(game: GameEntry) -> Self {
        Self {
            game: Some(game),
            filler: false,
            selected: false,
        }
    }

    pub(super) fn blank() -> Self {
        Self {
            game: None,
            filler: false,
            selected: false,
        }
    }

    pub(super) fn filler() -> Self {
        Self {
            game: None,
            filler: true,
            selected: false,
        }
    }

    pub(super) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for GameCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        if self.filler {
            return div().flex_shrink_0().w(CARD_WIDTH).into_any_element();
        }

        let ring = if self.selected {
            theme.colors.primary
        } else {
            transparent_black()
        };

        let base = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(rems(CARD_INNER_GAP_REM))
            .p(rems(CARD_PADDING_REM))
            .w(CARD_WIDTH)
            .rounded(theme.radius.lg)
            .border_1()
            .border_color(ring)
            .bg(theme.colors.card_background.opacity(0.));

        let mut cover = div()
            .aspect_ratio(CARD_ASPECT_RATIO)
            .w_full()
            .bg(theme.colors.card_background)
            .rounded(theme.radius.md);

        let mut title = div()
            .h(rems(CARD_TITLE_HEIGHT_REM))
            .overflow_hidden()
            .whitespace_nowrap()
            .w_full()
            .text_ellipsis()
            .text_color(theme.colors.primary)
            .text_size(rems(CARD_TEXT_SIZE_REM))
            .font_weight(FontWeight::LIGHT);

        let mut icon_row = div().h(rems(CARD_ICON_SIZE_REM)).flex_shrink_0();

        let Some(game) = self.game else {
            return base
                .child(cover)
                .child(title)
                .child(icon_row)
                .into_any_element();
        };

        if let Some(path) = game.cover_path {
            let mut cover_img = img(ImageSource::Resource(Resource::Path(path)))
                .object_fit(ObjectFit::Contain)
                .w_full()
                .h_full()
                .rounded(theme.radius.md);

            if let Some(url) = game.cover_url {
                let artwork_service = cx.artwork_service();
                cover_img = cover_img.with_fallback(move || {
                    artwork_service.enqueue_cover(game.id, &url);
                    div().into_any_element()
                });
            }

            cover = cover.child(cover_img);
        }

        title = title.child(game.name);

        if let Some(icon) = game.source_icon {
            icon_row = icon_row.child(img(ImageSource::Image(icon)).size(rems(CARD_ICON_SIZE_REM)));
        }

        let accent = game
            .accent_color
            .unwrap_or(theme.colors.card_background)
            .opacity(0.25);

        base.id(game.element_id.clone())
            .on_click(move |_, _, cx| {
                cx.navigate(Page::Game(game.id));
            })
            .when(self.selected, |style| style.bg(accent))
            .hover(move |style| style.bg(accent))
            .child(cover)
            .child(title)
            .child(icon_row)
            .into_any_element()
    }
}
