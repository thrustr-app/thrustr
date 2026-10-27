use domain::download::{Download, DownloadId};

#[derive(Debug, Default)]
pub struct Queue {
    downloads: Vec<Download>,
    paused: bool,
}

impl Queue {
    pub fn new(downloads: Vec<Download>, paused: bool) -> Self {
        Self { downloads, paused }
    }

    pub fn downloads(&self) -> &[Download] {
        &self.downloads
    }

    pub fn get(&self, id: DownloadId) -> Option<&Download> {
        self.downloads.iter().find(|d| d.id == id)
    }

    pub fn get_mut(&mut self, id: DownloadId) -> Option<&mut Download> {
        self.downloads.iter_mut().find(|d| d.id == id)
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn active(&self) -> Option<&Download> {
        self.downloads.iter().find(|d| d.error.is_none())
    }

    pub fn push(&mut self, download: Download) {
        self.downloads.push(download);
    }

    pub fn remove(&mut self, id: DownloadId) -> Option<Download> {
        let index = self.index_of(id)?;
        Some(self.downloads.remove(index))
    }

    pub fn move_to(&mut self, id: DownloadId, position: u32) -> bool {
        let Some(from) = self.index_of(id) else {
            return false;
        };
        let download = self.downloads.remove(from);
        let to = (position as usize).min(self.downloads.len());
        self.downloads.insert(to, download);
        true
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
            downloaded: 0,
            error: None,
        }
    }

    #[track_caller]
    fn apply(queue: &mut Queue, op: Op) {
        match op {
            Push(id) => queue.push(download(id)),
            Remove(id) => {
                let queued = queue.get(id.into()).is_some();
                assert_eq!(queue.remove(id.into()).is_some(), queued, "{op:?}");
            }
            Move(id, position) => {
                let queued = queue.get(id.into()).is_some();
                assert_eq!(queue.move_to(id.into(), position), queued, "{op:?}");
            }
            Fail(id) => queue.get_mut(id.into()).unwrap().error = Some("failed".to_string()),
            Retry(id) => queue.get_mut(id.into()).unwrap().error = None,
            Pause => queue.set_paused(true),
            Resume => queue.set_paused(false),
        }
    }

    /// `>` marks the active download and `!` a failed one.
    fn render(queue: &Queue) -> String {
        let active = queue.active().map(|d| d.id);
        let downloads = queue
            .downloads()
            .iter()
            .map(|d| {
                let marker = if Some(d.id) == active { ">" } else { "" };
                let failed = if d.error.is_some() { "!" } else { "" };
                format!("{marker}{}{failed}", d.id)
            })
            .collect::<Vec<_>>()
            .join(" ");

        if queue.is_paused() {
            format!("paused: {downloads}")
        } else {
            downloads
        }
    }

    #[track_caller]
    fn check(ops: &[Op], expected: &str) {
        let mut queue = Queue::default();
        for &op in ops {
            apply(&mut queue, op);
        }
        assert_eq!(render(&queue), expected);
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
                let mut queue = Queue::default();
                for other in ids {
                    apply(&mut queue, Push(other));
                }
                apply(&mut queue, Move(id, position));

                let mut expected: Vec<u64> = ids.into_iter().filter(|&other| other != id).collect();
                expected.insert((position as usize).min(expected.len()), id);
                let order: Vec<u64> = queue.downloads().iter().map(|d| d.id.into()).collect();
                assert_eq!(order, expected, "{:?}", Move(id, position));
            }
        }
    }
}
