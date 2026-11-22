//! The `crisp-mcp` binary over the real MCP stdio transport:
//! `cargo test -p crisp-mcp --test stdio`.
//!
//! Spawns the binary with a throwaway key file and library, connects an
//! rmcp client, and checks that a recipe drafted as JSON comes out as a
//! `.crisp` file that verifies.

use crisp_core::keystore;
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::IdentitySeed;
use crisp_core::verify::{Trust, verify};
use rmcp::ServiceExt as _;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::{ConfigureCommandExt as _, TokioChildProcess};
use serde_json::{Value, json};

struct Setup {
    _dir: tempfile::TempDir,
    library: std::path::PathBuf,
    user_id: String,
    client: RunningService<RoleClient, ()>,
}

async fn start() -> Setup {
    let dir = tempfile::tempdir().expect("tempdir");
    let seed = IdentitySeed::generate().expect("seed");
    let user_id = seed.keyset().user_id().expect("uid").0;
    let key = dir.path().join("me.crispkey");
    let wrapped = keystore::wrap(&seed, "a long passphrase").expect("wrap");
    std::fs::write(&key, wrapped.to_bytes().expect("bytes")).expect("key");
    let pass = dir.path().join("passphrase");
    std::fs::write(&pass, "a long passphrase\n").expect("passphrase");
    let library = dir.path().join("library");
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_crisp-mcp")).configure(|cmd| {
            cmd.env_clear()
                .env("CRISP_LIBRARY_DIR", &library)
                .env("CRISP_KEY_FILE", &key)
                .env("CRISP_KEY_PASSPHRASE_FILE", &pass)
                .env("CRISP_DISPLAY_NAME", "Stdio Cook")
                .env("CRISP_LOG", "debug");
        }),
    )
    .expect("spawns crisp-mcp");
    let client = ().serve(transport).await.expect("initializes");
    Setup {
        _dir: dir,
        library,
        user_id,
        client,
    }
}

async fn call(
    client: &RunningService<RoleClient, ()>,
    name: &'static str,
    arguments: Value,
) -> CallToolResult {
    let Value::Object(arguments) = arguments else {
        panic!("arguments must be an object");
    };
    client
        .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
        .await
        .expect("the call completes")
}

fn draft() -> Value {
    json!({
        "language": "en",
        "title": "Soft-boiled eggs",
        "yield": { "amount": "2", "unit_text": "eggs" },
        "ingredients": [{ "ref": 1, "name": "eggs", "quantity": "2" }],
        "steps": [{
            "text": "Lower the eggs into boiling water.",
            "ingredients": [{ "ref": 1 }],
            "timers": [{ "duration": "6 min 30 s", "label": "boil" }],
            "temperature": { "value": "100 °C", "target": "water" }
        }],
        "notes": "Timing sets the yolk; see [the science](https://example.com/eggs)."
    })
}

#[tokio::test]
async fn a_drafted_recipe_is_validated_saved_and_verifies() {
    let Setup {
        library,
        user_id,
        client,
        _dir,
    } = start().await;
    let info = client.peer_info().expect("server info");
    assert!(
        info.instructions
            .as_deref()
            .is_some_and(|i| i.contains("save_recipe"))
    );
    let mut names: Vec<String> = client
        .list_all_tools()
        .await
        .expect("lists tools")
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    names.sort();
    assert_eq!(names.len(), 7, "{names:?}");

    let status = call(&client, "crisp_status", json!({})).await;
    let status = status.structured_content.expect("structured status");
    assert_eq!(status["user_id"], json!(user_id));

    let mut bad = draft();
    bad["steps"][0]["heat"] = json!("volcanic");
    let checked = call(&client, "validate_recipe", json!({ "draft": bad })).await;
    let checked = checked.structured_content.expect("structured");
    assert_eq!(checked["valid"], json!(false));
    assert_eq!(checked["issues"][0]["path"], json!("/steps/0/heat"));

    let saved = call(&client, "save_recipe", json!({ "draft": draft() })).await;
    assert_ne!(saved.is_error, Some(true), "{saved:?}");
    let saved = saved.structured_content.expect("structured");
    assert_eq!(saved["outcome"], json!("created"));
    let doc_id = saved["doc_id"].as_str().expect("doc_id").to_owned();

    let bytes = std::fs::read(library.join(format!("{doc_id}.crisp"))).expect("the file exists");
    let report = verify(&bytes, &[], Timestamp(u64::MAX / 2));
    assert!(report.compliant, "{report:#?}");
    assert_eq!(report.trust, Trust::AuthorSigned);

    let got = call(&client, "get_recipe", json!({ "doc_id": doc_id })).await;
    let got = got.structured_content.expect("structured");
    assert_eq!(got["draft"]["title"], json!("Soft-boiled eggs"));
    assert_eq!(got["authors"], json!(["Stdio Cook"]));

    let publish = call(&client, "publish_recipe", json!({ "doc_id": doc_id })).await;
    assert_eq!(publish.is_error, Some(true));
    client.cancel().await.expect("shuts down");
}
