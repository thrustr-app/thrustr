use crate::{
    plugin::PluginState,
    wit::thrustr::plugin::download::{
        DownloadPhase as PluginPhase, Host as DownloadHost, HostReporter,
        PhaseProgress as PluginProgress,
    },
};
use domain::download::{DownloadPhase, PhaseProgress};
use wasmtime::component::Resource;
use wasmtime_wasi::WasiView;

pub struct Reporter;

impl PluginState {
    pub fn reporter(&mut self) -> wasmtime::Result<Resource<Reporter>> {
        let reporter = self.ctx().table.push(Reporter)?;
        Ok(Resource::new_borrow(reporter.rep()))
    }
}

impl DownloadHost for PluginState {}

impl HostReporter for PluginState {
    fn report(&mut self, _reporter: Resource<Reporter>, phase: PluginPhase) {
        if let Some(reporter) = &self.reporter {
            reporter.report(phase.into());
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
            PluginPhase::Preparing => Self::Preparing,
            PluginPhase::Downloading(progress) => Self::Downloading(progress.into()),
            PluginPhase::Installing(progress) => Self::Installing(progress.into()),
            PluginPhase::Verifying(progress) => Self::Verifying(progress.into()),
            PluginPhase::Finalizing => Self::Finalizing,
        }
    }
}

impl From<PluginProgress> for PhaseProgress {
    fn from(value: PluginProgress) -> Self {
        Self {
            processed: value.processed,
            total: value.total,
        }
    }
}
