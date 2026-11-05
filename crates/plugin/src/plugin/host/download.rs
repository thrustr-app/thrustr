use crate::{
    plugin::PluginState,
    wit::thrustr::plugin::download::{
        DownloadPhase as PluginPhase, Host as DownloadHost, HostReporter,
    },
};
use domain::download::DownloadPhase;
use wasmtime::component::Resource;
use wasmtime_wasi::WasiView;

pub struct Reporter;

impl PluginState {
    pub(crate) fn reporter(&mut self) -> wasmtime::Result<Resource<Reporter>> {
        let reporter = self.ctx().table.push(Reporter)?;
        Ok(Resource::new_borrow(reporter.rep()))
    }
}

impl DownloadHost for PluginState {}

impl HostReporter for PluginState {
    fn report_progress(
        &mut self,
        _reporter: Resource<Reporter>,
        phase: PluginPhase,
        processed: u64,
        total: Option<u64>,
    ) {
        if let Some(reporter) = &self.reporter {
            reporter.progress(phase.into(), processed, total);
        }
    }

    fn drop(&mut self, reporter: Resource<Reporter>) -> wasmtime::Result<()> {
        self.ctx().table.delete(reporter)?;
        Ok(())
    }
}

impl From<PluginPhase> for DownloadPhase {
    fn from(value: PluginPhase) -> Self {
        match value {
            PluginPhase::Preparing => DownloadPhase::Preparing,
            PluginPhase::Downloading => DownloadPhase::Downloading,
            PluginPhase::Installing => DownloadPhase::Installing,
            PluginPhase::Verifying => DownloadPhase::Verifying,
            PluginPhase::Finalizing => DownloadPhase::Finalizing,
        }
    }
}
