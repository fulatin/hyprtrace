//! Tool definitions and executor for AI agent mode.
//!
//! Exposes read-only Hyprland query APIs (via hyprland-rs) and HyprTrace
//! usage-data queries as function-calling tools for the LLM.

use crate::db::Database;
use serde_json::{json, Value};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

impl ToolDef {
    /// Serialize into the OpenAI/Ollama function-tool wire format.
    pub fn to_api_json(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

fn empty_params() -> Value {
    json!({"type": "object", "properties": {}, "required": [], "additionalProperties": false})
}

fn obj_params(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

/// All available tools: live Hyprland state queries + usage DB queries.
pub fn all_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_app_categories",
            description: "List the app categorization rules (pattern -> category) used to classify apps into development/browsing/gaming/etc.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "workspace_recommendations",
            description: "Analyze the last 14 days of sessions and recommend which workspace each app should be assigned to (where the user spends the most time per app).",
            parameters: empty_params(),
        },
        // ---- Write/action tools (mutate goals, fire desktop notifications) ----
        ToolDef {
            name: "get_goals",
            description: "List the user's daily usage goals (name, scope, target hours, enabled) with today's progress.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "set_goal",
            description: "Create or update a daily usage goal. This MERGES: a goal is matched by its scope (target_type + target_key) and updated in place, unknown scopes are added, and goals you do not mention are left untouched. To remove one, call delete_goal. To wipe every goal and start over, pass replace_all=true — only do that when the user explicitly asks to clear all of their goals. target_type is 'all' or 'class'; target_key is the app class when target_type is 'class'; daily_target_ms is milliseconds.",
            parameters: obj_params(
                json!({
                    "goals": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "target_type": {"type": "string", "enum": ["all", "class"]},
                                "target_key": {"type": "string"},
                                "daily_target_ms": {"type": "integer"},
                                "enabled": {"type": "boolean"}
                            },
                            "required": ["name", "target_type", "daily_target_ms"]
                        }
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Delete every existing goal before applying this list. Requires the user to have explicitly asked for it. Default false."
                    }
                }),
                &["goals"],
            ),
        },
        ToolDef {
            name: "delete_goal",
            description: "Delete one daily usage goal, matched by target_type plus target_key ('class' goals) or by target_type 'all'. Deleting goals is only ever done through this tool, never as a side effect of setting another goal.",
            parameters: obj_params(
                json!({
                    "target_type": {"type": "string", "enum": ["all", "class"]},
                    "target_key": {"type": "string", "description": "App class; omit for target_type 'all'"}
                }),
                &["target_type"],
            ),
        },
        ToolDef {
            name: "send_reminder",
            description: "Send a desktop notification to the user with a title and message (e.g. a productivity tip or caution).",
            parameters: obj_params(
                json!({
                    "title": {"type": "string"},
                    "message": {"type": "string"}
                }),
                &["title", "message"],
            ),
        },
        // ---- Hyprland live state (read-only) ----
        ToolDef {
            name: "active_window",
            description: "Get the currently focused window (class, title, workspace, pid). Returns null fields if no window is focused.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "clients",
            description: "List all open windows across workspaces with class, title, workspace, geometry and flags (up to 50).",
            parameters: empty_params(),
        },
        ToolDef {
            name: "active_workspace",
            description: "Get the currently active workspace (id, name, monitor, window count).",
            parameters: empty_params(),
        },
        ToolDef {
            name: "workspaces",
            description: "List all workspaces with id, name, monitor and window count.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "active_monitor",
            description: "Get the currently focused monitor (name, resolution, refresh rate, active workspace).",
            parameters: empty_params(),
        },
        ToolDef {
            name: "monitors",
            description: "List all monitors (including disabled) with resolution, position, scale and active workspace.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "layers",
            description: "List layer-shell surfaces (bars, launchers, wallpapers, notifications) per monitor with namespace and size.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "devices",
            description: "List input devices: mice, keyboards (with active keymap/layout) and tablets.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "version",
            description: "Get the running Hyprland version, branch, commit, tag and build flags.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "cursor_position",
            description: "Get the current cursor position (x, y) in layout coordinates.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "binds",
            description: "List configured keybinds (modifier mask, key, dispatcher, argument), up to 80.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "animations",
            description: "List Hyprland animation configurations (name, enabled, speed).",
            parameters: empty_params(),
        },
        ToolDef {
            name: "workspace_rules",
            description: "List configured workspace rules (workspace string, monitor, persistent, etc).",
            parameters: empty_params(),
        },
        ToolDef {
            name: "fullscreen_state",
            description: "Check whether the active workspace currently has a fullscreen window.",
            parameters: empty_params(),
        },
        ToolDef {
            name: "get_keyword",
            description: "Read a Hyprland config keyword value, e.g. 'general:gaps_in', 'decoration:rounding', 'animations:enabled'.",
            parameters: obj_params(
                json!({"key": {"type": "string", "description": "Config keyword, e.g. 'general:gaps_in'"}}),
                &["key"],
            ),
        },
        ToolDef {
            name: "list_plugins",
            description: "List loaded Hyprland plugins (name, author, version, description).",
            parameters: empty_params(),
        },
        ToolDef {
            name: "list_instances",
            description: "List running Hyprland instances (instance signature, pid, wl socket).",
            parameters: empty_params(),
        },
        // ---- HyprTrace usage data ----
        ToolDef {
            name: "get_today_summary",
            description: "Get usage summary for a date: total active/idle/focused time, app count, session count, top 5 apps.",
            parameters: obj_params(
                json!({"date": {"type": "string", "description": "YYYY-MM-DD, defaults to today"}}),
                &[],
            ),
        },
        ToolDef {
            name: "get_app_ranking",
            description: "Get app usage ranking for a date range with total time, percentage, session count and focused time.",
            parameters: obj_params(
                json!({
                    "from": {"type": "string", "description": "YYYY-MM-DD, defaults to today"},
                    "to": {"type": "string", "description": "YYYY-MM-DD, defaults to today"},
                    "limit": {"type": "integer", "description": "Max apps, default 10"}
                }),
                &[],
            ),
        },
        ToolDef {
            name: "get_sessions",
            description: "Get recent window sessions with app class, title, workspace, start/end time, duration, activity state and focus time.",
            parameters: obj_params(
                json!({
                    "from": {"type": "string", "description": "YYYY-MM-DD, defaults to today"},
                    "to": {"type": "string", "description": "YYYY-MM-DD, defaults to today"},
                    "limit": {"type": "integer", "description": "Max sessions, default 20, max 50"}
                }),
                &[],
            ),
        },
        ToolDef {
            name: "get_hourly_breakdown",
            description: "Get 24-hour activity breakdown for a date (per-hour active ms, session count, focused ms).",
            parameters: obj_params(
                json!({"date": {"type": "string", "description": "YYYY-MM-DD, defaults to today"}}),
                &[],
            ),
        },
        ToolDef {
            name: "get_app_trend",
            description: "Get daily usage trend for a specific app class over a date range.",
            parameters: obj_params(
                json!({
                    "class": {"type": "string", "description": "App class, e.g. 'firefox', 'code'"},
                    "from": {"type": "string", "description": "YYYY-MM-DD, defaults to 7 days ago"},
                    "to": {"type": "string", "description": "YYYY-MM-DD, defaults to today"}
                }),
                &["class"],
            ),
        },
    ]
}

/// LOCAL calendar day — must match the local-date buckets in the summary
/// tables (M3), not UTC.
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Ensure HYPRLAND_INSTANCE_SIGNATURE is set so hyprland-rs can find the IPC
/// socket. Running under a systemd user service, the variable isn't inherited
/// from the graphical session, so discover the instance via `hyprctl instances`.
fn ensure_hyprland_env() -> anyhow::Result<()> {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return Ok(());
    }
    let output = std::process::Command::new("hyprctl")
        .arg("instances")
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run hyprctl: {}", e))?;
    if !output.status.success() {
        anyhow::bail!("hyprctl instances failed (Is Hyprland running?)");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let signature = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("instance "))
        .map(|s| s.split(':').next().unwrap_or(s).trim().to_string())
        .ok_or_else(|| anyhow::anyhow!("No Hyprland instance found"))?;
    // Safety: setting the env var is process-wide but idempotent and harmless.
    unsafe {
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", &signature);
    }
    Ok(())
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

fn arg_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|v| v.as_i64())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{}…", t)
    }
}

/// Execute a tool by name. Hyprland queries are blocking IPC calls, so they
/// run on the blocking thread pool. DB queries lock the shared mutex briefly.
pub async fn execute_tool(
    name: &str,
    args: &Value,
    db: &tokio::sync::Mutex<Database>,
) -> anyhow::Result<Value> {
    match name {
        // ---- Usage DB tools (lock briefly, no await while holding) ----
        "get_today_summary" => {
            let date = arg_str(args, "date")
                .map(String::from)
                .unwrap_or_else(today);
            let s = db.lock().await.today_summary(&date)?;
            Ok(json!({
                "date": s.date,
                "total_active_ms": s.total_active_ms,
                "total_idle_ms": s.total_idle_ms,
                "total_focused_ms": s.total_focused_ms,
                "app_count": s.app_count,
                "session_count": s.session_count,
                "top_apps": s.top_apps,
            }))
        }
        "get_app_ranking" => {
            let from = arg_str(args, "from")
                .map(String::from)
                .unwrap_or_else(today);
            let to = arg_str(args, "to").map(String::from).unwrap_or_else(today);
            let limit = arg_i64(args, "limit").unwrap_or(10).clamp(1, 25) as usize;
            let r = db.lock().await.app_ranking(&from, &to, limit)?;
            Ok(serde_json::to_value(r)?)
        }
        "get_sessions" => {
            let from = arg_str(args, "from")
                .map(String::from)
                .unwrap_or_else(today);
            let to = arg_str(args, "to").map(String::from).unwrap_or_else(today);
            let limit = arg_i64(args, "limit").unwrap_or(20).clamp(1, 50) as u32;
            let (sessions, _total) = db
                .lock()
                .await
                .sessions_paginated(&from, &to, 1, limit, None)?;
            Ok(serde_json::to_value(sessions)?)
        }
        "get_hourly_breakdown" => {
            let date = arg_str(args, "date")
                .map(String::from)
                .unwrap_or_else(today);
            let r = db.lock().await.hourly_breakdown(&date)?;
            Ok(serde_json::to_value(r)?)
        }
        "get_app_categories" => {
            let rules = db.lock().await.categories()?;
            Ok(serde_json::to_value(rules)?)
        }
        "workspace_recommendations" => {
            let r = db.lock().await.workspace_recommendations(14)?;
            Ok(serde_json::to_value(r)?)
        }
        "get_goals" => {
            let db = db.lock().await;
            let progress = db.goal_progress()?;
            let payload: Vec<Value> = progress
                .iter()
                .map(|p| {
                    json!({
                        "name": p.goal.name,
                        "target_type": p.goal.target_type,
                        "target_key": p.goal.target_key,
                        "daily_target_ms": p.goal.daily_target_ms,
                        "enabled": p.goal.enabled,
                        "today_ms": p.today_ms,
                        "pct": p.pct,
                    })
                })
                .collect();
            Ok(json!(payload))
        }
        "set_goal" => {
            let goals = args
                .get("goals")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let parsed: Vec<crate::models::Goal> = goals
                .iter()
                .filter_map(|g| {
                    let name = g.get("name")?.as_str()?.to_string();
                    let target_type = g
                        .get("target_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("all")
                        .to_string();
                    let target_key = g
                        .get("target_key")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let daily_target_ms = g
                        .get("daily_target_ms")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    Some(crate::models::Goal {
                        id: None,
                        name,
                        target_type,
                        target_key,
                        daily_target_ms,
                        enabled: g.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                    })
                })
                .collect();
            // An empty list is almost always a parse failure on the model's
            // side. Under the old replace-all semantics that would have wiped
            // the user's goals, so refuse it unless the caller explicitly opted
            // into wiping (an empty list + replace_all is an unambiguous
            // "clear everything" request).
            let replace_all = args
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if parsed.is_empty() && !replace_all {
                anyhow::bail!("set_goal requires at least one goal with a name");
            }
            db.lock().await.set_goals(&parsed, replace_all)?;
            Ok(json!({
                "status": "ok",
                "goals_set": parsed.len(),
                "replace_all": replace_all,
            }))
        }
        "delete_goal" => {
            let target_type = arg_str(args, "target_type")
                .ok_or_else(|| anyhow::anyhow!("missing required argument: target_type"))?;
            let target_key = arg_str(args, "target_key");
            if target_type == "class" && target_key.is_none() {
                anyhow::bail!("delete_goal requires target_key when target_type is 'class'");
            }
            let removed = db
                .lock()
                .await
                .delete_goal(None, Some(target_type), target_key)?;
            if removed == 0 {
                return Ok(json!({
                    "status": "not_found",
                    "removed": 0,
                    "message": "No goal matches that scope; call get_goals to see the current goals.",
                }));
            }
            Ok(json!({"status": "ok", "removed": removed}))
        }
        "send_reminder" => {
            let title = arg_str(args, "title").unwrap_or("HyprTrace");
            let message = arg_str(args, "message").unwrap_or("");
            let spawned = std::process::Command::new("notify-send")
                .args(["-a", "hyprtrace", title, message])
                .spawn()
                .is_ok();
            Ok(json!({"sent": spawned}))
        }
        "get_app_trend" => {
            let class = arg_str(args, "class")
                .ok_or_else(|| anyhow::anyhow!("missing required argument: class"))?;
            let from = arg_str(args, "from").map(String::from).unwrap_or_else(|| {
                (chrono::Local::now() - chrono::Duration::days(7))
                    .format("%Y-%m-%d")
                    .to_string()
            });
            let to = arg_str(args, "to").map(String::from).unwrap_or_else(today);
            let r = db.lock().await.app_daily_trend(class, &from, &to)?;
            Ok(serde_json::to_value(r)?)
        }

        // ---- Hyprland live state (blocking IPC) ----
        _ => {
            let tool = name.to_string();
            let key = arg_str(args, "key").map(String::from);
            tokio::task::spawn_blocking(move || execute_hyprland_tool(&tool, key.as_deref()))
                .await
                .map_err(|e| anyhow::anyhow!("blocking task failed: {}", e))?
        }
    }
}

fn execute_hyprland_tool(name: &str, key: Option<&str>) -> anyhow::Result<Value> {
    use hyprland::prelude::*;

    // hyprland-rs resolves the IPC socket from the HYPRLAND_INSTANCE_SIGNATURE
    // env var. When running as a systemd user service that variable isn't
    // inherited from the graphical session, so resolve the running instance
    // via `hyprctl instances` and set it before making any hyprland call.
    ensure_hyprland_env()?;

    match name {
        "active_window" => {
            let w = hyprland::data::Client::get_active()?;
            Ok(match w {
                Some(c) => json!({
                    "class": c.class,
                    "title": truncate(&c.title, 120),
                    "workspace": c.workspace.name,
                    "pid": c.pid,
                    "monitor": c.monitor.map(|m| m as i64),
                    "floating": c.floating,
                    "fullscreen": format!("{:?}", c.fullscreen),
                    "xwayland": c.xwayland,
                }),
                None => json!({"focused": false, "note": "no active window (empty desktop)"}),
            })
        }
        "clients" => {
            let clients = hyprland::data::Clients::get()?.to_vec();
            let mut out: Vec<Value> = clients
                .into_iter()
                .map(|c| {
                    json!({
                        "class": c.class,
                        "title": truncate(&c.title, 80),
                        "workspace": c.workspace.name,
                        "pid": c.pid,
                        "monitor": c.monitor.map(|m| m as i64),
                        "floating": c.floating,
                        "fullscreen": format!("{:?}", c.fullscreen),
                        "focused": c.focus_history_id == 0,
                    })
                })
                .collect();
            out.truncate(50);
            Ok(json!({ "count": out.len(), "windows": out }))
        }
        "active_workspace" => {
            let w = hyprland::data::Workspace::get_active()?;
            Ok(json!({
                "id": w.id,
                "name": w.name,
                "monitor": w.monitor,
                "windows": w.windows,
                "fullscreen": w.fullscreen,
                "last_window_title": truncate(&w.last_window_title, 100),
            }))
        }
        "workspaces" => {
            let ws = hyprland::data::Workspaces::get()?.to_vec();
            let out: Vec<Value> = ws
                .into_iter()
                .map(|w| {
                    json!({
                        "id": w.id,
                        "name": w.name,
                        "monitor": w.monitor,
                        "windows": w.windows,
                        "fullscreen": w.fullscreen,
                    })
                })
                .collect();
            Ok(json!(out))
        }
        "active_monitor" => {
            let m = hyprland::data::Monitor::get_active()?;
            Ok(json!({
                "id": m.id as i64,
                "name": m.name,
                "width": m.width,
                "height": m.height,
                "refresh_rate": m.refresh_rate,
                "scale": m.scale,
                "active_workspace": m.active_workspace.name,
            }))
        }
        "monitors" => {
            let ms = hyprland::data::Monitors::get()?.to_vec();
            let out: Vec<Value> = ms
                .into_iter()
                .map(|m| {
                    json!({
                        "id": m.id as i64,
                        "name": m.name,
                        "width": m.width,
                        "height": m.height,
                        "refresh_rate": m.refresh_rate,
                        "x": m.x,
                        "y": m.y,
                        "scale": m.scale,
                        "focused": m.focused,
                        "active_workspace": m.active_workspace.name,
                        "disabled": m.disabled,
                    })
                })
                .collect();
            Ok(json!(out))
        }
        "layers" => {
            let layers = hyprland::data::Layers::get()?;
            let mut out = Vec::new();
            for (monitor, display) in layers.iter() {
                for (_level, clients) in display.levels.iter() {
                    for c in clients {
                        out.push(json!({
                            "monitor": monitor,
                            "namespace": c.namespace,
                            "w": c.w,
                            "h": c.h,
                        }));
                    }
                }
            }
            out.truncate(40);
            Ok(json!(out))
        }
        "devices" => {
            let d = hyprland::data::Devices::get()?;
            let mice: Vec<Value> = d.mice.iter().map(|m| json!({"name": m.name})).collect();
            let keyboards: Vec<Value> = d
                .keyboards
                .iter()
                .map(|k| {
                    json!({
                        "name": k.name,
                        "layout": k.layout,
                        "active_keymap": k.active_keymap,
                        "main": k.main,
                    })
                })
                .collect();
            Ok(json!({"mice": mice, "keyboards": keyboards, "tablet_count": d.tablets.len()}))
        }
        "version" => {
            let v = hyprland::data::Version::get()?;
            Ok(json!({
                "branch": v.branch,
                "version": v.version,
                "tag": v.tag,
                "commit": v.commit.chars().take(10).collect::<String>(),
                "commit_date": v.commit_date,
                "dirty": v.dirty,
                "flags": v.flags,
            }))
        }
        "cursor_position" => {
            let p = hyprland::data::CursorPosition::get()?;
            Ok(json!({"x": p.x, "y": p.y}))
        }
        "binds" => {
            let binds = hyprland::data::Binds::get()?.to_vec();
            let mut out: Vec<Value> = binds
                .into_iter()
                .map(|b| {
                    json!({
                        "modmask": b.modmask,
                        "key": b.key,
                        "dispatcher": b.dispatcher,
                        "arg": truncate(&b.arg, 80),
                        "submap": b.submap,
                    })
                })
                .collect();
            out.truncate(80);
            Ok(json!({ "count": out.len(), "binds": out }))
        }
        "animations" => {
            let anims = hyprland::data::Animations::get()?;
            let out: Vec<Value> = anims
                .0
                .iter()
                .map(|a| {
                    json!({
                        "name": a.name,
                        "enabled": a.enabled,
                        "speed": a.speed,
                    })
                })
                .collect();
            Ok(json!(out))
        }
        "workspace_rules" => {
            let rules = hyprland::data::WorkspaceRules::get()?.to_vec();
            let out: Vec<Value> = rules
                .into_iter()
                .map(|r| {
                    json!({
                        "workspace": r.workspace_string,
                        "monitor": r.monitor,
                        "default": r.default,
                        "persistent": r.persistent,
                    })
                })
                .collect();
            Ok(json!(out))
        }
        "fullscreen_state" => {
            let fs = hyprland::data::FullscreenState::get()?;
            Ok(json!({"fullscreen": fs.bool()}))
        }
        "get_keyword" => {
            let key = key.ok_or_else(|| anyhow::anyhow!("missing required argument: key"))?;
            let kw = hyprland::keyword::Keyword::get(key)?;
            let value = match kw.value {
                hyprland::keyword::OptionValue::Int(i) => json!(i),
                hyprland::keyword::OptionValue::Float(f) => json!(f),
                hyprland::keyword::OptionValue::String(s) => json!(s),
            };
            Ok(json!({"option": kw.option, "value": value, "set": kw.set}))
        }
        "list_plugins" => {
            let plugins = hyprland::ctl::plugin::list()?;
            let out: Vec<Value> = plugins
                .into_iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "author": p.author,
                        "version": p.version,
                        "description": truncate(&p.description, 100),
                    })
                })
                .collect();
            Ok(json!(out))
        }
        "list_instances" => {
            let instances = hyprland::ctl::instance::instance_list()?;
            let out: Vec<Value> = instances
                .into_iter()
                .map(|i| {
                    json!({
                        "instance": i.instance,
                        "pid": i.pid,
                        "wl_socket": i.wl_socket,
                    })
                })
                .collect();
            Ok(json!(out))
        }
        _ => anyhow::bail!("Unknown tool: {}", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_schema_rejects_extra_properties() {
        // Without additionalProperties:false a model can send unknown fields
        // that the executor silently ignores, masking a mismatch between the
        // schema and the real parameters.
        for t in all_tools() {
            let props = &t.parameters["properties"];
            let params_obj = t.parameters.as_object().expect("tool parameters are an object");
            assert_eq!(
                params_obj.get("additionalProperties").and_then(|v| v.as_bool()),
                Some(false),
                "tool {} must set additionalProperties=false",
                t.name
            );
            // Every required key must also exist in properties.
            if let Some(required) = params_obj.get("required").and_then(|v| v.as_array()) {
                for key in required {
                    let key = key.as_str().expect("required key is a string");
                    assert!(
                        props.get(key).is_some(),
                        "tool {}: required key '{}' is missing from properties",
                        t.name,
                        key
                    );
                }
            }
        }
    }
}
