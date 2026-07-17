# SRS §18 Acceptance Criteria — Coverage Map

Each row maps one of the 19 MVP acceptance criteria from `devday-srs.md`
§18 to the automated test(s) that exercise it, and/or the manual
verification step required when full automation isn't practical (mainly:
runs against a real, authenticated `gh`, a real Linear workspace, or a real
Slack workspace — things that need live external credentials this repo's
CI/test environment doesn't have).

Test locations referenced below:

- `tests/acceptance.rs` — CLI-observable, run against the compiled binary.
- `tests/report_golden.rs`, `tests/cli.rs`, `tests/no_secrets.rs` — other
  integration tests.
- `src/**/mod.rs` (`#[cfg(test)]` blocks) — unit tests colocated with the
  code they exercise.

| # | Criterion | Coverage |
|---|-----------|----------|
| 1 | User can initialize config with `devday config init`. | Automated: `tests/acceptance.rs::ac1_config_init_emits_valid_toml` (asserts all 8 required sections appear in stdout); `src/config.rs::tests::init_template_is_valid_toml` (the emitted template parses as valid TOML back into `Config`). |
| 2 | User can generate a 24-hour report from GitHub, Linear, and local git. | Automated (wiring + per-source parsing): `src/cli.rs::tests::report_defaults_since_to_24h` (default `--since` is `24h`); `--github`/`--linear`/`--git` flag parsing and `main.rs::build_report` source dispatch; each collector unit-tested against fixtures — `src/collect/github.rs::tests::parses_pr_with_blocker_signals`, `src/collect/linear.rs::tests` (GraphQL response mapping), `src/collect/git.rs::tests::finds_recent_commit`/`finds_commits_from_directory_of_repos`. `tests/cli.rs::report_runs` smoke-tests `report --git` end-to-end. **Manual**: a single real run pulling genuine data from all three sources simultaneously requires a real authenticated `gh`, a real Linear token, and real recently-active local repos — run `devday report --since 24h --github --linear --git --stdout` with your real config and confirm all three sources contribute items. |
| 3 | The report answers what was worked on, what is next, and what is blocked. | Automated: `tests/acceptance.rs::ac2_report_generates_all_sections` (all headings present via the CLI); `src/report/mod.rs::tests::worked_on_lists_every_item`, `next_up_labels_inferred_items`, `explicit_blocker_is_classified_high_confidence`, `stale_branch_is_classified_inferred_low_confidence`. |
| 4 | The first real Slack run posts a useful executive digest with source links. | Automated (mechanics): `src/deliver/slack.rs::tests::digest_includes_blockers` (digest format), `webhook_post_succeeds`, `bot_post_succeeds` (mocked HTTP via `wiremock`); `tests/acceptance.rs::ac11_ac16_webhook_post_and_dedup_via_local_state` drives the real binary's `send slack --post` path against a mock webhook end-to-end. **Manual**: the literal "first real Slack run" against a genuine Slack workspace/channel — configure `[slack]`, export the webhook or bot token env var, run `devday send slack --post`, and visually confirm the posted message in Slack. |
| 5 | The report correctly groups linked work by Linear issue/project/team. | Automated: `src/group.rs::tests::groups_by_explicit_key_then_extracted_then_repo`, `extracts_key_from_commit_message` (issue-key extraction from commit messages / branch-style text). |
| 6 | Unlinked GitHub and local git activity appears under the repository name. | Automated: `src/group.rs::tests::groups_by_explicit_key_then_extracted_then_repo` (the `repoB`/no-key case falls back to repo name), `no_key_when_absent`. |
| 7 | The report identifies explicit and inferred blockers. | Automated: `src/report/mod.rs::tests::explicit_blocker_is_classified_high_confidence` (explicit, high confidence) and `stale_branch_is_classified_inferred_low_confidence` (inferred, low confidence). Note: no current collector emits the `stale-branch` signal yet (git branch scanning is a documented deferred item — see README "What it collects"), so the inferred-blocker *code path* is proven correct by direct unit test, but not yet reachable through a live collector run. |
| 8 | Local git scanning is read-only and reports commits, branches, and uncommitted work where available. | Automated: `src/collect/git.rs::tests::finds_recent_commit`, `detects_uncommitted_changes`, `finds_commits_from_directory_of_repos`, `missing_root_becomes_warning_not_error`. Read-only by construction: `scan_repo`/`scan_root` only call `git2` read APIs (`revwalk`, `find_commit`, `statuses`) — no `checkout`, `commit`, `push`, or `fetch` call exists anywhere in `src/collect/git.rs`. **Known scope gap** (documented in `.superpowers/sdd/task-14-brief.md`): active branches and stashes are not yet collected — commits and uncommitted-changes (WIP) are. |
| 9 | AI summarization works with at least one configured local CLI provider among Codex or Claude. | Automated (wiring proof via a stand-in binary): `src/ai/mod.rs::tests::summarize_uses_cat_stand_in` (points `cfg.command` at `cat` so stdout deterministically equals the prompt, proving the provider-invocation plumbing); `src/ai/cli_provider.rs::tests::pipes_prompt_through_cat`. **Manual**: running against the real `claude` or `codex` CLI requires that binary installed and authenticated locally — run `devday report --ai claude --stdout` (or `--ai codex`) and confirm the Summary section contains an AI-generated paragraph instead of the deterministic item-count fallback. |
| 10 | AI failure falls back to a usable non-AI report. | Automated: `tests/acceptance.rs::ac10_ai_failure_falls_back_to_deterministic_report` (real binary, `--ai` points at a nonexistent binary, command still exits 0 with a normal report); `src/ai/mod.rs::tests::summarize_disabled_when_no_provider`; `src/ai/cli_provider.rs::tests::missing_binary_is_error`. |
| 11 | `devday send slack` can post via webhook or bot token. | Automated: `src/deliver/slack.rs::tests::webhook_post_succeeds`, `bot_post_succeeds` (both via mocked HTTP); `tests/acceptance.rs::ac11_ac16_webhook_post_and_dedup_via_local_state` exercises the webhook path through the real CLI end-to-end (config → env var → HTTP POST). The bot-token path is unit-tested at the transport layer but not driven through the CLI in an integration test (would require overriding the hardcoded `https://slack.com/api` base, which the CLI doesn't currently expose as a flag). **Manual**: confirm a real bot-token post to a real Slack channel. |
| 12 | Slack preview mode prevents posting and shows the message content. | Automated: `tests/acceptance.rs::ac12_slack_preview_does_not_post` (`--preview` prints the digest and exits 0, no network call is made since a fresh temp `HOME` has no Slack credentials configured — if it had tried to post, the run would need those credentials and none exist). |
| 13 | Slack auto-post mode posts without interactive approval only when explicitly configured. | Automated: `tests/acceptance.rs::ac13_post_without_auto_post_config_falls_back_to_preview` (`--post` alone, no `slack.auto_post = true` in config, falls back to preview); `tests/acceptance.rs::ac11_ac16_webhook_post_and_dedup_via_local_state` (`--post` **with** `slack.auto_post = true` does post, non-interactively); `tests/report_golden.rs::send_slack_post_without_auto_post_falls_back_to_preview` (duplicate coverage of the negative case from an earlier task). |
| 14 | `--verbose` produces a fuller report than the default executive digest. | Automated: `src/deliver/slack.rs::tests::verbose_adds_worked_on` (verbose digest contains the full worked-on list; non-verbose digest omits it, for identical report content). Not additionally covered at the CLI/integration level because a fresh temp `HOME` has no activity items to show the size difference over. |
| 15 | The CLI can be invoked non-interactively by cron. | Automated: `tests/acceptance.rs::ac15_runs_noninteractively` (no TTY is ever attached under `assert_cmd`, and the command completes without blocking on input). |
| 16 | Scheduled runs can avoid reposting duplicate items using local state. | Automated: `tests/acceptance.rs::ac11_ac16_webhook_post_and_dedup_via_local_state` (second identical `--post` run is deduped — the mock server's `.expect(1)` fails the test if a second POST occurs); `src/state.rs::tests::detects_already_posted_hash`, `roundtrips_via_file`. |
| 17 | Integration failures are reported clearly and partial reports are generated where possible. | Automated: `src/collect/git.rs::tests::missing_root_becomes_warning_not_error` (bad git root → warning, not a hard error, and the run still succeeds); `src/collect/linear.rs::tests::http_error_becomes_warning`; every `tests/acceptance.rs` test that runs with an isolated `HOME` implicitly demonstrates this for GitHub, since `gh` fails to authenticate against the empty temp `HOME` (see below) and the report still generates successfully with a warning rather than aborting. |
| 18 | No source system is mutated. | Verified by code inspection, not a runtime assertion: `src/collect/git.rs` only calls `git2` read APIs (`Repository::open`, `revwalk`, `find_commit`, `statuses`) — no `checkout`/`commit`/`push`/`fetch` call exists in the file; `src/collect/github.rs` only ever invokes `gh search prs` (a read query); `src/collect/linear.rs` only issues the read-only `Recent` GraphQL query (no mutation is defined anywhere in the file). **Manual**: run against a real GitHub repo/Linear workspace and confirm no commits, PRs, comments, issue updates, or Linear state changes result. |
| 19 | No secrets appear in stdout, report files, Slack messages, or logs. | Automated: `tests/no_secrets.rs::scrub_removes_all_known_prefixes` (all 5 known token prefixes — `ghp_`, `gho_`, `xoxb-`, `xoxp-`, `lin_api_` — each individually asserted stripped); `src/redact.rs::tests` (`redacts_github_token`, `redacts_slack_bot_token`, `extra_patterns_and_paths`); `src/deliver/slack.rs::tests::webhook_transport_error_does_not_leak_url`, `bot_transport_error_does_not_leak_token` (transport errors never echo the raw URL/token, by using a fixed generic message); `src/ai/mod.rs::tests::prompt_contains_activity_not_secrets` (AI prompts built from normalized report data only); `tests/acceptance.rs::ac19_help_output_has_no_secrets`. Config itself only ever stores env-var *names* (`token_env`, `webhook_env`, `bot_token_env`), never secret values, so there's nothing secret-shaped in a loaded `Config` to leak in the first place. |

## Why the temp-`HOME` pattern works as isolation, not just convenience

Every `tests/acceptance.rs` test runs `devday` with `HOME` pointed at a
fresh `tempfile::tempdir()`. Two effects fall out of that:

1. `Config::load(None)` finds no `~/.config/devday/config.toml` and uses
   `Config::default()` — the developer's real config, tokens, and Slack
   channel never influence the test.
2. The `gh` CLI resolves its own auth config relative to `$HOME`
   (`$HOME/.config/gh/hosts.yml`); against an empty temp `HOME`, `gh`
   reliably fails to authenticate and `devday` records that as a warning
   rather than making a real, authenticated network call to GitHub. This
   was confirmed directly:

   ```
   $ HOME=$(mktemp -d) gh search prs --author @me --limit 1 --json title
   To get started with GitHub CLI, please run:  gh auth login
   ...
   exit=4
   ```

   So the acceptance suite never depends on, nor accidentally exercises,
   the real developer's live GitHub/Linear/Slack credentials — while still
   observing the real binary's actual behavior end-to-end.
