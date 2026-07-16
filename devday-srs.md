# Software Requirements Specification: devday

## 1. Purpose

`devday` is an open-source Rust CLI that generates shareable engineering status reports from GitHub, Linear, and local git activity.

The first version is for the user, but the report must be suitable for co-workers via scheduled Slack delivery. It should answer:

- What was worked on?
- What is going to be worked on next?
- What blockers exist?

`devday` is a private-first personal operating report, not an employee-surveillance dashboard.

## 2. MVP Product Goals

- Generate a daily status report from GitHub, Linear, and local git.
- Use provider-configured AI summarization, initially Codex or Claude.
- Send scheduled Slack reports when invoked by cron or another external scheduler.
- Support configurable preview/approval before Slack posting, with optional fully automatic posting.
- Prefer the local `gh` CLI for GitHub collection.
- Call the Linear API directly with a configured token.
- Use local Codex or Claude CLIs for AI summarization when available.
- Support both Slack incoming webhooks and Slack bot tokens.
- Keep local state to avoid reposting duplicate items during scheduled runs.
- Keep all integrations read-only except explicit report output and Slack message delivery.
- Avoid printing or sending secrets.
- Ship as a Rust CLI suitable for local use and cron execution.

## 3. Target Users

### Primary MVP User

A builder or AI-assisted developer working across GitHub, Linear, and local repositories who wants a reliable daily status report.

### Secondary MVP Reader

Co-workers who receive a concise Slack status report showing progress, next work, and blockers.

## 4. Core User Story

As a builder, I want `devday` to collect my recent GitHub, Linear, and local git activity, summarize it with a configured AI provider, and post a Slack-ready status report so I and my co-workers can see what moved, what is next, and what is blocked.

## 5. MVP Scope

### Included

- Rust CLI.
- GitHub activity collection.
- Linear activity collection.
- Local git activity collection.
- Provider-configured AI summarization with Codex or Claude.
- Markdown report generation.
- Stdout and file output.
- Slack message delivery.
- Cron-compatible command execution.
- Configurable preview/approval versus automatic Slack posting.
- Local state for scheduled-run deduplication.
- Linear-first grouping with GitHub/local git fallback grouping.
- Executive Slack digest by default, with verbose mode.
- Config file plus CLI flags.

### Excluded

- Built-in long-running scheduler or daemon.
- Mutating GitHub, Linear, or local repositories.
- Employee-surveillance dashboards or manager analytics.
- Multi-user account administration.
- Web application UI.
- Remote host scanning.
- Calendar integration.
- Email or Telegram delivery.

## 6. Runtime and Scheduling Model

`devday` must not own scheduling in MVP. It should expose commands that can be run manually or by cron, launchd, GitHub Actions, or another external scheduler.

Example:

```bash
devday report --since 24h --github --linear --git --ai claude --output report.md
devday send slack --since 24h --github --linear --git --ai codex
```

If Slack preview mode is enabled, the command should generate a preview instead of posting directly. If auto-post mode is enabled, the command may post to the configured Slack channel without interactive approval.

## 7. Source Requirements

### 7.1 GitHub

The system shall prefer the local `gh` CLI for GitHub activity collection. Direct GitHub API calls may be used later as a fallback or advanced mode, but MVP behavior should assume `gh` is available and authenticated.

The system shall collect authenticated-user activity within the selected time window, including:

- Pull requests opened, updated, merged, reviewed, or commented on.
- Issues opened, updated, assigned, closed, commented on, or mentioned.
- Commits associated with the authenticated user where available.
- Review requests and requested changes.
- Failing or pending checks when relevant to blockers.

Each GitHub item should preserve repository, title, URL, timestamp, activity type, status, and any Linear issue references discovered in branch names, PR titles, PR bodies, issue links, or commit messages.

### 7.2 Linear

The system shall call the Linear API directly with a configured token.

The system shall collect authenticated-user activity within the selected time window, including:

- Assigned issues.
- Issues updated by the user.
- Status changes.
- Comments and mentions involving the user.
- Priority and due-date signals.
- Blocked states, labels, relations, or blocked-like workflow states.

Each Linear item should preserve issue ID, title, team/project, status, assignee, URL, timestamp, and blocker signals.

### 7.3 Local Git

The system shall scan configured local repositories or roots for:

- Commits authored by the user in the selected time window.
- Active branches.
- Uncommitted changes.
- Recently modified tracked files.
- Stashes or work-in-progress indicators where practical.

Local git scanning must be read-only. It must not run mutating commands.

## 8. Report Requirements

The report must be generated in Markdown and include:

1. Summary
2. Worked On
3. Next Up
4. Blockers
5. Source Links

The report should group items primarily by Linear issue, project, or team. GitHub and local git activity should join to the matching Linear issue when a link, issue key, branch name, PR title, PR body, or commit message indicates they are the same work item. Unlinked GitHub or local git activity should fall back to the repository name.

### Worked On

"Worked on" means source activity showing user involvement during the selected time window.

Signals include GitHub PRs/issues/comments/reviews, Linear issue movement/comments/assignments, and local git commits or work-in-progress changes.

### Next Up

"Next up" means work likely to deserve attention next.

Signals include:

- Linear issues assigned to the user in active statuses.
- GitHub PRs awaiting user action.
- PRs with failing checks or requested changes.
- Recent local branches with unmerged or uncommitted work.
- High-priority Linear issues.

The report must label inferred next work as inferred when it is not explicit.

### Blockers

"Blockers" means items that appear unable to progress without review, approval, fixes, external input, or user action.

Signals include:

- Linear blocked status, labels, or relations.
- GitHub failing checks.
- Requested changes.
- PRs waiting for review.
- Local work with unresolved conflicts or failing known checks, if detectable.

The report must distinguish explicit blockers from inferred blockers.

## 9. AI Summarization Requirements

AI summarization is included in MVP and must be provider-configured.

Supported initial providers:

- Codex
- Claude

Requirements:

- AI provider must be explicitly configured.
- Local Codex or Claude CLIs should be used when available.
- The tool must be able to run without AI only if the user disables summarization.
- Prompts should receive normalized activity data, not raw secret-bearing payloads.
- The report should retain source links so AI output can be checked.
- AI failures should produce a useful non-AI fallback report rather than total failure.
- Provider selection and model/settings should be configurable.

## 10. Slack Delivery Requirements

The MVP shall support posting the generated report to a configured Slack channel.

Requirements:

- Slack channel must be configured explicitly.
- Slack delivery must support both incoming webhooks and bot tokens.
- Slack token or webhook credentials must not be printed.
- Slack delivery must support preview/approval mode.
- Slack delivery must support automatic posting mode.
- Slack output should default to a short executive digest while preserving key source links.
- Slack output must support a verbose mode for fuller detail.
- If Slack posting fails, the command must return a non-zero exit code and preserve the generated report locally or in stdout.

## 11. CLI Requirements

Required commands:

```bash
devday report
devday send slack
devday config init
devday doctor
```

Required report flags:

- `--since <duration>`: lookback window, default `24h`.
- `--github`: include GitHub source.
- `--linear`: include Linear source.
- `--git`: include local git source.
- `--ai <provider>`: summarize with configured provider.
- `--no-ai`: generate deterministic non-AI report.
- `--output <path>`: write Markdown report.
- `--stdout`: print Markdown.
- `--config <path>`: use explicit config.

Required Slack flags:

- `--channel <channel>`: override configured Slack channel.
- `--preview`: generate Slack preview without posting.
- `--post`: post to Slack if config allows it.
- `--verbose`: generate a fuller Slack/report output instead of the default executive digest.

## 12. Configuration Requirements

`devday` shall support a local config file defining:

- Enabled sources.
- GitHub account, organizations, repositories, and filters.
- Linear workspace, teams, projects, labels, and filters.
- Local git roots and repo include/exclude rules.
- AI provider and provider settings.
- Slack channel and delivery mode.
- Slack webhook and bot-token delivery configuration.
- Preview-required versus auto-post behavior.
- Project grouping rules.
- Local state path and deduplication behavior.
- Redaction rules for coworker-facing output.
- Default output path and report window.

Secrets should be read from environment variables, provider CLIs, OS credential storage, or explicitly configured secret references. Plaintext secrets in config should be discouraged.

## 13. Rust Implementation Requirements

The MVP shall be implemented in Rust.

The implementation should prioritize:

- Reliable single-binary CLI distribution.
- Fast startup for cron use.
- Clear error handling.
- Read-only source collection.
- Structured normalized activity models.
- Deterministic report generation when AI is disabled.

Suggested Rust design:

- `devday-core`: activity model, grouping, report generation.
- `devday-sources`: GitHub, Linear, and local git collectors.
- `devday-ai`: Codex and Claude summarization adapters.
- `devday-delivery`: Slack delivery and preview formatting.
- `devday-cli`: command parsing and orchestration.

The exact crate layout may change during planning, but source collection, summarization, report generation, and delivery should stay separable.

## 14. Privacy and Safety Requirements

- No mutation of GitHub, Linear, or local repositories.
- No secrets in generated reports.
- No authorization headers or tokens in logs.
- No automatic Slack posting unless explicitly configured.
- Preview mode must be available for Slack.
- AI prompts must exclude secret-bearing payloads.
- Coworker-facing reports may show local filesystem paths and private repo details by default, but redaction must be configurable.

## 15. Functional Requirements

| ID | Requirement |
| --- | --- |
| FR-1 | The system shall authenticate to GitHub using configured credentials or supported local auth. |
| FR-2 | The system shall prefer the authenticated local `gh` CLI for GitHub collection. |
| FR-3 | The system shall authenticate to Linear using a configured token and call the Linear API directly. |
| FR-4 | The system shall scan configured local git repositories read-only. |
| FR-5 | The system shall normalize GitHub, Linear, and git activity into a common activity model. |
| FR-6 | The system shall group activity primarily by Linear issue/project/team, joining GitHub and local git activity when linked. |
| FR-7 | The system shall group unlinked GitHub and local git activity by repository name. |
| FR-8 | The system shall generate a Markdown report with Summary, Worked On, Next Up, Blockers, and Source Links. |
| FR-9 | The system shall summarize activity with local Codex or Claude CLIs when available and configured. |
| FR-10 | The system shall produce a non-AI fallback report when AI fails or is disabled. |
| FR-11 | The system shall support stdout and file output. |
| FR-12 | The system shall post reports to Slack when configured and invoked. |
| FR-13 | The system shall support Slack incoming webhooks and Slack bot tokens. |
| FR-14 | The system shall support preview and auto-post Slack modes. |
| FR-15 | The system shall support executive digest output by default and verbose output with `--verbose`. |
| FR-16 | The system shall keep local state for deduplicating scheduled Slack posts. |
| FR-17 | The system shall return useful non-zero errors for failed integrations while preserving partial results when possible. |
| FR-18 | The system shall avoid printing, logging, or sending secrets. |

## 16. Non-Functional Requirements

| ID | Requirement |
| --- | --- |
| NFR-1 | A technically capable user should be able to configure and generate a first report in under 15 minutes. |
| NFR-2 | The CLI should be suitable for cron execution. |
| NFR-3 | The tool should produce deterministic reports when AI is disabled. |
| NFR-4 | The tool should fail partially when one source is unavailable and continue with available sources. |
| NFR-5 | Markdown output should be readable without a custom renderer. |
| NFR-6 | Slack output should be concise enough for a channel update by default and support verbose detail when requested. |
| NFR-7 | Empty activity windows should produce a useful empty-state report. |
| NFR-8 | Source links should be preserved for verification. |
| NFR-9 | Cron-triggered runs should avoid reposting duplicate items when local state is enabled. |

## 17. Suggested Data Model

### ActivityItem

- `source`: `github`, `linear`, or `git`
- `source_id`
- `title`
- `url`
- `project_key`
- `repo`
- `activity_type`
- `status`
- `actor`
- `timestamp`
- `summary`
- `signals`

### Report

- `window_start`
- `window_end`
- `groups`
- `summary`
- `worked_on`
- `next_up`
- `blockers`
- `source_links`
- `generation_warnings`

### LocalState

- `last_successful_run_at`
- `posted_item_ids`
- `posted_report_hashes`
- `source_cursor_by_integration`
- `dedupe_window`

### BlockerSignal

- `kind`: `explicit` or `inferred`
- `reason`
- `source_item_url`
- `confidence`: `high`, `medium`, or `low`

## 18. Acceptance Criteria

The MVP is acceptable when:

1. A user can initialize config with `devday config init`.
2. A user can generate a 24-hour report from GitHub, Linear, and local git.
3. The report answers what was worked on, what is next, and what is blocked.
4. The first real Slack run posts a useful executive digest with source links.
5. The report correctly groups linked work by Linear issue/project/team.
6. Unlinked GitHub and local git activity appears under the repository name.
7. The report identifies explicit and inferred blockers.
8. Local git scanning is read-only and reports commits, branches, and uncommitted work where available.
9. AI summarization works with at least one configured local CLI provider among Codex or Claude.
10. AI failure falls back to a usable non-AI report.
11. `devday send slack` can post via webhook or bot token.
12. Slack preview mode prevents posting and shows the message content.
13. Slack auto-post mode posts without interactive approval only when explicitly configured.
14. `--verbose` produces a fuller report than the default executive digest.
15. The CLI can be invoked non-interactively by cron.
16. Scheduled runs can avoid reposting duplicate items using local state.
17. Integration failures are reported clearly and partial reports are generated where possible.
18. No source system is mutated.
19. No secrets appear in stdout, report files, Slack messages, or logs.

## 19. Open Decisions for Planning

- Exact `gh` commands and parsing strategy.
- Exact Linear GraphQL queries and pagination strategy.
- Exact local CLI invocation contract for Codex and Claude.
- Slack message formatting details for webhook versus bot-token delivery.
- Local state storage format and dedupe window.
- Default rules for recognizing Linear issue keys in branches, PRs, commits, and local repo names.

## 20. Recommended Next Step

Create a technical plan that validates the Rust crate structure, authentication strategy, provider APIs, and Slack posting flow before implementation.
