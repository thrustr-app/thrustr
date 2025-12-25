mod pdk;

use extism_pdk::*;
use pdk::*;
use serde_json::{Map, Value};

pub(crate) fn initialize() -> Result<(), Error> {
    get_plugin_data()?;

    let dummy_auth_call_response =
        http::request::<()>(&HttpRequest::new("https://httpbin.org/get"), None)?;
    let json: Map<String, Value> = serde_json::from_slice(&dummy_auth_call_response.body())?;

    set_plugin_data(json)?;
    Ok(())
}
