use crate::plugin::{DownloadCall, PluginCache, PluginState, host::Writer};
use crate::wit::{export_name, thrustr::plugin::types::Error as PluginError};
use component::ComponentLink;
use domain::component::{ComponentStorage, ConfigSchema, Error as ComponentError};
use reqwest::Client;
use runtime::{AbortOnDrop, TokioHandle};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use wasmtime::component::{Instance, InstancePre};
use wasmtime::{Engine, ResourceLimiter, Store};

type CallResult<R> = wasmtime::Result<Result<R, PluginError>>;

const MAX_FUEL: u64 = 10_000_000_000;
const FUEL_YIELD_INTERVAL: u64 = 10_000_000;

pub struct PluginRuntime {
    pub id: String,
    pub allowed_hosts: Arc<[String]>,
    pub engine: Engine,
    pub pre: InstancePre<PluginState>,
    pub storage: Arc<dyn ComponentStorage>,
    pub tokio_handle: TokioHandle,
    pub http_client: Client,
    pub link: ComponentLink,
    pub cache: Arc<PluginCache>,
    pub config_schema: Option<Arc<ConfigSchema>>,
}

impl PluginRuntime {
    /// `None` if the plugin doesn't export `interface`; an error if it does
    /// but it doesn't match the host's bindings.
    pub fn export<T>(
        &self,
        interface: &str,
        new: impl FnOnce(&InstancePre<PluginState>) -> wasmtime::Result<T>,
    ) -> anyhow::Result<Option<T>> {
        let name = export_name(interface);
        if self.pre.component().get_export_index(None, &name).is_none() {
            return Ok(None);
        }
        let indices = new(&self.pre).map_err(|e| {
            e.context(format!(
                "plugin exports `{name}` but it does not match the host; rebuild it against the current pdk"
            ))
        })?;
        Ok(Some(indices))
    }

    pub async fn call<R, F, Fut>(self: &Arc<Self>, f: F) -> Result<R, ComponentError>
    where
        R: Send + 'static,
        F: FnOnce(Instance, Store<PluginState>) -> Fut + Send + 'static,
        Fut: Future<Output = CallResult<R>> + Send + 'static,
    {
        let runtime = Arc::clone(self);
        self.spawn(async move {
            let result = runtime.run(None, &CancellationToken::new(), f).await?;
            Ok(result.expect("the call isn't cancelled"))
        })
        .await
    }

    /// Runs `task` on the plugin runtime, aborting it if dropped.
    pub(crate) async fn spawn<T: Send + 'static>(
        &self,
        task: impl Future<Output = Result<T, ComponentError>> + Send + 'static,
    ) -> Result<T, ComponentError> {
        AbortOnDrop(self.tokio_handle.spawn(task))
            .await
            .map_err(|e| ComponentError::Other(format!("plugin task failed: {e}")))?
    }

    /// Instantiates the plugin in a new store and runs `f` on it, or returns
    /// `None` if cancelled. Either way, returns once its writes have stopped.
    pub(crate) async fn run<R, F, Fut>(
        &self,
        download: Option<DownloadCall>,
        cancel: &CancellationToken,
        f: F,
    ) -> Result<Option<R>, ComponentError>
    where
        F: FnOnce(Instance, Store<PluginState>) -> Fut,
        Fut: Future<Output = CallResult<R>>,
    {
        let writer = Writer::new(download.as_ref().map(|d| d.reporter.clone()));
        let call = async {
            let (instance, store) = self
                .instantiate(download, writer.clone())
                .await
                .map_err(|e| ComponentError::Other(format!("plugin setup failed: {e:#}")))?;

            f(instance, store)
                .await
                .map_err(|e| ComponentError::Other(format!("wasm call failed: {e}")))?
                .map_err(ComponentError::from)
        };

        let Some(result) = cancel.run_until_cancelled(call).await else {
            writer.abandon();
            let _ = writer.drain().await;
            return Ok(None);
        };
        let written = writer.drain().await;
        written.map_err(|e| ComponentError::Other(format!("writing failed: {e}")))?;
        result.map(Some)
    }

    async fn instantiate(
        &self,
        download: Option<DownloadCall>,
        writer: Writer,
    ) -> wasmtime::Result<(Instance, Store<PluginState>)> {
        let fuel = if download.is_some() {
            u64::MAX
        } else {
            MAX_FUEL
        };

        let mut store = Store::new(&self.engine, PluginState::new(self, download, writer)?);
        store.limiter(|state| state.limits() as &mut dyn ResourceLimiter);
        store.set_fuel(fuel)?;
        store.fuel_async_yield_interval(Some(FUEL_YIELD_INTERVAL))?;

        let instance = self.pre.instantiate_async(&mut store).await?;
        Ok((instance, store))
    }
}

/// Runs one regular guest call on the export behind `indices`.
macro_rules! guest_call {
    ($runtime:expr, $indices:expr, |$guest:ident, $accessor:ident| $call:expr) => {
        $runtime
            .call($crate::plugin::guest_fn!($indices, |$guest, $accessor| {
                $call
            }))
            .await
    };
}
pub(crate) use guest_call;

/// Builds a guest call on the export behind `indices`.
macro_rules! guest_fn {
    ($indices:expr, |$guest:ident, $accessor:ident| $call:expr) => {{
        let indices = $indices.clone();
        |instance: ::wasmtime::component::Instance,
         mut store: ::wasmtime::Store<$crate::plugin::PluginState>| async move {
            let $guest = indices.load(&mut store, &instance)?;
            store
                .run_concurrent(async |$accessor| $call.await)
                .await
                .and_then(|result| result)
        }
    }};
}
pub(crate) use guest_fn;
