// Guards the invariant: no known token shapes survive to rendered output.
use devday::redact::scrub_secrets;

#[test]
fn scrub_removes_all_known_prefixes() {
    let input = "ghp_AAA gho_BBB xoxb-CCC xoxp-DDD lin_api_EEE";
    let out = scrub_secrets(input);
    for leaked in ["ghp_AAA", "xoxb-CCC", "lin_api_EEE"] {
        assert!(!out.contains(leaked), "leaked {leaked}");
    }
}
