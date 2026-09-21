use artwork::ArtworkService;
use gpui::{App, Global};

pub(super) struct ArtworkServiceGlobal(pub ArtworkService);
impl Global for ArtworkServiceGlobal {}

pub(super) fn init(cx: &mut App, service: ArtworkService) {
    cx.set_global(ArtworkServiceGlobal(service));
}

pub trait ArtworkServiceExt {
    fn artwork_service(&self) -> ArtworkService;
}

impl ArtworkServiceExt for App {
    fn artwork_service(&self) -> ArtworkService {
        self.global::<ArtworkServiceGlobal>().0.clone()
    }
}
