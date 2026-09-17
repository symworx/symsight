// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! `symsight._native` — PyO3 surface over `symsight-core`.
//!
//! Complex values (Brand, AppConfig, GenerateRequest, DraftMeta) are passed as
//! Pydantic objects and converted at the boundary so existing tests stay intact.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use pyo3::create_exception;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule};
use serde::de::DeserializeOwned;
use serde::Serialize;
use symsight_core::{
    char_count, check_path, check_text, clean_body, extract_social_text, extract_title_body,
    finalize_draft, find_hits, generate_and_write, generate_content, is_plausible_title,
    iter_scan_files, length_rewrite_prompt, list_brand_files, list_brands, list_drafts, list_final,
    load_brand_file, parse_front_matter, read_draft, render_front_matter, resolve_brand,
    save_draft_body, scan_paths, set_status, slugify, strip_disclaimer_from_body, system_prompt,
    unique_draft_path, user_prompt, word_count, write_draft_content, write_new_draft, AppConfig,
    Brand, BrandError, CompletionRequest, ContentFormat, DraftMeta, FinalizeError, FrontValue,
    GenerateError, GenerateRequest, LlmClient, LlmError, WriteNewDraft,
};

create_exception!(
    _native,
    BrandErrorPy,
    pyo3::exceptions::PyException,
    "Brand load / resolve failure."
);
create_exception!(
    _native,
    GenerateErrorPy,
    pyo3::exceptions::PyException,
    "Generation failure."
);
create_exception!(
    _native,
    FinalizeErrorPy,
    pyo3::exceptions::PyException,
    "Finalize failure."
);

fn pydantic_to<T: DeserializeOwned>(obj: &Bound<'_, PyAny>) -> PyResult<T> {
    let py = obj.py();
    let dumped = if obj.hasattr("model_dump")? {
        obj.call_method0("model_dump")?
    } else {
        obj.clone()
    };
    let json = py.import("json")?;
    let text: String = json.call_method1("dumps", (dumped,))?.extract()?;
    serde_json::from_str(&text).map_err(|e| PyValueError::new_err(e.to_string()))
}

fn to_json_obj<T: Serialize>(py: Python<'_>, value: &T) -> PyResult<Py<PyAny>> {
    let text = serde_json::to_string(value).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let json = py.import("json")?;
    Ok(json.call_method1("loads", (text,))?.unbind())
}

fn path_from_py(obj: &Bound<'_, PyAny>) -> PyResult<PathBuf> {
    if let Ok(s) = obj.extract::<String>() {
        return Ok(PathBuf::from(s));
    }
    let s: String = obj.call_method0("__str__")?.extract()?;
    Ok(PathBuf::from(s))
}

fn extract_format(obj: &Bound<'_, PyAny>) -> PyResult<ContentFormat> {
    let s: String = if let Ok(v) = obj.getattr("value") {
        v.extract()?
    } else {
        obj.str()?.to_string()
    };
    match s.as_str() {
        "article" => Ok(ContentFormat::Article),
        "social" => Ok(ContentFormat::Social),
        other => Err(PyValueError::new_err(format!("unknown format {other}"))),
    }
}

fn extract_req(obj: &Bound<'_, PyAny>) -> PyResult<GenerateRequest> {
    let brand = pydantic_to::<Brand>(&obj.getattr("brand")?)?;
    let type_id: String = obj.getattr("type_id")?.extract()?;
    let format = extract_format(&obj.getattr("format")?)?;
    let topic: Option<String> = obj.getattr("topic")?.extract()?;
    let min_words: Option<i64> = obj.getattr("min_words")?.extract()?;
    let max_words: Option<i64> = obj.getattr("max_words")?.extract()?;
    let max_chars: Option<i64> = obj.getattr("max_chars")?.extract()?;
    let use_search: Option<bool> = obj.getattr("use_search")?.extract()?;
    let model: Option<String> = obj.getattr("model")?.extract()?;
    Ok(GenerateRequest {
        brand,
        type_id,
        format,
        topic,
        min_words,
        max_words,
        max_chars,
        use_search,
        model,
    })
}

fn extract_cfg(obj: &Bound<'_, PyAny>) -> PyResult<AppConfig> {
    let xai_api_key: String = obj.getattr("xai_api_key")?.extract().unwrap_or_default();
    let model: String = obj.getattr("model")?.extract()?;
    let base_url: String = obj.getattr("base_url")?.extract()?;
    let active_brand: String = obj.getattr("active_brand")?.extract()?;
    Ok(AppConfig {
        xai_api_key,
        model,
        base_url,
        active_brand,
        brands_dir: path_from_py(&obj.getattr("brands_dir")?)?,
        drafts_dir: path_from_py(&obj.getattr("drafts_dir")?)?,
        final_dir: path_from_py(&obj.getattr("final_dir")?)?,
        project_root: path_from_py(&obj.getattr("project_root")?)?,
    }
    .resolve_paths())
}

fn extract_meta(obj: &Bound<'_, PyAny>) -> PyResult<DraftMeta> {
    pydantic_to::<DraftMeta>(obj)
}

fn front_from_py(obj: &Bound<'_, PyAny>) -> PyResult<IndexMap<String, FrontValue>> {
    let dict = obj.cast::<PyDict>()?;
    let mut out = IndexMap::new();
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        let val = if v.is_none() {
            FrontValue::Null
        } else if let Ok(b) = v.extract::<bool>() {
            FrontValue::Bool(b)
        } else if let Ok(i) = v.extract::<i64>() {
            FrontValue::Int(i)
        } else if let Ok(f) = v.extract::<f64>() {
            FrontValue::Float(f)
        } else {
            FrontValue::String(v.extract()?)
        };
        out.insert(key, val);
    }
    Ok(out)
}

fn front_to_py(py: Python<'_>, front: &IndexMap<String, FrontValue>) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    for (k, v) in front {
        match v {
            FrontValue::Null => dict.set_item(k, py.None())?,
            FrontValue::Bool(b) => dict.set_item(k, b)?,
            FrontValue::Int(n) => dict.set_item(k, n)?,
            FrontValue::Float(n) => dict.set_item(k, n)?,
            FrontValue::String(s) => dict.set_item(k, s)?,
        }
    }
    Ok(dict.unbind())
}

fn map_brand(err: BrandError) -> PyErr {
    BrandErrorPy::new_err(err.to_string())
}

fn map_gen(err: GenerateError) -> PyErr {
    GenerateErrorPy::new_err(err.to_string())
}

fn map_fin(err: FinalizeError) -> PyErr {
    FinalizeErrorPy::new_err(err.to_string())
}

fn map_io(err: std::io::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

struct PyLlmClient {
    inner: Py<PyAny>,
}

impl LlmClient for PyLlmClient {
    fn create_completion(&self, req: &CompletionRequest) -> Result<String, LlmError> {
        Python::attach(|py| {
            let kwargs = PyDict::new(py);
            kwargs
                .set_item("model", &req.model)
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let input = PyList::empty(py);
            let sys = PyDict::new(py);
            sys.set_item("role", "system")
                .and_then(|_| sys.set_item("content", &req.system))
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let user = PyDict::new(py);
            user.set_item("role", "user")
                .and_then(|_| user.set_item("content", &req.user))
                .map_err(|e| LlmError::Http(e.to_string()))?;
            input
                .append(sys)
                .and_then(|_| input.append(user))
                .map_err(|e| LlmError::Http(e.to_string()))?;
            kwargs
                .set_item("input", input)
                .map_err(|e| LlmError::Http(e.to_string()))?;
            if req.use_search {
                let tools = PyList::empty(py);
                let tool = PyDict::new(py);
                tool.set_item("type", "web_search")
                    .map_err(|e| LlmError::Http(e.to_string()))?;
                tools
                    .append(tool)
                    .map_err(|e| LlmError::Http(e.to_string()))?;
                kwargs
                    .set_item("tools", tools)
                    .map_err(|e| LlmError::Http(e.to_string()))?;
            }
            let resp = self
                .inner
                .bind(py)
                .getattr("responses")
                .and_then(|r| r.call_method("create", (), Some(&kwargs)))
                .map_err(|e| LlmError::Http(e.to_string()))?;
            if let Ok(text) = resp.getattr("output_text") {
                if let Ok(s) = text.extract::<String>() {
                    if !s.trim().is_empty() {
                        return Ok(s);
                    }
                }
            }
            Ok(String::new())
        })
    }
}

#[pyfunction]
#[pyo3(name = "word_count")]
fn word_count_py(text: &str) -> usize {
    word_count(text)
}

#[pyfunction]
#[pyo3(name = "char_count")]
fn char_count_py(text: &str) -> usize {
    char_count(text)
}

#[pyfunction]
#[pyo3(name = "slugify", signature = (text, max_len=60))]
fn slugify_py(text: &str, max_len: usize) -> String {
    slugify(text, max_len)
}

#[pyfunction]
#[pyo3(name = "clean_body")]
fn clean_body_py(body: &str) -> String {
    clean_body(body)
}

#[pyfunction]
#[pyo3(name = "is_plausible_title")]
fn is_plausible_title_py(title: &str) -> bool {
    is_plausible_title(title)
}

#[pyfunction]
#[pyo3(name = "extract_title_body")]
fn extract_title_body_py(raw: &str) -> PyResult<(String, String)> {
    extract_title_body(raw).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "extract_social_text")]
fn extract_social_text_py(raw: &str) -> PyResult<String> {
    extract_social_text(raw).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "load_brand_file")]
fn load_brand_file_py(path: &str) -> PyResult<String> {
    let brand = load_brand_file(Path::new(path)).map_err(map_brand)?;
    serde_json::to_string(&brand).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "list_brand_files")]
fn list_brand_files_py(brands_dir: &str) -> Vec<String> {
    list_brand_files(Path::new(brands_dir))
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}

#[pyfunction]
#[pyo3(name = "list_brands")]
fn list_brands_py(brands_dir: &str) -> PyResult<Vec<String>> {
    list_brands(Path::new(brands_dir))
        .into_iter()
        .map(|b| serde_json::to_string(&b).map_err(|e| PyValueError::new_err(e.to_string())))
        .collect()
}

#[pyfunction]
#[pyo3(name = "resolve_brand", signature = (brands_dir, brand_id=None, brand_path=None))]
fn resolve_brand_py(
    brands_dir: &str,
    brand_id: Option<&str>,
    brand_path: Option<&str>,
) -> PyResult<String> {
    let path = brand_path.map(Path::new);
    let brand = resolve_brand(Path::new(brands_dir), brand_id, path).map_err(map_brand)?;
    serde_json::to_string(&brand).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "find_hits")]
fn find_hits_py(text: &str, forbidden: Vec<String>) -> Vec<String> {
    find_hits(text, &forbidden)
}

#[pyfunction]
#[pyo3(name = "check_text")]
fn check_text_py(text: &str, brand: Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let brand = pydantic_to::<Brand>(&brand)?;
    Ok(check_text(text, &brand))
}

#[pyfunction]
#[pyo3(name = "check_path")]
fn check_path_py(path: &str, brand: Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let brand = pydantic_to::<Brand>(&brand)?;
    Ok(check_path(Path::new(path), &brand))
}

#[pyfunction]
#[pyo3(name = "iter_scan_files")]
fn iter_scan_files_py(roots: Vec<String>) -> Vec<String> {
    let paths: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
    iter_scan_files(&paths)
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}

#[pyfunction]
#[pyo3(name = "scan_paths")]
fn scan_paths_py(
    roots: Vec<String>,
    brand: Bound<'_, PyAny>,
) -> PyResult<Vec<(String, Vec<String>)>> {
    let brand = pydantic_to::<Brand>(&brand)?;
    let paths: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
    Ok(scan_paths(&paths, &brand)
        .into_iter()
        .map(|(p, hits)| (p.display().to_string(), hits))
        .collect())
}

#[pyfunction]
#[pyo3(name = "system_prompt")]
fn system_prompt_py(req: Bound<'_, PyAny>) -> PyResult<String> {
    Ok(system_prompt(&extract_req(&req)?))
}

#[pyfunction]
#[pyo3(name = "user_prompt")]
fn user_prompt_py(req: Bound<'_, PyAny>, today: &str) -> PyResult<String> {
    user_prompt(&extract_req(&req)?, today).map_err(map_gen)
}

#[pyfunction]
#[pyo3(name = "length_rewrite_prompt")]
fn length_rewrite_prompt_py(
    req: Bound<'_, PyAny>,
    title: &str,
    body: &str,
    current_count: i64,
) -> PyResult<String> {
    Ok(length_rewrite_prompt(
        &extract_req(&req)?,
        title,
        body,
        current_count,
    ))
}

#[pyfunction]
#[pyo3(name = "parse_front_matter")]
fn parse_front_matter_py(py: Python<'_>, raw: &str) -> PyResult<(Py<PyDict>, String)> {
    let (fm, body) = parse_front_matter(raw);
    Ok((front_to_py(py, &fm)?, body))
}

#[pyfunction]
#[pyo3(name = "render_front_matter")]
fn render_front_matter_py(front: Bound<'_, PyAny>) -> PyResult<String> {
    Ok(render_front_matter(&front_from_py(&front)?))
}

#[pyfunction]
#[pyo3(name = "strip_disclaimer_from_body")]
fn strip_disclaimer_py(body: &str) -> String {
    strip_disclaimer_from_body(body)
}

#[pyfunction]
#[pyo3(name = "read_draft")]
fn read_draft_py(py: Python<'_>, path: &str) -> PyResult<Py<PyDict>> {
    let draft = read_draft(Path::new(path)).map_err(map_io)?;
    let dict = PyDict::new(py);
    dict.set_item(
        "path",
        draft
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| path.to_string()),
    )?;
    dict.set_item("title", draft.title)?;
    dict.set_item("body", draft.body)?;
    dict.set_item("front_matter", front_to_py(py, &draft.front_matter)?)?;
    if let Some(meta) = draft.meta {
        dict.set_item("meta", to_json_obj(py, &meta)?)?;
    } else {
        dict.set_item("meta", py.None())?;
    }
    Ok(dict.unbind())
}

#[pyfunction]
#[pyo3(name = "list_drafts")]
fn list_drafts_py(drafts_dir: &str) -> Vec<String> {
    list_drafts(Path::new(drafts_dir))
        .into_iter()
        .filter_map(|d| d.path.map(|p| p.display().to_string()))
        .collect()
}

#[pyfunction]
#[pyo3(name = "write_draft_content", signature = (path, front, body, disclaimer=None))]
fn write_draft_content_py(
    path: &str,
    front: Bound<'_, PyAny>,
    body: &str,
    disclaimer: Option<&str>,
) -> PyResult<()> {
    write_draft_content(Path::new(path), &front_from_py(&front)?, body, disclaimer).map_err(map_io)
}

#[pyfunction]
#[pyo3(name = "save_draft_body", signature = (path, body, disclaimer=None))]
fn save_draft_body_py(path: &str, body: &str, disclaimer: Option<&str>) -> PyResult<()> {
    save_draft_body(Path::new(path), body, disclaimer)
        .map(|_| ())
        .map_err(map_io)
}

#[pyfunction]
#[pyo3(name = "set_status")]
fn set_status_py(path: &str, status: &str) -> PyResult<()> {
    set_status(Path::new(path), status).map_err(map_io)
}

#[pyfunction]
#[pyo3(name = "unique_draft_path")]
fn unique_draft_path_py(drafts_dir: &str, stem: &str) -> PyResult<String> {
    unique_draft_path(Path::new(drafts_dir), stem)
        .map(|p| p.display().to_string())
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::InvalidInput {
                PyValueError::new_err(err.to_string())
            } else {
                map_io(err)
            }
        })
}

#[pyfunction]
#[pyo3(
    name = "write_new_draft",
    signature = (drafts_dir, title, body, brand_id, brand_display, type_id, fmt, meta, topic=None, disclaimer=None)
)]
#[allow(clippy::too_many_arguments)]
fn write_new_draft_py(
    drafts_dir: &str,
    title: &str,
    body: &str,
    brand_id: &str,
    brand_display: &str,
    type_id: &str,
    fmt: &str,
    meta: Bound<'_, PyAny>,
    topic: Option<&str>,
    disclaimer: Option<&str>,
) -> PyResult<String> {
    let format = match fmt {
        "article" => ContentFormat::Article,
        "social" => ContentFormat::Social,
        other => return Err(PyValueError::new_err(format!("unknown format {other}"))),
    };
    let meta = extract_meta(&meta)?;
    write_new_draft(WriteNewDraft {
        drafts_dir: Path::new(drafts_dir),
        title,
        body,
        brand_id,
        brand_display,
        type_id,
        format,
        topic,
        disclaimer,
        meta: &meta,
    })
    .map(|p| p.display().to_string())
    .map_err(map_io)
}

#[pyfunction]
#[pyo3(name = "finalize_draft", signature = (draft_path, final_dir, brand=None, copy=false))]
fn finalize_draft_py(
    draft_path: &str,
    final_dir: &str,
    brand: Option<Bound<'_, PyAny>>,
    copy: bool,
) -> PyResult<String> {
    let brand = match brand {
        Some(b) if !b.is_none() => Some(pydantic_to::<Brand>(&b)?),
        _ => None,
    };
    finalize_draft(
        Path::new(draft_path),
        Path::new(final_dir),
        brand.as_ref(),
        copy,
    )
    .map(|p| p.display().to_string())
    .map_err(map_fin)
}

#[pyfunction]
#[pyo3(name = "list_final")]
fn list_final_py(final_dir: &str) -> Vec<String> {
    list_final(Path::new(final_dir))
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}

#[pyfunction]
#[pyo3(name = "generate_content", signature = (req, cfg, client=None))]
fn generate_content_py(
    py: Python<'_>,
    req: Bound<'_, PyAny>,
    cfg: Bound<'_, PyAny>,
    client: Option<Bound<'_, PyAny>>,
) -> PyResult<(String, String, Py<PyAny>)> {
    let req = extract_req(&req)?;
    let cfg = extract_cfg(&cfg)?;
    let (title, body, meta) = match client {
        Some(c) if !c.is_none() => {
            let adapter = PyLlmClient { inner: c.unbind() };
            generate_content(&req, &cfg, Some(&adapter)).map_err(map_gen)?
        }
        _ => py
            .detach(|| generate_content(&req, &cfg, None))
            .map_err(map_gen)?,
    };
    Ok((title, body, to_json_obj(py, &meta)?))
}

#[pyfunction]
#[pyo3(name = "generate_and_write", signature = (req, cfg, drafts_dir=None, client=None))]
fn generate_and_write_py(
    py: Python<'_>,
    req: Bound<'_, PyAny>,
    cfg: Bound<'_, PyAny>,
    drafts_dir: Option<&str>,
    client: Option<Bound<'_, PyAny>>,
) -> PyResult<String> {
    let req = extract_req(&req)?;
    let cfg = extract_cfg(&cfg)?;
    let drafts = drafts_dir.map(Path::new);
    let path = match client {
        Some(c) if !c.is_none() => {
            let adapter = PyLlmClient { inner: c.unbind() };
            generate_and_write(&req, &cfg, drafts, Some(&adapter)).map_err(map_gen)?
        }
        _ => py
            .detach(|| generate_and_write(&req, &cfg, drafts, None))
            .map_err(map_gen)?,
    };
    Ok(path.display().to_string())
}

#[pyfunction]
#[pyo3(name = "make_client", signature = (_api_key, _base_url=None))]
fn make_client_py(_api_key: &str, _base_url: Option<&str>) -> PyResult<()> {
    Err(PyRuntimeError::new_err(
        "make_client is unused on the Rust path; pass client= to generate_and_write",
    ))
}

#[pyfunction]
#[pyo3(name = "response_text")]
fn response_text_py(response: Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(text) = response.getattr("output_text") {
        if let Ok(s) = text.extract::<String>() {
            if !s.trim().is_empty() {
                return Ok(s);
            }
        }
    }
    Ok(String::new())
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("BrandError", m.py().get_type::<BrandErrorPy>())?;
    m.add("GenerateError", m.py().get_type::<GenerateErrorPy>())?;
    m.add("FinalizeError", m.py().get_type::<FinalizeErrorPy>())?;
    m.add_function(wrap_pyfunction!(word_count_py, m)?)?;
    m.add_function(wrap_pyfunction!(char_count_py, m)?)?;
    m.add_function(wrap_pyfunction!(slugify_py, m)?)?;
    m.add_function(wrap_pyfunction!(clean_body_py, m)?)?;
    m.add_function(wrap_pyfunction!(is_plausible_title_py, m)?)?;
    m.add_function(wrap_pyfunction!(extract_title_body_py, m)?)?;
    m.add_function(wrap_pyfunction!(extract_social_text_py, m)?)?;
    m.add_function(wrap_pyfunction!(load_brand_file_py, m)?)?;
    m.add_function(wrap_pyfunction!(list_brand_files_py, m)?)?;
    m.add_function(wrap_pyfunction!(list_brands_py, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_brand_py, m)?)?;
    m.add_function(wrap_pyfunction!(find_hits_py, m)?)?;
    m.add_function(wrap_pyfunction!(check_text_py, m)?)?;
    m.add_function(wrap_pyfunction!(check_path_py, m)?)?;
    m.add_function(wrap_pyfunction!(iter_scan_files_py, m)?)?;
    m.add_function(wrap_pyfunction!(scan_paths_py, m)?)?;
    m.add_function(wrap_pyfunction!(system_prompt_py, m)?)?;
    m.add_function(wrap_pyfunction!(user_prompt_py, m)?)?;
    m.add_function(wrap_pyfunction!(length_rewrite_prompt_py, m)?)?;
    m.add_function(wrap_pyfunction!(parse_front_matter_py, m)?)?;
    m.add_function(wrap_pyfunction!(render_front_matter_py, m)?)?;
    m.add_function(wrap_pyfunction!(strip_disclaimer_py, m)?)?;
    m.add_function(wrap_pyfunction!(read_draft_py, m)?)?;
    m.add_function(wrap_pyfunction!(list_drafts_py, m)?)?;
    m.add_function(wrap_pyfunction!(write_draft_content_py, m)?)?;
    m.add_function(wrap_pyfunction!(save_draft_body_py, m)?)?;
    m.add_function(wrap_pyfunction!(set_status_py, m)?)?;
    m.add_function(wrap_pyfunction!(unique_draft_path_py, m)?)?;
    m.add_function(wrap_pyfunction!(write_new_draft_py, m)?)?;
    m.add_function(wrap_pyfunction!(finalize_draft_py, m)?)?;
    m.add_function(wrap_pyfunction!(list_final_py, m)?)?;
    m.add_function(wrap_pyfunction!(generate_content_py, m)?)?;
    m.add_function(wrap_pyfunction!(generate_and_write_py, m)?)?;
    m.add_function(wrap_pyfunction!(make_client_py, m)?)?;
    m.add_function(wrap_pyfunction!(response_text_py, m)?)?;
    Ok(())
}
