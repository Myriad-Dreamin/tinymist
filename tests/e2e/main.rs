use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    process::Command,
};

use lsp_server::RequestId;
use serde_json::{json, Value};

fn handle_io<T>(res: io::Result<T>) -> T {
    match res {
        Ok(status) => status,
        Err(err) => panic!("Error: {}", err),
    }
}

fn find_git_root() -> io::Result<PathBuf> {
    while !PathBuf::from(".git").exists() {
        if std::env::set_current_dir("..").is_err() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Git root not found",
            ));
        }
    }

    std::env::current_dir()
}

// fn exec<'a>(cmd: &str, args: impl IntoIterator<Item = &'a str>) -> ExitStatus
// {     handle_io(Command::new(cmd).args(args).status())
// }

fn find_char_boundary(s: &str, i: usize) -> usize {
    for j in -4..4 {
        let k = i as i64 + j;
        if k < 0 || k >= s.len() as i64 {
            continue;
        }
        if s.is_char_boundary(k as usize) {
            return k as usize;
        }
    }

    panic!("char boundary not found");
}

fn exec_output<'a>(cmd: &str, args: impl IntoIterator<Item = &'a str>) -> Vec<u8> {
    let output = handle_io(Command::new(cmd).args(args).output());
    let err = output.stderr;
    // if contains panic
    let err = std::str::from_utf8(&err).unwrap();
    let panic = err.find("panic");
    if let Some(p) = panic {
        // capture surrounding lines
        let panic_prev = p.saturating_sub(1024);
        let panic_next = (p + 10240).min(err.len());
        // find char boundary
        let panic_prev = find_char_boundary(err, panic_prev);
        let panic_next = find_char_boundary(err, panic_next);

        panic!(
            "panic found in stderr logging: PANIC_BEGIN\n\n{}\n\nPANIC_END",
            &err[panic_prev..panic_next]
        );
    }

    output.stdout
}

struct ReplayBuilder {
    id: i32,
    messages: Vec<lsp_server::Message>,
}

impl ReplayBuilder {
    fn request_(&mut self, method: String, req: Value) {
        let id = RequestId::from(self.id);
        self.id += 1;
        self.messages
            .push(lsp_server::Message::Request(lsp_server::Request::new(
                id, method, req,
            )));
    }

    fn request<R: lsp_types::request::Request>(&mut self, req: Value) {
        self.request_(R::METHOD.to_owned(), req);
    }

    fn notify_(&mut self, method: String, params: Value) {
        self.messages.push(lsp_server::Message::Notification(
            lsp_server::Notification::new(method, params),
        ));
    }

    fn notify<N: lsp_types::notification::Notification>(&mut self, params: Value) {
        self.notify_(N::METHOD.to_owned(), params);
    }
}

fn fixture(o: &str, f: impl FnOnce(&mut Value)) -> Value {
    // tests/fixtures/o.json
    let content = std::fs::read_to_string(format!("tests/fixtures/{}.json", o)).unwrap();
    let mut req = serde_json::from_str(&content).unwrap();
    f(&mut req);
    req
}

fn gen(root: &Path, f: impl FnOnce(&mut ReplayBuilder)) {
    let mut builder = ReplayBuilder {
        id: 1,
        messages: Vec::new(),
    };
    f(&mut builder);
    // mkdir
    handle_io(std::fs::create_dir_all(root));
    // open root/mirror.log
    let mut log = std::fs::File::create(root.join("mirror.log")).unwrap();
    for msg in builder.messages {
        msg.write(&mut log).unwrap();
    }
}

fn messages(output: Vec<u8>) -> Vec<lsp_server::Message> {
    let mut output = std::io::BufReader::new(output.as_slice());
    // read all messages
    let mut messages = Vec::new();
    while let Ok(Some(msg)) = lsp_server::Message::read(&mut output) {
        // match msg
        messages.push(msg);
    }
    messages
}

struct SmokeArgs {
    root: PathBuf,
    init: String,
    log: String,
}

fn gen_smoke(args: SmokeArgs) {
    use lsp_types::notification::*;
    use lsp_types::request::*;
    use lsp_types::*;

    let SmokeArgs { root, init, log } = args;
    gen(&root, |srv| {
        let root_uri = lsp_types::Url::from_directory_path(&root).unwrap();
        srv.request::<Initialize>(fixture(&init, |v| {
            v["rootUri"] = json!(root_uri);
            v["rootPath"] = json!(root);
            v["workspaceFolders"] = json!([{
                "uri": root_uri,
                "name": "tinymist",
            }]);
        }));
        srv.notify::<Initialized>(json!({}));

        // open editions/base.log and readlines
        let log = std::fs::read_to_string(&log).unwrap();
        let log = log.trim().split('\n').collect::<Vec<_>>();
        let mut uri_set = HashSet::new();
        let mut uris = Vec::new();
        let log_lines = log.len();
        for (idx, line) in log.into_iter().enumerate() {
            let mut v: Value = serde_json::from_str(line).unwrap();

            // discover range in contentChanges and construct signatureHelp
            let mut range_seeds = vec![];
            if let Some(content_changes) = v
                .get_mut("params")
                .and_then(|v| v.get_mut("contentChanges"))
            {
                for change in content_changes.as_array_mut().unwrap() {
                    let range = change.get("range");
                    if let Some(range) = range {
                        let range: Range = serde_json::from_value(range.clone()).unwrap();
                        range_seeds.push(range);
                    }
                }
            }

            let uri_name = v["params"]["textDocument"]["uri"].as_str().unwrap();
            let url_v = if uri_name.starts_with("file:") || uri_name.starts_with("untitled:") {
                lsp_types::Url::parse(uri_name).unwrap()
            } else {
                root_uri.join(uri_name).unwrap()
            };
            v["params"]["textDocument"]["uri"] = json!(url_v);
            let method = v["method"].as_str().unwrap();
            srv.notify_("textDocument/".to_owned() + method, v["params"].clone());

            let mut request_at_loc = |loc: Position| {
                let pos = TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url_v.clone() },
                    position: loc,
                };
                srv.request::<SignatureHelpRequest>(json!(SignatureHelpParams {
                    context: None,
                    work_done_progress_params: Default::default(),
                    text_document_position_params: pos.clone(),
                }));
                srv.request::<HoverRequest>(json!(HoverParams {
                    work_done_progress_params: Default::default(),
                    text_document_position_params: pos.clone(),
                }));
                if log_lines == idx + 1 || log_lines == idx + 5 || log_lines == idx + 10 {
                    srv.request::<Completion>(json!(CompletionParams {
                        text_document_position: pos.clone(),
                        context: None,
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    }));
                }
                srv.request::<GotoDefinition>(json!(GotoDefinitionParams {
                    text_document_position_params: pos.clone(),
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                }));
                srv.request::<References>(json!(ReferenceParams {
                    text_document_position: pos.clone(),
                    context: ReferenceContext {
                        include_declaration: false,
                    },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                }));
            };

            let mut seed_at_loc = |loc: Position| {
                for i in loc.character.saturating_sub(2)..loc.character + 2 {
                    request_at_loc(Position {
                        line: loc.line,
                        character: i,
                    });
                }
                for l_delta in -1i32..1i32 {
                    if l_delta == 0 {
                        continue;
                    }
                    let l = (loc.line as i32) + l_delta;
                    if l < 0 {
                        continue;
                    }
                    let l = l as u32;
                    for i in 0..3 {
                        request_at_loc(Position {
                            line: l,
                            character: i,
                        });
                    }
                }

                // 10..100
                for l_delta in -20i32..20i32 {
                    if l_delta == 0 {
                        continue;
                    }
                    let l = (loc.line as i32) + l_delta * 5;
                    if l < 0 {
                        continue;
                    }
                    let l = l as u32;
                    request_at_loc(Position {
                        line: l,
                        character: 0,
                    });
                    request_at_loc(Position {
                        line: l,
                        character: 2,
                    });
                }
            };

            for r in range_seeds {
                seed_at_loc(r.start);
                seed_at_loc(r.end);
            }

            if uri_set.insert(url_v.clone()) {
                uris.push(url_v);
            }
            const MI_POS: Position = Position {
                line: 0,
                character: 0,
            };
            const MX_POS: Position = Position {
                line: u32::MAX / 1024,
                character: u32::MAX / 1024,
            };
            for u in &uris {
                srv.request::<FoldingRangeRequest>(json!(FoldingRangeParams {
                    text_document: TextDocumentIdentifier { uri: u.clone() },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default()
                }));
                srv.request::<DocumentSymbolRequest>(json!(DocumentSymbolParams {
                    text_document: TextDocumentIdentifier { uri: u.clone() },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default()
                }));
                srv.request::<CodeLensRequest>(json!(CodeLensParams {
                    text_document: TextDocumentIdentifier { uri: u.clone() },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default()
                }));
                srv.request::<InlayHintRequest>(json!(InlayHintParams {
                    text_document: TextDocumentIdentifier { uri: u.clone() },
                    work_done_progress_params: Default::default(),
                    range: Range {
                        start: MI_POS,
                        end: MX_POS
                    }
                }));

                if log_lines == idx + 1 {
                    srv.request::<SemanticTokensFullRequest>(json!(SemanticTokensParams {
                        text_document: TextDocumentIdentifier { uri: u.clone() },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    }));
                }
            }
        }
    });
}

fn replay_log(tinymist_binary: &Path, root: &Path) -> String {
    let tinymist_binary = tinymist_binary.to_str().unwrap();

    let log_file = root.join("mirror.log").to_str().unwrap().to_owned();
    let mut res = messages(exec_output(tinymist_binary, ["lsp", "--replay", &log_file]));
    // retain not notification
    res.retain(|msg| matches!(msg, lsp_server::Message::Response(_)));
    // sort by id
    res.sort_by_key(|msg| match msg {
        lsp_server::Message::Request(req) => req.id.clone(),
        lsp_server::Message::Response(res) => res.id.clone(),
        lsp_server::Message::Notification(_) => RequestId::from(0),
    });
    // print to result.log
    let res = serde_json::to_value(&res).unwrap();
    let c = serde_json::to_string_pretty(&res).unwrap();
    std::fs::write(root.join("result.json"), c).unwrap();
    // let sorted_res
    let sorted_res = sort_and_redact_value(res);
    let c = serde_json::to_string_pretty(&sorted_res).unwrap();
    let hash = reflexo::hash::hash128(&c);
    std::fs::write(root.join("result_sorted.json"), c).unwrap();

    format!("siphash128_13:{:x}", hash)
}

#[test]
fn e2e() {
    std::env::set_var("RUST_BACKTRACE", "full");

    let cwd = find_git_root().unwrap();

    let tinymist_binary = if cfg!(windows) {
        cwd.join("editors/vscode/out/tinymist.exe")
    } else {
        cwd.join("editors/vscode/out/tinymist")
    };

    let root = cwd.join("target/e2e/tinymist");

    {
        gen_smoke(SmokeArgs {
            root: root.join("neovim"),
            init: "initialization/neovim-0.9.4".to_owned(),
            log: "tests/fixtures/editions/neovim_unnamed_buffer.log".to_owned(),
        });

        let hash = replay_log(&tinymist_binary, &root.join("neovim"));
        insta::assert_snapshot!(hash, @"siphash128_13:1739b86d5e2de99b19db308496ff94ae");
    }

    {
        gen_smoke(SmokeArgs {
            root: root.join("vscode"),
            init: "initialization/vscode-1.87.2".to_owned(),
            log: "tests/fixtures/editions/base.log".to_owned(),
        });

        let hash = replay_log(&tinymist_binary, &root.join("vscode"));
        insta::assert_snapshot!(hash, @"siphash128_13:360f6d60de40f590e63ebf23521e3d50");
    }
}

fn sort_and_redact_value(v: Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Bool(b),
        Value::Number(n) => Value::Number(n),
        Value::String(s) => Value::String(s),
        Value::Array(a) => {
            let mut a = a;
            a.sort_by(json_cmp);
            Value::Array(a.into_iter().map(sort_and_redact_value).collect())
        }
        Value::Object(o) => {
            let mut keys = o.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| {
                        (k.clone(), {
                            let v = &o[k];
                            if k == "uri" || k == "targetUri" {
                                // get uri and set as file name
                                let uri = v.as_str().unwrap();
                                if uri == "file://" || uri == "file:///" {
                                    Value::String("".to_owned())
                                } else {
                                    let uri = lsp_types::Url::parse(uri).unwrap();

                                    match uri.to_file_path() {
                                        Ok(path) => {
                                            let path = path.file_name().unwrap().to_str().unwrap();
                                            Value::String(path.to_owned())
                                        }
                                        Err(_) => Value::String(uri.to_string()),
                                    }
                                }
                            } else {
                                sort_and_redact_value(v.clone())
                            }
                        })
                    })
                    .collect(),
            )
        }
    }
}

fn json_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Number(a), Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                a.cmp(&b)
            } else if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
                a.cmp(&b)
            } else if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) {
                a.partial_cmp(&b).unwrap()
            } else {
                panic!("unexpected number type");
            }
        }
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Array(a), Value::Array(b)) => {
            let mut a = a.clone();
            let mut b = b.clone();
            if a.len() != b.len() {
                return a.len().cmp(&b.len());
            }

            a.sort_by(json_cmp);
            b.sort_by(json_cmp);
            for (a, b) in a.iter().zip(b.iter()) {
                let cmp = json_cmp(a, b);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }

            std::cmp::Ordering::Equal
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut keys_a = a.keys().collect::<Vec<_>>();
            let mut keys_b = b.keys().collect::<Vec<_>>();
            keys_a.sort();
            keys_b.sort();
            if keys_a != keys_b {
                return keys_a.cmp(&keys_b);
            }
            for k in keys_a {
                let cmp = json_cmp(&a[k], &b[k]);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            std::cmp::Ordering::Equal
        }
        _ => std::cmp::Ordering::Equal,
    }
}
