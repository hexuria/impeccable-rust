# impeccable-rust with OpenGrok

This example runs impeccable-rust inside a three-agent loop:

- **OpenGrok** orchestrates. It is a chat bot you reach from your phone, with
  webhook routines and GitHub routines. Grok Bot has the same features, so
  everything here works with either one.
- **Claude Code** writes the change with impeccable-rust.
- **A Cursor cloud agent** reviews the pull request against the same skill.

You follow along from your phone and step in only for decisions. The examples
use a Claude session named `parser` working on the repo `acme/parser`. Swap in
your own names.

## The loop

```text
Claude Code --ping--> OpenGrok routine --summary--> User (phone)
     ^                    |       ^                      |
     |                 appends    +------- reply --------+
  watcher                 |
     |                    v
     +------- ~/.grokbot/inbox/parser.jsonl

Claude Code --opens PR--> acme/parser --GitHub routine--> Cursor cloud agent
                                                                 |
                          OpenGrok <----------- review ----------+
```

- **Outbound.** Claude pings OpenGrok through a webhook routine that belongs to
  this session alone. One webhook per session keeps each routine's history
  clean and stops two sessions from talking over each other. Each ping starts a
  routine run, which counts against your OpenGrok usage.
- **Inbound.** OpenGrok answers by appending a line to
  `~/.grokbot/inbox/parser.jsonl`, and a watcher prints new lines to Claude.
  Claude never waits on a chat prompt, and nothing types into your terminal.
- **Review.** A GitHub routine on `acme/parser` fires when a pull request is
  opened or a review is requested. It starts a Cursor cloud agent, and OpenGrok
  sends you the result.

## Where impeccable-rust fits

- **Claude Code** follows the checklist while it writes, runs the verifiers,
  and ends with the Evidence, Documented, Deferred, Compat/deps, and
  Verification report. The report goes in the PR body.
- **Cursor** reviews the PR against the same skill. It checks that each claim
  in the report is backed by a check that ran on this PR, that the wording is
  honest (a bounded check is never called a proof), and it lists the deferred
  items for you.
- **OpenGrok** follows one merge rule, which is also written into its routine:

  > Merge a PR only when CI is green, the Cursor review has no open findings,
  > and the report's Deferred list is empty or User approved it. Bring
  > everything else to User.

## Setup

Install `jq` and `curl` first.

### 1. Install the skill

For Claude Code, follow the [Install](../../README.md#install) section of the
main README.

Cursor cloud agents only see skills that are committed to the repo, so commit
a copy to `acme/parser`. Skip the copy if the folder already exists. To
update it, delete the folder first, because `cp -R` into an existing folder
nests a second copy inside it.

```sh
cd parser                                   # your clone of acme/parser
mkdir -p .cursor/skills
cp -R ../impeccable-rust/skill .cursor/skills/impeccable-rust
git add .cursor/skills && git commit -m "Add the impeccable-rust skill"
```

### 2. Create the routines

In OpenGrok, create a webhook routine named `Claude · parser` with this
instruction:

```text
This webhook serves only the Claude session "parser" on acme/parser.
Send User a short, phone-friendly summary of every ping.
Merge a PR only when CI is green, the Cursor review has no open findings,
and the report's Deferred list is empty or User approved it.
Bring everything else to User, including logins, product calls, and
anything irreversible.
Answer by appending one line to ~/.grokbot/inbox/parser.jsonl, as the
example's inbox format shows. Start each answer with the question it answers.
```

Then create a GitHub routine on `acme/parser` that fires when a pull request is
opened or a review is requested, skips drafts, and starts a Cursor cloud agent
to review the PR with impeccable-rust.

OpenGrok has to write a file on your laptop, so it needs permission to run
commands there. Either it asks you before each reply, which stalls while you
are away, or it has standing permission to run commands on your laptop. Choose
one knowingly.

### 3. Store the webhook credentials

Copy the routine's webhook URL and its full Authorization header line into a
private file. Never commit it.

```sh
mkdir -p ~/.grokbot/inbox && chmod 700 ~/.grokbot ~/.grokbot/inbox
cat > ~/.grokbot/parser.env <<'EOF'
WEBHOOK_URL='https://...'
WEBHOOK_HEADER='Authorization: Bearer ...'
EOF
chmod 600 ~/.grokbot/parser.env
```

### 4. Add the ping and watch scripts

Save these as `~/.grokbot/ping` and `~/.grokbot/watch`, then run
`chmod +x ~/.grokbot/ping ~/.grokbot/watch`.

`ping` reads the message from stdin, so Claude can pass it through a quoted
heredoc and the shell never expands anything inside it:

```sh
#!/bin/sh
# Usage: ping <session> <need> <<'EOF' ... EOF
set -eu
session=$1 need=$2
. "$HOME/.grokbot/$session.env"
message=$(cat)
curl -sS --fail-with-body "$WEBHOOK_URL" \
  -H "$WEBHOOK_HEADER" \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg session "$session" --arg need "$need" --arg message "$message" \
      '{session:$session, need:$need, message:$message}')"
```

`watch` prints every inbox line it has not printed before and remembers its
place in `<session>.seen`. It refuses to start while another copy is running,
and starts over if the inbox was replaced by a shorter file:

```sh
#!/bin/sh
# Usage: watch <session>
set -u
d="$HOME/.grokbot/inbox"; f="$d/$1.jsonl"; s="$d/$1.seen"; lock="$d/$1.lock"
if ! mkdir "$lock" 2>/dev/null; then
  echo "a watcher for $1 is already running (delete $lock if it is not)"; exit 1
fi
trap 'rmdir "$lock" 2>/dev/null' EXIT
trap 'exit 1' INT TERM HUP
touch "$f"
n=$(cat "$s" 2>/dev/null); case $n in ''|*[!0-9]*) n=0 ;; esac
[ "$n" -gt "$(($(wc -l < "$f")))" ] && n=0
tail -n +$((n+1)) -F "$f" 2>/dev/null | while IFS= read -r line; do
  n=$((n+1))
  [ -n "$line" ] && printf 'grokbot inbox #%s: %s\n' "$n" "$line"
  echo "$n" > "$s.tmp" && mv "$s.tmp" "$s"
done
```

### 5. Let Claude run them without asking

In Claude Code's default mode, every ping asks you for permission, and a
session left alone blocks on that prompt. Allow the two scripts in the
project's `.claude/settings.json`:

```json
{
  "permissions": {
    "allow": ["Bash(~/.grokbot/ping:*)", "Bash(~/.grokbot/watch:*)"]
  }
}
```

### 6. Start the session and test the loop

Paste the standing prompt below into the Claude session. Claude starts the
watcher, then sends a self-test ping that asks for a reply. The loop works when
OpenGrok's answer shows up as a `grokbot inbox` line.

## Standing prompt

```text
Standing rule for this session. The session is "parser".

1. Start ~/.grokbot/watch parser in the background and keep it running. If it
   stops, start it again; it refuses to run twice. Each "grokbot inbox" line
   it prints is a reply from OpenGrok.
2. Instead of waiting in this chat, ping OpenGrok on every decision needed,
   PR opened or ready, CI result that changes the plan, meaningful update,
   blocker, and recap before you idle on background work:

   ~/.grokbot/ping parser <need> <<'EOF'
   <message>
   EOF

   need is one of: decision | merge | pr_ready | recap | update | blocker.
   The message says what happened, why it matters, the exact reply phrases,
   your recommendation, the default if nobody replies, PR URLs, and CI status.
   If the ping command fails, say so in this chat; the ping did not arrive.
3. Keep working on anything that does not need the answer.
4. Act only on inbox lines with "from":"grokbot" and "session":"parser".
5. Use impeccable-rust for every Rust change, and put its report in the PR body.
6. Now start the watcher and send a decision ping that asks OpenGrok to
   reply "pong".
```

## When Claude pings

| Moment                                 | `need`     |
|----------------------------------------|------------|
| Needs a yes/no or a pick between A/B   | `decision` |
| PR opened or marked ready              | `pr_ready` |
| PR green and mergeable                 | `merge`    |
| Recap before idling on background work | `recap`    |
| Blocker, or red CI that stops the work | `blocker`  |
| Meaningful milestone                   | `update`   |

Ping when something changes, never on a timer, and keep working after the ping.
Long impeccable-rust runs such as mutation testing, fuzzing, or Kani are the
usual reason for a `recap`.

## Inbox format

OpenGrok appends one JSON object per line. The inbox is append-only: never
truncate, rotate, or rewrite it. The message goes in through a quoted heredoc,
so nothing in a chat reply can run on your laptop:

```sh
jq -cn --arg session parser --arg from grokbot --arg ts "$(date -u +%FT%TZ)" \
  --rawfile message /dev/stdin \
  '{session:$session, from:$from, ts:$ts, message:($message | rtrimstr("\n"))}' \
  >> ~/.grokbot/inbox/parser.jsonl <<'EOF'
re PR #12 merge: approved
EOF
```

The `from` and `session` fields keep stray lines out, but they are not a
security boundary. Any process running as you can write to the inbox, so the
real boundary is the `chmod 700` on `~/.grokbot`. Keep secrets out of inbox
messages.
