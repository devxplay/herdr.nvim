use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SOURCE_PRESENCE: &str = "herdr.nvim:presence";
const SOURCE_FOCUS: &str = "herdr.nvim:navigator";
const AGENT_PRESENCE: &str = "herdr.nvim";

#[derive(Clone, Copy, Deserialize, Serialize)]
struct Rect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

#[derive(Default, Deserialize, Serialize)]
struct LayoutCache {
    #[serde(default)]
    tabs: HashMap<String, CachedTab>,
    #[serde(default)]
    internal_aliases: HashMap<String, String>,
    #[serde(default)]
    max_internal: i64,
    #[serde(default)]
    nvim_panes: HashMap<String, u128>,
}

#[derive(Deserialize, Serialize)]
struct CachedTab {
    panes: HashMap<String, Rect>,
    updated_ns: u128,
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}

fn socket_path() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("HERDR_SOCKET_PATH") {
        return Ok(PathBuf::from(path));
    }

    let home = home_dir()?;
    if let Some(session) = env::var_os("HERDR_SESSION") {
        return Ok(home
            .join(".config")
            .join("herdr")
            .join("sessions")
            .join(session)
            .join("herdr.sock"));
    }

    Ok(home.join(".config").join("herdr").join("herdr.sock"))
}

fn cache_path() -> Result<PathBuf, String> {
    let base = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".cache"));
    Ok(base.join("herdr.nvim").join("layout-cache.json"))
}

fn load_cache() -> LayoutCache {
    let Ok(path) = cache_path() else {
        return LayoutCache::default();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return LayoutCache::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_cache(cache: &LayoutCache) -> Result<(), String> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let content = serde_json::to_string(cache).map_err(|err| err.to_string())?;
    fs::write(&path, content).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn request(method: &str, params: Value) -> Result<Value, String> {
    let path = socket_path()?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|err| format!("failed to connect to {}: {err}", path.display()))?;
    let timeout = Some(Duration::from_secs(1));
    stream
        .set_read_timeout(timeout)
        .map_err(|err| format!("failed to set socket read timeout: {err}"))?;
    stream
        .set_write_timeout(timeout)
        .map_err(|err| format!("failed to set socket write timeout: {err}"))?;

    let payload = json!({
        "id": format!("herdr.nvim:{}", now_ns()),
        "method": method,
        "params": params,
    });
    let mut line = serde_json::to_vec(&payload).map_err(|err| err.to_string())?;
    line.push(b'\n');
    stream
        .write_all(&line)
        .map_err(|err| format!("failed to write Herdr request: {err}"))?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .map_err(|err| format!("failed to read Herdr response: {err}"))?;
    if response.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&response).map_err(|err| format!("invalid Herdr response: {err}"))
}

fn result(method: &str, params: Value) -> Result<Value, String> {
    let response = request(method, params)?;
    if let Some(error) = response.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Herdr request failed");
        return Err(message.to_string());
    }
    Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
}

fn active_pane_id() -> Result<Option<String>, String> {
    for name in ["HERDR_ACTIVE_PANE_ID", "HERDR_PANE_ID"] {
        if let Some(value) = env::var_os(name) {
            let value = value.to_string_lossy().to_string();
            if !value.is_empty() {
                return Ok(Some(value));
            }
        }
    }

    let panes = result("pane.list", json!({}))?;
    let panes = panes
        .get("panes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Herdr pane.list returned no panes array".to_string())?;
    Ok(panes.iter().find_map(|pane| {
        pane.get("focused")
            .and_then(Value::as_bool)
            .filter(|focused| *focused)
            .and_then(|_| pane.get("pane_id").and_then(Value::as_str))
            .map(str::to_string)
    }))
}

fn active_workspace_and_tab() -> Result<(Value, Value), String> {
    let session_file = socket_path()?
        .parent()
        .ok_or_else(|| "Herdr socket path has no parent".to_string())?
        .join("session.json");
    let content = fs::read_to_string(&session_file)
        .map_err(|err| format!("failed to read {}: {err}", session_file.display()))?;
    let data: Value = serde_json::from_str(&content)
        .map_err(|err| format!("failed to parse {}: {err}", session_file.display()))?;

    let workspaces = data
        .get("workspaces")
        .and_then(Value::as_array)
        .ok_or_else(|| "session.json has no workspaces array".to_string())?;
    let workspace_index = data.get("active").and_then(Value::as_u64).unwrap_or(0) as usize;
    let workspace = workspaces
        .get(workspace_index)
        .cloned()
        .ok_or_else(|| "active Herdr workspace not found".to_string())?;

    let tabs = workspace
        .get("tabs")
        .and_then(Value::as_array)
        .ok_or_else(|| "active Herdr workspace has no tabs array".to_string())?;
    let tab_index = workspace
        .get("active_tab")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let tab = tabs
        .get(tab_index)
        .cloned()
        .ok_or_else(|| "active Herdr tab not found".to_string())?;

    Ok((workspace, tab))
}

fn pane_public_mapping(workspace: &Value, tab: &Value) -> Result<BTreeMap<i64, String>, String> {
    let workspace_id = workspace
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "active Herdr workspace has no id".to_string())?;
    let panes = tab
        .get("panes")
        .and_then(Value::as_object)
        .ok_or_else(|| "active Herdr tab has no panes object".to_string())?;

    let mut internal_ids = panes
        .keys()
        .filter_map(|pane_id| pane_id.parse::<i64>().ok())
        .collect::<Vec<_>>();
    internal_ids.sort_unstable();

    Ok(internal_ids
        .into_iter()
        .enumerate()
        .map(|(index, internal_id)| (internal_id, format!("{workspace_id}-{}", index + 1)))
        .collect())
}

fn record_session_aliases(cache: &mut LayoutCache, mapping: &BTreeMap<i64, String>) {
    for (internal, public) in mapping {
        cache.max_internal = cache.max_internal.max(*internal);
        cache
            .internal_aliases
            .insert(format!("p_{internal}"), public.clone());
    }
}

fn internal_id_from_pane_id(pane_id: &str, mapping: &BTreeMap<i64, String>) -> Option<i64> {
    for (internal, public) in mapping {
        if public == pane_id {
            return Some(*internal);
        }
    }

    let raw = pane_id.strip_prefix("p_").unwrap_or(pane_id);
    let internal = raw.parse::<i64>().ok()?;
    mapping.contains_key(&internal).then_some(internal)
}

fn public_id_from_cache(pane_id: &str, cache: &LayoutCache) -> String {
    if pane_id.starts_with("p_") {
        cache
            .internal_aliases
            .get(pane_id)
            .cloned()
            .unwrap_or_else(|| pane_id.to_string())
    } else {
        pane_id.to_string()
    }
}

fn resolve_public_pane_id(
    pane_id: &str,
    cache: &mut LayoutCache,
) -> Result<Option<String>, String> {
    if !pane_id.starts_with("p_") {
        return Ok(Some(pane_id.to_string()));
    }

    if let Some(public) = cache.internal_aliases.get(pane_id) {
        return Ok(Some(public.clone()));
    }

    let _ = seed_active_tab_cache(cache)?;
    Ok(cache.internal_aliases.get(pane_id).cloned())
}

fn pane_from_node(node: &Value) -> Option<i64> {
    node.get("Pane").and_then(Value::as_i64)
}

fn collect_rects(node: &Value, rect: Rect, out: &mut HashMap<i64, Rect>) {
    if let Some(pane) = pane_from_node(node) {
        out.insert(pane, rect);
        return;
    }

    let Some(split) = node.get("Split").and_then(Value::as_object) else {
        return;
    };
    let ratio = split.get("ratio").and_then(Value::as_f64).unwrap_or(0.5);
    let Some(first) = split.get("first") else {
        return;
    };
    let Some(second) = split.get("second") else {
        return;
    };

    if split.get("direction").and_then(Value::as_str) == Some("Horizontal") {
        let xm = rect.x0 + (rect.x1 - rect.x0) * ratio;
        collect_rects(
            first,
            Rect {
                x0: rect.x0,
                y0: rect.y0,
                x1: xm,
                y1: rect.y1,
            },
            out,
        );
        collect_rects(
            second,
            Rect {
                x0: xm,
                y0: rect.y0,
                x1: rect.x1,
                y1: rect.y1,
            },
            out,
        );
    } else {
        let ym = rect.y0 + (rect.y1 - rect.y0) * ratio;
        collect_rects(
            first,
            Rect {
                x0: rect.x0,
                y0: rect.y0,
                x1: rect.x1,
                y1: ym,
            },
            out,
        );
        collect_rects(
            second,
            Rect {
                x0: rect.x0,
                y0: ym,
                x1: rect.x1,
                y1: rect.y1,
            },
            out,
        );
    }
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    f64::max(0.0, f64::min(a1, b1) - f64::max(a0, b0))
}

fn adjacent_internal(direction: &str, current: i64, rects: &HashMap<i64, Rect>) -> Option<i64> {
    let current_rect = rects.get(&current)?;
    let mut candidates: Vec<(f64, f64, i64)> = Vec::new();
    let epsilon = 0.000001;

    for (pane, rect) in rects {
        if *pane == current {
            continue;
        }

        match direction {
            "left" if rect.x1 <= current_rect.x0 + epsilon => candidates.push((
                current_rect.x0 - rect.x1,
                -overlap(current_rect.y0, current_rect.y1, rect.y0, rect.y1),
                *pane,
            )),
            "right" if rect.x0 >= current_rect.x1 - epsilon => candidates.push((
                rect.x0 - current_rect.x1,
                -overlap(current_rect.y0, current_rect.y1, rect.y0, rect.y1),
                *pane,
            )),
            "up" if rect.y1 <= current_rect.y0 + epsilon => candidates.push((
                current_rect.y0 - rect.y1,
                -overlap(current_rect.x0, current_rect.x1, rect.x0, rect.x1),
                *pane,
            )),
            "down" if rect.y0 >= current_rect.y1 - epsilon => candidates.push((
                rect.y0 - current_rect.y1,
                -overlap(current_rect.x0, current_rect.x1, rect.x0, rect.x1),
                *pane,
            )),
            _ => {}
        }
    }

    candidates.retain(|candidate| candidate.1 < 0.0);
    candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    candidates.first().map(|candidate| candidate.2)
}

fn adjacent_public(
    direction: &str,
    current: &str,
    rects: &HashMap<String, Rect>,
) -> Option<String> {
    let current_rect = rects.get(current)?;
    let mut candidates: Vec<(f64, f64, String)> = Vec::new();
    let epsilon = 0.000001;

    for (pane, rect) in rects {
        if pane == current {
            continue;
        }

        match direction {
            "left" if rect.x1 <= current_rect.x0 + epsilon => candidates.push((
                current_rect.x0 - rect.x1,
                -overlap(current_rect.y0, current_rect.y1, rect.y0, rect.y1),
                pane.clone(),
            )),
            "right" if rect.x0 >= current_rect.x1 - epsilon => candidates.push((
                rect.x0 - current_rect.x1,
                -overlap(current_rect.y0, current_rect.y1, rect.y0, rect.y1),
                pane.clone(),
            )),
            "up" if rect.y1 <= current_rect.y0 + epsilon => candidates.push((
                current_rect.y0 - rect.y1,
                -overlap(current_rect.x0, current_rect.x1, rect.x0, rect.x1),
                pane.clone(),
            )),
            "down" if rect.y0 >= current_rect.y1 - epsilon => candidates.push((
                rect.y0 - current_rect.y1,
                -overlap(current_rect.x0, current_rect.x1, rect.x0, rect.x1),
                pane.clone(),
            )),
            _ => {}
        }
    }

    candidates.retain(|candidate| candidate.1 < 0.0);
    candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    candidates.first().map(|candidate| candidate.2.clone())
}

fn focus_pane(pane_id: &str) -> Result<(), String> {
    let seq = now_ns();
    let normalized = pane_id.replace(['-', ':'], "_");
    let label = format!("herdr.nvim.focus.{normalized}.{seq}");

    result(
        "pane.report_agent",
        json!({
            "pane_id": pane_id,
            "source": SOURCE_FOCUS,
            "agent": label,
            "state": "idle",
            "seq": seq,
        }),
    )?;
    result("agent.focus", json!({ "target": label }))?;
    result(
        "pane.release_agent",
        json!({
            "pane_id": pane_id,
            "source": SOURCE_FOCUS,
            "agent": label,
            "seq": seq + 1,
        }),
    )?;

    Ok(())
}

fn pane_list() -> Result<Vec<Value>, String> {
    let panes = result("pane.list", json!({}))?;
    Ok(panes
        .get("panes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn tab_key_for_public_pane(public_pane_id: &str, panes: &[Value]) -> Option<String> {
    panes
        .iter()
        .find(|pane| pane.get("pane_id").and_then(Value::as_str) == Some(public_pane_id))
        .and_then(|pane| pane.get("tab_id").and_then(Value::as_str))
        .map(str::to_string)
}

fn live_tab_pane_ids(tab_id: &str, panes: &[Value]) -> Vec<String> {
    let mut ids = panes
        .iter()
        .filter(|pane| pane.get("tab_id").and_then(Value::as_str) == Some(tab_id))
        .filter_map(|pane| pane.get("pane_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn cache_tab_is_live(cached: &CachedTab, tab_id: &str, panes: &[Value]) -> bool {
    let mut cached_ids = cached.panes.keys().cloned().collect::<Vec<_>>();
    cached_ids.sort();
    cached_ids == live_tab_pane_ids(tab_id, panes)
}

fn cached_adjacent(direction: &str, current_pane_id: &str) -> Result<Option<String>, String> {
    let cache = load_cache();
    let current_public = public_id_from_cache(current_pane_id, &cache);
    if current_public.starts_with("p_") {
        return Ok(None);
    }

    let panes = pane_list()?;
    let Some(tab_id) = tab_key_for_public_pane(&current_public, &panes) else {
        return Ok(None);
    };
    let Some(cached) = cache.tabs.get(&tab_id) else {
        return Ok(None);
    };
    if !cache_tab_is_live(cached, &tab_id, &panes) {
        return Ok(None);
    }

    Ok(adjacent_public(direction, &current_public, &cached.panes))
}

fn seed_active_tab_cache(cache: &mut LayoutCache) -> Result<Option<String>, String> {
    let (workspace, tab) = active_workspace_and_tab()?;
    let tab_id = env::var("HERDR_ACTIVE_TAB_ID")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            tab.get("tab_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    let workspace_id = workspace.get("id").and_then(Value::as_str)?;
                    let tab_number = workspace
                        .get("active_tab")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        + 1;
                    Some(format!("{workspace_id}:{tab_number}"))
                })
        })
        .ok_or_else(|| "active Herdr tab has no id".to_string())?;
    let mapping = pane_public_mapping(&workspace, &tab)?;
    record_session_aliases(cache, &mapping);

    let mut internal_rects = HashMap::new();
    collect_rects(
        tab.get("layout").unwrap_or(&Value::Null),
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        &mut internal_rects,
    );

    let mut panes = HashMap::new();
    for (internal, rect) in internal_rects {
        if let Some(public) = mapping.get(&internal) {
            panes.insert(public.clone(), rect);
        }
    }

    if panes.is_empty() {
        return Ok(None);
    }

    cache.tabs.insert(
        tab_id.clone(),
        CachedTab {
            panes,
            updated_ns: now_ns(),
        },
    );
    Ok(Some(tab_id))
}

fn focus_adjacent(direction: &str, current_pane_id: Option<String>) -> Result<i32, String> {
    let Some(current_pane_id) = current_pane_id.or(active_pane_id()?) else {
        return Ok(1);
    };

    if let Some(target_public) = cached_adjacent(direction, &current_pane_id)? {
        focus_pane(&target_public)?;
        return Ok(0);
    }

    let (workspace, tab) = active_workspace_and_tab()?;
    let mapping = pane_public_mapping(&workspace, &tab)?;
    let Some(current_internal) = internal_id_from_pane_id(&current_pane_id, &mapping) else {
        return Ok(1);
    };

    let mut rects = HashMap::new();
    collect_rects(
        tab.get("layout").unwrap_or(&Value::Null),
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        &mut rects,
    );
    let Some(target_internal) = adjacent_internal(direction, current_internal, &rects) else {
        return Ok(1);
    };
    let Some(target_public) = mapping.get(&target_internal) else {
        return Ok(1);
    };

    focus_pane(target_public)?;
    Ok(0)
}

fn prune_nvim_panes(cache: &mut LayoutCache, live_public_panes: &HashSet<String>) -> bool {
    let before = cache.nvim_panes.len();
    cache
        .nvim_panes
        .retain(|pane_id, _| live_public_panes.contains(pane_id));
    cache.nvim_panes.len() != before
}

fn pane_is_registered_nvim(pane_id: &str) -> Result<bool, String> {
    let mut cache = load_cache();
    let Some(public_pane_id) = resolve_public_pane_id(pane_id, &mut cache)? else {
        return Ok(false);
    };

    let panes = pane_list()?;
    let live_public_panes = panes
        .iter()
        .filter_map(|pane| pane.get("pane_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    if prune_nvim_panes(&mut cache, &live_public_panes) {
        save_cache(&cache)?;
    }

    Ok(cache.nvim_panes.contains_key(&public_pane_id))
}

fn ctrl_text(direction: &str) -> Option<&'static str> {
    match direction {
        "left" => Some("\u{0008}"),
        "down" => Some("\u{000a}"),
        "up" => Some("\u{000b}"),
        "right" => Some("\u{000c}"),
        _ => None,
    }
}

fn send_ctrl_to_pane(pane_id: &str, direction: &str) -> Result<(), String> {
    let Some(text) = ctrl_text(direction) else {
        return Err(format!("unsupported direction {direction}"));
    };
    result(
        "pane.send_text",
        json!({
            "pane_id": pane_id,
            "text": text,
        }),
    )?;
    Ok(())
}

fn split_rect(rect: Rect, direction: &str) -> (Rect, Rect) {
    if direction == "right" {
        let xm = rect.x0 + (rect.x1 - rect.x0) * 0.5;
        (
            Rect {
                x0: rect.x0,
                y0: rect.y0,
                x1: xm,
                y1: rect.y1,
            },
            Rect {
                x0: xm,
                y0: rect.y0,
                x1: rect.x1,
                y1: rect.y1,
            },
        )
    } else {
        let ym = rect.y0 + (rect.y1 - rect.y0) * 0.5;
        (
            Rect {
                x0: rect.x0,
                y0: rect.y0,
                x1: rect.x1,
                y1: ym,
            },
            Rect {
                x0: rect.x0,
                y0: ym,
                x1: rect.x1,
                y1: rect.y1,
            },
        )
    }
}

fn split_pane(direction: &str) -> Result<i32, String> {
    let Some(target_pane_id) = active_pane_id()? else {
        return Ok(1);
    };
    if target_pane_id.starts_with("p_") {
        return Err("split requires Herdr's active public pane id".to_string());
    }

    let mut cache = load_cache();
    let tab_id = env::var("HERDR_ACTIVE_TAB_ID")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| seed_active_tab_cache(&mut cache).ok().flatten())
        .ok_or_else(|| "active Herdr tab id is unavailable".to_string())?;
    if !cache.tabs.contains_key(&tab_id) {
        let _ = seed_active_tab_cache(&mut cache)?;
    }

    let before_max_internal = cache.max_internal;
    let cwd = env::var("HERDR_ACTIVE_PANE_CWD")
        .ok()
        .filter(|value| !value.is_empty());
    let split_result = result(
        "pane.split",
        json!({
            "target_pane_id": target_pane_id,
            "direction": direction,
            "cwd": cwd,
            "focus": true,
        }),
    )?;
    let Some(new_public) = split_result
        .get("pane")
        .and_then(|pane| pane.get("pane_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(1);
    };

    let new_internal = before_max_internal + 1;
    cache.max_internal = cache.max_internal.max(new_internal);
    cache
        .internal_aliases
        .insert(format!("p_{new_internal}"), new_public.clone());

    if let Some(tab) = cache.tabs.get_mut(&tab_id) {
        if let Some(old_rect) = tab.panes.get(&target_pane_id).copied() {
            let (first, second) = split_rect(old_rect, direction);
            tab.panes.insert(target_pane_id, first);
            tab.panes.insert(new_public, second);
            tab.updated_ns = now_ns();
        }
    }
    save_cache(&cache)?;

    Ok(0)
}

fn register() -> Result<i32, String> {
    let Some(pane_id) = active_pane_id()? else {
        return Ok(1);
    };
    let mut cache = load_cache();
    let Some(public_pane_id) = resolve_public_pane_id(&pane_id, &mut cache)? else {
        return Ok(1);
    };
    cache.nvim_panes.insert(public_pane_id, now_ns());
    save_cache(&cache)?;
    Ok(0)
}

fn release() -> Result<i32, String> {
    let Some(pane_id) = active_pane_id()? else {
        return Ok(1);
    };
    let mut cache = load_cache();
    let Some(public_pane_id) = resolve_public_pane_id(&pane_id, &mut cache)? else {
        return Ok(1);
    };
    cache.nvim_panes.remove(&public_pane_id);
    save_cache(&cache)?;
    Ok(0)
}

fn cleanup_agents() -> Result<i32, String> {
    let agents = result("agent.list", json!({}))?;
    let Some(agents) = agents.get("agents").and_then(Value::as_array) else {
        return Ok(0);
    };

    let mut cache = load_cache();
    let mut changed = false;
    let seq = now_ns();
    for (index, agent) in agents.iter().enumerate() {
        if agent.get("agent").and_then(Value::as_str) != Some(AGENT_PRESENCE) {
            continue;
        }
        let Some(pane_id) = agent.get("pane_id").and_then(Value::as_str) else {
            continue;
        };

        cache.nvim_panes.insert(pane_id.to_string(), now_ns());
        changed = true;
        result(
            "pane.release_agent",
            json!({
                "pane_id": pane_id,
                "source": SOURCE_PRESENCE,
                "agent": AGENT_PRESENCE,
                "seq": seq + index as u128,
            }),
        )?;
    }

    if changed {
        save_cache(&cache)?;
    }
    Ok(0)
}

fn dispatch(direction: &str) -> Result<i32, String> {
    let Some(pane_id) = active_pane_id()? else {
        return Ok(1);
    };

    if pane_is_registered_nvim(&pane_id)? {
        send_ctrl_to_pane(&pane_id, direction)?;
        return Ok(0);
    }

    focus_adjacent(direction, Some(pane_id))
}

fn usage() {
    eprintln!(
        "usage: herdr-navigator dispatch|focus|register|release|split|cleanup-agents [left|down|up|right]"
    );
}

fn run() -> Result<i32, String> {
    let args = env::args().collect::<Vec<_>>();
    let Some(command) = args.get(1).map(String::as_str) else {
        usage();
        return Ok(2);
    };
    let direction = args.get(2).map(String::as_str);

    match (command, direction) {
        ("register", _) => register(),
        ("release", _) => release(),
        ("cleanup-agents", _) => cleanup_agents(),
        ("focus", Some(direction)) if ctrl_text(direction).is_some() => {
            focus_adjacent(direction, None)
        }
        ("split", Some(direction)) if matches!(direction, "right" | "down") => {
            split_pane(direction)
        }
        ("dispatch", Some(direction)) if ctrl_text(direction).is_some() => dispatch(direction),
        _ => {
            usage();
            Ok(2)
        }
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("herdr-navigator: {err}");
            std::process::exit(1);
        }
    }
}
