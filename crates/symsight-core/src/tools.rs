// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Local tool registry and the generate turn loop.
//!
//! No tool is registered by generate. Context files are read here, under
//! [`ToolPolicy`], and inserted into the prompt by the caller. A later
//! `read_file` tool should use this same policy.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{ContextError, LlmError, ToolError};
use crate::llm::{CompletionRequest, FunctionTool, LlmSession, ModelTurn, ToolCall, ToolResult};

/// Tool rounds allowed before the loop stops. The final message is not a turn.
pub const DEFAULT_MAX_TURNS: u32 = 3;

/// Per-file cap for prompt context.
pub const DEFAULT_MAX_CONTEXT_BYTES: u64 = 32 * 1024;

/// Total cap for every context file on one generate.
pub const DEFAULT_TOTAL_CONTEXT_BYTES: u64 = DEFAULT_MAX_CONTEXT_BYTES * 4;

/// Where a local tool is allowed to look, and how long the loop may run.
#[derive(Debug, Clone)]
pub struct ToolPolicy {
    pub max_turns: u32,
    pub max_bytes: u64,
    pub total_bytes: u64,
    pub roots: Vec<PathBuf>,
}

impl ToolPolicy {
    pub fn for_project(root: &Path) -> Self {
        Self {
            max_turns: DEFAULT_MAX_TURNS,
            max_bytes: DEFAULT_MAX_CONTEXT_BYTES,
            total_bytes: DEFAULT_TOTAL_CONTEXT_BYTES,
            roots: vec![root.to_path_buf()],
        }
    }
}

/// JSON-schema tool the model may call. Generate advertises none today.
pub trait Tool {
    fn spec(&self) -> FunctionTool;
    fn call(&self, args: &Value) -> Result<String, ToolError>;
}

/// Registered tools plus the policy that bounds them.
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    policy: ToolPolicy,
}

impl ToolRegistry {
    pub fn empty(policy: ToolPolicy) -> Self {
        Self {
            tools: Vec::new(),
            policy,
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn policy(&self) -> &ToolPolicy {
        &self.policy
    }

    pub fn specs(&self) -> Vec<FunctionTool> {
        self.tools.iter().map(|tool| tool.spec()).collect()
    }

    pub fn invoke(&self, call: &ToolCall) -> Result<String, ToolError> {
        let Some(tool) = self.tools.iter().find(|tool| tool.spec().name == call.name) else {
            return Err(ToolError::NotRegistered {
                name: call.name.clone(),
            });
        };
        let args = if call.arguments.trim().is_empty() {
            Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&call.arguments).map_err(|err| ToolError::BadArguments {
                name: call.name.clone(),
                message: err.to_string(),
            })?
        };
        tool.call(&args)
    }
}

/// One file accepted into the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFile {
    /// Path relative to the project root.
    pub path: String,
    pub bytes: u64,
    pub text: String,
}

/// Read attached files. Relative paths resolve from the process current directory.
/// Prints `Read {path}, {bytes} bytes` for each file that is accepted.
pub fn load_context_files(
    paths: &[PathBuf],
    policy: &ToolPolicy,
) -> Result<Vec<ContextFile>, ContextError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for path in paths {
        if path.as_os_str().is_empty() {
            return Err(ContextError::NotFound(path.clone()));
        }
        let canon = canonicalize_user_path(path)?;
        if !seen.insert(canon.clone()) {
            continue;
        }
        let root = match find_root(&canon, &policy.roots)? {
            RootMatch::Inside(root) => root,
            RootMatch::NoRoot => return Err(ContextError::NoRoot),
            RootMatch::Outside => return Err(ContextError::OutsideRoot(path.clone())),
        };
        if !canon.is_file() {
            return Err(ContextError::NotAFile(display_path(path, &canon, &root)));
        }
        let shown = display_path(path, &canon, &root);
        if is_denied(&canon) {
            return Err(ContextError::Denied(shown));
        }
        let meta = fs::metadata(&canon)?;
        let len = meta.len();
        if len > policy.max_bytes {
            return Err(ContextError::TooLarge {
                path: shown,
                bytes: len,
                limit: policy.max_bytes,
            });
        }
        if total.saturating_add(len) > policy.total_bytes {
            return Err(ContextError::TotalTooLarge {
                bytes: total.saturating_add(len),
                limit: policy.total_bytes,
            });
        }
        let bytes = fs::read(&canon)?;
        if bytes.len() as u64 > policy.max_bytes {
            return Err(ContextError::TooLarge {
                path: shown,
                bytes: bytes.len() as u64,
                limit: policy.max_bytes,
            });
        }
        if bytes.contains(&0) {
            return Err(ContextError::Denied(shown));
        }
        let text = String::from_utf8(bytes).map_err(|_| ContextError::NotUtf8(shown.clone()))?;
        let nbytes = text.len() as u64;
        total = total.saturating_add(nbytes);
        if total > policy.total_bytes {
            return Err(ContextError::TotalTooLarge {
                bytes: total,
                limit: policy.total_bytes,
            });
        }
        eprintln!("Read {}, {nbytes} bytes", shown.display());
        files.push(ContextFile {
            path: shown.to_string_lossy().into_owned(),
            bytes: nbytes,
            text,
        });
    }
    Ok(files)
}

/// Text appended to the user prompt. Empty when nothing was attached.
pub fn context_block(files: &[ContextFile]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\nContext files (read locally before this request):\n");
    for file in files {
        out.push_str("\n## ");
        out.push_str(&file.path);
        out.push('\n');
        out.push_str(file.text.trim_end());
        out.push('\n');
    }
    out.push_str(
        "\nUse these notes as source material. The final answer is still only the required output format.\n",
    );
    out
}

/// Send `req`, run any tool calls, and return the final assistant text.
///
/// `registry` is empty during generate, so a tool call stops with
/// [`ToolError::NotRegistered`] instead of running anything.
pub fn run_to_message(
    session: &mut dyn LlmSession,
    req: &CompletionRequest,
    registry: &ToolRegistry,
) -> Result<String, LlmError> {
    let mut turn = session.complete(req, &registry.specs())?;
    let mut used = 0u32;
    loop {
        match turn {
            ModelTurn::Message(text) => return Ok(text),
            ModelTurn::Calls(calls) => {
                if calls.is_empty() {
                    return Err(LlmError::Http(
                        "model returned no tool calls and no message".into(),
                    ));
                }
                used += 1;
                if used > registry.policy().max_turns {
                    return Err(LlmError::Http(format!(
                        "stopped after {} tool turns",
                        registry.policy().max_turns
                    )));
                }
                let mut results = Vec::with_capacity(calls.len());
                for call in &calls {
                    let output = registry
                        .invoke(call)
                        .map_err(|err| LlmError::Http(err.to_string()))?;
                    eprintln!("Tool {} ({})", call.name, call.call_id);
                    results.push(ToolResult {
                        call_id: call.call_id.clone(),
                        output,
                    });
                }
                turn = session.resume(&results)?;
            }
        }
    }
}

fn canonicalize_user_path(path: &Path) -> Result<PathBuf, ContextError> {
    path.canonicalize().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            ContextError::NotFound(path.to_path_buf())
        } else {
            ContextError::Io(err)
        }
    })
}

enum RootMatch {
    Inside(PathBuf),
    Outside,
    NoRoot,
}

/// `Outside` keeps the caller's path in the error so a symlink target is not revealed.
fn find_root(canon: &Path, roots: &[PathBuf]) -> Result<RootMatch, ContextError> {
    let mut saw_root = false;
    for root in roots {
        let root_canon = match root.canonicalize() {
            Ok(path) => path,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(ContextError::Io(err)),
        };
        saw_root = true;
        if canon.starts_with(&root_canon) {
            return Ok(RootMatch::Inside(root_canon));
        }
    }
    if !saw_root {
        return Ok(RootMatch::NoRoot);
    }
    Ok(RootMatch::Outside)
}

fn display_path(original: &Path, canon: &Path, root: &Path) -> PathBuf {
    canon
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| original.to_path_buf())
}

fn is_denied(path: &Path) -> bool {
    for component in path.components() {
        if component.as_os_str() == ".git" {
            return true;
        }
    }
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return true;
    };
    let lower = name.to_ascii_lowercase();
    if lower == ".env" || lower.starts_with(".env.") {
        return true;
    }
    if matches!(
        lower.as_str(),
        "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519"
    ) {
        return true;
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let ext = ext.to_ascii_lowercase();
            ext == "pem" || ext == "key"
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ScriptedSession;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("symsight-context-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Echo;

    impl Tool for Echo {
        fn spec(&self) -> FunctionTool {
            FunctionTool {
                name: "echo".into(),
                description: "Echo arguments".into(),
                parameters: json!({"type": "object"}),
            }
        }

        fn call(&self, args: &Value) -> Result<String, ToolError> {
            Ok(args.to_string())
        }
    }

    fn prompt() -> CompletionRequest {
        CompletionRequest {
            model: "test".into(),
            system: "sys".into(),
            user: "user".into(),
            use_search: false,
        }
    }

    #[test]
    fn reads_a_note_under_the_project_root() {
        let root = tmp_dir();
        fs::write(root.join("notes.md"), "alpha beta\n").unwrap();
        let files =
            load_context_files(&[root.join("notes.md")], &ToolPolicy::for_project(&root)).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "notes.md");
        assert_eq!(files[0].text, "alpha beta\n");
        assert_eq!(files[0].bytes, 11);
        let block = context_block(&files);
        assert!(block.contains("## notes.md"));
        assert!(block.contains("alpha beta"));
    }

    #[test]
    fn rejects_env_git_keys_outside_and_binaries() {
        let root = tmp_dir();
        let policy = ToolPolicy::for_project(&root);
        fs::write(root.join(".env"), "TOKEN=secret").unwrap();
        let err = load_context_files(&[root.join(".env")], &policy).unwrap_err();
        assert!(err.to_string().contains("denied"), "{err}");

        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "x").unwrap();
        let err = load_context_files(&[root.join(".git/config")], &policy).unwrap_err();
        assert!(err.to_string().contains("denied"), "{err}");

        fs::write(root.join("id_rsa"), "key").unwrap();
        let err = load_context_files(&[root.join("id_rsa")], &policy).unwrap_err();
        assert!(err.to_string().contains("denied"), "{err}");

        let outside = std::env::temp_dir().join(format!(
            "symsight-outside-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&outside, "nope").unwrap();
        let err = load_context_files(std::slice::from_ref(&outside), &policy).unwrap_err();
        assert!(err.to_string().contains("outside"), "{err}");
        let _ = fs::remove_file(&outside);

        fs::write(root.join("notes.md"), b"hello\0").unwrap();
        let err = load_context_files(&[root.join("notes.md")], &policy).unwrap_err();
        assert!(err.to_string().contains("denied"), "{err}");

        let big = "a".repeat((DEFAULT_MAX_CONTEXT_BYTES as usize) + 1);
        fs::write(root.join("big.md"), big).unwrap();
        let err = load_context_files(&[root.join("big.md")], &policy).unwrap_err();
        assert!(err.to_string().contains("limit"), "{err}");

        fs::write(root.join("bad.md"), [0xff, 0xfe]).unwrap();
        let err = load_context_files(&[root.join("bad.md")], &policy).unwrap_err();
        assert!(err.to_string().contains("UTF-8"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlink_that_leaves_the_root() {
        let root = tmp_dir();
        let outside = std::env::temp_dir().join(format!(
            "symsight-link-target-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("notes.md")).unwrap();
        let err = load_context_files(&[root.join("notes.md")], &ToolPolicy::for_project(&root))
            .unwrap_err();
        let target_name = outside.file_name().unwrap().to_string_lossy().into_owned();
        assert!(err.to_string().contains("outside"), "{err}");
        assert!(err.to_string().contains("notes.md"), "{err}");
        assert!(!err.to_string().contains(&target_name), "{err}");
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn empty_registry_refuses_a_call_and_a_tool_can_round_trip() {
        let call = ToolCall {
            call_id: "c1".into(),
            name: "echo".into(),
            arguments: r#"{"text":"hi"}"#.into(),
        };
        let mut session = ScriptedSession::new(vec![
            ModelTurn::Calls(vec![call.clone()]),
            ModelTurn::Message("done".into()),
        ]);
        let registry = ToolRegistry::empty(ToolPolicy::for_project(Path::new(".")));
        let err = run_to_message(&mut session, &prompt(), &registry).unwrap_err();
        assert!(err.to_string().contains("not registered"), "{err}");

        let mut session = ScriptedSession::new(vec![
            ModelTurn::Calls(vec![call]),
            ModelTurn::Message("done".into()),
        ]);
        let mut registry = ToolRegistry::empty(ToolPolicy::for_project(Path::new(".")));
        registry.register(Box::new(Echo));
        let text = run_to_message(&mut session, &prompt(), &registry).unwrap();
        assert_eq!(text, "done");
        assert_eq!(session.resumes.len(), 1);
        assert!(session.resumes[0][0].output.contains("hi"));
    }

    #[test]
    fn loop_stops_at_the_turn_cap() {
        let call = ToolCall {
            call_id: "c1".into(),
            name: "echo".into(),
            arguments: "{}".into(),
        };
        let mut session = ScriptedSession::new(vec![
            ModelTurn::Calls(vec![call.clone()]),
            ModelTurn::Calls(vec![call]),
            ModelTurn::Message("done".into()),
        ]);
        let mut policy = ToolPolicy::for_project(Path::new("."));
        policy.max_turns = 1;
        let mut registry = ToolRegistry::empty(policy);
        registry.register(Box::new(Echo));
        let err = run_to_message(&mut session, &prompt(), &registry).unwrap_err();
        assert!(err.to_string().contains("stopped after 1"), "{err}");
    }

    #[test]
    fn duplicate_paths_are_read_once() {
        let root = tmp_dir();
        fs::write(root.join("notes.md"), "once").unwrap();
        let path = root.join("notes.md");
        let files =
            load_context_files(&[path.clone(), path], &ToolPolicy::for_project(&root)).unwrap();
        assert_eq!(files.len(), 1);
    }
}
