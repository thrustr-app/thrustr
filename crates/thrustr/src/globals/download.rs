use download::DownloadService;
use gpui::{App, Global};

pub(super) struct DownloadServiceGlobal(DownloadService);

impl Global for DownloadServiceGlobal {}

pub(super) fn init(cx: &mut App, service: DownloadService) {
    cx.set_global(DownloadServiceGlobal(service));
}

pub trait DownloadServiceExt {
    fn download_service(&self) -> DownloadService;
}

impl DownloadServiceExt for App {
    fn download_service(&self) -> DownloadService {
        self.global::<DownloadServiceGlobal>().0.clone()
    }
}
