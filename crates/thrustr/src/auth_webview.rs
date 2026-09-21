use std::process::{Command, Stdio};

pub enum WebviewError {
    Internal(String),
    UserCancelled,
}

pub fn open_auth_webview(url: &str, target: &str) -> Result<(String, String), WebviewError> {
    let helper = std::env::current_exe()
        .map_err(|e| WebviewError::Internal(e.to_string()))?
        .parent()
        .ok_or(WebviewError::Internal(
            "failed to find parent directory".into(),
        ))?
        .join("webview-helper");

    let output = Command::new(helper)
        .arg(url)
        .arg(target)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| WebviewError::Internal(e.to_string()))?
        .wait_with_output()
        .map_err(|e| WebviewError::Internal(e.to_string()))?;

    if output.status.success() {
        let result =
            String::from_utf8(output.stdout).map_err(|e| WebviewError::Internal(e.to_string()))?;
        let parsed: serde_json::Value = serde_json::from_str(result.trim())
            .map_err(|e| WebviewError::Internal(e.to_string()))?;
        let url = parsed["url"]
            .as_str()
            .ok_or(WebviewError::Internal("could not find url".into()))?;
        let body = parsed["body"]
            .as_str()
            .ok_or(WebviewError::Internal("could not find body".into()))?;

        Ok((url.to_string(), body.to_string()))
    } else {
        Err(WebviewError::UserCancelled)
    }
}
