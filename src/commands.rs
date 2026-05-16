use crate::config::Config;
use crate::error::AppError;
use crate::models::{CommandRequest, CommandResponse, TailscaleDevice};
use std::time::Instant;
use tokio::time::timeout;

const ARG_FORBIDDEN: &[char] = &[
    ';', '|', '&', '$', '`', '>', '<', '\n', '\r', '\0', '\\', '*', '?', '~', '(', ')', '{', '}',
    '\'', '"',
];

pub async fn run_ssh(
    device: &TailscaleDevice,
    config: &Config,
    req: &CommandRequest,
) -> Result<CommandResponse, AppError> {
    validate_command(config, &req.command, &req.args)?;

    let requested = req.timeout_secs.unwrap_or(30);
    let capped = requested.min(config.max_command_timeout_secs).max(1);
    let timeout_dur = std::time::Duration::from_secs(capped);
    let start = Instant::now();

    let result = timeout(
        timeout_dur,
        execute_ssh(
            &device.ip,
            &config.ssh_user,
            &config.ssh_known_hosts,
            &req.command,
            &req.args,
        ),
    )
    .await;

    let (stdout, stderr, exit_code) = match result {
        Ok(Ok(triple)) => triple,
        Ok(Err(e)) => {
            tracing::warn!(device = %device.id, error = %e, "ssh execution failed");
            return Err(AppError::Upstream(e));
        }
        Err(_) => ("".to_string(), "command timed out".to_string(), 124),
    };

    Ok(CommandResponse {
        stdout,
        stderr,
        exit_code,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

fn validate_command(
    config: &Config,
    command: &str,
    args: &[String],
) -> Result<(), AppError> {
    if command.is_empty() {
        return Err(AppError::BadRequest {
            code: "command_empty",
            message: "command must not be empty".into(),
        });
    }

    if command.contains('/') || command.contains(char::is_whitespace) {
        return Err(AppError::BadRequest {
            code: "command_invalid",
            message: "command must be a bare executable name (no slashes or whitespace)".into(),
        });
    }

    if !config
        .allowed_commands
        .iter()
        .any(|c| c == command)
    {
        return Err(AppError::Forbidden {
            code: "command_not_allowed",
            message: format!("command '{}' is not in the allowlist", command),
        });
    }

    for arg in args {
        if arg.chars().any(|c| {
            c.is_control() || ARG_FORBIDDEN.contains(&c)
        }) {
            return Err(AppError::BadRequest {
                code: "arg_invalid",
                message: "argument contains forbidden characters".into(),
            });
        }
    }

    Ok(())
}

async fn execute_ssh(
    ip: &str,
    user: &str,
    known_hosts: &std::path::Path,
    command: &str,
    args: &[String],
) -> anyhow::Result<(String, String, i32)> {
    let mut builder = openssh::SessionBuilder::default();
    builder
        .user(user.to_string())
        .known_hosts_check(openssh::KnownHosts::Strict)
        .user_known_hosts_file(known_hosts);

    let session = builder.connect(ip).await?;

    let mut cmd = session.command(command);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(1);

    Ok((stdout, stderr, exit_code))
}
