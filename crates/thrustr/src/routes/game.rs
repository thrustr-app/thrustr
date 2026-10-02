use super::{Route, cover_path};
use crate::{
    adapters::ColorExt,
    context::SpawnTaskExt,
    globals::{ComponentRegistryExt, GameServiceExt},
    navigation::NavigatorExt,
};
use anyhow::Context as _;
use config::paths;
use domain::{game::GameId, platform::Platform};
use download::{InstallTarget, InstallTargetError};
use gpui::{
    AnyElement, AppContext, BoxShadow, ClickEvent, Context, Entity, FontWeight, Hsla, ImageSource,
    IntoElement, ObjectFit, ParentElement, PathPromptOptions, Rems, Render, Resource,
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
    Alert, Button, Icon, InputEvent, PortalContext, Select, WithField, WithFocus, WithRadius,
    WithSize, WithVariant, input,
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
        let name = self.name.clone();
        let cover_path = self.cover_path.clone();
        let image_cache = self.image_cache.clone();
        let install_dialog =
            cx.new(|cx| InstallDialog::new(self.id, source_id, name, cover_path, image_cache, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(rems(36.))
                .title(title.clone())
                .ok_text("Install")
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
                            .radius_pill()
                            .variant_accent()
                            .shadow(vec![
                                BoxShadow::new(px(0.), px(0.), theme.colors.accent.opacity(0.35))
                                    .blur_radius(px(6.)),
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
    let shadow = vec![BoxShadow::new(px(0.), px(4.), black().opacity(0.3)).blur_radius(px(5.))];

    let Some(path) = path else {
        return div()
            .flex_shrink_0()
            .h(height)
            .aspect_ratio(COVER_PLACEHOLDER_ASPECT_RATIO)
            .rounded(theme.radius.md)
            .bg(theme.colors.surface)
            .shadow(shadow)
            .into_any_element();
    };

    img(ImageSource::Resource(Resource::Path(path)))
        .flex_shrink_0()
        .h(height)
        .aspect_ratio(COVER_PLACEHOLDER_ASPECT_RATIO)
        .object_fit(ObjectFit::Cover)
        .rounded(theme.radius.md)
        .bg(theme.colors.surface)
        .shadow(shadow)
        .into_any_element()
}

#[derive(Clone, PartialEq)]
struct VersionOption {
    id: SharedString,
    label: SharedString,
    platform: Platform,
}

fn platform_icon(platform: Platform) -> Option<Icon> {
    match platform {
        Platform::Windows => Some(Icon::windows()),
        Platform::Linux => Some(Icon::linux()),
        Platform::Macos => Some(Icon::apple()),
    }
}

struct InstallDialog {
    name: SharedString,
    cover_path: Option<Arc<Path>>,
    image_cache: Entity<RetainAllImageCache>,
    versions: Vec<VersionOption>,
    selected: Option<VersionOption>,
    install_dir: SharedString,
    install_target: Option<Result<InstallTarget, InstallTargetError>>,
    loading: bool,
    error: Option<SharedString>,
    _load_task: Task<()>,
    _browse_task: Option<Task<()>>,
    _install_target_task: Option<Task<()>>,
}

impl InstallDialog {
    fn new(
        game_id: GameId,
        source_id: &str,
        name: SharedString,
        cover_path: Option<Arc<Path>>,
        image_cache: Entity<RetainAllImageCache>,
        cx: &mut Context<Self>,
    ) -> Self {
        let storefront = cx.component_registry().storefront(source_id);
        let load_task = cx.spawn_and_update(
            async move {
                let storefront = storefront.context("storefront is not available")?;
                anyhow::Ok(storefront.list_game_versions(game_id).await?)
            },
            |this, result, _| {
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
                        this.selected = this.versions.first().cloned();
                    }
                    Err(e) => this.error = Some(format!("{e:#}").into()),
                }
            },
        );

        let mut this = Self {
            name,
            cover_path,
            image_cache,
            versions: Vec::new(),
            selected: None,
            install_dir: paths::default_install_dir()
                .to_string_lossy()
                .into_owned()
                .into(),
            install_target: None,
            loading: true,
            error: None,
            _load_task: load_task,
            _browse_task: None,
            _install_target_task: None,
        };
        this.update_install_target(cx);
        this
    }

    fn update_install_target(&mut self, cx: &mut Context<Self>) {
        let dir = PathBuf::from(self.install_dir.as_ref());
        let name = self.name.clone();
        self._install_target_task = Some(cx.spawn_and_update(
            async move { InstallTarget::resolve(&dir, &name) },
            |this, result, _| this.install_target = Some(result),
        ));
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
                        .children(
                            platform_icon(version.platform)
                                .map(|icon| icon.size_sm().color(cx.theme().colors.primary)),
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
                    this.selected = Some(version.clone());
                    cx.notify();
                })),
            )
            .child(
                input("install-dir")
                    .size_lg()
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
            .when_some(
                self.install_target.as_ref().and_then(|r| r.as_ref().ok()),
                |fields, target| {
                    let theme = cx.theme();
                    fields.child(
                        div()
                            .text_size(theme.text.sm)
                            .text_color(theme.colors.secondary)
                            .child(format!(
                                "{} free of {}",
                                format_size(target.available_space),
                                format_size(target.total_space)
                            )),
                    )
                },
            )
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024. && unit < UNITS.len() - 1 {
        size /= 1024.;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
