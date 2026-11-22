use std::collections::BTreeMap;
use std::sync::Arc;

use labvpn_core::config::AppConfig;
use labvpn_core::generator::{generate, RuntimeContext};
use labvpn_core::rules::import::{self, ImportError, Issue, Outlet};
use labvpn_core::rules::matcher::match_target;

const RULE_SET: &str = "https://example.org/source.list";

#[test]
fn imports_shadowrocket_without_sensitive_sections_and_keeps_order() {
    let source = format!(
        "\u{feff}[Proxy]\nnode=hysteria2,example.org,443,password=private\n\
        [Proxy Group]\nStreaming=select,DIRECT,PROXY,policy-select-name=PROXY\n\
        Apple=select,PROXY,DIRECT,policy-select-name=DIRECT\n\
        [Rule]\nRULE-SET,{RULE_SET},STREAMING\nHOST,apple.example,Apple\n\
        IP-CIDR,203.0.113.1/24,DIRECT,no-resolve\nURL-REGEX,secret,REJECT\n\
        FINAL,PROXY\nDOMAIN-SUFFIX,ignored.example,REJECT\n\
        [Other-Section]\nDOMAIN-SUFFIX,also-ignored.example,REJECT\n\
        [MITM]\nsecret=private"
    );
    let sets = BTreeMap::from([(
        RULE_SET.into(),
        "HOST,video.example,Original\nHOST-SUFFIX,video-cdn.example,Original\nIP6-CIDR,2001:db8::1/32,Original,no-resolve\nUSER-AGENT,Private*,Original\n".into(),
    )]);
    assert_eq!(import::referenced_sets(&source).unwrap(), vec![RULE_SET]);
    let preview = import::preview("demo.conf", &source, Outlet::Direct, &sets).unwrap();
    assert_eq!((preview.rule_count, preview.set_count), (5, 1));
    assert_eq!(preview.skipped[0].reason, Issue::UnsupportedRule);
    assert_eq!(preview.skipped[0].count, 2);
    assert_eq!(preview.skipped[1].reason, Issue::AfterFallback);
    let profile = preview.profile.unwrap();
    assert!(profile.valid());
    assert_eq!(profile.fallback, Some(Outlet::Proxy));
    assert_eq!(profile.entries[1].outlet, Outlet::Direct);
    assert_eq!(
        profile.entries[2].matchers,
        ["IP-CIDR,203.0.113.0/24,no-resolve"]
    );
    let json = serde_json::to_string(&profile).unwrap();
    assert!(!json.contains("private") && !json.contains(RULE_SET));

    let mut config = AppConfig::default();
    config
        .routing
        .custom_rules
        .push("DOMAIN,manual.example,REJECT".into());
    config.routing.imported_rules = Some(Arc::new(profile));
    let generated = generate(&config, &RuntimeContext::default()).unwrap();
    assert_eq!(generated.plan.rules.last().unwrap(), "MATCH,🚀 代理");
    assert!(generated.yaml.contains("type: inline"));
    assert!(!generated.yaml.contains(RULE_SET));
    let pos = |prefix: &str| {
        generated
            .plan
            .rules
            .iter()
            .position(|r| r.starts_with(prefix))
            .unwrap()
    };
    assert!(pos("RULE-SET,FDUDomain") < pos("DOMAIN,manual.example"));
    assert!(pos("DOMAIN,manual.example") < pos("RULE-SET,LabVPNImport0"));
    assert!(pos("RULE-SET,LabVPNImport0") < pos("DOMAIN,apple.example"));
    let payloads: Vec<_> = generated
        .plan
        .providers
        .iter()
        .filter_map(|provider| provider.inline_payload())
        .collect();
    for (host, outlet) in [
        ("video.example", "🚀 代理"),
        ("apple.example", "DIRECT"),
        ("203.0.113.12", "DIRECT"),
    ] {
        assert_eq!(
            match_target(host, &generated.plan, &payloads).outbound,
            outlet
        );
    }
}

#[test]
fn accepts_yaml_payload_and_plain_file_and_rejects_raw_yaml() {
    let text = "payload:\n  - '+.example.org'\n  - '2001:db8::/32'\n  - 'DOMAIN-KEYWORD,sample'\n";
    let preview = import::preview("payload.yaml", text, Outlet::Reject, &BTreeMap::new()).unwrap();
    assert_eq!(preview.rule_count, 3);
    assert!(preview
        .profile
        .unwrap()
        .entries
        .iter()
        .all(|entry| entry.outlet == Outlet::Reject));
    let text = "DOMAIN-SUFFIX,example.org,DIRECT\n198.51.100.4\nMATCH,PROXY\n";
    let preview = import::preview("rules.list", text, Outlet::Proxy, &BTreeMap::new()).unwrap();
    assert_eq!(preview.rule_count, 2);
    assert_eq!(
        preview.profile.unwrap().entries[1].matchers[0],
        "IP-CIDR,198.51.100.4/32"
    );
    assert!(matches!(
        import::preview(
            "not.conf",
            "[General]\nserver=x",
            Outlet::Proxy,
            &BTreeMap::new()
        ),
        Err(ImportError::InvalidFormat)
    ));
}

#[test]
fn reports_unknown_outlet_and_never_resolves_network_or_local_references_itself() {
    let text = "[Proxy Group]\nLoop=select,loop\n[Rule]\nRULE-SET,file:///tmp/private,PROXY\n\
        DOMAIN,corp.example,Loop\nFINAL,Unknown\nDOMAIN,after.example,DIRECT\n";
    let preview = import::preview("rules.conf", text, Outlet::Proxy, &BTreeMap::new()).unwrap();
    assert!(preview.profile.is_none());
    assert_eq!(preview.skipped.iter().map(|r| r.count).sum::<usize>(), 4);
    assert!(!serde_json::to_string(&preview)
        .unwrap()
        .contains("file:///"));
}
