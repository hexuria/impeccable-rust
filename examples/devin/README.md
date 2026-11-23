# impeccable-rust with Devin

This example runs impeccable-rust in a loop with three roles:

- **Claude Code** orchestrates from your machine. It hands tasks to Devin and
  reads the answers through
  [devin-claude-bridge](https://github.com/codeitlikemiley/devin-claude-bridge),
  an MCP server.
- **A Devin session** writes each change with impeccable-rust on its own Linux
  machine and opens the pull request.
- **A Devin automation** is the merge gate. On every pull request it starts a
  fresh session that reruns the checks and audits the report.

You step in only for decisions. The examples use the repo `acme/parser` and a
Devin account the bridge calls `work`. Swap in your own names.

## The loop

```text
User <------ decisions ------> Claude Code
                                 |      ^
     create_session, send_message|      | check_inbox, wait_for_reply
                                 v      |
                           Devin session (writer) --opens PR--> acme/parser
                                                                     |
                                                 opened or updated   v
                           PR comment <----- Devin automation (gate)
```

- **Outbound.** Claude starts one Devin session per task and messages it
  through the bridge. Every session and every message spends Devin usage.
- **Inbound.** Devin answers in its own session, and Claude polls for the
  answer. Nothing pushes to your machine, so Claude checks the inbox at the
  points the standing prompt names.
- **Gate.** The automation fires when a pull request is opened or gets new
  commits. It starts a new session, so the gate has never seen the writer's
  reasoning, and it posts its verdict as a comment on the pull request.

## Where impeccable-rust fits

Devin's machine is Linux on x86_64, so every tool the skill names runs on it
directly. That includes the ones a Mac cannot run: Valgrind, gungraun,
MemorySanitizer, and LeakSanitizer. Long runs such as Kani, mutation testing,
and fuzzing also leave your laptop free.

- **The writer session** follows the checklist, runs the verifiers through
  `impeccable`, and ends with the Evidence, Documented, Deferred, Compat/deps,
  and Verification report. The report goes in the PR body.
- **The gate** reruns the repo's checks on the PR head and audits the report.
  It checks that each claim is backed by a check on this head, that the wording
  is honest (a bounded check is never called a proof), and it copies the
  Deferred list for you.
- **Claude Code** relays the gate's findings to the writer, brings decisions to
  you, and follows one merge rule:

  > Merge a PR only when the gate says "Safe to merge" for the current head
  > commit, Devin Review has no open findings, and the report's Deferred list
  > is empty or User approved it. Bring everything else to User, including
  > logins, product calls, and anything irreversible.

## Setup

### 1. Install the skill

For Claude Code, follow the [Install](../../README.md#install) section of the
main README.

Devin loads skills from the repositories it clones, so commit a copy to
`acme/parser`. Two files are enough, because the CLI comes from the
environment in step 2:

```sh
cd parser                                   # your clone of acme/parser
mkdir -p .agents/skills/impeccable-rust
cp ../impeccable-rust/skill/SKILL.md ../impeccable-rust/skill/performance.md \
   .agents/skills/impeccable-rust/
git add .agents/skills && git commit -m "Add the impeccable-rust skill"
```

Devin reads the copy on the branch it is working on, and scans
`.claude/skills/` and `.cursor/skills/` too, so a repo that already carries the
skill for another agent needs no second copy.

### 2. Put the tools in Devin's environment

Open **Settings > Environment**, pick the repository, and add these steps to
its blueprint. They run once when the snapshot is built, so every session and
every gate run starts with the tools already installed:

```yaml
initialize:
  - name: Install cargo-binstall at a pinned version
    run: curl -fsSL "https://github.com/cargo-bins/cargo-binstall/releases/download/v1.24.0/cargo-binstall-$(uname -m)-unknown-linux-musl.tgz" | tar -xz -C "${CARGO_HOME:-$HOME/.cargo}/bin"
  - name: Install the impeccable CLI
    run: cargo binstall -y --disable-telemetry impeccable@0.2.0
  - name: Install the pinned verification tools on this machine
    run: sudo apt-get install -y valgrind && IMPECCABLE_BACKEND=host impeccable setup host --with-kani
  - name: Keep every impeccable run on this machine
    run: echo "IMPECCABLE_BACKEND=host" >> $ENVRC
knowledge:
  - name: verify
    contents: |
      impeccable doctor   # where each verification tool runs
```

- `cargo install --locked cargo-binstall` installs the newest release, which
  can need a newer Rust than the machine has. The pinned download does not.
- `IMPECCABLE_BACKEND=host` makes a missing tool show up as a gap in the
  report. Without it, a missing tool starts a multi-gigabyte toolbox image
  build inside the session.
- On a Devin machine these steps took about a minute in total. Afterwards
  `impeccable doctor` shows every tool running on the host except TLC, which
  needs Java and the TLA+ tools jar.

### 3. Connect Claude Code to Devin

Follow the bridge's
[Setup](https://github.com/codeitlikemiley/devin-claude-bridge#setup). Allow
its read-only tools, and leave `create_session` and `send_message` on a
prompt until you trust the loop, because they spend Devin usage.

### 4. Create the gate

In Devin, open **Automations > Create automation** and set:

- **Trigger:** Pull request in `acme/parser`, with Action in Opened,
  Synchronize, Ready For Review, and Draft off.
- **Reply:** Comments on the PR.
- **Agent type:** Start new session.
- **Spend limit per session** and **Rate limit:** set both. The gate runs on
  every push to an open pull request.

Use these instructions, with step 2 changed to the checks your repo requires:

```text
A pull request was opened or updated in acme/parser. You are the merge gate.
The PR number, head branch and title are in the trigger event.
Use @skills:impeccable-rust as the standard.

1. Check out the PR's head branch and note the head commit.
2. Run the required checks and record pass or fail for each:
   - cargo fmt --check
   - cargo clippy --all-targets -- -D warnings
   - cargo test
3. Read the impeccable-rust report in the PR body: Evidence, Documented,
   Deferred, Compat/deps, Verification. For each Evidence claim, find the
   check that backs it on this head. Rerun it with `impeccable run` when it
   takes under ten minutes. Otherwise confirm the harness exists and say you
   did not rerun it. Flag a claim with no check behind it, and wording
   stronger than its check, such as a bounded check called a proof.
4. Read the diff for what a test run cannot show: new `unsafe` with no test
   that Miri runs, a public API change with no semver note, a new dependency
   with no cargo-deny or cargo-audit result.
5. Do not push commits, merge, close or approve the PR. Only report.

Your final response is posted as a comment on the PR. Keep it short:
- First line: "Safe to merge" or "Do not merge yet", with the head commit.
- The checks, one line each, with the first error line when one fails.
- Report findings, each with file:line and one sentence on what is unbacked.
  Report only what you verified in the code.
- The Deferred list, copied from the report.
If the PR has no report, reply "No impeccable-rust report" after step 2.
If the PR only changes Markdown files, reply "Docs only, checks skipped".
```

By default a GitHub automation fires only on private repositories. On a
public repository anyone can open a pull request, and the gate reads its
text, so widen the connection's automation scope only if you accept that.

### 5. Optional: Devin Review and Autoapprover

Devin Review reads a `REVIEW.md` in the repository, and Autoapprover reads an
`APPROVAL_POLICY.md`. Two short files carry the same standard into both:

```markdown
<!-- REVIEW.md -->
A Rust change needs the impeccable-rust report in the PR body. Treat a missing
report, a claim with no check behind it, and a bounded check called a proof
as findings.
```

```markdown
<!-- APPROVAL_POLICY.md -->
Never approve a PR whose report has a Deferred item, that adds `unsafe`, that
changes a public API, or that adds a dependency. Those need a person.
```

To block merges on Devin Review, mark its commit status as required in the
repository's branch rules.

## Standing prompt

Paste this into the Claude Code session:

```text
Standing rule for this session. You are chief of staff for acme/parser.
The Devin account is "work".

1. Give each task its own Devin session with create_session, an alias, and
   this brief: the task and its acceptance check; "use
   @skills:impeccable-rust"; "open one PR and put the impeccable-rust report
   in its body"; "reply in this session when the PR is open or when you are
   blocked". Send Linux-only and long verification to Devin the same way:
   Kani, MemorySanitizer, LeakSanitizer, gungraun, mutation testing, fuzzing.
2. Never wait in this chat for Devin. Keep working on what does not need
   the answer. Call check_inbox before you end a turn and whenever you
   finish a piece of work. To wait on one session, use wait_for_reply with
   until_idle and a timeout under 1800 seconds.
3. When a session reports its PR, read the gate's comment with
   `gh pr view <number> --comments`. On "Do not merge yet", send the findings
   to the writer session with send_message and wait for the next verdict.
   Trust a verdict only for the PR's current head commit.
4. Merge a PR only when the gate says "Safe to merge" for the current head
   commit, Devin Review has no open findings, and the report's Deferred list
   is empty or User approved it. Bring everything else to User, including
   logins, product calls, and anything irreversible, with the options and
   your recommendation.
5. Use impeccable-rust for any Rust you write yourself, and put its report
   in the PR body.
6. Now call list_accounts and tell me which Devin accounts you can reach.
```

To have Claude check the inbox without being asked, add the bridge's
[`UserPromptSubmit` hook](https://github.com/codeitlikemiley/devin-claude-bridge#hooks--cron).

## What runs where

| Work                                     | Where           | Why                               |
|------------------------------------------|-----------------|-----------------------------------|
| Writing a change                         | Devin session   | Its own machine, many in parallel |
| MemorySanitizer, LeakSanitizer, gungraun | Devin session   | They need Linux                   |
| Kani, mutation testing, fuzzing          | Devin session   | Long runs stay off your laptop    |
| Auditing the report                      | Gate automation | A fresh session, not the writer   |
| Planning, relaying findings, merging     | Claude Code     | It holds the whole picture        |
| Decisions and anything irreversible      | You             |                                   |

## Limits

- **The gate comments; it does not block.** A comment is not a required check.
  The merge rule is what holds a PR back, or Devin Review's commit status when
  you mark it required.
- **A verdict covers one commit.** A push after "Safe to merge" starts a new
  gate run. Read the commit in the first line of the comment.
- **Devin's replies are polled.** Claude sees a reply the next time it calls
  the bridge, so a session nobody polls waits.
- **An automation's network policy narrows access.** With the tools already
  in the snapshot, the gate needs only what `cargo` needs: `crates.io`,
  `index.crates.io`, `static.crates.io`, and `github.com`.
