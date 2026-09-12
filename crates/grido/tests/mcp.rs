//! The spreadsheet's MCP surface.
//!
//! The transport lives in the `enclave` host and is tested there; what matters
//! here is that the tools and the skill are ones a model can act on.

/// Every advertised tool must have a name, a description and an object schema
/// — a tool the model cannot understand is worse than no tool.
#[test]
fn tool_definitions_are_well_formed() {
    let tools = grido::mcp::tools::definitions();
    assert!(tools.len() >= 15, "expected a useful tool surface");
    for tool in &tools {
        let name = tool["name"].as_str().expect("name");
        // One server carries every product, so a name says which one it is.
        assert!(
            name.starts_with(grido::mcp::tools::PREFIX),
            "{name} must be named grido_<verb>"
        );
        let desc = tool["description"].as_str().expect("description");
        assert!(
            desc.len() > 20,
            "{name} needs a description the model can act on"
        );
        assert_eq!(tool["inputSchema"]["type"], "object", "{name} schema");
        assert!(
            tool["inputSchema"]["properties"].is_object(),
            "{name} properties"
        );
    }
}

/// Registration writes one entry, drops the ones this install superseded, and
/// leaves everything else in the file alone — it is the user's config.
#[test]
fn registering_replaces_only_what_it_owns() {
    let exe = std::env::current_exe().expect("exe");
    let dir = std::env::temp_dir().join(format!("enclave-register-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let before = serde_json::json!({
        "mcpServers": {
            // Ours, under the names this server used to have.
            "sheetz": {"command": exe.display().to_string(), "args": ["mcp"]},
            "grido": {"command": exe.display().to_string(), "args": ["mcp"]},
            // The Go shim this binary replaces.
            "enclave": {"type": "stdio", "command": "/home/someone/.local/bin/enclave-mcp"},
            // Someone else's server that happens to share a name.
            "notes": {"command": "/usr/bin/notes-mcp"},
        },
        "theme": "dark",
    });
    std::fs::write(&path, serde_json::to_string_pretty(&before).unwrap()).expect("write");

    assert!(grido::mcp::register::register_one(&path).expect("register"));
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let servers = after["mcpServers"].as_object().expect("servers");
    assert_eq!(
        servers["enclave"]["command"],
        serde_json::json!(exe.display().to_string())
    );
    assert_eq!(servers["enclave"]["args"], serde_json::json!(["mcp"]));
    assert!(!servers.contains_key("sheetz"), "the old name must go");
    assert!(!servers.contains_key("grido"), "the old name must go");
    assert!(servers.contains_key("notes"), "someone else's server stays");
    assert_eq!(after["theme"], "dark", "every other setting stays");
    assert!(path.with_extension("json.bak").is_file(), "kept a backup");

    // Correct already: nothing rewritten, however the client spells the rest.
    assert!(!grido::mcp::register::register_one(&path).expect("register"));
    std::fs::remove_dir_all(&dir).ok();
}

/// The skill must carry frontmatter a client can index, and a description
/// specific enough to trigger on real requests.
#[test]
fn the_skill_has_usable_frontmatter() {
    let skill = std::fs::read_to_string("src/mcp/skill.rs").expect("skill source");
    let start = skill.find("---\nname: grido").expect("frontmatter");
    let body = &skill[start..];
    assert!(body.contains("description: "), "needs a description");
    let desc_line = body
        .lines()
        .find(|l| l.starts_with("description: "))
        .unwrap();
    assert!(
        desc_line.len() > 120,
        "the description decides whether the skill triggers; make it specific"
    );
    assert!(body.contains("spreadsheet"));
}
