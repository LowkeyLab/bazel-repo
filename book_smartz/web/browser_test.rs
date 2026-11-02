//! Focused Chromium journey through the real packaged HTTP app and PostgreSQL.
#[allow(
    dead_code,
    reason = "The shared fixture also supports HTTP and persistence suites."
)]
mod test_support;
use std::{
    io::Write,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use runfiles::{Runfiles, rlocation};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use test_support::Fixture;

#[googletest::test]
#[tokio::test]
async fn sign_in_search_add_and_keyboard_preference_in_responsive_workspace() -> anyhow::Result<()>
{
    let mut lifecycle = BrowserLifecycle::new();
    let app = Fixture::new().await?;
    let native_reader = Arc::new(AtomicBool::new(false));
    let provider_native_reader = Arc::clone(&native_reader);
    Mock::given(method("GET")).and(path("/authorize")).respond_with(move |request: &wiremock::Request| {
        let query: std::collections::HashMap<_,_>=request.url.query_pairs().into_owned().collect();
        let claims=json!({"iss":"https://accounts.google.com","aud":"fixture-client","azp":"fixture-client","sub":if provider_native_reader.load(Ordering::SeqCst) { "native-browser-reader" } else { "browser-reader" },"nonce":query["nonce"],"exp":chrono::Utc::now().timestamp()+300});
        let key=jsonwebtoken::EncodingKey::from_rsa_pem(include_bytes!("fixtures/public-test-key.pem")).unwrap();
        let mut header=jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);header.kid=Some("fixture".into());
        let token=jsonwebtoken::encode(&header,&claims,&key).unwrap();
        let query=url::form_urlencoded::Serializer::new(String::new()).extend_pairs([("state",query["state"].as_str()),("code",token.as_str())]).finish();
        ResponseTemplate::new(302).insert_header("location",format!("https://reader.example/auth/google/callback?{query}"))
    }).mount(&app.provider).await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(|request: &wiremock::Request| {
            let values: std::collections::HashMap<_, _> =
                url::form_urlencoded::parse(&request.body)
                    .into_owned()
                    .collect();
            ResponseTemplate::new(200).set_body_json(json!({"id_token":values["code"]}))
        })
        .mount(&app.provider)
        .await;
    Mock::given(method("GET")).and(path("/search.json")).respond_with(|request: &wiremock::Request| {
        if request.url.query_pairs().any(|(key,value)|key=="q" && value=="Unavailable") {
            return ResponseTemplate::new(503);
        }
        let query: std::collections::HashMap<_, _> = request.url.query_pairs().into_owned().collect();
        if query.get("q").is_some_and(|value| value == "No matches") {
            return ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":0,"docs":[]}));
        }
        if query.get("q").is_some_and(|value| value == "Pages") {
            let second = query.get("page").is_some_and(|value| value == "2");
            let (start,id,title)=if second {(20,"OL511W","Page two")} else {(0,"OL510W","Page one")};
            return ResponseTemplate::new(200).set_body_json(json!({"start":start,"numFound":21,"docs":[{"key":id,"title":title,"author_name":[]}]}));
        }
        if query.get("q").is_some_and(|value| value == "Long title") {
            return ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL520W","title":"A long title about reading and remembering books across a lifetime of changing interests and unexpected discoveries".repeat(3),"author_name":[]}]}));
        }
        if let Some(title) = query.get("q").filter(|value| matches!(value.as_str(), "Third book" | "Reduced book" | "Interrupted book" | "Recovered book" | "Native book")) {
            let id = match title.as_str() { "Third book" => "OL503W", "Reduced book" => "OL504W", "Interrupted book" => "OL505W", "Recovered book" => "OL508W", _ => "OL506W" };
            return ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":id,"title":title,"author_name":["Fixture Author"],"cover_i": if title == "Third book" { Some(123) } else { None }}]}));
        }
        let first=request.url.query_pairs().any(|(key,value)|key=="q" && value=="First book");
        let (id,title)=if first {("OL501W","First book")} else {("OL502W","Later book")};
        ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":id,"title":title,"author_name":["Fixture Author"]}]}))
    }).mount(&app.provider).await;
    let files = Runfiles::create()?;
    let runner = rlocation!(files, std::env::var("BROWSER_RUNNER")?)
        .ok_or_else(|| anyhow::anyhow!("Missing browser runner"))?;
    let key = rlocation!(files, std::env::var("BROWSER_KEY")?)
        .ok_or_else(|| anyhow::anyhow!("Missing fixture TLS key"))?;
    let certificate = rlocation!(files, std::env::var("BROWSER_CERTIFICATE")?)
        .ok_or_else(|| anyhow::anyhow!("Missing fixture certificate"))?;
    let mut child = tokio::process::Command::new(runner)
        .args([
            app.base.as_str(),
            key.to_str().unwrap(),
            certificate.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let child_pid = child.id();
    let mut input = child.stdin.take().unwrap();
    let mut output = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
    let mut errors = Some(tokio::io::BufReader::new(child.stderr.take().unwrap()).lines());
    // This test-only fault closes the real pipe. Child diagnostic writes then fail.
    if std::env::var("BOOK_SMARTZ_BROWSER_CLOSE_DIAGNOSTIC_PIPE").as_deref() == Ok("1") {
        errors = None;
    }
    let mut output_open = true;
    let mut exit = None;
    let mut child_phase = None;
    let mut failed_phase = None;
    // Finish before the collection supervisor. Keep thirty seconds for fixture cleanup.
    let outer_seconds = std::env::var("TEST_TIMEOUT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300);
    let budget = Duration::from_secs(outer_seconds)
        .saturating_sub(lifecycle.started.elapsed() + Duration::from_secs(45));
    lifecycle.record("runner", "start", child_pid, "running", None);
    let result = tokio::time::timeout(budget, async {
        while output_open || errors.is_some() || exit.is_none() {
            tokio::select! {
                line = output.next_line(), if output_open => {
                    let Some(line) = line? else { output_open = false; continue; };
                    if line == "fixture:native-reader" {
                        native_reader.store(true, Ordering::SeqCst);
                        input.write_all(b"ack\n").await?;
                        input.flush().await?;
                        continue;
                    }
                    let sql = match line.as_str() {
                        "fixture:reject" => Some("CREATE FUNCTION browser_reject() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled failure'; END $$; CREATE TRIGGER browser_reject BEFORE INSERT ON book_smartz.ranking_events FOR EACH ROW EXECUTE FUNCTION browser_reject()"),
                        "fixture:allow" => Some("DROP TRIGGER browser_reject ON book_smartz.ranking_events; DROP FUNCTION browser_reject()"),
                        "fixture:profile-reject" => Some("CREATE FUNCTION browser_profile_reject() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled profile failure'; END $$; CREATE TRIGGER browser_profile_reject BEFORE INSERT OR UPDATE ON book_smartz_profiles.profiles FOR EACH ROW EXECUTE FUNCTION browser_profile_reject()"),
                        "fixture:profile-allow" => Some("DROP TRIGGER browser_profile_reject ON book_smartz_profiles.profiles; DROP FUNCTION browser_profile_reject()"),
                        "fixture:profile-text" => Some("INSERT INTO book_smartz_profiles.profiles (reader_id, display_name, biography) SELECT reader_id, repeat('N',100), 'First paragraph.' || chr(10) || chr(10) || '<b>Plain text</b> ' || repeat('B',260) FROM book_smartz_web.accounts WHERE subject = 'browser-reader' ON CONFLICT (reader_id) DO UPDATE SET display_name = EXCLUDED.display_name, biography = EXCLUDED.biography"),
                        "fixture:expire" => Some("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second'"),
                        _ => None,
                    };
                    if let Some(sql) = sql {
                        sqlx::raw_sql(sql).execute(&app.pool).await?;
                        input.write_all(b"ack\n").await?;
                        input.flush().await?;
                    } else {
                        println!("{line}");
                    }
                }
                line = async { errors.as_mut().unwrap().next_line().await }, if errors.is_some() => {
                    let Ok(Some(line)) = line else { errors = None; continue; };
                    let _ = writeln!(std::io::stderr().lock(), "{line}");
                    if let Some(record) = line.strip_prefix("browser-lifecycle ")
                        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok()) {
                        let phase = record["phase"].as_str().unwrap_or_default();
                        if matches!(phase, "runner" | "assertions" | "control_shutdown" | "browser_shutdown" | "proxy_shutdown" | "journey_teardown") {
                            match record["state"].as_str() {
                                Some("start") => child_phase = Some(phase.to_owned()),
                                Some("failed") => { failed_phase = Some(phase.to_owned()); child_phase = None; },
                                Some("complete") if child_phase.as_deref() == Some(phase) => child_phase = None,
                                _ => (),
                            }
                        }
                    }
                }
                status = child.wait(), if exit.is_none() => exit = Some(status?),
            }
        }
        Ok::<_, anyhow::Error>(exit.unwrap())
    }).await;
    let result = result.unwrap_or_else(|_| {
        let status = match child.try_wait() {
            Ok(Some(status)) => status.to_string(),
            Ok(None) => "running".to_owned(),
            Err(_) => "unavailable".to_owned(),
        };
        lifecycle.record(
            "child_exit",
            "failed",
            child_pid,
            &status,
            child_phase.as_deref().or(failed_phase.as_deref()),
        );
        Err(anyhow::anyhow!("browser child exceeded its exit deadline"))
    });
    if result.is_err() {
        let _ = child.kill().await;
    }
    let status = child
        .try_wait()
        .ok()
        .flatten()
        .map_or_else(|| "unavailable".to_owned(), |status| status.to_string());
    lifecycle.record(
        "child_exit",
        if result.as_ref().is_ok_and(std::process::ExitStatus::success) {
            "complete"
        } else {
            "failed"
        },
        child_pid,
        &status,
        child_phase.as_deref().or(failed_phase.as_deref()),
    );
    drop(input);
    lifecycle.record("fixture_cleanup", "start", child_pid, &status, None);
    drop(app);
    lifecycle.record("fixture_cleanup", "complete", child_pid, &status, None);
    anyhow::ensure!(result?.success(), "browser journey failed");
    lifecycle.record("harness_complete", "complete", child_pid, &status, None);
    Ok(())
}

struct BrowserLifecycle {
    started: Instant,
    sequence: u64,
}

impl BrowserLifecycle {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            sequence: 0,
        }
    }

    fn record(
        &mut self,
        phase: &str,
        state: &str,
        child_pid: Option<u32>,
        child_status: &str,
        child_phase: Option<&str>,
    ) {
        self.sequence += 1;
        let record = json!({
            "role": "rust_fixture", "pid": std::process::id(), "sequence": self.sequence,
            "elapsed_ms": self.started.elapsed().as_millis(), "phase": phase, "state": state,
            "child_pid": child_pid, "child_status": child_status, "child_phase": child_phase,
        });
        let _ = writeln!(std::io::stderr().lock(), "browser-lifecycle {record}");
        if let Some(directory) = std::env::var_os("TEST_UNDECLARED_OUTPUTS_DIR") {
            let directory = std::path::PathBuf::from(directory);
            let _ = std::fs::create_dir_all(&directory);
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join(format!("browser-lifecycle-{}.jsonl", std::process::id())))
            {
                let _ = writeln!(file, "{record}");
            }
        }
    }
}
