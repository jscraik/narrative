//! git-ai CLI detection

use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitAiCliStatus {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

pub async fn detect_git_ai_cli() -> GitAiCliStatus {
    let mut command = Command::new("git-ai");
    command.arg("--version");
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = match timeout(Duration::from_secs(2), command.output()).await {
        Ok(result) => match result {
            Ok(output) => output,
            Err(error) => {
                return GitAiCliStatus {
                    available: false,
                    version: None,
                    error: Some(error.to_string()),
                };
            }
        },
        Err(_) => {
            return GitAiCliStatus {
                available: false,
                version: None,
                error: Some("git-ai CLI detection timed out".to_string()),
            };
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return GitAiCliStatus {
            available: false,
            version: None,
            error: if stderr.is_empty() {
                Some("git-ai CLI not available".to_string())
            } else {
                Some(stderr)
            },
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    };

    GitAiCliStatus {
        available: true,
        version,
        error: None,
    }
}
