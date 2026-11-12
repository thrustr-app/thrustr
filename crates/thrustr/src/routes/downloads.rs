use super::Route;
use crate::{
    context::{EventListenerExt, SpawnTaskExt},
    format::format_size,
    globals::{DownloadServiceExt, GameServiceExt},
};
use anyhow::Result;
use domain::{
    download::{Download, DownloadPhase, PhaseProgress},
    game::GameId,
};
use download::{Activity, DownloadService, DownloadState};
use event::Topic;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window, div, prelude::FluentBuilder, rems,
};
use std::collections::HashMap;
use theme::ThemeExt;
use tracing::error;
use ui::{Button, Empty, Meter, WithRadius, WithScrollbar, WithSize, WithVariant};

pub struct Downloads {
    state: DownloadState,
    names: HashMap<GameId, SharedString>,
    _names_task: Option<Task<()>>,
    _downloads_task: Task<()>,
}

impl Route for Downloads {
    type Args = ();
    type State = ();

    fn build(_args: (), _state: (), _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let downloads_task = cx.listen(Topic::Downloads, |this, cx| {
            this.state = cx.download_service().state();
            this.load_names(cx);
            cx.notify();
        });

        let mut this = Self {
            state: cx.download_service().state(),
            names: HashMap::new(),
            _names_task: None,
            _downloads_task: downloads_task,
        };
        this.load_names(cx);
        this
    }

    fn header(&self, _: &Entity<Self>, _cx: &App) -> Option<AnyElement> {
        let paused = self.state.is_paused();
        Some(
            Button::new("pause-downloads")
                .variant_outline()
                .radius_pill()
                .size_lg()
                .child(if paused { "Resume" } else { "Pause" })
                .on_click(move |_, _, cx| update_queue(cx, move |s| s.set_paused(!paused)))
                .into_any_element(),
        )
    }
}

impl Downloads {
    fn load_names(&mut self, cx: &mut Context<Self>) {
        let missing: Vec<GameId> = self
            .state
            .downloads()
            .iter()
            .map(|download| download.game_id)
            .filter(|id| !self.names.contains_key(id))
            .collect();
        if missing.is_empty() {
            return;
        }

        let game_service = cx.game_service();
        self._names_task = Some(cx.spawn_and_update(
            async move { game_service.list_by_ids(&missing) },
            |this, result, _| {
                match result {
                    Ok(games) => this
                        .names
                        .extend(games.into_iter().map(|game| (game.id, game.name.into()))),
                    Err(e) => error!("failed to load the games being downloaded: {e:#}"),
                }
            },
        ));
    }

    fn status(&self, download: &Download) -> &'static str {
        let is = |other: Option<&Download>| other.is_some_and(|other| other.id == download.id);

        if download.error.is_some() {
            "Failed"
        } else if is(self.state.running()) {
            match self.state.activity() {
                Some(Activity::Downloading) => phase_label(&download.phase),
                Some(Activity::WaitingForStorefront) => "Waiting for the storefront",
                Some(Activity::WaitingForConnection) => "Waiting for a connection",
                Some(Activity::Retrying) => "Retrying",
                Some(Activity::Preparing) | None => "Preparing",
            }
        } else if is(self.state.active()) {
            "Paused"
        } else {
            "Queued"
        }
    }

    fn speed(&self, download: &Download) -> Option<String> {
        let running = self.state.running()?.id == download.id;
        let speed = self.state.speed().filter(|_| running)?;
        let rate = |rate: Option<u64>| match rate {
            Some(rate) => format!("{}/s", format_size(rate)),
            None => "–".to_string(),
        };
        Some(format!(
            "Network {} · Disk {}",
            rate(speed.network),
            rate(speed.disk)
        ))
    }

    fn render_download(&self, index: usize, download: &Download, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let id = download.id;
        let key = u64::from(id);
        let last = index + 1 == self.state.downloads().len();
        let name = self
            .names
            .get(&download.game_id)
            .cloned()
            .unwrap_or_default();

        let progress = download.phase.progress();
        let meter = match progress {
            Some(PhaseProgress {
                processed,
                total: Some(total),
            }) => Meter::new(total as f64).segment(processed as f64, theme.colors.accent),
            _ => Meter::new(0.),
        };

        let action = |name: &'static str, label: &'static str| {
            Button::new((name, key))
                .size_sm()
                .variant_outline()
                .child(label)
        };

        div()
            .flex()
            .flex_col()
            .gap(rems(0.5))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(rems(1.))
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.colors.primary)
                            .child(name),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .gap(rems(0.5))
                            .when(download.error.is_some(), |actions| {
                                actions.child(action("retry-download", "Retry").on_click(
                                    move |_, _, cx| update_queue(cx, move |s| s.retry(id)),
                                ))
                            })
                            .child(
                                action("move-download-up", "Up")
                                    .when(index == 0, Button::disabled)
                                    .on_click(move |_, _, cx| {
                                        let position = index.saturating_sub(1) as u32;
                                        update_queue(cx, move |s| s.move_to(id, position));
                                    }),
                            )
                            .child(
                                action("move-download-down", "Down")
                                    .when(last, Button::disabled)
                                    .on_click(move |_, _, cx| {
                                        let position = index as u32 + 1;
                                        update_queue(cx, move |s| s.move_to(id, position));
                                    }),
                            )
                            .child(
                                action("cancel-download", "Cancel").on_click(move |_, _, cx| {
                                    update_queue(cx, move |s| s.cancel(id))
                                }),
                            ),
                    ),
            )
            .child(
                meter
                    .id(("download-progress", key))
                    .label(self.status(download))
                    .value_label(progress.map(progress_label).unwrap_or_default()),
            )
            .children(self.speed(download).map(|speed| {
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.colors.secondary)
                    .child(speed)
            }))
            .children(download.error.clone().map(|error| {
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.colors.danger)
                    .child(error)
            }))
    }
}

impl Render for Downloads {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let downloads = self.state.downloads();
        if downloads.is_empty() {
            return Empty::new("No downloads").into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .flex_grow_1()
            .h_0()
            .gap(rems(1.5))
            .id("downloads")
            .overflow_y_scrollbar()
            .children(
                downloads
                    .iter()
                    .enumerate()
                    .map(|(index, download)| self.render_download(index, download, cx)),
            )
            .into_any_element()
    }
}

fn update_queue(
    cx: &mut App,
    action: impl FnOnce(&DownloadService) -> Result<()> + Send + 'static,
) {
    let service = cx.download_service();
    cx.background_spawn(async move {
        if let Err(e) = action(&service) {
            error!("failed to update the download queue: {e:#}");
        }
    })
    .detach();
}

fn phase_label(phase: &DownloadPhase) -> &'static str {
    match phase {
        DownloadPhase::Preparing => "Preparing",
        DownloadPhase::Downloading(_) => "Downloading",
        DownloadPhase::Installing(_) => "Installing",
        DownloadPhase::Verifying(_) => "Verifying",
        DownloadPhase::Finalizing => "Finalizing",
    }
}

fn progress_label(progress: PhaseProgress) -> String {
    let processed = format_size(progress.processed);
    match progress.total {
        Some(total) => format!("{processed} / {}", format_size(total)),
        None => processed,
    }
}
