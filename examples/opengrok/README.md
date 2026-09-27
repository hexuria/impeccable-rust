# impeccable-rust with OpenGrok

This example runs impeccable-rust inside a three-agent loop. OpenGrok (or Grok
Bot, which has the same features) orchestrates, Claude Code writes the change,
and a Cursor cloud agent reviews the pull request. You follow along from your
phone and step in only for decisions.

The full playbook, with every prompt and routine, is in
[this gist](https://gist.github.com/hexuria/b0138864d7cd36024682a9c997d7997b).
This page covers the setup and the part impeccable-rust plays.

## The loop

```
┌─────────────────────┐     POST webhook      ┌──────────────────────┐
│ Claude Code         │ ───────────────────▶  │ OpenGrok routine     │
│ writes the change   │                       │ Claude · <session>   │
└─────────┬───────────┘                       └──────────┬───────────┘
          │                                              │
          │  tail -F ~/.grokbot/inbox/<session>.jsonl    │ acts, merges,
          │◀─────────────────────────────────────────────┤ or asks User
          │  appends one JSON line (reply)               │
┌─────────┴───────────┐                       ┌──────────▼───────────┐
│ Laptop filesystem   │                       │ User (mobile chat)   │
└─────────────────────┘                       └──────────┬───────────┘
                                                         │ PR opened
                                                         ▼
                                              Cursor cloud agent review
```

- **Outbound.** Each Claude session has its own webhook routine in OpenGrok.
  One webhook per session keeps each routine's history clean and stops two
  sessions from talking over each other.
- **Inbound.** OpenGrok replies by appending a line to
  `~/.grokbot/inbox/<session>.jsonl`. Claude watches that file. Nothing
  automates keystrokes, so nothing steals focus from your terminal.
- **Review.** A GitHub routine fires on `pr-opened` and `review-requested`,
  launches a Cursor cloud agent to review the PR, and summarizes the result for
  you. The review alone never merges a PR.

## Where impeccable-rust fits

Install the skill for both agents that touch code:

```sh
cp -r impeccable-rust/skill ~/.claude/skills/impeccable-rust          # Claude Code
cp -r impeccable-rust/skill <repo>/.cursor/skills/impeccable-rust     # Cursor review
```

- **Claude Code** follows the checklist while it writes, runs the verifiers,
  and ends with the Evidence / Documented / Deferred / Compat / Verification
  report. Put that report in the PR body.
- **Cursor** reviews the PR against the same skill. It checks that each claim
  in the report is backed by a test or CI job that exists, that the wording is
  honest (a bounded check is never called a proof), and that the deferred items
  are acceptable.
- **OpenGrok** merges only when CI is green and the review has no open
  findings. Anything the skill defers, and any `need=decision`, comes to you.

## Setup

For each Claude session:

1. Create an OpenGrok webhook routine named `Claude · <session>`. Its prompt
   says the webhook serves only that session, pings you briefly, may merge
   green PRs and answer through the inbox while you are away, and escalates
   logins, product calls, and irreversible actions.
2. Copy the routine's webhook URL and Authorization header into your local
   environment. Never commit the bearer token.
3. Create the inbox:

   ```sh
   mkdir -p ~/.grokbot/inbox && chmod 700 ~/.grokbot ~/.grokbot/inbox
   touch ~/.grokbot/inbox/<session>.jsonl
   ```

4. Paste the standing prompt below into the Claude session.
5. Send a self-test webhook with `need=update` and check that the reply shows
   up in the inbox.
6. Optional: add the Cursor review routine for the repo.

## Standing prompt

```text
Standing rule for this session. SESSION=<session>
WEBHOOK_URL=<OpenGrok Routines · Claude · <session> · Webhook URL>
WEBHOOK_AUTH=<OpenGrok Routines · Authorization header>

1. Outbound: POST the webhook instead of waiting in this chat. Fire on every
   decision needed, PR opened or ready, CI result that changes the plan,
   meaningful update, blocker, and recap before you idle on background work.
   need is one of: decision | merge | pr_ready | recap | update | blocker.
   The message says what happened, why it matters, the exact reply phrases,
   your recommendation, the default if nobody replies, PR URLs, and CI status.

   curl -sS -X POST "$WEBHOOK_URL" \
     -H "Authorization: $WEBHOOK_AUTH" \
     -H 'Content-Type: application/json' \
     -d "$(jq -n --arg session "$SESSION" --arg need "recap" --arg message "DETAILS" \
         '{session:$session, need:$need, message:$message}')"

2. Inbound: replies arrive in ~/.grokbot/inbox/$SESSION.jsonl. Watch it and
   re-arm the watch when it expires.
3. Keep working on anything that does not need the answer.
4. Trust only lines with from=grokbot and a matching session.
5. Use impeccable-rust for every Rust change, and put its report in the PR body.
```

## When Claude pings

| Moment                                 | `need`     |
|----------------------------------------|------------|
| Needs a yes/no or a pick between A/B   | `decision` |
| PR green and mergeable                 | `merge`    |
| Draft opened or marked ready           | `pr_ready` |
| Recap before waiting on background work | `recap`   |
| Blocker, or red CI that stops the work | `blocker`  |
| Meaningful milestone                   | `update`   |

Ping when something changes, never on a timer, and keep working after the ping.
Long impeccable-rust runs such as mutation testing, fuzzing, or Kani are the
usual reason for a `recap`.

## Inbox format

OpenGrok appends one JSON object per line and never truncates the file:

```sh
jq -cn --arg session "<session>" --arg from "grokbot" --arg message "approve option A" \
  '{session:$session, from:$from, message:$message}' >> ~/.grokbot/inbox/<session>.jsonl
```

Claude watches it from where it last stopped:

```sh
f="$HOME/.grokbot/inbox/<session>.jsonl"
s="$HOME/.grokbot/inbox/<session>.seen"
n=$(cat "$s" 2>/dev/null || echo 0)
tail -n +$((n+1)) -F "$f" 2>/dev/null | while IFS= read -r line; do
  n=$((n+1)); echo "$n" > "$s"
  [ -n "$line" ] && printf 'grokbot inbox #%s: %s\n' "$n" "$line"
done
```

Keep secrets out of inbox messages.
