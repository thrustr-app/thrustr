use anyhow::{Context, Result};
use domain::component::AuthFlow;
use smol::unblock;
use std::process::{Command, Stdio};

// TODO: tell apart user cancellation/window close from helper errors.
pub async fn open_auth_webview(flow: AuthFlow) -> Result<Option<(String, String)>> {
    unblock(move || run_helper(&flow.url, &flow.target)).await
}

fn run_helper(url: &str, target: &str) -> Result<Option<(String, String)>> {
    let helper = std::env::current_exe()?
        .parent()
        .context("failed to find parent directory")?
        .join("webview-helper");

    let output = Command::new(helper)
        .arg(url)
        .arg(target)
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    if !output.status.success() {
        return Ok(None);
    }

    let result = String::from_utf8(output.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(result.trim())?;
    let url = parsed["url"].as_str().context("could not find url")?;
    let body = parsed["body"].as_str().context("could not find body")?;

    Ok(Some((url.to_string(), body.to_string())))
}
