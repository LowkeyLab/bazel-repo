# Hermes execution and recovery interfaces

Research for LowkeyLab/bazel-repo#3136, under map #3135. Checked 2026-11-11. **Planning evidence only; no activation or implementation approval.**

## Evidence and conclusion

The full issue, dispatch comment, and parent map were read with `gh issue view`. The shared checkout was read only. Its glossary map has no dispatcher context. ADR 0001 makes the standalone worktree-pool repository authoritative; Hermes worktrees must not silently replace the repository's ownership and receipt requirements. [1][2][3]

Official `llms.txt` identifies supported local sessions, programmatic protocols, API Server, webhooks, cron, and Kanban. Current official source was inspected at commit **`33cbf1e813d4ff35c7709b21c809f55eb3af9288`**, whose committer time is `2026-11-11T02:07:35Z`. Source references below are pinned to this commit. These are current-main findings, not certification of an installed release. [4][5]

**Finding:** Hermes already has durable admissions and a durable task board. A new execution queue is not an established need. Native gateway sessions offer the clearest explicit skill/cwd policy and retry identity; HTTP Runs offers the simplest remote run lifecycle; Kanban adds durable task/attempt history and managed workers. However, Kanban's automatic recovery and retry budgets do **not** directly implement the map's no-automatic-redispatch-on-uncertainty and shared three-repair-cycle policy. This is an integration decision, not a missing-feature conclusion. [6][8][10][13]

## Interface comparison — verified facts

| Interface | Execution and identity | Completion and recovery | Fit and constraint |
|---|---|---|---|
| `hermes chat` | A fresh session when no resume/continue option is supplied. Current CLI connects to the gateway; launching another CLI process does not establish an independent execution owner. [7][11] | `--format stream-json` gives session init and terminal `result`; preserve both output and exit status. Lost transport is an unknown outcome, not a retry signal. [7][12] | Useful adapter or smoke-test surface. Native create request IDs are generated internally, so a restarted wrapper must not rerun the command blindly. [11] |
| Native gateway WebSocket | `session.create` with stable `request_id`, `source`, `cwd`, `skills`, model/tool policy; then `prompt.submit` with stable `submission_id`. Logical session and admission identities are durable. [6] | Resume, receipt reads, event subscription, generation-bound controls; started work becomes unknown after owner loss instead of replaying. [6][14] | Strong local integration seam when launch policy matters. More protocol work than HTTP; inspect `runtime.describe`. [6][9] |
| HTTP API Server | `POST /v1/runs` returns `run_id`; keyed retries reuse it. GET status, SSE events, stop, approval, clarification, unknown resolution. [8][15] | Canonical owner admissions support durable status; legacy idempotency storage has different recovery behavior and can fall back to memory. [15][16] | Strong remote lifecycle seam. Do not assume Runs accepts the native session creation fields `cwd` and `skills`. [15] |
| Webhook routes | Authenticated event ingress can run an agent in an event-specific session. Immediate-path 202 includes `delivery_id`, not HTTP Runs identity. [17] | Durable producer admission precedes ordinary agent-route acknowledgement; delivery and inference remain separate. [17][18] | Suitable intake adapter, not a substitute for an execution/result contract. `deliver_only` invokes no agent. [18] |
| Cron / Jobs API | Persistent schedules, skills/workdir, fresh agent sessions, execution history; HTTP Jobs manages scheduled work. [8][19] | Fire identity, execution ledger, receipt reconciliation, cancellation; unknown attempts are not automatically replayed. [19] | Suitable scheduled reconciliation. A scheduler's definition ID is not an issue-job execution attempt. |
| Kanban | SQLite task, `task_runs` attempt, claim, worker session, workspace, skills and assignee profile. CLI, tools and dashboard share kernel operations. [10][20] | Structured summaries/metadata/artifacts, heartbeat, managed worker receipts, bounded retry and recovery. [13][20] | Most existing queue machinery. Requires a deliberate policy adapter; defaults can restart failed work. [13][21] |

ACP is also supported (`hermes acp`, stdio JSON-RPC), with session creation, permission requests and cancellation. It targets editor clients; this research found no reason to prefer its editor-facing lifecycle over the explicit native or HTTP admission controls for this dispatcher. That preference is an inference. [9]

## Headless execution and worker isolation

### CLI and skill invocation

For a finite programmatic call, the documented shape is:

```text
hermes --profile <worker-profile> chat --oneshot \
  --query-file <approved-prompt-file> --skills <required-skill> \
  --format stream-json
```

This is an illustrative invocation, not an executed deployment command. Start it in the approved absolute worktree directory, or use the supported `--in` launch option. Current source forwards the caller cwd into frozen gateway policy and rejects explicit cwd selection if the gateway cannot support it. `--query-file` avoids shell interpretation; `-q` content is literal, not slash-command dispatch. Use `--skills`, not a prompt beginning `/research`, to establish skill loading. [7][11]

The JSONL protocol reports `system/init.session_id` and terminal `result` with exit code, text and usage. Chat exits 0 for a completed turn, 1 for failed/partial/budget outcomes, and 130 for interruption. `hermes -z` intentionally differs: 2 for failed/partial, 1 for completed-but-empty; `--usage-file` adds outcome flags and session identity. Neither prose nor a nonempty response proves success. [7]

Native `session.create.skills` renders skills into frozen policy, without changing daemon configuration. Kanban has repeatable `--skill`; source builds preloaded skill context but refuses only when requested skills are missing **and none loaded**. A partial skill load is therefore not a sufficient contract for mandatory review skills. Webhook source uses the **first matching** configured skill, despite accepting a list. Validate required skill resolution explicitly. [6][20][22][17]

### What “fresh” does and does not mean

A fresh conversation is not a fresh profile, OS sandbox, credential set, or clean filesystem. Profiles separate homes, config, sessions and memory. Default profile creation can copy keys/model and share default-profile logins; `--no-credentials` and authentication options change this. Provision credentials deliberately rather than treating profile creation as credential isolation. [23]

Native creation freezes an existing absolute gateway-local cwd and policy. It does not create worktrees or switch profiles. Kanban supports scratch, existing absolute `dir:<path>`, and automatic git worktrees. Scratch cleanup removes undeclared output; declare artifacts or use a retained directory. For this repository, a proposed adapter should pass a worktree-pool-authorized directory rather than ask Hermes to create a competing worktree. [6][10][3]

Managed Kanban workers execute under the assigned profile's gateway owner. Under a systemd-managed gateway, restart-safe worker scopes are mandatory; missing user-session bus access is infrastructure failure, not permission to launch an unsafe fallback. `kanban.max_in_progress: 1` caps tasks on a board, not all activity across all boards/profiles. A terminal task can also retain a finishing worker briefly. Enforce the map's one-active-worker boundary at the selected dispatch scope, including finishing workers. [10][20][24]

## Identity, waits and evidence

Keep issue **job**, execution **attempt**, Hermes **worker session**, and transport request separate. For native execution retain profile/owner identity, creation request ID, logical session ID, submission ID, admission ID and execution generation. Stable creation IDs survive daemon death; compression can rotate physical transcripts without changing the client logical ID. Read `prompt.receipt(..., include_result=true)` when reconciling. [6][14]

HTTP Runs should always receive a persisted `Idempotency-Key`. Identical requests replay the original run, changed payloads conflict. Retain the request before sending it. The legacy replay ledger has a 24-hour terminal retention window and a `durable` capability; canonical admission ownership is a separate transaction/lifetime. This is not globally exactly-once execution. API-created sessions cannot be bound to foreign CLI/TUI sessions. The native socket is the supported route for those sessions. [8][15][16]

HTTP SSE is progress transport, not the record of truth: detached event buffers expire after five minutes. Poll status after reconnect. Detaching does not prove cancellation. For resumable background delegation, completion can be persisted to session history without reopening a terminal run or starting an unsolicited model turn. A workflow waiting for CI therefore needs an explicit later poll/continuation; it must not infer that passive delivery resumes its review loop. [8]

Kanban's board/task/run IDs and owner receipt complement the worker session ID. Store `summary`, structured `metadata`, and declared artifacts; inspect `show --json`, `runs --json`, and task events. Preserve repair accounting separately from these execution records unless a later design proves one authoritative schema can represent both. [20][21]

## Cancellation and crash recovery

**Do not equate client process death with stopped execution.** Current CLI source handles outer KeyboardInterrupt by saying accepted work continues at the gateway. It also explicitly warns that a disconnected turn must be inspected, not resent. Native viewers can disconnect while execution and pending approvals remain alive. Thus a CLI exit 130 is not sufficient evidence that the worker stopped. [11][12][6]

HTTP `/stop` requests interruption; legacy executor-backed work stays tracked until exit. Canonical controls target the admission and generation. For unknown canonical execution, `/stop` returns `409 unknown_execution`; `/resolve-unknown` requires the exact admission ID and execution generation. Resolution releases the blocked FIFO without replaying the lost head. It is an operator acknowledgement, not rollback or proof that external effects never happened. [8][14][15]

Kanban recovery has a crucial distinction: while a bound interpreter might still execute, unknown admissions or unreadable owner ledgers hold the claim. Once interpreter death is proven, the crash sweep can close the attempt and return the task to ready/review under the normal crash budget, **potentially repeating already-performed effects**. Explicit unknown resolution can also permit another attempt. Reclaim/terminate is therefore not a safe “cancel permanently” abstraction. Inspect effects and place the task into an intentional blocked/complete state before allowing recovery to dispatch again. [13][25]

Use these classifications in a proposed reconciliation contract:

- **Active:** owner receipt is queued/started/running/stopping or waiting on a human. Missing recent output does not change ownership.
- **Completed:** exact attempt has a durable terminal result; independently verify its declared output and acceptance evidence.
- **Stopped:** execution cessation is confirmed, with its terminal receipt and matching process/worker identity where relevant. Earlier side effects still need reconciliation.
- **Uncertain:** transport loss without receipt, unknown owner generation, inaccessible ledger, or contradictory identities. Freeze dispatch; do not manufacture completion or automatically replay.

These are proposed dispatcher classifications, not additional Hermes status enum values. The underlying facts are native receipt, API projection, and Kanban owner-recovery behavior. [13][14][15]

## Decision inputs for the parent map

**Proposal A:** reuse Kanban for task/attempt execution, with a GitHub eligibility adapter and explicit recovery restrictions. Disable automatic decomposition for approved issues; constrain the board/profile and all automatic retry paths. Prove that unknown/dead-worker recovery cannot bypass issue-level reconciliation before choosing it. [10][13][21]

**Proposal B:** use native gateway sessions directly, with a small durable job-to-admission ledger. This gives explicit skills/cwd and stable create/submit identities without adopting Kanban's scheduling policy. It still requires durable issue ownership, attempt accounting and operator recovery. Prefer HTTP Runs instead if remote lifecycle simplicity outweighs the need for native frozen launch fields. [6][8][15]

Neither proposal is approved. Native protocol support must be negotiated through `runtime.describe`; HTTP support through `/v1/capabilities`. Test the selected deployed revision, not just current-main documentation. [6][8]

Kanban already has a PR completion contract: it binds a published PR and reads exact-head required CI with the assignee profile's GitHub credentials. It does not establish the map's separate Matt Pocock Standards+Spec review, hosted Codex review, automatic merge verification, or shared three-repair-cycle contract. Its consecutive-failure counters can reset on unblock and treat failure classes differently. **Do not substitute `max_retries: 3` for three durable shared repair cycles.** Preserve exhaustion and HITL pauses on the originating issue; deployment stays human-controlled. [2][21][26]

Before implementation approval, require probes for lost create/submit acknowledgements; owner restart before/after claim; missing ledger; live versus dead bound interpreter; stop acknowledgement versus actual exit; partial/missing skills; credential expiry; disk-full refusal; CI wait/reconnect; and no redispatch after exhaustion. Bind every probe to the pinned/deployed version and the approved worktree receipt. These are proposed acceptance tests, not tests run here.

## Limitations

No agent jobs, cancellation experiments, builds, services, remote writes, branch operations, or tests were run. Source inspection does not certify host permissions, provider behavior or crash safety end to end. `web_extract` failed because its configured backend is search-only; available web search recovered through alternate backends, while full official index/source reads used HTTPS and GitHub. `web.run` was not exposed in this worker's executable tool set. The required `asd-ste100` skill was unavailable; clear English was used. Local Hermes skill notes lag current source (notably CLI gateway ownership and cron timeout/recovery); they were not treated as authoritative or modified.

## Primary sources

[1]: https://github.com/LowkeyLab/bazel-repo/issues/3136
[2]: https://github.com/LowkeyLab/bazel-repo/issues/3135
[3]: https://github.com/LowkeyLab/bazel-repo/blob/main/docs/adr/0001-extract-worktree-pool.md
[4]: https://hermes-agent.nousresearch.com/docs/llms.txt
[5]: https://github.com/NousResearch/hermes-agent/commit/33cbf1e813d4ff35c7709b21c809f55eb3af9288
[6]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/website/docs/developer-guide/gateway-local-sessions.md
[7]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/website/docs/reference/cli-commands.md
[8]: https://hermes-agent.nousresearch.com/docs/user-guide/features/api-server
[9]: https://hermes-agent.nousresearch.com/docs/developer-guide/programmatic-integration
[10]: https://hermes-agent.nousresearch.com/docs/user-guide/features/kanban
[11]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/hermes_cli/gateway_chat.py
[12]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/hermes_cli/gateway_client.py
[13]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/hermes_cli/kanban_owner_recovery.py
[14]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/gateway/session_authority.py
[15]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/gateway/platforms/api_server_runs.py
[16]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/gateway/platforms/api_server_run_idempotency.py
[17]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/gateway/platforms/webhook.py
[18]: https://hermes-agent.nousresearch.com/docs/user-guide/messaging/webhooks
[19]: https://hermes-agent.nousresearch.com/docs/user-guide/features/cron
[20]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/gateway/session_kanban.py
[21]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/hermes_cli/kanban_parser.py
[22]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/agent/skill_commands.py
[23]: https://hermes-agent.nousresearch.com/docs/user-guide/profiles
[24]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/hermes_cli/kanban_ops.py
[25]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/plugins/kanban/dashboard/plugin_api.py
[26]: https://github.com/NousResearch/hermes-agent/blob/33cbf1e813d4ff35c7709b21c809f55eb3af9288/website/docs/user-guide/features/kanban.md
