//! Read-only GitHub transport with private raw invocation receipts.
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub struct Github {
    directory: PathBuf,
    sequence: u64,
}
impl Github {
    pub fn new(root: &Path) -> Result<Self> {
        ensure!(root.is_absolute(), "receipt directory must be absolute");
        let mut ancestor = root;
        while !ancestor.exists() {
            ancestor = ancestor.parent().context("missing receipt ancestor")?;
        }
        let probe = Command::new("git")
            .args(["-C"])
            .arg(ancestor)
            .args(["rev-parse", "--absolute-git-dir"])
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_CEILING_DIRECTORIES")
            .env("GIT_DISCOVERY_ACROSS_FILESYSTEM", "1")
            .env("LC_ALL", "C")
            .output()
            .context("cannot inspect receipt location")?;
        ensure!(
            !probe.status.success(),
            "receipts must be outside Git worktrees"
        );
        ensure!(
            probe.status.code() == Some(128)
                && String::from_utf8_lossy(&probe.stderr).contains("not a git repository"),
            "receipt location inspection failed"
        );
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
        // Group and other permissions occupy the low six mode bits.
        ensure!(
            fs::metadata(root)?.permissions().mode().trailing_zeros() >= 6,
            "receipt root must be private"
        );
        let directory = root.join(uuid::Uuid::new_v4().to_string());
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        Ok(Self {
            directory,
            sequence: 0,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn save(&self, name: &str, value: &Value) -> Result<()> {
        self.bytes(name, &serde_json::to_vec_pretty(value)?)
    }
    fn bytes(&self, name: &str, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.directory.join(name))?
            .write_all(bytes)?;
        Ok(())
    }
    fn invoke(
        &mut self,
        endpoint: &str,
        pages: bool,
        query: Option<&str>,
    ) -> Result<(Value, bool)> {
        self.sequence += 1;
        let name = format!("{:04}", self.sequence);
        let mut args = vec![
            "api".to_owned(),
            "--hostname".into(),
            "github.com".into(),
            endpoint.into(),
        ];
        if pages {
            args.extend(["--paginate".into(), "--slurp".into()]);
        }
        if let Some(query) = query {
            args.extend(["-f".into(), format!("query={query}")]);
        }
        self.save(
            &format!("{name}-invocation.json"),
            &json!({"program":"gh","args":args}),
        )?;
        let output = Command::new("gh").args(&args).output();
        let output = match output {
            Ok(v) => v,
            Err(e) => {
                self.bytes(&format!("{name}-stderr"), e.to_string().as_bytes())?;
                self.save(&format!("{name}-exit.json"), &json!({"code":null}))?;
                return Err(e).context("cannot start gh");
            }
        };
        self.bytes(&format!("{name}-stdout"), &output.stdout)?;
        self.bytes(&format!("{name}-stderr"), &output.stderr)?;
        self.save(
            &format!("{name}-exit.json"),
            &json!({"code":output.status.code()}),
        )?;
        let value: Value =
            serde_json::from_slice(&output.stdout).context("invalid GitHub JSON response")?;
        Ok((value, output.status.success()))
    }
    pub fn read(&mut self, endpoint: &str) -> Result<Value> {
        let (value, success) = self.invoke(endpoint, false, None)?;
        ensure!(success, "GitHub read failed: {endpoint}");
        Ok(value)
    }
    pub fn classic(&mut self, endpoint: &str) -> Result<Value> {
        let (value, success) = self.invoke(endpoint, false, None)?;
        if !success
            && value["status"].as_str() == Some("404")
            && value["message"].as_str() == Some("Branch not protected")
        {
            return Ok(json!({"contexts":[],"checks":[]}));
        }
        ensure!(success, "branch protection read failed");
        Ok(value)
    }
    pub fn pages(&mut self, endpoint: &str, key: Option<&str>) -> Result<Vec<Value>> {
        let (value, success) = self.invoke(endpoint, true, None)?;
        ensure!(success, "GitHub paginated read failed: {endpoint}");
        let pages = value.as_array().context("missing API page array")?;
        ensure!(!pages.is_empty(), "API pages are empty");
        let mut result = Vec::new();
        let mut total = None;
        for page in pages {
            let rows = if let Some(key) = key {
                let count = page["total_count"]
                    .as_u64()
                    .context("missing API total count")?;
                ensure!(
                    total.is_none_or(|n| n == count),
                    "API count changed during pagination"
                );
                total = Some(count);
                page[key].as_array().context("missing API collection")?
            } else {
                page.as_array().context("invalid API page")?
            };
            result.extend(rows.iter().cloned());
        }
        ensure!(
            total.is_none_or(|n| n == result.len() as u64),
            "API collection is incomplete"
        );
        Ok(result)
    }
    pub fn threads(&mut self, pr: u64) -> Result<Vec<Value>> {
        let mut cursor = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut rows = Vec::new();
        let mut total = None;
        loop {
            let encoded = cursor.as_ref().map(serde_json::to_string).transpose()?;
            let after = encoded.map_or(String::new(), |v| format!(", after: {v}"));
            let query = format!(
                "query {{ repository(owner: \"LowkeyLab\", name: \"bazel-repo\") {{ pullRequest(number: {pr}) {{ reviewThreads(first: 100{after}) {{ totalCount nodes {{ isResolved }} pageInfo {{ hasNextPage endCursor }} }} }} }} }}"
            );
            let (value, success) = self.invoke("graphql", false, Some(&query))?;
            ensure!(
                success && value.get("errors").is_none(),
                "review thread read failed"
            );
            let threads = &value["data"]["repository"]["pullRequest"]["reviewThreads"];
            let count = threads["totalCount"]
                .as_u64()
                .context("missing thread count")?;
            ensure!(
                total.is_none_or(|n| n == count),
                "thread count changed during pagination"
            );
            total = Some(count);
            rows.extend(
                threads["nodes"]
                    .as_array()
                    .context("missing review threads")?
                    .iter()
                    .cloned(),
            );
            let info = &threads["pageInfo"];
            if !info["hasNextPage"]
                .as_bool()
                .context("missing thread pagination")?
            {
                break;
            }
            ensure!(
                !threads["nodes"]
                    .as_array()
                    .context("missing review threads")?
                    .is_empty()
                    && rows.len() as u64 <= count,
                "thread pagination did not advance"
            );
            let next = info["endCursor"]
                .as_str()
                .context("missing thread cursor")?
                .to_owned();
            ensure!(
                !next.is_empty() && seen.insert(next.clone()),
                "thread pagination did not advance"
            );
            cursor = Some(next);
        }
        ensure!(
            total == Some(rows.len() as u64),
            "review threads are incomplete"
        );
        Ok(rows)
    }
}
