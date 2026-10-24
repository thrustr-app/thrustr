use super::{Route, cover_path};
use crate::{
    adapters::{ColorExt, PlatformExt},
    context::{EventListenerExt, SpawnTaskExt},
    format::format_size,
    globals::{ComponentRegistryExt, GameServiceExt},
    navigation::NavigatorExt,
};
use anyhow::Context as _;
use component::StorefrontHandle;
use config::paths;
use domain::{
    component::StorefrontOperation,
    game::{GameId, GameVersionDetails},
    platform::Platform,
};
use download::InstallTarget;
use event::Topic;
use gpui::{
    AnyElement, App, AppContext, BoxShadow, ClickEvent, Context, Div, Entity, FontWeight, Hsla,
    ImageSource, IntoElement, ObjectFit, ParentElement, PathPromptOptions, Rems, Render, Resource,
    RetainAllImageCache, SharedString, Styled, StyledImage, Subscription, Task, Window, black, div,
    img, linear_color_stop, linear_gradient, prelude::FluentBuilder, px, relative, rems,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use theme::{Theme, ThemeExt};
use tracing::error;
use ui::{
    Alert, Button, Dialog, Icon, Input, InputEvent, InputState, Meter, PortalContext, Select,
    WithField, WithRadius, WithSize, WithVariant,
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
    _components_task: Task<()>,
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
            _components_task: cx.listen_any(
                &[Topic::ComponentRegistered, Topic::ComponentState],
                |_, cx| cx.notify(),
            ),
        }
    }
}

impl Game {
    fn can_list_versions(&self, cx: &App) -> bool {
        self.source_id
            .as_ref()
            .and_then(|id| cx.component_registry().storefront(id))
            .is_some_and(|storefront| can_list_versions(&storefront))
    }

    fn open_install_dialog(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source_id) = &self.source_id else {
            return;
        };
        let title: SharedString = format!("Install {}", self.name).into();
        let install_dialog = cx.new(|cx| InstallDialog::new(self, source_id, window, cx));
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
                            .when(!self.can_list_versions(cx), Button::disabled)
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

fn can_list_versions(storefront: &StorefrontHandle) -> bool {
    storefront
        .component()
        .allows(StorefrontOperation::ListVersions)
}

#[derive(Clone, PartialEq)]
struct VersionOption {
    id: SharedString,
    label: SharedString,
    /// `None` for nested versions.
    platform: Option<Platform>,
    children_label: Option<SharedString>,
}

struct VersionLevel {
    label: SharedString,
    options: Vec<VersionOption>,
    selected: Option<VersionOption>,
    loading: bool,
    error: Option<SharedString>,
    _task: Task<()>,
}

#[derive(Default)]
struct Labels {
    target_dir: Option<SharedString>,
    free_space: Option<SharedString>,
    download_size: Option<SharedString>,
    install_size: Option<SharedString>,
    missing_space: Option<SharedString>,
}

struct InstallDialog {
    game_id: GameId,
    storefront: Option<StorefrontHandle>,
    game_name: SharedString,
    cover_path: Option<Arc<Path>>,
    image_cache: Entity<RetainAllImageCache>,
    levels: Vec<VersionLevel>,
    details: Option<GameVersionDetails>,
    details_loading: bool,
    details_failed: bool,
    install_location: Entity<InputState>,
    install_target: Option<Result<InstallTarget, SharedString>>,
    labels: Labels,
    _details_task: Option<Task<()>>,
    _browse_task: Option<Task<()>>,
    _install_target_task: Option<Task<()>>,
    _components_task: Task<()>,
    _install_location_subscription: Subscription,
}

impl InstallDialog {
    fn new(game: &Game, source_id: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let install_location = cx.new(|cx| {
            let location = paths::default_install_dir().to_string_lossy().into_owned();
            InputState::new(window, cx).with_value(location)
        });
        let _install_location_subscription =
            cx.subscribe(&install_location, |this, _, event: &InputEvent, cx| {
                if *event == InputEvent::Input {
                    this.resolve_install_target(cx);
                    cx.notify();
                }
            });

        let mut this = Self {
            game_id: game.id,
            storefront: cx.component_registry().storefront(source_id),
            game_name: game.name.clone(),
            cover_path: game.cover_path.clone(),
            image_cache: game.image_cache.clone(),
            levels: Vec::new(),
            details: None,
            details_loading: false,
            details_failed: false,
            install_location,
            install_target: None,
            labels: Labels::default(),
            _details_task: None,
            _browse_task: None,
            _install_target_task: None,
            _components_task: cx.listen_any(
                &[Topic::ComponentRegistered, Topic::ComponentState],
                |_, cx| cx.notify(),
            ),
            _install_location_subscription,
        };
        this.load_level("Game version".into(), cx);
        this.resolve_install_target(cx);
        this
    }

    fn load_level(&mut self, label: SharedString, cx: &mut Context<Self>) {
        let depth = self.levels.len();
        let storefront = self.storefront.clone();
        let game_id = self.game_id;
        let path = self.selected_path();

        let task = cx.spawn_and_update(
            async move {
                let storefront = storefront.context("storefront is not available")?;
                anyhow::Ok(storefront.list_game_versions(game_id, path).await?)
            },
            move |this, result, cx| {
                let Some(level) = this.levels.get_mut(depth) else {
                    return;
                };
                level.loading = false;
                match result {
                    Ok(versions) => {
                        level.options = versions
                            .into_iter()
                            .filter(|v| depth > 0 || v.platform.is_supported())
                            .map(|v| VersionOption {
                                label: v.pretty_name.unwrap_or_else(|| v.id.clone()).into(),
                                id: v.id.into(),
                                platform: (depth == 0).then_some(v.platform),
                                children_label: v.children_label.map(Into::into),
                            })
                            .collect();
                        level
                            .options
                            .sort_by_key(|o| o.platform != Some(Platform::CURRENT));
                        let first = level.options.first().cloned();
                        this.select_version(depth, first, cx);
                    }
                    Err(e) => level.error = Some(format!("{e:#}").into()),
                }
            },
        );

        self.levels.push(VersionLevel {
            label,
            options: Vec::new(),
            selected: None,
            loading: true,
            error: None,
            _task: task,
        });
    }

    fn select_version(
        &mut self,
        depth: usize,
        version: Option<VersionOption>,
        cx: &mut Context<Self>,
    ) {
        self.levels.truncate(depth + 1);
        self.levels[depth].selected = version.clone();
        self.details = None;
        self.details_loading = false;
        self.details_failed = false;
        self._details_task = None;
        self.update_labels();

        let Some(version) = version else {
            return;
        };
        if let Some(label) = version.children_label {
            self.load_level(label, cx);
            return;
        }

        let Some(storefront) = self.storefront.clone() else {
            return;
        };
        let game_id = self.game_id;
        let path = self.selected_path();
        self.details_loading = true;
        self._details_task = Some(cx.spawn_and_update(
            async move { storefront.game_version_details(game_id, path).await },
            |this, result, _| {
                this.details_loading = false;
                match result {
                    Ok(details) => this.details = Some(details),
                    Err(e) => {
                        error!("failed to load version details: {e:#}");
                        this.details_failed = true;
                    }
                }
                this.update_labels();
            },
        ));
    }

    fn selected_path(&self) -> Vec<String> {
        self.levels
            .iter()
            .map_while(|level| level.selected.as_ref())
            .map(|version| version.id.to_string())
            .collect()
    }

    fn browse_install_location(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select".into()),
        });
        self._browse_task = Some(cx.spawn_and_update(paths, |this, result, cx| match result {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let location = path.to_string_lossy().into_owned();
                    this.install_location
                        .update(cx, |input, cx| input.set_value(location, cx));
                    this.resolve_install_target(cx);
                }
            }
            Ok(Err(e)) => error!("failed to open the folder picker: {e:#}"),
            Ok(Ok(None)) | Err(_) => {}
        }));
    }

    fn resolve_install_target(&mut self, cx: &mut Context<Self>) {
        let location = PathBuf::from(self.install_location.read(cx).value().as_ref());
        let game_name = self.game_name.clone();
        self._install_target_task = Some(cx.spawn_and_update(
            async move { InstallTarget::resolve(&location, &game_name) },
            |this, result, _| {
                this.install_target = Some(result.map_err(|e| e.to_string().into()));
                this.update_labels();
            },
        ));
    }

    fn update_labels(&mut self) {
        let target = self.target();
        let details = self.details.as_ref();
        self.labels = Labels {
            target_dir: target.map(|target| target.dir.to_string_lossy().into_owned().into()),
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

    fn required_space(&self) -> Option<u64> {
        let details = self.details.as_ref()?;
        details.install_size.or(details.download_size)
    }

    fn target(&self) -> Option<&InstallTarget> {
        self.install_target.as_ref()?.as_ref().ok()
    }

    fn target_error(&self) -> Option<&SharedString> {
        self.install_target.as_ref()?.as_ref().err()
    }

    fn missing_space(&self) -> Option<u64> {
        self.required_space()?
            .checked_sub(self.target()?.available_space)
            .filter(|&missing| missing > 0)
    }

    fn selected_leaf(&self) -> Option<&VersionOption> {
        self.levels
            .last()?
            .selected
            .as_ref()
            .filter(|version| version.children_label.is_none())
    }

    fn can_install(&self) -> bool {
        self.storefront.as_ref().is_some_and(can_list_versions)
            && self.selected_leaf().is_some()
            && !self.details_loading
            && self.target().is_some()
            && self.missing_space().is_none()
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
            .children(
                self.levels
                    .iter()
                    .filter_map(|level| level.error.clone())
                    .map(Alert::new),
            )
            .children(self.levels.iter().enumerate().map(|(depth, level)| {
                Select::new(("version", depth), render_version)
                    .size_lg()
                    .w_full()
                    .label(level.label.clone())
                    .placeholder("Select a version")
                    .loading_placeholder("Fetching available versions")
                    .empty_placeholder(match (&level.error, depth) {
                        (Some(_), _) => "Couldn't load versions",
                        (None, 0) => "No versions available for this platform",
                        (None, _) => "No versions available",
                    })
                    .items(level.options.iter().cloned())
                    .value(level.selected.clone())
                    .when(level.loading, Select::loading)
                    .on_change(cx.listener(move |this, version: &VersionOption, _, cx| {
                        this.select_version(depth, Some(version.clone()), cx);
                        cx.notify();
                    }))
            }))
            .child(
                Input::new(&self.install_location)
                    .size_lg()
                    .w_full()
                    .label("Install location")
                    .when_some(self.labels.target_dir.clone(), WithField::description)
                    .when_some(self.target_error().cloned(), WithField::error)
                    .trailing(
                        Button::icon("browse-install-location", Icon::folder())
                            .size_lg()
                            .variant_field()
                            .on_click(cx.listener(Self::browse_install_location)),
                    ),
            )
            .children(self.render_storage(cx))
    }

    fn render_storage(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        if self.target_error().is_some() {
            return None;
        }

        let theme = cx.theme();
        let labels = &self.labels;

        let game_color = if labels.missing_space.is_some() {
            theme.colors.danger
        } else {
            theme.colors.primary
        };
        let download_color = if labels.install_size.is_some() {
            theme.colors.secondary
        } else {
            game_color
        };

        let meter = match self.target() {
            Some(target) => Meter::new(target.total_space as f64)
                .segment(
                    target.total_space.saturating_sub(target.available_space) as f64,
                    theme.colors.tertiary,
                )
                .segment(self.required_space().unwrap_or(0) as f64, game_color),
            None => Meter::new(0.),
        };

        let size_status = if self.details_loading || self.levels.iter().any(|level| level.loading) {
            Some("Calculating size…")
        } else if self.details_failed {
            Some("Couldn't fetch the game size")
        } else {
            None
        };

        Some(
            div()
                .flex()
                .flex_col()
                .gap(rems(0.625))
                .child(
                    meter.id("storage-meter").label("Storage").value_label(
                        labels
                            .free_space
                            .clone()
                            .unwrap_or_else(|| "Calculating free space…".into()),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(rems(0.5))
                        .text_size(theme.text.sm)
                        .line_height(relative(1.))
                        .text_color(theme.colors.secondary)
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(rems(1.))
                                .children(size_status)
                                .children(labels.download_size.clone().map(|label| {
                                    size_item(Icon::download(), label, download_color)
                                }))
                                .children(
                                    labels
                                        .install_size
                                        .clone()
                                        .map(|label| size_item(Icon::disk(), label, game_color)),
                                ),
                        )
                        .children(
                            labels
                                .missing_space
                                .clone()
                                .map(|label| div().text_color(theme.colors.danger).child(label)),
                        ),
                ),
        )
    }
}

fn render_version(version: &VersionOption, _: &mut Window, cx: &mut App) -> Div {
    div()
        .flex()
        .items_center()
        .gap(rems(0.5))
        .children(
            version
                .platform
                .and_then(Platform::icon)
                .map(|icon| icon.size_sm().color(cx.theme().colors.primary)),
        )
        .child(div().min_w_0().child(version.label.clone()))
}

fn size_item(icon: Icon, label: SharedString, color: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .gap(rems(0.375))
        .child(icon.size_sm().color(color))
        .child(label)
}
