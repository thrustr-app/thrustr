use domain::download::{Download, DownloadId, DownloadPhase};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    WaitingForStorefront,
    WaitingForConnection,
    Retrying,
    Downloading,
}

/// Bytes per second, `None` while unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Speed {
    pub network: Option<u64>,
    pub disk: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    id: DownloadId,
    activity: Activity,
    speed: Speed,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadState {
    downloads: Vec<Download>,
    paused: bool,
    run: Option<Run>,
}

impl DownloadState {
    pub(crate) fn new(downloads: Vec<Download>, paused: bool) -> Self {
        Self {
            downloads,
            paused,
            run: None,
        }
    }

    pub fn downloads(&self) -> &[Download] {
        &self.downloads
    }

    pub fn get(&self, id: DownloadId) -> Option<&Download> {
        self.downloads.iter().find(|d| d.id == id)
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn active(&self) -> Option<&Download> {
        self.downloads.iter().find(|d| d.error.is_none())
    }

    pub fn running(&self) -> Option<&Download> {
        match self.paused {
            true => None,
            false => self.active(),
        }
    }

    pub fn activity(&self) -> Option<Activity> {
        self.run().map(|run| run.activity)
    }

    pub fn speed(&self) -> Option<Speed> {
        self.run()
            .filter(|run| run.activity == Activity::Downloading)
            .map(|run| run.speed)
    }

    fn run(&self) -> Option<&Run> {
        self.run
            .as_ref()
            .filter(|run| self.running_id() == Some(run.id))
    }

    pub(crate) fn running_id(&self) -> Option<DownloadId> {
        self.running().map(|d| d.id)
    }

    pub(crate) fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Returns whether the error changed.
    pub(crate) fn set_error(&mut self, id: DownloadId, error: Option<String>) -> bool {
        let Some(download) = self.get_mut(id) else {
            return false;
        };
        let changed = download.error != error;
        download.error = error;
        changed
    }

    pub(crate) fn push(&mut self, download: Download) {
        self.downloads.push(download);
    }

    pub(crate) fn remove(&mut self, id: DownloadId) -> Option<Download> {
        let index = self.index_of(id)?;
        Some(self.downloads.remove(index))
    }

    pub(crate) fn move_to(&mut self, id: DownloadId, position: u32) -> bool {
        let Some(from) = self.index_of(id) else {
            return false;
        };
        let download = self.downloads.remove(from);
        let to = (position as usize).min(self.downloads.len());
        self.downloads.insert(to, download);
        true
    }

    /// Returns whether the activity changed.
    pub(crate) fn set_activity(&mut self, id: DownloadId, activity: Activity) -> bool {
        match &mut self.run {
            Some(run) if run.id == id => {
                let changed = run.activity != activity;
                run.activity = activity;
                changed
            }
            run => {
                *run = Some(Run {
                    id,
                    activity,
                    speed: Speed::default(),
                });
                true
            }
        }
    }

    /// Records the speed of a download that has an activity. Returns whether
    /// it changed.
    pub(crate) fn set_speed(&mut self, id: DownloadId, speed: Speed) -> bool {
        match &mut self.run {
            Some(run) if run.id == id => {
                let changed = run.speed != speed;
                run.speed = speed;
                changed
            }
            _ => false,
        }
    }

    pub(crate) fn clear_run(&mut self) -> bool {
        self.run.take().is_some()
    }

    /// Records progress of the running download. Returns whether it changed.
    pub(crate) fn report(
        &mut self,
        id: DownloadId,
        phase: DownloadPhase,
        processed: u64,
        total: Option<u64>,
    ) -> bool {
        if self.running_id() != Some(id) {
            return false;
        }
        let Some(download) = self.get_mut(id) else {
            return false;
        };
        let changed =
            (download.phase, download.processed, download.total) != (phase, processed, total);
        download.phase = phase;
        download.processed = processed;
        download.total = total;
        changed
    }

    fn get_mut(&mut self, id: DownloadId) -> Option<&mut Download> {
        self.downloads.iter_mut().find(|d| d.id == id)
    }

    fn index_of(&self, id: DownloadId) -> Option<usize> {
        self.downloads.iter().position(|d| d.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    enum Op {
        Push(u64),
        Remove(u64),
        Move(u64, u32),
        Fail(u64),
        Retry(u64),
        Pause,
        Resume,
        SetActivity(u64, Activity),
        SetSpeed(u64, Speed),
        Report(u64, DownloadPhase, u64, Option<u64>),
    }

    use Op::*;

    fn download(id: u64) -> Download {
        Download {
            id: id.into(),
            game_id: id.into(),
            version_path: vec![],
            target_dir: format!("/games/{id}").into(),
            install_size: None,
            download_size: None,
            phase: DownloadPhase::Downloading,
            processed: 0,
            total: None,
            error: None,
        }
    }

    /// Returns whether the operation reported a change.
    #[track_caller]
    fn apply(state: &mut DownloadState, op: Op) -> bool {
        match op {
            Push(id) => {
                state.push(download(id));
                true
            }
            Remove(id) => {
                let queued = state.get(id.into()).is_some();
                let removed = state.remove(id.into()).is_some();
                assert_eq!(removed, queued, "{op:?}");
                removed
            }
            Move(id, position) => {
                let queued = state.get(id.into()).is_some();
                let moved = state.move_to(id.into(), position);
                assert_eq!(moved, queued, "{op:?}");
                moved
            }
            Fail(id) => state.set_error(id.into(), Some("failed".to_string())),
            Retry(id) => state.set_error(id.into(), None),
            Pause => {
                state.set_paused(true);
                true
            }
            Resume => {
                state.set_paused(false);
                true
            }
            SetActivity(id, activity) => state.set_activity(id.into(), activity),
            SetSpeed(id, speed) => state.set_speed(id.into(), speed),
            Report(id, phase, processed, total) => state.report(id.into(), phase, processed, total),
        }
    }

    #[track_caller]
    fn state(ops: &[Op]) -> DownloadState {
        let mut state = DownloadState::default();
        for &op in ops {
            apply(&mut state, op);
        }
        state
    }

    /// `>` marks the active download and `!` a failed one. The running
    /// download shows its `[activity]` and measured `{network disk}` speeds,
    /// and reported progress shows as `(phase processed/total)`.
    fn render(state: &DownloadState) -> String {
        let active = state.active().map(|d| d.id);
        let running = state.running().map(|d| d.id);
        let downloads = state
            .downloads()
            .iter()
            .map(|d| {
                let marker = if Some(d.id) == active { ">" } else { "" };
                let failed = if d.error.is_some() { "!" } else { "" };
                let activity = match state.activity() {
                    Some(activity) if Some(d.id) == running => format!("[{activity:?}]"),
                    _ => String::new(),
                };
                let speed = match state.speed() {
                    Some(speed) if Some(d.id) == running && speed != Speed::default() => {
                        let rate =
                            |rate: Option<u64>| rate.map_or("?".to_string(), |r| r.to_string());
                        format!("{{{} {}}}", rate(speed.network), rate(speed.disk))
                    }
                    _ => String::new(),
                };
                let progress = match (d.phase, d.processed, d.total) {
                    (DownloadPhase::Downloading, 0, None) => String::new(),
                    (phase, processed, total) => {
                        let total = total.map_or("?".to_string(), |total| total.to_string());
                        format!("({phase:?} {processed}/{total})")
                    }
                };
                format!("{marker}{}{failed}{activity}{speed}{progress}", d.id)
            })
            .collect::<Vec<_>>()
            .join(" ");

        if state.is_paused() {
            format!("paused: {downloads}")
        } else {
            downloads
        }
    }

    #[track_caller]
    fn check(ops: &[Op], expected: &str) {
        assert_eq!(render(&state(ops)), expected);
    }

    /// Checks whether `op` reports a change after `ops`.
    #[track_caller]
    fn check_changes(ops: &[Op], op: Op, expected: bool) {
        assert_eq!(apply(&mut state(ops), op), expected, "{op:?} after {ops:?}");
    }

    #[test]
    fn empty_queue_has_no_active_download() {
        check(&[], "");
    }

    #[test]
    fn first_queued_download_is_active() {
        check(&[Push(1), Push(2), Push(3)], ">1 2 3");
    }

    #[test]
    fn pausing_keeps_the_active_download() {
        check(&[Push(1), Push(2), Pause], "paused: >1 2");
        check(&[Push(1), Push(2), Pause, Resume], ">1 2");
    }

    #[test]
    fn failed_downloads_are_skipped() {
        check(&[Push(1), Push(2), Push(3), Fail(1)], "1! >2 3");
        check(&[Push(1), Push(2), Fail(1), Fail(2)], "1! 2!");
    }

    #[test]
    fn retrying_a_failed_download_makes_it_active_again() {
        check(&[Push(1), Push(2), Fail(1), Retry(1)], ">1 2");
    }

    #[test]
    fn cancelling_the_active_download_activates_the_next() {
        check(&[Push(1), Push(2), Push(3), Remove(1)], ">2 3");
    }

    #[test]
    fn cancelling_an_unknown_download_does_nothing() {
        check(&[Push(1), Remove(2)], ">1");
    }

    #[test]
    fn moving_a_download_to_the_front_makes_it_active() {
        check(&[Push(1), Push(2), Push(3), Move(3, 0)], ">3 1 2");
    }

    #[test]
    fn moving_the_active_download_back_activates_the_next() {
        check(&[Push(1), Push(2), Push(3), Move(1, 1)], ">2 1 3");
    }

    #[test]
    fn moving_past_the_end_moves_to_the_end() {
        check(&[Push(1), Push(2), Push(3), Move(1, 99)], ">2 3 1");
    }

    #[test]
    fn moving_an_unknown_download_does_nothing() {
        check(&[Push(1), Push(2), Move(3, 0)], ">1 2");
    }

    #[test]
    fn moving_the_active_download_behind_a_failed_one_keeps_it_active() {
        check(&[Push(1), Push(2), Push(3), Fail(2), Move(1, 1)], "2! >1 3");
    }

    #[test]
    fn moving_while_paused_changes_the_active_download() {
        check(&[Push(1), Push(2), Pause, Move(2, 0)], "paused: >2 1");
    }

    #[test]
    fn moving_keeps_the_other_downloads_in_order() {
        let ids = [1, 2, 3, 4];
        for id in ids {
            for position in 0..=ids.len() as u32 + 1 {
                let mut ops: Vec<Op> = ids.into_iter().map(Push).collect();
                ops.push(Move(id, position));
                let state = state(&ops);

                let mut expected: Vec<u64> = ids.into_iter().filter(|&other| other != id).collect();
                expected.insert((position as usize).min(expected.len()), id);
                let order: Vec<u64> = state.downloads().iter().map(|d| d.id.into()).collect();
                assert_eq!(order, expected, "{:?}", Move(id, position));
            }
        }
    }

    fn installing(id: u64) -> Op {
        Report(id, DownloadPhase::Installing, 10, Some(20))
    }

    fn downloading(id: u64) -> Op {
        SetActivity(id, Activity::Downloading)
    }

    fn measured(id: u64) -> Op {
        SetSpeed(
            id,
            Speed {
                network: Some(1000),
                disk: Some(500),
            },
        )
    }

    #[test]
    fn a_speed_from_another_download_is_ignored() {
        check(
            &[Push(1), Push(2), downloading(1), measured(1)],
            ">1[Downloading]{1000 500} 2",
        );
        check(
            &[Push(1), Push(2), downloading(1), measured(2)],
            ">1[Downloading] 2",
        );
        check(
            &[Push(1), Push(2), downloading(1), measured(1), Pause],
            "paused: >1 2",
        );
        check(
            &[Push(1), Push(2), downloading(1), measured(1), Move(2, 0)],
            ">2 1",
        );
    }

    #[test]
    fn a_speed_needs_a_run() {
        check(&[Push(1), measured(1)], ">1");
    }

    #[test]
    fn the_speed_only_shows_while_downloading() {
        let retrying = [
            Push(1),
            downloading(1),
            measured(1),
            SetActivity(1, Activity::Retrying),
        ];
        check(&retrying, ">1[Retrying]");
        check(
            &[retrying.as_slice(), &[downloading(1)]].concat(),
            ">1[Downloading]{1000 500}",
        );
    }

    #[test]
    fn a_new_run_starts_without_a_speed() {
        check(
            &[
                Push(1),
                Push(2),
                downloading(1),
                measured(1),
                Move(2, 0),
                downloading(2),
            ],
            ">2[Downloading] 1",
        );
    }

    #[test]
    fn a_speed_not_measured_yet_is_unknown() {
        let network_only = Speed {
            network: Some(1000),
            disk: None,
        };
        check(
            &[Push(1), downloading(1), SetSpeed(1, network_only)],
            ">1[Downloading]{1000 ?}",
        );
    }

    #[test]
    fn only_the_running_download_shows_its_activity() {
        check(&[Push(1), Push(2), downloading(1)], ">1[Downloading] 2");
        check(&[Push(1), Push(2), downloading(2)], ">1 2");
        check(&[Push(1), Push(2), downloading(1), Pause], "paused: >1 2");
        check(&[Push(1), Push(2), downloading(1), Move(2, 0)], ">2 1");
    }

    #[test]
    fn progress_is_only_recorded_for_the_running_download() {
        check(&[Push(1), Push(2), installing(1)], ">1(Installing 10/20) 2");
        check(&[Push(1), Push(2), installing(2)], ">1 2");
        check(&[Push(1), Pause, installing(1)], "paused: >1");
    }

    #[test]
    fn the_last_progress_is_kept_after_stopping() {
        check(
            &[Push(1), installing(1), Pause],
            "paused: >1(Installing 10/20)",
        );
        check(
            &[Push(1), Push(2), installing(1), Move(2, 0)],
            ">2 1(Installing 10/20)",
        );
    }

    #[test]
    fn a_report_replaces_the_previous_one() {
        check(
            &[
                Push(1),
                Report(1, DownloadPhase::Downloading, 40, Some(80)),
                Report(1, DownloadPhase::Installing, 10, None),
            ],
            ">1(Installing 10/?)",
        );
    }

    #[test]
    fn repeating_an_update_is_not_a_change() {
        for op in [installing(1), downloading(1), Fail(1)] {
            check_changes(&[Push(1)], op, true);
            check_changes(&[Push(1), op], op, false);
        }
        check_changes(&[Push(1)], Retry(1), false);
        check_changes(&[Push(1), downloading(1)], measured(1), true);
        check_changes(&[Push(1), downloading(1), measured(1)], measured(1), false);
    }
}
