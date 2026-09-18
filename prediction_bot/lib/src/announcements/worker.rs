use serenity::http::HttpError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttemptOutcome {
    Delivered {
        message_id: u64,
    },
    Retry {
        reason: &'static str,
        provider_delay: Option<i64>,
    },
    Pause {
        reason: &'static str,
    },
}

pub(crate) fn retry_at(now: i64, failures: i64, provider_delay: Option<i64>) -> i64 {
    let exponent = failures.saturating_sub(1).clamp(0, 6) as u32;
    let local_delay = 5_i64.saturating_mul(1_i64 << exponent).min(300);
    let delay = local_delay.max(provider_delay.unwrap_or(0).max(0));
    now.saturating_add(delay)
}

pub(crate) fn classify_delivery_failure(error: &serenity::Error) -> AttemptOutcome {
    match error {
        serenity::Error::Http(HttpError::UnsuccessfulRequest(response)) => {
            classify_http_failure(response.status_code.as_u16(), response.error.code)
        }
        serenity::Error::Http(HttpError::Request(error)) if error.is_timeout() => retry("timeout"),
        serenity::Error::Http(HttpError::Request(_))
        | serenity::Error::Io(_)
        | serenity::Error::Gateway(_)
        | serenity::Error::Tungstenite(_) => retry("transport"),
        serenity::Error::Http(_) => retry("discord_http"),
        _ => retry("discord_transport"),
    }
}

pub(crate) fn classify_http_failure(status: u16, discord_code: isize) -> AttemptOutcome {
    if status == 401 {
        return pause(
            "Discord authentication failed; an operator must correct the bot credentials.",
        );
    }
    if status == 403 || matches!(discord_code, 50_001 | 50_013) {
        return pause("Announcement delivery lacks permission to use the configured channel.");
    }
    if status == 404 || discord_code == 10_003 {
        return pause("The configured announcement channel is unavailable.");
    }
    if status == 429 {
        return retry("discord_rate_limit");
    }
    if (500..=599).contains(&status) {
        return retry("discord_server_error");
    }
    pause(
        "Discord rejected the announcement payload; reconfigure the destination or contact an operator.",
    )
}

fn retry(reason: &'static str) -> AttemptOutcome {
    AttemptOutcome::Retry {
        reason,
        provider_delay: None,
    }
}

fn pause(reason: &'static str) -> AttemptOutcome {
    AttemptOutcome::Pause { reason }
}
