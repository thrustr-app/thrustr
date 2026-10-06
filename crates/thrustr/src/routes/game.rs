use super::{Route, cover_path};
use crate::{
    adapters::{ColorExt, PlatformExt},
    context::SpawnTaskExt,
    format::format_size,
    globals::{ComponentRegistryExt, GameServiceExt},
    navigation::NavigatorExt,
};
use anyhow::Context as _;
use component::StorefrontHandle;
use config::paths;
use domain::{
    game::{GameId, GameVersionDetails},
    platform::Platform,
};
use download::{InstallTarget, InstallTargetError};
use gpui::{
    AnyElement, AppContext, BoxShadow, ClickEvent, Context, Div, Entity, FontWeight, Hsla,
    ImageSource, IntoElement, ObjectFit, ParentElement, PathPromptOptions, Rems, Render, Resource,
    RetainAllImageCache, SharedString, Styled, StyledImage, Task, Window, black, div, img,
    linear_color_stop, linear_gradient, prelude::FluentBuilder, px, relative, rems,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use theme::{Theme, ThemeExt};
use tracing::error;
use ui::{
    Alert, Button, Dialog, Icon, InputEvent, Meter, PortalContext, Select, WithField, WithFocus,
    WithRadius, WithSize, WithVariant, input,
};

// FIXME: gpui gradients only take two stops, so this is a workaround to simulate
// a multi-stop gradient by stacking layers
const HEADER_GRADIENT_LAYERS: usize = 3;
const COVER_HEIGHT: Rems = rems(12.);
const DIALOG_COVER_HEIGHT: Rems = rems(10.);
const COVER_PLACEHOLDER_ASPECT_RATIO: f32 = 2. / 3.;

pub struct Game {
    id: GameId,
    source_id: Option<SharedString>,
    name: SharedString,
    summary: Option<SharedString>,
    cover_path: Option<Arc<Path>>,
    accent: Option<Hsla>,
    image_cache: Entity<RetainAllImageCache>,
    _load_task: Task<()>,
}

impl Route for Game {
    const TOPBAR: bool = false;
    const PADDING: Rems = rems(0.);

    type Args = GameId;
    type State = ();

    fn build(id: GameId, _state: (), _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let game_service = cx.game_service();
        let load_task = cx.spawn_and_update(
            async move { game_service.get(id) },
            |game, result, _| match result {
                Ok(Some(loaded)) => {
                    game.source_id = Some(loaded.source.id.into());
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
            id,
            source_id: None,
            name: SharedString::default(),
            summary: None,
            cover_path: None,
            accent: None,
            image_cache: RetainAllImageCache::new(cx),
            _load_task: load_task,
        }
    }
}

impl Game {
    fn open_install_dialog(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source_id) = &self.source_id else {
            return;
        };
        let title: SharedString = format!("Install {}", self.name).into();
        let install_dialog = cx.new(|cx| InstallDialog::new(self, source_id, cx));
        window.open_dialog(cx, move |dialog, _, cx| {
            dialog
                .w(rems(36.))
                .title(title.clone())
                .ok_text("Install")
                .when(!install_dialog.read(cx).can_install(), Dialog::disabled)
                .child(install_dialog.clone())
        });
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.colors.background;
        let fade = (0..HEADER_GRADIENT_LAYERS).map(|_| {
            div().absolute().inset_0().bg(linear_gradient(
                0.,
                linear_color_stop(background, 0.),
                linear_color_stop(background.opacity(0.), 1.),
            ))
        });

        div()
            .relative()
            .flex_shrink_0()
            .h(rems(22.))
            .p(rems(2.))
            .flex()
            .items_end()
            .justify_between()
            .gap(rems(1.5))
            .bg(self.accent.unwrap_or(theme.colors.surface))
            .children(fade)
            .child(
                Button::icon("back-button", Icon::arrow())
                    .auto_focus()
                    .variant_outline()
                    .size_sm()
                    .radius_pill()
                    .absolute()
                    .top(rems(2.))
                    .left(rems(2.))
                    .on_click(|_, _, cx| cx.navigate_back()),
            )
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(rems(1.5))
                    .child(render_cover(self.cover_path.clone(), COVER_HEIGHT, &theme))
                    .child(
                        div()
                            .min_w_0()
                            .line_clamp(2)
                            .text_ellipsis()
                            .text_size(theme.text.display)
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
                            .radius_pill()
                            .variant_accent()
                            .shadow(vec![
                                BoxShadow::new(px(0.), px(6.), theme.colors.accent.opacity(0.2))
                                    .blur_radius(px(20.)),
                            ])
                            .with_icon(Icon::download())
                            .child("Download")
                            .on_click(cx.listener(Self::open_install_dialog)),
                    )
                    .child(
                        Button::icon("menu", Icon::menu())
                            .size_xl()
                            .radius_pill()
                            .variant_outline(),
                    ),
            )
    }
}

impl Render for Game {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .image_cache(self.image_cache.clone())
            .flex()
            .flex_col()
            .child(self.render_header(cx))
    }
}

fn render_cover(path: Option<Arc<Path>>, height: Rems, theme: &Theme) -> AnyElement {
    let Some(path) = path else {
        return cover_style(div(), height, theme).into_any_element();
    };

    cover_style(
        img(ImageSource::Resource(Resource::Path(path))),
        height,
        theme,
    )
    .object_fit(ObjectFit::Cover)
    .into_any_element()
}

fn cover_style<E: Styled>(element: E, height: Rems, theme: &Theme) -> E {
    element
        .flex_shrink_0()
        .h(height)
        .aspect_ratio(COVER_PLACEHOLDER_ASPECT_RATIO)
        .rounded(theme.radius.md)
        .bg(theme.colors.surface)
        .shadow(vec![
            BoxShadow::new(px(0.), px(4.), black().opacity(0.3)).blur_radius(px(5.)),
        ])
}

#[derive(Clone, PartialEq)]
struct VersionOption {
    id: SharedString,
    label: SharedString,
    platform: Platform,
}

struct InstallDialog {
    game_id: GameId,
    storefront: Option<StorefrontHandle>,
    name: SharedString,
    cover_path: Option<Arc<Path>>,
    image_cache: Entity<RetainAllImageCache>,
    versions: Vec<VersionOption>,
    selected: Option<VersionOption>,
    details: Option<GameVersionDetails>,
    details_loading: bool,
    install_dir: SharedString,
    install_target: Option<Result<InstallTarget, InstallTargetError>>,
    storage_labels: StorageLabels,
    loading: bool,
    error: Option<SharedString>,
    _load_task: Task<()>,
    _details_task: Option<Task<()>>,
    _browse_task: Option<Task<()>>,
    _install_target_task: Option<Task<()>>,
}

#[derive(Default)]
struct StorageLabels {
    free_space: Option<SharedString>,
    download_size: Option<SharedString>,
    install_size: Option<SharedString>,
    missing_space: Option<SharedString>,
}

impl InstallDialog {
    fn new(game: &Game, source_id: &str, cx: &mut Context<Self>) -> Self {
        let game_id = game.id;
        let storefront = cx.component_registry().storefront(source_id);
        let load_task = cx.spawn_and_update(
            {
                let storefront = storefront.clone();
                async move {
                    let storefront = storefront.context("storefront is not available")?;
                    anyhow::Ok(storefront.list_game_versions(game_id).await?)
                }
            },
            |this, result, cx| {
                this.loading = false;
                match result {
                    Ok(versions) => {
                        this.versions = versions
                            .into_iter()
                            .map(|v| VersionOption {
                                label: v.pretty_name.unwrap_or_else(|| v.id.clone()).into(),
                                id: v.id.into(),
                                platform: v.platform,
                            })
                            .collect();
                        let first = this.versions.first().cloned();
                        this.select_version(first, cx);
                    }
                    Err(e) => this.error = Some(format!("{e:#}").into()),
                }
            },
        );

        let mut this = Self {
            game_id,
            storefront,
            name: game.name.clone(),
            cover_path: game.cover_path.clone(),
            image_cache: game.image_cache.clone(),
            versions: Vec::new(),
            selected: None,
            details: None,
            details_loading: false,
            install_dir: paths::default_install_dir()
                .to_string_lossy()
                .into_owned()
                .into(),
            install_target: None,
            storage_labels: StorageLabels::default(),
            loading: true,
            error: None,
            _load_task: load_task,
            _details_task: None,
            _browse_task: None,
            _install_target_task: None,
        };
        this.update_install_target(cx);
        this
    }

    fn select_version(&mut self, version: Option<VersionOption>, cx: &mut Context<Self>) {
        self.selected = version;
        self.details = None;
        self.details_loading = false;
        self._details_task = None;
        self.update_storage_labels();

        let (Some(version), Some(storefront)) = (&self.selected, self.storefront.clone()) else {
            return;
        };
        let game_id = self.game_id;
        let version_id = version.id.to_string();
        self.details_loading = true;
        self._details_task = Some(cx.spawn_and_update(
            async move { storefront.game_version_details(game_id, version_id).await },
            |this, result, _| {
                this.details_loading = false;
                match result {
                    Ok(details) => this.details = Some(details),
                    Err(e) => error!("failed to load version details: {e:#}"),
                }
                this.update_storage_labels();
            },
        ));
    }

    fn update_install_target(&mut self, cx: &mut Context<Self>) {
        let dir = PathBuf::from(self.install_dir.as_ref());
        let name = self.name.clone();
        self._install_target_task = Some(cx.spawn_and_update(
            async move { InstallTarget::resolve(&dir, &name) },
            |this, result, _| {
                this.install_target = Some(result);
                this.update_storage_labels();
            },
        ));
    }

    fn update_storage_labels(&mut self) {
        let target = self.target();
        let details = self.details.as_ref();
        self.storage_labels = StorageLabels {
            free_space: target.map(|target| {
                format!(
                    "{} free of {}",
                    format_size(target.available_space),
                    format_size(target.total_space)
                )
                .into()
            }),
            download_size: details
                .and_then(|details| details.download_size)
                .map(|size| format!("{} download", format_size(size)).into()),
            install_size: details
                .and_then(|details| details.install_size)
                .map(|size| format!("{} installed", format_size(size)).into()),
            missing_space: self
                .missing_space()
                .map(|size| format!("Not enough space: {} more needed.", format_size(size)).into()),
        };
    }

    fn browse_install_dir(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select".into()),
        });
        self._browse_task = Some(cx.spawn_and_update(paths, |this, result, cx| match result {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    this.install_dir = path.to_string_lossy().into_owned().into();
                    this.update_install_target(cx);
                }
            }
            Ok(Err(e)) => error!("failed to open the folder picker: {e:#}"),
            Ok(Ok(None)) | Err(_) => {}
        }));
    }
}

impl Render for InstallDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .image_cache(self.image_cache.clone())
            .flex()
            .items_start()
            .gap(rems(1.5))
            .child(render_cover(
                self.cover_path.clone(),
                DIALOG_COVER_HEIGHT,
                &cx.theme(),
            ))
            .child(self.render_fields(cx))
    }
}

impl InstallDialog {
    fn render_fields(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(rems(1.))
            .children(self.error.clone().map(Alert::new))
            .child(
                Select::new("version", |version: &VersionOption, _, cx| {
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(0.5))
                        .child(
                            version
                                .platform
                                .icon()
                                .size_sm()
                                .color(cx.theme().colors.primary),
                        )
                        .child(div().min_w_0().child(version.label.clone()))
                })
                .size_lg()
                .w_full()
                .label("Game version")
                .placeholder("Select a version")
                .items(self.versions.iter().cloned())
                .value(self.selected.clone())
                .when(self.loading, Select::loading)
                .on_change(cx.listener(|this, version: &VersionOption, _, cx| {
                    this.select_version(Some(version.clone()), cx);
                    cx.notify();
                })),
            )
            .child(
                input("install-dir")
                    .size_lg()
                    .w_full()
                    .label("Install location")
                    .value(self.install_dir.clone())
                    .map(|input| match &self.install_target {
                        Some(Ok(target)) => {
                            input.description(target.dir.to_string_lossy().into_owned())
                        }
                        Some(Err(e)) => input.error(e.to_string()),
                        None => input,
                    })
                    .trailing(
                        Button::icon("browse-install-dir", Icon::folder())
                            .size_lg()
                            .variant_field()
                            .on_click(cx.listener(Self::browse_install_dir)),
                    )
                    .on_input(cx.listener(|this, event: &InputEvent, _, cx| {
                        this.install_dir = event.value.clone();
                        this.update_install_target(cx);
                        cx.notify();
                    })),
            )
            .children(self.render_storage(cx))
    }

    fn render_storage(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        if let Some(Err(_)) = self.install_target {
            return None;
        }

        let theme = cx.theme();
        let labels = &self.storage_labels;
        let required = self.required_space();

        let game_color = if labels.missing_space.is_some() {
            theme.colors.danger
        } else {
            theme.colors.primary
        };

        let free_space = labels
            .free_space
            .clone()
            .unwrap_or_else(|| "Calculating free space…".into());

        let meter = match self.target() {
            Some(target) => Meter::new(target.total_space as f64)
                .segment(
                    target.total_space.saturating_sub(target.available_space) as f64,
                    theme.colors.tertiary,
                )
                .segment(required.unwrap_or(0) as f64, game_color),
            None => Meter::new(0.),
        };

        let sizes = if self.loading || self.details_loading {
            vec![div().child("Calculating size…")]
        } else {
            let download_color = if labels.install_size.is_some() {
                theme.colors.secondary
            } else {
                game_color
            };
            [
                labels
                    .download_size
                    .clone()
                    .map(|label| size_item(Icon::download(), label, download_color)),
                labels
                    .install_size
                    .clone()
                    .map(|label| size_item(Icon::disk(), label, game_color)),
            ]
            .into_iter()
            .flatten()
            .collect()
        };

        Some(
            div()
                .flex()
                .flex_col()
                .gap(rems(0.625))
                .child(
                    meter
                        .id("storage-meter")
                        .label("Storage")
                        .value_label(free_space),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(rems(0.5))
                        .text_size(theme.text.sm)
                        .line_height(relative(1.))
                        .text_color(theme.colors.secondary)
                        .when(!sizes.is_empty(), |legend| {
                            legend.child(div().flex().flex_wrap().gap(rems(1.)).children(sizes))
                        })
                        .children(
                            labels
                                .missing_space
                                .clone()
                                .map(|label| div().text_color(theme.colors.danger).child(label)),
                        ),
                ),
        )
    }

    fn required_space(&self) -> Option<u64> {
        let details = self.details.as_ref()?;
        details.install_size.or(details.download_size)
    }

    fn target(&self) -> Option<&InstallTarget> {
        self.install_target.as_ref()?.as_ref().ok()
    }

    fn missing_space(&self) -> Option<u64> {
        self.required_space()?
            .checked_sub(self.target()?.available_space)
            .filter(|&missing| missing > 0)
    }

    fn can_install(&self) -> bool {
        self.selected.is_some() && self.target().is_some() && self.missing_space().is_none()
    }
}

fn size_item(icon: Icon, label: SharedString, color: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .gap(rems(0.375))
        .child(icon.size_sm().color(color))
        .child(label)
}
