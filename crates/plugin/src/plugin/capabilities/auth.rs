use crate::plugin::{PluginRuntime, guest_call};
use crate::wit::exports::thrustr::plugin::auth::{
    self, AuthFlow as PluginAuthFlow, LoginFlow, LoginForm as PluginLoginForm,
    LoginRequest as PluginLoginRequest,
};
use anyhow::bail;
use async_trait::async_trait;
use domain::component::{Auth, AuthFlow, Error, LoginForm, LoginRequest};
use std::sync::Arc;

pub struct PluginAuth {
    runtime: Arc<PluginRuntime>,
    indices: auth::GuestIndices,
    form: Option<LoginForm>,
}

impl PluginAuth {
    pub fn resolve(
        runtime: &Arc<PluginRuntime>,
        form: Option<domain::component::LoginForm>,
    ) -> anyhow::Result<Option<Self>> {
        let indices = runtime.export("auth", auth::GuestIndices::new)?;
        Ok(pair(indices, form)?.map(|(indices, form)| Self {
            runtime: runtime.clone(),
            indices,
            form,
        }))
    }
}

fn pair<I, F>(indices: Option<I>, form: Option<F>) -> anyhow::Result<Option<(I, Option<F>)>> {
    match (indices, form) {
        (Some(indices), form) => Ok(Some((indices, form))),
        (None, None) => Ok(None),
        (None, Some(_)) => bail!("plugin manifest has [auth] but the plugin does not export Auth"),
    }
}

#[async_trait]
impl Auth for PluginAuth {
    fn login_form(&self) -> Option<&LoginForm> {
        self.form.as_ref()
    }

    async fn login_flow(&self) -> Result<Option<AuthFlow>, Error> {
        let flow = guest_call!(self.runtime, self.indices, |auth, accessor| {
            auth.call_get_login_flow(accessor)
        })?;

        Ok(flow.map(Into::into))
    }

    async fn logout_flow(&self) -> Result<Option<AuthFlow>, Error> {
        let flow = guest_call!(self.runtime, self.indices, |auth, accessor| {
            auth.call_get_logout_flow(accessor)
        })?;

        Ok(flow.map(Into::into))
    }

    async fn login(&self, request: LoginRequest) -> Result<(), Error> {
        guest_call!(self.runtime, self.indices, |auth, accessor| {
            auth.call_login(accessor, request.into())
        })
    }

    async fn logout(&self) -> Result<(), Error> {
        guest_call!(self.runtime, self.indices, |auth, accessor| {
            auth.call_logout(accessor)
        })
    }
}

impl From<PluginAuthFlow> for AuthFlow {
    fn from(value: PluginAuthFlow) -> Self {
        AuthFlow {
            url: value.url,
            target: value.target,
        }
    }
}

impl From<LoginRequest> for PluginLoginRequest {
    fn from(value: LoginRequest) -> Self {
        match value {
            LoginRequest::Flow { url, body } => PluginLoginRequest::Flow(LoginFlow { url, body }),
            LoginRequest::Form { fields } => PluginLoginRequest::Form(PluginLoginForm { fields }),
        }
    }
}
