use crate::clipboard::ClipboardWatcher;
use crate::credentials::CredentialStore;
use crate::manager::ProcessManager;
use crate::todo::TodoStore;
use crate::ytdl::YtDlpEngine;
use std::sync::Arc;

pub fn handle_cli(
    args: &[String],
    pm: &Arc<ProcessManager>,
    cred: &Arc<CredentialStore>,
    clips: &Arc<ClipboardWatcher>,
    todo: &Arc<TodoStore>,
    _ytdl: &Arc<YtDlpEngine>,
) -> bool {
    if args.len() <= 1 {
        return false; // Launch GUI
    }

    let cmd = args[1].to_lowercase();
    match cmd.as_str() {
        "gui" => false, // Launch GUI

        "list" | "ps" => {
            let summary = pm.get_summary();
            println!("\n{}", "=".repeat(110));
            println!(
                "  PROCESS MANAGER (RUST ENGINE) - Active Services [{}]",
                if summary.admin { "ADMIN: ELEVATED" } else { "USER: STANDARD" }
            );
            println!("{}", "=".repeat(110));

            if summary.services.is_empty() {
                println!("  No services registered.");
            } else {
                println!(
                    "{:<16} {:<15} {:<12} {:<8} {:<8} {:<12} {:<10} {:<10} {:<10}",
                    "NAME", "GROUP", "SHORTCUT", "PORT", "PID", "STATUS", "RESTARTS", "RAM", "UPTIME"
                );
                println!("{}", "-".repeat(110));

                for s in summary.services {
                    let port_str = s.port.map(|p| format!(":{}", p)).unwrap_or_else(|| "-".to_string());
                    let pid_str = s.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".to_string());
                    let shortcut_str = s.shortcut.unwrap_or_else(|| "-".to_string());
                    let ram_str = if s.memory_mb > 0.0 { format!("{:.1}MB", s.memory_mb) } else { "-".to_string() };
                    let uptime_str = if s.uptime_seconds > 0 { format!("{}s", s.uptime_seconds) } else { "-".to_string() };

                    println!(
                        "{:<16} {:<15} {:<12} {:<8} {:<8} {:<12} {:<10} {:<10} {:<10}",
                        s.name, s.group, shortcut_str, port_str, pid_str, s.status.to_uppercase(), s.restarts, ram_str, uptime_str
                    );
                }
            }
            println!("{}\n", "=".repeat(110));
            true
        }

        "groups" => {
            let summary = pm.get_summary();
            println!("\n{}", "=".repeat(75));
            println!("  REGISTERED SERVICE GROUPS ({} groups total)", summary.groups.len());
            println!("{}", "=".repeat(75));
            println!("{:<20} {:<8} {:<9} {}", "GROUP", "TOTAL", "RUNNING", "SERVICES");
            println!("{}", "-".repeat(75));

            for g in summary.groups {
                println!("{:<20} {:<8} {:<9} {}", g.name, g.total, g.running, g.services.join(", "));
            }
            println!("{}\n", "=".repeat(75));
            true
        }

        "start" => {
            if args.len() < 3 {
                println!("Usage: pm_rs start <name|all>");
                return true;
            }
            let target = &args[2];
            if target == "all" {
                pm.start_all();
                println!("[OK] Started all services.");
            } else if pm.start_service(target) {
                println!("[OK] Started service '{}'.", target);
            } else {
                println!("[ERROR] Failed to start service '{}'.", target);
            }
            true
        }

        "stop" => {
            if args.len() < 3 {
                println!("Usage: pm_rs stop <name|all>");
                return true;
            }
            let target = &args[2];
            if target == "all" {
                pm.stop_all();
                println!("[OK] Stopped all services.");
            } else if pm.stop_service(target) {
                println!("[OK] Stopped service '{}'.", target);
            } else {
                println!("[ERROR] Service '{}' not found.", target);
            }
            true
        }

        "restart" => {
            if args.len() < 3 {
                println!("Usage: pm_rs restart <name|all>");
                return true;
            }
            let target = &args[2];
            if target == "all" {
                pm.restart_all();
                println!("[OK] Restarted all services.");
            } else if pm.restart_service(target) {
                println!("[OK] Restarted service '{}'.", target);
            } else {
                println!("[ERROR] Service '{}' not found.", target);
            }
            true
        }

        "delete" => {
            if args.len() < 3 {
                println!("Usage: pm_rs delete <name>");
                return true;
            }
            let target = &args[2];
            if pm.delete_service(target) {
                println!("[OK] Deleted service '{}'.", target);
            } else {
                println!("[ERROR] Service '{}' not found.", target);
            }
            true
        }

        "logs" => {
            if args.len() < 3 {
                println!("Usage: pm_rs logs <name> [-n <lines>]");
                return true;
            }
            let target = &args[2];
            let mut lines = 50;
            if args.len() >= 5 && args[3] == "-n" {
                if let Ok(l) = args[4].parse::<usize>() {
                    lines = l;
                }
            }
            let logs = pm.get_logs(target, lines);
            println!("\n--- Logs for '{}' (Last {} lines) ---\n", target, lines);
            println!("{}", logs);
            println!("--- End of logs ---\n");
            true
        }

        "creds" => {
            let search = if args.len() >= 3 { &args[2] } else { "" };
            let items = cred.list(search);
            println!("\n{}", "=".repeat(80));
            println!("  CREDENTIALS VAULT ({} accounts)", items.len());
            println!("{}", "=".repeat(80));
            println!("{:<20} {:<25} {:<20} {}", "PLATFORM", "USERNAME", "PASSWORD", "URL");
            println!("{}", "-".repeat(80));
            for it in items {
                println!("{:<20} {:<25} {:<20} {}", it.platform, it.username, "••••••••••", it.url.unwrap_or_default());
            }
            println!("{}\n", "=".repeat(80));
            true
        }

        "clips" => {
            let search = if args.len() >= 3 { &args[2] } else { "" };
            let items = clips.list(search, false);
            let limit = items.len().min(15);
            println!("\n{}", "=".repeat(80));
            println!("  PERMANENT CLIPBOARD HISTORY (Showing latest {} of {})", limit, items.len());
            println!("{}", "=".repeat(80));
            for it in items.iter().take(limit) {
                let pin = if it.pinned { "⭐" } else { "  " };
                let preview = it.content.replace('\n', " ");
                let short_preview = if preview.len() > 55 {
                    format!("{}...", &preview[..52])
                } else {
                    preview
                };
                println!(" {} [{}] ({} chars) : {}", pin, it.timestamp.format("%H:%M:%S"), it.length, short_preview);
            }
            println!("{}\n", "=".repeat(80));
            true
        }

        "todo" | "todos" => {
            let items = todo.list("", None, None);
            println!("\n{}", "=".repeat(80));
            println!("  KANBAN TASK BOARD ({} tasks total)", items.len());
            println!("{}", "=".repeat(80));
            println!("{:<8} {:<15} {:<12} {:<10} {}", "ID", "PROJECT", "STATUS", "PRIORITY", "TITLE");
            println!("{}", "-".repeat(80));
            for it in items {
                let prio = it.priority.unwrap_or_else(|| "normal".to_string());
                let proj = it.project.unwrap_or_else(|| "General".to_string());
                println!("{:<8} {:<15} {:<12} {:<10} {}", it.id, proj, it.status.to_uppercase(), prio, it.title);
            }
            println!("{}\n", "=".repeat(80));
            true
        }

        "elevate" => {
            if crate::win32::is_admin() {
                println!("Already running with Administrator privileges.");
            } else {
                println!("Requesting Windows Administrator elevation...");
                let _ = crate::win32::request_admin_elevation(&["gui"]);
            }
            true
        }

        _ => {
            println!("Unknown command '{}'. Available: list, groups, start, stop, restart, delete, logs, creds, clips, todo, elevate, gui", cmd);
            true
        }
    }
}
