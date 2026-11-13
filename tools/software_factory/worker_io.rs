//! Native command transport and GitHub PR publication adapters. No default executable or token.
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::{Value, json};

use crate::{
    Error, Result,
    github::Transport,
    model::Identity,
    native_worker::{NativeRpc, PoolInspector, PublicationApi},
    require,
    worker::{PoolReceipt, PublicationRequest, PublicationResult},
};

/// Adapter for an explicitly provisioned authenticated native JSON-RPC bridge executable.
///
/// This is not a guessed `hermes` CLI command. The operator must supply a supported bridge;
/// no bridge is installed or selected by this library. It gets one packet on stdin, no arguments,
/// and only its gateway credential in the environment. It must return the exact JSON-RPC reply.
pub struct NativeRpcCommand {
    command: PathBuf,
    token_variable: String,
    sequence: u64,
}
impl NativeRpcCommand {
    /// # Errors
    /// Requires an approved absolute executable and explicit credential environment source.
    pub fn new(command: PathBuf, token_variable: String) -> Result<Self> {
        require(
            command.is_absolute() && !token_variable.is_empty(),
            "native bridge or gateway credential source missing",
        )?;
        Ok(Self {
            command,
            token_variable,
            sequence: 0,
        })
    }
}
impl NativeRpc for NativeRpcCommand {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        require(
            matches!(
                method,
                "ping"
                    | "runtime.describe"
                    | "session.create"
                    | "session.info"
                    | "prompt.submit"
                    | "prompt.receipt"
                    | "prompt.cancel"
            ),
            "unsupported native RPC",
        )?;
        let token = std::env::var(&self.token_variable)
            .map_err(|_| Error::Stop("native gateway credential unavailable".into()))?;
        require(!token.is_empty(), "empty native gateway credential")?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| Error::Stop("native request sequence exhausted".into()))?;
        let packet = json!({"jsonrpc":"2.0","id":self.sequence,"method":method,"params":params});
        let mut child = Command::new(&self.command)
            .env_clear()
            .env("HERMES_GATEWAY_TOKEN", token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let write = child
            .stdin
            .take()
            .ok_or_else(|| Error::Stop("native stdin unavailable".into()))
            .and_then(|mut stdin| {
                serde_json::to_writer(&mut stdin, &packet)?;
                stdin.write_all(b"\n")?;
                Ok(())
            });
        if let Err(error) = write {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let output = child.wait_with_output()?;
        require(
            output.status.success() && output.stdout.len() <= 2 * 1024 * 1024,
            "native bridge failed or reply exceeded bound; inspect exact target",
        )?;
        let reply: Value = serde_json::from_slice(&output.stdout)?;
        require(
            reply["jsonrpc"] == "2.0"
                && reply["id"] == self.sequence
                && reply.get("error").is_none()
                && reply.get("result").is_some(),
            "native JSON-RPC error or response identity mismatch",
        )?;
        Ok(reply["result"].clone())
    }
}

pub struct GitHubPublications<T> {
    transport: T,
    max_pages: usize,
}
impl<T: Transport> GitHubPublications<T> {
    /// # Errors
    /// Requires a positive approved pagination bound. A bounded incomplete read stops publication.
    pub fn new(transport: T, max_pages: usize) -> Result<Self> {
        require(max_pages > 0, "missing publication read bound")?;
        Ok(Self {
            transport,
            max_pages,
        })
    }
    fn marker(intent: &Identity) -> String {
        format!("<!-- software-factory-publication:{} -->", intent.as_str())
    }
    fn endpoint(request: &PublicationRequest, page: usize) -> String {
        format!(
            "repos/LowkeyLab/bazel-repo/pulls?state=all&head=LowkeyLab%3A{}&base=main&per_page=100&page={page}",
            encode_component(&request.branch)
        )
    }
    fn exact(
        &mut self,
        request: &PublicationRequest,
        intent: &Identity,
        number: u64,
    ) -> Result<PublicationResult> {
        let value = self.transport.request(
            &format!("repos/LowkeyLab/bazel-repo/pulls/{number}"),
            None,
            false,
        )?;
        require(
            value["number"] == number
                && value["body"]
                    .as_str()
                    .is_some_and(|b| b.contains(&Self::marker(intent)))
                && value["head"]["repo"]["full_name"] == "LowkeyLab/bazel-repo"
                && value["base"]["repo"]["full_name"] == "LowkeyLab/bazel-repo"
                && value["head"]["sha"] == request.head
                && value["head"]["ref"] == request.branch
                && value["base"]["ref"] == "main"
                && value["state"] == "open"
                && value["merged"] == false,
            "publication exact PR binding mismatch",
        )?;
        let text = |name: &str| -> Result<String> {
            value[name]
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| Error::Stop("missing exact PR identity".into()))
        };
        Ok(PublicationResult {
            number,
            node_id: text("node_id")?,
            head: request.head.clone(),
            base: "main".into(),
            branch: request.branch.clone(),
            intent: intent.clone(),
            url: text("html_url")?,
        })
    }
}
fn encode_component(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Uses the existing pool-receipt executable only for exact read-only assignment inspection.
/// It has no acquire, release, recover, or capacity method.
pub struct CapturedPoolInspector {
    command: PathBuf,
    task: String,
    path: String,
    receipt_root: PathBuf,
    catalog_dir: PathBuf,
}
impl CapturedPoolInspector {
    /// # Errors
    /// Requires an explicit executable, task, child PATH, private receipt root, and catalog.
    /// The catalog must be the authority used for the retained acquisition, never a default.
    pub fn new(
        command: PathBuf,
        task: String,
        path: String,
        receipt_root: PathBuf,
        catalog_dir: PathBuf,
    ) -> Result<Self> {
        require(
            command.is_absolute() && !task.is_empty() && !path.is_empty(),
            "pool-receipt executable/task/PATH unavailable",
        )?;
        private_root(&receipt_root)?;
        require(
            catalog_dir.is_absolute()
                && catalog_dir.canonicalize()? == catalog_dir
                && catalog_dir.is_dir(),
            "approved pool catalog directory unavailable",
        )?;
        Ok(Self {
            command,
            task,
            path,
            receipt_root,
            catalog_dir,
        })
    }
    fn original(receipt: &PoolReceipt) -> Result<()> {
        receipt.validate()?;
        private_root(receipt.directory())?;
        require(
            !receipt
                .directory()
                .join("wrapper-error.json")
                .try_exists()?,
            "original pool capture has a wrapper error",
        )?;
        let selected = private_json(&receipt.directory().join("selected-assignment.json"))?;
        let status = private_json(&receipt.directory().join("exit-status.json"))?;
        let invocation = private_json(&receipt.directory().join("invocation.json"))?;
        let raw = private_json(&receipt.directory().join("stdout"))?;
        require(
            &selected == receipt.selected()
                && status["started"] == true
                && status["code"] == 0
                && status["signal"].is_null()
                && raw["schema_version"] == 1
                && raw["outcome"] == "completed"
                && raw["command"] == "acquire"
                && raw["data"]["assignment"] == selected
                && invocation["version"] == 1
                && invocation["program"] == "worktree-pool"
                && invocation["arguments"].as_array().is_some_and(|args| {
                    args.iter()
                        .any(|arg| arg["bytes"] == json!([97, 99, 113, 117, 105, 114, 101]))
                }),
            "original acquisition capture does not match retained receipt",
        )
    }
}
fn private_root(path: &Path) -> Result<()> {
    require(
        path.is_absolute() && path.canonicalize()? == path,
        "private evidence root is not canonical",
    )?;
    let metadata = fs::symlink_metadata(path)?;
    require(
        metadata.is_dir()
            && metadata.uid() == fs::metadata("/proc/self")?.uid()
            && metadata.permissions().mode().trailing_zeros() >= 6
            && !path.ancestors().any(|p| p.join(".git").exists()),
        "private evidence root is not protected/outside checkouts",
    )
}
fn private_json(path: &Path) -> Result<Value> {
    let metadata = fs::symlink_metadata(path)?;
    require(
        metadata.is_file()
            && metadata.nlink() == 1
            && metadata.uid() == fs::metadata("/proc/self")?.uid()
            && metadata.permissions().mode().trailing_zeros() >= 6
            && metadata.len() <= 16 * 1024 * 1024,
        "private evidence file is unsafe or exceeds read bound",
    )?;
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
impl PoolInspector for CapturedPoolInspector {
    fn verify(&mut self, receipt: &PoolReceipt) -> Result<()> {
        Self::original(receipt)?;
        private_root(&self.receipt_root)?;
        let original = private_json(&receipt.directory().join("stdout"))?;
        let catalog = original["context"]["catalog_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::Stop("original pool catalog authority missing".into()))?;
        let handle = receipt.assignment().as_str();
        let output = Command::new(&self.command)
            .env_clear()
            .env("PATH", &self.path)
            .arg("--receipt-dir")
            .arg(&self.receipt_root)
            .args([
                "--task",
                &self.task,
                "--assignment",
                handle,
                "--",
                "--catalog-dir",
            ])
            .arg(&self.catalog_dir)
            .args(["assignment", "inspect", handle])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;
        require(
            output.status.success() && output.stdout.len() <= 8192,
            "pool inspection pending/unknown; retain original acquisition",
        )?;
        let summary: Value = serde_json::from_slice(&output.stdout)?;
        let directory = summary["receipt_directory"]
            .as_str()
            .ok_or_else(|| Error::Stop("inspection receipt missing".into()))?;
        private_root(Path::new(directory))?;
        require(
            Path::new(directory).parent() == Some(self.receipt_root.as_path()),
            "inspection receipt escaped approved root",
        )?;
        let inspected = private_json(&Path::new(directory).join("stdout"))?;
        require(
            inspected["context"]["catalog_id"] == catalog,
            "exact pool catalog authority changed",
        )?;
        let current = PoolReceipt::from_capture(
            directory.into(),
            &summary,
            private_json(&Path::new(directory).join("selected-assignment.json"))?,
        )?;
        for field in [
            "assignment_handle",
            "operation_id",
            "repository_id",
            "worktree_id",
        ] {
            require(
                current.selected()[field] == receipt.selected()[field],
                "exact pool assignment identity changed; no reacquisition",
            )?;
        }
        require(
            current.path_bytes() == receipt.path_bytes(),
            "lossless pool worktree path changed",
        )
    }
}

impl<T: Transport> PublicationApi for GitHubPublications<T> {
    fn publish(&mut self, request: &PublicationRequest, intent: &Identity) -> Result<()> {
        require(
            self.inspect(request, intent)?.is_none(),
            "publication already exists; reconcile, do not repeat",
        )?;
        let reference = self.transport.request(
            &format!(
                "repos/LowkeyLab/bazel-repo/git/ref/heads/{}",
                encode_component(&request.branch)
            ),
            None,
            false,
        )?;
        require(
            reference["object"]["type"] == "commit" && reference["object"]["sha"] == request.head,
            "publication branch moved before send",
        )?;
        let body = format!("{}\n\n{}", request.body, Self::marker(intent));
        self.transport.request("repos/LowkeyLab/bazel-repo/pulls",Some(json!({"title":request.title,"body":body,"head":request.branch,"base":"main","draft":false,"maintainer_can_modify":false})),false)?;
        Ok(())
    }
    fn inspect(
        &mut self,
        request: &PublicationRequest,
        intent: &Identity,
    ) -> Result<Option<PublicationResult>> {
        let mut seen = std::collections::BTreeSet::new();
        let mut found = None;
        for page in 1..=self.max_pages {
            let response = self
                .transport
                .request(&Self::endpoint(request, page), None, false)?;
            let rows = response
                .as_array()
                .ok_or_else(|| Error::Stop("partial PR inventory".into()))?;
            require(rows.len() <= 100, "invalid PR page size")?;
            for row in rows {
                let number = row["number"]
                    .as_u64()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| Error::Stop("PR identity missing".into()))?;
                require(seen.insert(number), "duplicate PR inventory identity")?;
                if row["body"]
                    .as_str()
                    .is_some_and(|b| b.contains(&Self::marker(intent)))
                {
                    require(found.is_none(), "ambiguous publication marker")?;
                    found = Some(number);
                }
            }
            if rows.len() < 100 {
                return found
                    .map(|number| self.exact(request, intent, number))
                    .transpose();
            }
        }
        Err(Error::Stop(
            "publication inventory incomplete within approved bound".into(),
        ))
    }
}
