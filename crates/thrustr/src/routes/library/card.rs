use crate::{
    adapters::ColorExt,
    globals::{ArtworkServiceExt, ComponentRegistryExt},
    navigation::{NavigatorExt, Page},
    routes::cover_path,
    routes::library::{
        CARD_ASPECT_RATIO, CARD_ICON_SIZE, CARD_INNER_GAP, CARD_PADDING, CARD_TITLE_SIZE,
    },
};
use domain::game::{GameId, GameListItem};
use gpui::{
    App, Empty, FontWeight, Hsla, ImageSource, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, Pixels, RenderOnce, Resource, SharedString, StatefulInteractiveElement, Styled,
    StyledImage, Window, div, img, prelude::FluentBuilder, relative, transparent_black,
};
use std::{path::Path, sync::Arc};
use theme::ThemeExt;

#[derive(Clone)]
pub(super) struct GameEntry {
    pub id: GameId,
    pub name: SharedString,
    pub cover_url: Option<SharedString>,
    pub cover_path: Option<Arc<Path>>,
    pub accent: Option<Hsla>,
    pub source_id: SharedString,
}

impl GameEntry {
    pub(super) fn from_list_item(item: GameListItem) -> Self {
        let (cover_path, accent) = match item.cover {
            Some(art) => (cover_path(&art.hash), art.accent.map(|c| c.to_gpui_hsla())),
            None => (None, None),
        };
        Self {
            id: item.id,
            name: item.name.into(),
            cover_url: item.cover_url.map(Into::into),
            source_id: item.source_id.into(),
            cover_path,
            accent,
        }
    }
}

enum CardKind {
    Game(GameEntry),
    Unloaded,
    Spacer,
}

#[derive(IntoElement)]
pub(super) struct GameCard {
    kind: CardKind,
    selected: bool,
    width: Pixels,
}

impl GameCard {
    pub(super) fn new(game: GameEntry, width: Pixels) -> Self {
        Self {
            kind: CardKind::Game(game),
            selected: false,
            width,
        }
    }

    pub(super) fn unloaded(width: Pixels) -> Self {
        Self {
            kind: CardKind::Unloaded,
            selected: false,
            width,
        }
    }

    pub(super) fn spacer(width: Pixels) -> Self {
        Self {
            kind: CardKind::Spacer,
            selected: false,
            width,
        }
    }

    pub(super) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for GameCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let game = match self.kind {
            CardKind::Game(game) => Some(game),
            CardKind::Unloaded => None,
            CardKind::Spacer => {
                return div().flex_shrink_0().w(self.width).into_any_element();
            }
        };

        let ring = if self.selected {
            theme.colors.primary
        } else {
            transparent_black()
        };

        let base = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(CARD_INNER_GAP)
            .p(CARD_PADDING)
            .w(self.width)
            .rounded(theme.radius.lg.to_rems(window.rem_size()) + CARD_PADDING)
            .border_1()
            .border_color(ring);

        let mut cover = div()
            .aspect_ratio(CARD_ASPECT_RATIO)
            .w_full()
            .bg(theme.colors.surface)
            .rounded(theme.radius.lg);

        let mut title = div()
            .h(CARD_TITLE_SIZE)
            .overflow_hidden()
            .whitespace_nowrap()
            .w_full()
            .text_ellipsis()
            .text_color(theme.colors.primary)
            .text_size(CARD_TITLE_SIZE)
            .line_height(relative(1.))
            .font_weight(FontWeight::SEMIBOLD);

        let mut icon_row = div().h(CARD_ICON_SIZE).flex_shrink_0();

        let Some(game) = game else {
            return base
                .child(cover)
                .child(title)
                .child(icon_row)
                .into_any_element();
        };

        if let Some(path) = game.cover_path {
            let mut cover_img = img(ImageSource::Resource(Resource::Path(path)))
                .object_fit(ObjectFit::Cover)
                .w_full()
                .h_full()
                .rounded(theme.radius.lg);

            // The file is recorded but can still be missing or unreadable, in
            // which case the download is worth another try.
            if let Some(url) = game.cover_url {
                let artwork_service = cx.artwork_service();
                cover_img = cover_img.with_fallback(move || {
                    artwork_service.enqueue_cover(game.id, &url);
                    Empty.into_any_element()
                });
            }

            cover = cover.child(cover_img);
        }

        title = title.child(game.name);

        if let Some(icon) = cx.component_icon(&game.source_id) {
            icon_row = icon_row.child(img(ImageSource::Image(icon)).size(CARD_ICON_SIZE));
        }

        let accent = game.accent.unwrap_or(theme.colors.surface).opacity(0.3);

        base.id(("game-card", u64::from(game.id)))
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
