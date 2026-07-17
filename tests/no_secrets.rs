// Guards the invariant: no known token shapes survive to rendered output.
use devday::redact::scrub_secrets;

#[test]
fn scrub_removes_all_known_prefixes() {
    // One distinctive secret per known token prefix, so a regression in any
    // single prefix's matching/redaction logic is caught individually.
    let input = "ghp_GITHUBPAT gho_GITHUBOAUTH xoxb-SLACKBOT xoxp-SLACKUSER lin_api_LINEARKEY";
    let out = scrub_secrets(input);
    for leaked in [
        "ghp_GITHUBPAT",
        "gho_GITHUBOAUTH",
        "xoxb-SLACKBOT",
        "xoxp-SLACKUSER",
        "lin_api_LINEARKEY",
    ] {
        assert!(!out.contains(leaked), "leaked {leaked}");
    }
}
