# devday

`devday` is a Rust CLI that generates a shareable engineering status report
from your GitHub, Linear, and local git activity. It answers three
questions: what got worked on, what's next, and what's blocked — and
optionally posts an executive-digest version to Slack.

`devday` is a private-first personal operating report, not an
employee-surveillance dashboard. All source access is read-only, and it
never posts to Slack unless you explicitly ask it to.

## What it collects (MVP scope)

- **GitHub** — pull requests you authored, via `gh search prs --author @me`
  (requires the local `gh` CLI, authenticated). Read-only.
- **Linear** — issues assigned to you, via a direct GraphQL call to the
  Linear API using a token from an environment variable. Read-only.
- **Local git** — commits you authored and uncommitted working-tree changes
  (WIP) in repositories under configured roots, via `git2`'s read-only APIs
  (no checkout, no commit, no fetch, no push).

This is intentionally a slice of the full SRS wishlist (issues/reviews/check
status on GitHub, comments/mentions/relations on Linear, branches/stashes in
git). See `devday-srs.md` §7 for the full target scope and
`.superpowers/sdd/task-14-brief.md` for the documented deferred items — they
extend the existing collectors without changing their interfaces.

## Install

```bash
cargo install --path .
```

This builds and installs the `devday` binary (Rust 2021, MSRV 1.75+).

## Quick start

1. Generate a starter config:

   ```bash
   mkdir -p ~/.config/devday
   devday config init > ~/.config/devday/config.toml
   ```

2. Edit `~/.config/devday/config.toml`:
   - Set `[git] roots` to the directories you want scanned locally (see
     **Local git roots** below — this step is required for local git
     activity to appear at all).
   - Set `[linear] token_env` to the name of an environment variable that
     holds your Linear API key (the template defaults to
     `LINEAR_API_KEY`), and export that variable in your shell.
   - Set `[ai] provider` to `"claude"` or `"codex"` if you want AI
     summarization (requires the corresponding CLI to be installed and on
     `PATH`). Leave it commented out to always use the deterministic
     (non-AI) report.
   - Set `[slack]` if/when you want Slack delivery (see **Slack delivery**
     below).

3. Check your environment:

   ```bash
   devday doctor
   ```

   This reports whether `gh` is present/authenticated, whether a Linear
   token env var is set, whether the configured AI provider binary is on
   `PATH`, and whether auto-post Slack credentials are configured — without
   printing any secret values.

4. Generate a report:

   ```bash
   devday report --since 24h --github --linear --git --stdout
   ```

## Local git roots

`--git` only **enables** the local-git source; it does not tell `devday`
*where* to scan. Local git activity comes exclusively from `[git] roots` in
your config file. If you run `devday report --git` with no config file (or
a config with an empty `roots` list), local git scanning finds nothing and
the report correctly shows an empty state for that source — this is
intended, not a bug. Configure `[git] roots = ["~/code", "~/work/myrepo"]`
(each entry may be a single repo or a directory containing multiple repos,
one level deep) to get local activity.

## Example invocations

From the SRS runtime model (§6), the two commands you'd typically run from
cron or another scheduler:

```bash
devday report --since 24h --github --linear --git --ai claude --output report.md
devday send slack --since 24h --github --linear --git --ai codex
```

Other useful invocations:

```bash
# Deterministic report (no AI), printed to stdout.
devday report --since 24h --github --linear --git --no-ai --stdout

# Preview what would be posted to Slack, without posting.
devday send slack --since 24h --github --linear --git --preview

# Fuller Slack digest instead of the default executive summary.
devday send slack --since 24h --github --linear --git --verbose --preview

# Use an explicit config file (e.g. for a different profile/cron job).
devday report --config ~/.config/devday/work.toml --stdout

# Write a PDF report (format inferred from the .pdf extension).
devday report --since 24h --github --linear --git --output report.pdf
```

## Output formats

`devday report` writes Markdown by default. The output format is chosen as
follows:

- If `--format md` or `--format pdf` is given explicitly, it wins.
- Otherwise, a `.pdf` extension on `--output`/`output` (from `--output`, or
  from `output` in the config file) selects PDF; anything else stays
  Markdown.
- `--stdout` always prints Markdown, regardless of the file format written
  to `--output` (both can happen in the same run).
- `--format pdf` with no output path (`--output` and no `output` in the
  config) is an error — PDF has nowhere to be written and there is no PDF
  stdout mode.

The PDF renderer is pure Rust (no external tools like `wkhtmltopdf` or a
headless browser required). It produces the same five report sections as
the Markdown output (plus a Warnings section when the run recorded
generation warnings), with the same redaction applied before any content is
laid out on the page.

In the TUI, pressing `w` on the Report tab writes the report using the same
shared writer, inferring the format from the configured `output` path's
extension the same way the CLI does.

## Cron example

```cron
# Post a daily digest at 9am on weekdays.
0 9 * * 1-5 /usr/local/bin/devday send slack --since 24h --github --linear --git --ai claude --post >> ~/.local/state/devday/cron.log 2>&1
```

`devday` does not run its own scheduler (SRS §6) — invoke it from cron,
launchd, a GitHub Actions workflow, or any external scheduler. It exits
non-interactively and returns a non-zero exit code on failure, which is
safe for cron's default "mail me on failure" behavior.

## Slack delivery: preview vs. posting

- `devday send slack --preview` **always** prints the digest to stdout and
  **never** posts, regardless of any other flags.
- `devday send slack --post` posts **only if both** of the following are
  true:
  1. `--post` was passed on the command line, **and**
  2. `slack.auto_post = true` is set in the loaded config file.

  If either condition is missing (e.g. you pass `--post` but
  `slack.auto_post` is `false` or unset, which is the `config init`
  default), `devday` falls back to printing the preview and exits `0` —
  it never posts by accident.
- Slack delivery supports both an incoming webhook (`slack.webhook_env`)
  and a bot token (`slack.bot_token_env` + a channel); configure one of the
  two. Credentials are read from the named environment variable at
  send-time and are never printed, even in webhook/bot-token network
  errors (those are reported as a fixed generic message, not the
  underlying error text, so a URL or `Authorization` header can never leak
  through).
- Default output is a short executive digest; pass `--verbose` for the
  fuller worked-on list and unlimited source links.
- Scheduled/auto-post runs use local state
  (`[state] path`, default `~/.local/state/devday/state.json`) to avoid
  re-posting an identical report; if the exact same report content was
  already posted, the run skips posting and exits `0`.
- If a Slack post fails, `devday` prints the report to stdout and exits
  non-zero, so the content is not lost.

## Interactive TUI

`devday tui` launches a full-screen terminal UI (built on `ratatui`) for
exploring, generating, and posting a report interactively, as an
alternative to running the `report`/`send slack`/`doctor` CLI commands one
at a time.

```bash
devday tui
devday tui --config ~/.config/devday/work.toml
```

### Tabs

Switch tabs with the number keys `1`-`4`:

1. **Report** — generates a report (auto-generated on first open), shows
   groups and items in a two-pane view (`Tab` to move between panes,
   arrows/`j`/`k` to move the selection, `Enter` to open the selected
   item's URL), lets you cycle the time window (`s`) and toggle AI
   summarization (`a`) before regenerating (`r`), and can write the
   rendered report to disk (`w`).
2. **Slack** — shows the redacted Slack digest for the current report,
   toggles the verbose digest (`v`), and posts it (`p`) after an
   interactive yes/no confirmation.
3. **Config** — a fixed field list (enabled sources, git roots, Slack
   channel, the `webhook_env`/`bot_token_env` *variable names* (never
   values), `auto_post`, AI provider, output path, default since window)
   that you can navigate (arrows/`j`/`k`), edit or toggle (`Enter`), and
   save back to the loaded config file (`S`, then confirm).
4. **Doctor** — runs the same checks as `devday doctor` (`r` to re-run),
   and lets you clear local dedup state (`x`) after typing the literal
   word `clear` to confirm — an accidental keypress cannot wipe state.

### Key map

Mirrors the in-app `?` help overlay:

```
1-4 switch tab   q quit   ? help
Report: r regen  s window  a AI  Tab pane  Enter open  w write
Slack:  v verbose  p post (confirm)
Config: arrows move  Enter edit/toggle  S save
Doctor: r re-run  x clear state
```

`Ctrl-C` quits from anywhere. Any key closes the help overlay.

### TTY requirement

`devday tui` requires stdout to be an interactive terminal. If stdout is
not a TTY (piped, redirected, or run under a non-interactive script/CI
job), it exits immediately with an error instead of trying to enter the
alternate screen — use the non-interactive `devday report` / `devday send
slack` commands for automation instead.

### Posting semantics differ from the CLI

Posting from the TUI requires only the interactive confirmation; the
`slack.auto_post` config gate applies to the non-interactive `devday send
slack --post` path only. Concretely: in the Slack tab, pressing `p` and
then confirming posts as long as Slack credentials are configured,
regardless of `slack.auto_post` — a human pressing the key in real time
*is* the consent. `slack.auto_post` exists to gate the unattended CLI path
(cron, scripts) where no human is present to confirm; see **Slack
delivery: preview vs. posting** above for that gate's rules.

## Safety

- **Read-only sources.** GitHub collection uses `gh search prs` (a read
  query); Linear collection issues a read-only GraphQL query; local git
  scanning uses `git2`'s read APIs only. No source is ever mutated —
  no commits, no pushes, no comments, no state changes on GitHub or Linear.
- **No secrets in output.** Config never stores secrets directly — it
  stores the *name* of an environment variable to read a token from at
  runtime (`linear.token_env`, `slack.webhook_env`, `slack.bot_token_env`).
  All rendered output (report Markdown, Slack digest) is passed through a
  redaction pass that strips any GitHub (`ghp_`/`gho_`), Slack
  (`xoxb-`/`xoxp-`), or Linear (`lin_api_`) token-shaped substring before
  it's printed or sent. Slack network-transport errors are reported as a
  fixed, generic message rather than the underlying error text, since that
  text could otherwise embed a webhook URL or bearer token.
- **Preview by default.** `send slack --preview` never posts. Posting via
  `--post` additionally requires `slack.auto_post = true` in config — see
  **Slack delivery** above.
- **Local git is read-only.** No checkout, commit, fetch, or push is ever
  issued against a scanned repository.
- **Partial failure, not total failure.** If one source fails (e.g. `gh`
  not authenticated, Linear token unset, AI provider binary missing), that
  failure is recorded as a warning and the report is still generated from
  whatever sources succeeded.

## Acceptance criteria

See [`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) for the SRS §18 acceptance
criteria mapped to automated tests or documented manual verification
steps.

## Development

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

The full test suite includes unit tests alongside each module, a
deterministic-report golden test (`tests/report_golden.rs`), a secret-leak
guard (`tests/no_secrets.rs`), general CLI smoke tests (`tests/cli.rs`), and
the SRS §18 acceptance tests (`tests/acceptance.rs`).
