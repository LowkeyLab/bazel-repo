//! Focused Chromium journey through the real packaged HTTP app and PostgreSQL.
#[allow(
    dead_code,
    reason = "The shared fixture also supports HTTP and persistence suites."
)]
mod test_support;
use runfiles::{Runfiles, rlocation};
use serde_json::json;
use std::io::{BufRead, Write};
use std::process::Stdio;
use test_support::Fixture;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

#[googletest::test]
#[tokio::test]
async fn sign_in_search_add_and_keyboard_preference_in_responsive_workspace() -> anyhow::Result<()>
{
    let app = Fixture::new().await?;
    Mock::given(method("GET")).and(path("/authorize")).respond_with(|request: &wiremock::Request| {
        let query: std::collections::HashMap<_,_>=request.url.query_pairs().into_owned().collect();
        let claims=json!({"iss":"https://accounts.google.com","aud":"fixture-client","azp":"fixture-client","sub":"browser-reader","nonce":query["nonce"],"exp":chrono::Utc::now().timestamp()+300});
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
    let base = app.base.clone();
    let pool = app.pool.clone();
    let runtime = tokio::runtime::Handle::current();
    let status = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let mut child = std::process::Command::new(runner)
            .args([
                base.as_str(),
                key.to_str().unwrap(),
                certificate.to_str().unwrap(),
            ])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let mut input = child.stdin.take().unwrap();
        for line in std::io::BufReader::new(child.stdout.take().unwrap()).lines() {
            let line = line?;
            let sql = match line.as_str() {
                "fixture:reject" => Some("CREATE FUNCTION browser_reject() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled failure'; END $$; CREATE TRIGGER browser_reject BEFORE INSERT ON book_smartz.ranking_events FOR EACH ROW EXECUTE FUNCTION browser_reject()"),
                "fixture:allow" => Some("DROP TRIGGER browser_reject ON book_smartz.ranking_events; DROP FUNCTION browser_reject()"),
                "fixture:expire" => Some("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second'"),
                _ => None,
            };
            if let Some(sql) = sql {
                runtime.block_on(sqlx::raw_sql(sql).execute(&pool))?;
                writeln!(input, "ack")?;
                input.flush()?;
            } else {
                println!("{line}");
            }
        }
        Ok(child.wait()?)
    })
    .await??;
    anyhow::ensure!(status.success(), "browser journey failed");
    Ok(())
}
