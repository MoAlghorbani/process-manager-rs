use crate::manager::get_data_dir;
use crate::models::{ChatConfig, ChatMessage, FileTransferInfo, PeerInfo};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum NetworkPacket {
    #[serde(rename = "presence")]
    Presence {
        user_id: String,
        username: String,
        hostname: String,
        status: String,
    },
    #[serde(rename = "message")]
    Message {
        id: String,
        sender_id: String,
        sender_name: String,
        sender_host: String,
        recipient_id: String, // "all" for general room, or specific peer user_id
        content: String,
        timestamp: DateTime<Utc>,
        #[serde(default)]
        file_info: Option<FileTransferInfo>,
    },
    #[serde(rename = "typing")]
    Typing {
        sender_id: String,
        sender_name: String,
        recipient_id: String,
    },
    #[serde(rename = "buzz")]
    Buzz {
        id: String,
        sender_id: String,
        sender_name: String,
        sender_host: String,
        recipient_id: String,
        buzz_type: String, // "hard" or "soft"
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "file_offer")]
    FileOffer {
        transfer_id: String,
        sender_id: String,
        sender_name: String,
        sender_host: String,
        recipient_id: String,
        file_name: String,
        file_size: u64,
        transfer_port: u16,
    },
    #[serde(rename = "file_decision")]
    FileDecision {
        transfer_id: String,
        sender_id: String,
        recipient_id: String,
        accepted: bool,
    },
    #[serde(rename = "file_cancel")]
    FileCancel {
        transfer_id: String,
        sender_id: String,
        recipient_id: String,
    },
    #[serde(rename = "goodbye")]
    Goodbye {
        user_id: String,
    },
}

pub struct TransferSession {
    pub info: RwLock<FileTransferInfo>,
    pub cancel_flag: AtomicBool,
}

pub struct ChatEngine {
    pub base_dir: Arc<RwLock<PathBuf>>,
    pub config_file: Arc<RwLock<PathBuf>>,
    pub history_file: Arc<RwLock<PathBuf>>,
    pub config: Arc<RwLock<ChatConfig>>,
    pub messages: Arc<RwLock<Vec<ChatMessage>>>,
    pub peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    pub typing_status: Arc<RwLock<HashMap<String, (String, Instant)>>>,
    pub is_running: Arc<AtomicBool>,
    pub outbound_socket: Arc<Mutex<Option<UdpSocket>>>,
    pub tcp_port: Arc<AtomicU16>,
    pub active_transfers: Arc<RwLock<HashMap<String, Arc<TransferSession>>>>,
}

pub fn get_local_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    if let Ok(SocketAddr::V4(addr)) = s.local_addr() {
        Some(*addr.ip())
    } else {
        None
    }
}

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    const TB: u64 = 1024 * 1024 * 1024 * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn get_default_download_path(filename: &str) -> PathBuf {
    let base = std::env::var("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Downloads"))
        .unwrap_or_else(|_| PathBuf::from("."));

    let mut target = base.join(filename);
    if target.exists() {
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let mut counter = 1;
        loop {
            let candidate_name = if ext.is_empty() {
                format!("{} ({})", stem, counter)
            } else {
                format!("{} ({}).{}", stem, counter, ext)
            };
            target = base.join(candidate_name);
            if !target.exists() {
                break;
            }
            counter += 1;
        }
    }
    target
}

impl ChatEngine {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let config_file = base_dir.join("chat_config.json");
        let history_file = base_dir.join("chat_history.json");

        let mut config = ChatConfig::default();
        if config_file.exists() {
            if let Ok(data) = fs::read_to_string(&config_file) {
                if let Ok(loaded) = serde_json::from_str::<ChatConfig>(&data) {
                    config = loaded;
                }
            }
        }

        let mut messages = Vec::new();
        if history_file.exists() {
            if let Ok(data) = fs::read_to_string(&history_file) {
                if let Ok(list) = serde_json::from_str::<Vec<ChatMessage>>(&data) {
                    messages = list;
                }
            }
        }

        Self {
            base_dir: Arc::new(RwLock::new(base_dir)),
            config_file: Arc::new(RwLock::new(config_file)),
            history_file: Arc::new(RwLock::new(history_file)),
            config: Arc::new(RwLock::new(config)),
            messages: Arc::new(RwLock::new(messages)),
            peers: Arc::new(RwLock::new(HashMap::new())),
            typing_status: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(AtomicBool::new(false)),
            outbound_socket: Arc::new(Mutex::new(None)),
            tcp_port: Arc::new(AtomicU16::new(38384)),
            active_transfers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn reload_from_dir(&self, new_dir: &Path) {
        let new_base = new_dir.to_path_buf();
        let new_cfg = new_base.join("chat_config.json");
        let new_hist = new_base.join("chat_history.json");

        if new_cfg.exists() {
            if let Ok(data) = fs::read_to_string(&new_cfg) {
                if let Ok(loaded) = serde_json::from_str::<ChatConfig>(&data) {
                    *self.config.write().unwrap() = loaded;
                }
            }
        }

        let mut messages = Vec::new();
        if new_hist.exists() {
            if let Ok(data) = fs::read_to_string(&new_hist) {
                if let Ok(list) = serde_json::from_str::<Vec<ChatMessage>>(&data) {
                    messages = list;
                }
            }
        }
        *self.messages.write().unwrap() = messages;

        *self.base_dir.write().unwrap() = new_base;
        *self.config_file.write().unwrap() = new_cfg;
        *self.history_file.write().unwrap() = new_hist;
    }

    pub fn get_config(&self) -> ChatConfig {
        self.config.read().unwrap().clone()
    }

    pub fn save_config(&self, new_cfg: ChatConfig) {
        {
            let mut guard = self.config.write().unwrap();
            *guard = new_cfg.clone();
        }
        let cfg_path = self.config_file.read().unwrap().clone();
        if let Ok(json) = serde_json::to_string_pretty(&new_cfg) {
            let _ = fs::write(&cfg_path, json);
        }

        self.broadcast_presence();
    }

    pub fn save_messages(&self) {
        let list = self.messages.read().unwrap();
        let hist_path = self.history_file.read().unwrap().clone();
        if let Ok(json) = serde_json::to_string_pretty(&*list) {
            let _ = fs::write(&hist_path, json);
        }
    }

    pub fn update_message_file_status(
        &self,
        transfer_id: &str,
        new_status: &str,
        save_path: Option<&str>,
    ) {
        let mut msgs = self.messages.write().unwrap();
        let mut updated = false;
        for m in msgs.iter_mut() {
            if let Some(ref mut fi) = m.file_info {
                if fi.transfer_id == transfer_id {
                    fi.status = new_status.to_string();
                    if let Some(p) = save_path {
                        fi.save_path = Some(p.to_string());
                    }
                    if new_status == "completed" {
                        fi.progress = 100.0;
                        fi.bytes_transferred = fi.file_size;
                        fi.completed_at = Some(Utc::now());
                    }
                    updated = true;
                }
            }
        }
        if updated {
            drop(msgs);
            self.save_messages();
        }
    }

    pub fn start(self: &Arc<Self>) {
        if self.is_running.swap(true, Ordering::SeqCst) {
            return;
        }

        let port = self.config.read().unwrap().port;
        let file_port_target = self.config.read().unwrap().file_transfer_port;

        // 1. Create UDP socket with reuse address & broadcast
        let socket_res = (|| -> Result<UdpSocket, std::io::Error> {
            let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            let _ = sock.set_reuse_address(true);
            let _ = sock.set_broadcast(true);
            let addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);
            sock.bind(&addr.into())?;
            let std_sock: UdpSocket = sock.into();
            std_sock.set_read_timeout(Some(Duration::from_millis(500)))?;
            Ok(std_sock)
        })();

        let recv_socket = match socket_res {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[LAN Chat] Failed to bind broadcast socket on port {}: {}", port, e);
                self.is_running.store(false, Ordering::SeqCst);
                return;
            }
        };

        if let Ok(out_sock) = recv_socket.try_clone() {
            *self.outbound_socket.lock().unwrap() = Some(out_sock);
        }

        // 2. Create TCP File Transfer Listener
        let tcp_listener_res = TcpListener::bind(format!("0.0.0.0:{}", file_port_target))
            .or_else(|_| TcpListener::bind("0.0.0.0:0"));

        if let Ok(tcp_listener) = tcp_listener_res {
            let actual_tcp_port = tcp_listener.local_addr().map(|a| a.port()).unwrap_or(file_port_target);
            self.tcp_port.store(actual_tcp_port, Ordering::SeqCst);
            let _ = tcp_listener.set_nonblocking(true);

            let is_running_tcp = self.is_running.clone();
            let transfers_tcp = self.active_transfers.clone();
            let engine_weak = Arc::downgrade(self);

            std::thread::spawn(move || {
                while is_running_tcp.load(Ordering::SeqCst) {
                    match tcp_listener.accept() {
                        Ok((mut stream, _peer_addr)) => {
                            let transfers_h = transfers_tcp.clone();
                            let engine_opt = engine_weak.upgrade();
                            std::thread::spawn(move || {
                                Self::handle_tcp_sender_stream(&mut stream, transfers_h, engine_opt);
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        Err(e) => {
                            eprintln!("[LAN Chat] TCP accept error: {}", e);
                            std::thread::sleep(Duration::from_millis(500));
                        }
                    }
                }
            });
        }

        // 3. Packet Receiver Worker
        let is_running_rx = self.is_running.clone();
        let peers_rx = self.peers.clone();
        let messages_rx = self.messages.clone();
        let config_rx = self.config.clone();
        let history_file_rx = self.history_file.clone();
        let typing_rx = self.typing_status.clone();
        let outbound_rx = self.outbound_socket.clone();
        let transfers_rx = self.active_transfers.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 65535];
            while is_running_rx.load(Ordering::SeqCst) {
                match recv_socket.recv_from(&mut buf) {
                    Ok((amt, src_addr)) => {
                        let my_config = config_rx.read().unwrap().clone();

                        if let Ok(text) = std::str::from_utf8(&buf[..amt]) {
                            if let Ok(packet) = serde_json::from_str::<NetworkPacket>(text) {
                                match packet {
                                    NetworkPacket::Presence {
                                        user_id,
                                        username,
                                        hostname,
                                        status,
                                    } => {
                                        if user_id != my_config.user_id {
                                            let mut peers_guard = peers_rx.write().unwrap();
                                            let is_new = !peers_guard.contains_key(&user_id);
                                            peers_guard.insert(
                                                user_id.clone(),
                                                PeerInfo {
                                                    id: user_id.clone(),
                                                    username: username.clone(),
                                                    hostname,
                                                    ip: src_addr.ip().to_string(),
                                                    status,
                                                    last_seen: Utc::now(),
                                                    is_online: true,
                                                },
                                            );

                                            if is_new {
                                                drop(peers_guard);
                                                let reply = NetworkPacket::Presence {
                                                    user_id: my_config.user_id,
                                                    username: my_config.username,
                                                    hostname: my_config.hostname,
                                                    status: my_config.status,
                                                };
                                                if let Ok(json) = serde_json::to_vec(&reply) {
                                                    if let Some(ref sock) = *outbound_rx.lock().unwrap() {
                                                        let _ = sock.send_to(&json, src_addr);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    NetworkPacket::Message {
                                        id,
                                        sender_id,
                                        sender_name,
                                        sender_host,
                                        recipient_id,
                                        content,
                                        timestamp,
                                        file_info,
                                    } => {
                                        if sender_id != my_config.user_id {
                                            let for_me = recipient_id == "all"
                                                || recipient_id == my_config.user_id;

                                            if for_me {
                                                let mut msgs = messages_rx.write().unwrap();
                                                if !msgs.iter().any(|m| m.id == id) {
                                                    msgs.push(ChatMessage {
                                                        id,
                                                        sender_id,
                                                        sender_name: sender_name.clone(),
                                                        sender_host,
                                                        recipient_id: recipient_id.clone(),
                                                        content: content.clone(),
                                                        timestamp,
                                                        is_mine: false,
                                                        file_info,
                                                        buzz_type: None,
                                                    });

                                                    if msgs.len() > 1000 {
                                                        let excess = msgs.len() - 1000;
                                                        msgs.drain(0..excess);
                                                    }

                                                    let hist_path =
                                                        history_file_rx.read().unwrap().clone();
                                                    if let Ok(json) =
                                                        serde_json::to_string_pretty(&*msgs)
                                                    {
                                                        let _ = fs::write(&hist_path, json);
                                                    }

                                                    if my_config.desktop_notifications {
                                                        let notif_title = if recipient_id == "all" {
                                                            format!("💬 #General LAN ({})", sender_name)
                                                        } else {
                                                            format!("💬 Direct from {}", sender_name)
                                                        };
                                                        crate::win32::show_desktop_notification(&notif_title, &content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    NetworkPacket::Typing {
                                        sender_id,
                                        sender_name,
                                        recipient_id,
                                    } => {
                                        if sender_id != my_config.user_id
                                            && (recipient_id == "all"
                                                || recipient_id == my_config.user_id)
                                        {
                                            let mut typ = typing_rx.write().unwrap();
                                            typ.insert(
                                                sender_id,
                                                (
                                                    sender_name,
                                                    Instant::now() + Duration::from_secs(3),
                                                ),
                                            );
                                        }
                                    }
                                    NetworkPacket::Buzz {
                                        id,
                                        sender_id,
                                        sender_name,
                                        sender_host,
                                        recipient_id,
                                        buzz_type,
                                        timestamp,
                                    } => {
                                        if sender_id != my_config.user_id {
                                            let for_me = recipient_id == "all"
                                                || recipient_id == my_config.user_id;
                                            if for_me {
                                                let display_content = if buzz_type == "hard" {
                                                    format!("⚡ {} sent you a HARD BUZZ!", sender_name)
                                                } else {
                                                    format!("🔔 {} nudged you.", sender_name)
                                                };

                                                let mut msgs = messages_rx.write().unwrap();
                                                if !msgs.iter().any(|m| m.id == id) {
                                                    msgs.push(ChatMessage {
                                                        id: id.clone(),
                                                        sender_id: sender_id.clone(),
                                                        sender_name: sender_name.clone(),
                                                        sender_host: sender_host.clone(),
                                                        recipient_id: recipient_id.clone(),
                                                        content: display_content,
                                                        timestamp,
                                                        is_mine: false,
                                                        file_info: None,
                                                        buzz_type: Some(buzz_type.clone()),
                                                    });

                                                    if msgs.len() > 1000 {
                                                        let excess = msgs.len() - 1000;
                                                        msgs.drain(0..excess);
                                                    }

                                                    let hist_path = history_file_rx.read().unwrap().clone();
                                                    if let Ok(json) = serde_json::to_string_pretty(&*msgs) {
                                                        let _ = fs::write(&hist_path, json);
                                                    }

                                                    if my_config.desktop_notifications {
                                                        if buzz_type == "hard" {
                                                            crate::win32::show_desktop_notification(
                                                                &format!("⚡ HARD BUZZ from {}!", sender_name),
                                                                "URGENT: Screen vibrating!",
                                                            );
                                                        } else {
                                                            crate::win32::show_desktop_notification(
                                                                &format!("🔔 Nudge from {}", sender_name),
                                                                "Peer sent you a soft buzz.",
                                                            );
                                                        }
                                                    }

                                                    crate::win32::activate_and_bring_to_front();
                                                    crate::win32::flash_window_attention();
                                                }
                                            }
                                        }
                                    }
                                    NetworkPacket::FileOffer {
                                        transfer_id,
                                        sender_id,
                                        sender_name,
                                        sender_host,
                                        recipient_id,
                                        file_name,
                                        file_size,
                                        transfer_port,
                                    } => {
                                        let for_me = recipient_id == "all" || recipient_id == my_config.user_id;
                                        if for_me && sender_id != my_config.user_id {
                                            let file_info = FileTransferInfo {
                                                transfer_id: transfer_id.clone(),
                                                file_name: file_name.clone(),
                                                file_size,
                                                file_path: None,
                                                save_path: None,
                                                sender_id: sender_id.clone(),
                                                sender_name: sender_name.clone(),
                                                sender_host: sender_host.clone(),
                                                recipient_id: recipient_id.clone(),
                                                sender_ip: src_addr.ip().to_string(),
                                                transfer_port,
                                                direction: "incoming".to_string(),
                                                status: "offered".to_string(),
                                                bytes_transferred: 0,
                                                progress: 0.0,
                                                speed_bps: 0,
                                                eta_sec: 0,
                                                error: None,
                                                created_at: Utc::now(),
                                                completed_at: None,
                                            };

                                            let session = Arc::new(TransferSession {
                                                info: RwLock::new(file_info.clone()),
                                                cancel_flag: AtomicBool::new(false),
                                            });
                                            transfers_rx.write().unwrap().insert(transfer_id.clone(), session);

                                            let mut msgs = messages_rx.write().unwrap();
                                            let msg_id = format!("ft-{}", transfer_id);
                                            if !msgs.iter().any(|m| m.id == msg_id) {
                                                msgs.push(ChatMessage {
                                                    id: msg_id,
                                                    sender_id: sender_id.clone(),
                                                    sender_name: sender_name.clone(),
                                                    sender_host: sender_host.clone(),
                                                    recipient_id: recipient_id.clone(),
                                                    content: format!("📎 Sent file offer: {} ({})", file_name, format_size(file_size)),
                                                    timestamp: Utc::now(),
                                                    is_mine: false,
                                                    file_info: Some(file_info),
                                                    buzz_type: None,
                                                });

                                                let hist_path = history_file_rx.read().unwrap().clone();
                                                if let Ok(json) = serde_json::to_string_pretty(&*msgs) {
                                                    let _ = fs::write(&hist_path, json);
                                                }

                                                if my_config.desktop_notifications {
                                                    let notif_title = if recipient_id == "all" {
                                                        format!("📁 File Shared in #General by {}", sender_name)
                                                    } else {
                                                        format!("📁 File Offer from {}", sender_name)
                                                    };
                                                    crate::win32::show_desktop_notification(
                                                        &notif_title,
                                                        &format!("{} ({})", file_name, format_size(file_size)),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    NetworkPacket::FileDecision {
                                        transfer_id,
                                        recipient_id,
                                        accepted,
                                        ..
                                    } => {
                                        if recipient_id == my_config.user_id {
                                            if !accepted {
                                                let is_general_offer = {
                                                    let guard = transfers_rx.read().unwrap();
                                                    guard.get(&transfer_id)
                                                        .map(|s| s.info.read().unwrap().recipient_id == "all")
                                                        .unwrap_or(false)
                                                };

                                                if !is_general_offer {
                                                    if let Some(session) = transfers_rx.read().unwrap().get(&transfer_id) {
                                                        let mut info = session.info.write().unwrap();
                                                        info.status = "declined".to_string();
                                                    }
                                                    let mut msgs = messages_rx.write().unwrap();
                                                    for m in msgs.iter_mut() {
                                                        if let Some(ref mut fi) = m.file_info {
                                                            if fi.transfer_id == transfer_id {
                                                                fi.status = "declined".to_string();
                                                            }
                                                        }
                                                    }
                                                    let hist_path = history_file_rx.read().unwrap().clone();
                                                    if let Ok(json) = serde_json::to_string_pretty(&*msgs) {
                                                        let _ = fs::write(&hist_path, json);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    NetworkPacket::FileCancel {
                                        transfer_id,
                                        recipient_id,
                                        ..
                                    } => {
                                        if recipient_id == my_config.user_id {
                                            if let Some(session) = transfers_rx.read().unwrap().get(&transfer_id) {
                                                session.cancel_flag.store(true, Ordering::Relaxed);
                                                let mut info = session.info.write().unwrap();
                                                info.status = "cancelled".to_string();
                                            }
                                            let mut msgs = messages_rx.write().unwrap();
                                            for m in msgs.iter_mut() {
                                                if let Some(ref mut fi) = m.file_info {
                                                    if fi.transfer_id == transfer_id {
                                                        fi.status = "cancelled".to_string();
                                                    }
                                                }
                                            }
                                            let hist_path = history_file_rx.read().unwrap().clone();
                                            if let Ok(json) = serde_json::to_string_pretty(&*msgs) {
                                                let _ = fs::write(&hist_path, json);
                                            }
                                        }
                                    }
                                    NetworkPacket::Goodbye { user_id } => {
                                        let mut peers_guard = peers_rx.write().unwrap();
                                        if let Some(p) = peers_guard.get_mut(&user_id) {
                                            p.is_online = false;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        // Timeout tick, continue
                    }
                    Err(e) => {
                        eprintln!("[LAN Chat] Receiver error: {}", e);
                    }
                }
            }
        });

        // 4. Heartbeat & Peer Maintenance Worker
        let is_running_hb = self.is_running.clone();
        let peers_hb = self.peers.clone();
        let config_hb = self.config.clone();
        let outbound_hb = self.outbound_socket.clone();

        std::thread::spawn(move || {
            let mut counter = 0;
            while is_running_hb.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(1));
                counter += 1;

                if counter >= 4 {
                    counter = 0;
                    let cfg = config_hb.read().unwrap().clone();
                    if cfg.broadcast_enabled {
                        let packet = NetworkPacket::Presence {
                            user_id: cfg.user_id,
                            username: cfg.username,
                            hostname: cfg.hostname,
                            status: cfg.status,
                        };
                        if let Ok(bytes) = serde_json::to_vec(&packet) {
                            if let Some(ref sock) = *outbound_hb.lock().unwrap() {
                                let _ = sock.send_to(&bytes, format!("255.255.255.255:{}", cfg.port));

                                if let Some(local_ip) = get_local_ip() {
                                    let octets = local_ip.octets();
                                    let subnet_bcast = format!(
                                        "{}.{}.{}.255:{}",
                                        octets[0], octets[1], octets[2], cfg.port
                                    );
                                    let _ = sock.send_to(&bytes, &subnet_bcast);
                                }
                            }
                        }
                    }

                    let now = Utc::now();
                    let mut peers_guard = peers_hb.write().unwrap();
                    peers_guard.retain(|_, p| {
                        let age = (now - p.last_seen).num_seconds();
                        if age > 16 {
                            p.is_online = false;
                        }
                        age < 60
                    });
                }
            }
        });

        self.broadcast_presence();
    }

    pub fn broadcast_presence(&self) {
        let cfg = self.config.read().unwrap().clone();
        let packet = NetworkPacket::Presence {
            user_id: cfg.user_id,
            username: cfg.username,
            hostname: cfg.hostname,
            status: cfg.status,
        };
        self.send_packet(&packet, None);
    }

    pub fn send_packet(&self, packet: &NetworkPacket, target_ip: Option<&str>) {
        let port = self.config.read().unwrap().port;
        if let Ok(bytes) = serde_json::to_vec(packet) {
            if let Some(ref sock) = *self.outbound_socket.lock().unwrap() {
                if let Some(ip) = target_ip {
                    let dest = format!("{}:{}", ip, port);
                    let _ = sock.send_to(&bytes, &dest);
                } else {
                    let _ = sock.send_to(&bytes, format!("255.255.255.255:{}", port));
                    if let Some(local_ip) = get_local_ip() {
                        let octets = local_ip.octets();
                        let subnet = format!("{}.{}.{}.255:{}", octets[0], octets[1], octets[2], port);
                        let _ = sock.send_to(&bytes, &subnet);
                    }
                }
            }
        }
    }

    pub fn send_message(&self, recipient_id: &str, content: &str) -> Result<ChatMessage, String> {
        let text = content.trim();
        if text.is_empty() {
            return Err("Cannot send an empty message".to_string());
        }

        let cfg = self.config.read().unwrap().clone();
        let msg_id = format!("{}-{}", Utc::now().timestamp_millis(), rand::random::<u32>());
        let now = Utc::now();

        let chat_msg = ChatMessage {
            id: msg_id.clone(),
            sender_id: cfg.user_id.clone(),
            sender_name: cfg.username.clone(),
            sender_host: cfg.hostname.clone(),
            recipient_id: recipient_id.to_string(),
            content: text.to_string(),
            timestamp: now,
            is_mine: true,
            file_info: None,
            buzz_type: None,
        };

        {
            let mut msgs = self.messages.write().unwrap();
            msgs.push(chat_msg.clone());
            if msgs.len() > 1000 {
                let excess = msgs.len() - 1000;
                msgs.drain(0..excess);
            }
        }
        self.save_messages();

        let packet = NetworkPacket::Message {
            id: msg_id,
            sender_id: cfg.user_id,
            sender_name: cfg.username,
            sender_host: cfg.hostname,
            recipient_id: recipient_id.to_string(),
            content: text.to_string(),
            timestamp: now,
            file_info: None,
        };

        if recipient_id == "all" {
            self.send_packet(&packet, None);
        } else {
            let target_ip = {
                let peers = self.peers.read().unwrap();
                peers.get(recipient_id).map(|p| p.ip.clone())
            };

            if let Some(ref ip) = target_ip {
                self.send_packet(&packet, Some(ip));
            }
            self.send_packet(&packet, None);
        }

        Ok(chat_msg)
    }


    pub fn send_buzz(&self, recipient_id: &str, buzz_type: &str) -> Result<ChatMessage, String> {
        let b_type = if buzz_type == "hard" { "hard" } else { "soft" };
        let cfg = self.config.read().unwrap().clone();
        let msg_id = format!("buzz-{}-{}", Utc::now().timestamp_millis(), rand::random::<u32>());
        let now = Utc::now();

        let display_content = if b_type == "hard" {
            "⚡ Sent a HARD BUZZ!".to_string()
        } else {
            "🔔 Sent a Soft Buzz.".to_string()
        };

        let chat_msg = ChatMessage {
            id: msg_id.clone(),
            sender_id: cfg.user_id.clone(),
            sender_name: cfg.username.clone(),
            sender_host: cfg.hostname.clone(),
            recipient_id: recipient_id.to_string(),
            content: display_content,
            timestamp: now,
            is_mine: true,
            file_info: None,
            buzz_type: Some(b_type.to_string()),
        };

        {
            let mut msgs = self.messages.write().unwrap();
            msgs.push(chat_msg.clone());
            if msgs.len() > 1000 {
                let excess = msgs.len() - 1000;
                msgs.drain(0..excess);
            }
        }
        self.save_messages();

        let packet = NetworkPacket::Buzz {
            id: msg_id,
            sender_id: cfg.user_id,
            sender_name: cfg.username,
            sender_host: cfg.hostname,
            recipient_id: recipient_id.to_string(),
            buzz_type: b_type.to_string(),
            timestamp: now,
        };

        if recipient_id == "all" {
            self.send_packet(&packet, None);
        } else {
            let target_ip = {
                let peers = self.peers.read().unwrap();
                peers.get(recipient_id).map(|p| p.ip.clone())
            };
            if let Some(ref ip) = target_ip {
                self.send_packet(&packet, Some(ip));
            }
            self.send_packet(&packet, None);
        }

        Ok(chat_msg)
    }

    pub fn send_typing(&self, recipient_id: &str) {
        let cfg = self.config.read().unwrap().clone();
        let packet = NetworkPacket::Typing {
            sender_id: cfg.user_id,
            sender_name: cfg.username,
            recipient_id: recipient_id.to_string(),
        };

        if recipient_id == "all" {
            self.send_packet(&packet, None);
        } else {
            let target_ip = {
                let peers = self.peers.read().unwrap();
                peers.get(recipient_id).map(|p| p.ip.clone())
            };
            if let Some(ref ip) = target_ip {
                self.send_packet(&packet, Some(ip));
            } else {
                self.send_packet(&packet, None);
            }
        }
    }

    pub fn get_messages(&self, recipient: Option<&str>) -> Vec<ChatMessage> {
        let my_id = self.config.read().unwrap().user_id.clone();
        let msgs = self.messages.read().unwrap();

        match recipient {
            None | Some("all") => msgs
                .iter()
                .filter(|m| m.recipient_id == "all")
                .cloned()
                .collect(),
            Some(peer_id) => msgs
                .iter()
                .filter(|m| {
                    (m.sender_id == my_id && m.recipient_id == peer_id)
                        || (m.sender_id == peer_id && m.recipient_id == my_id)
                })
                .cloned()
                .collect(),
        }
    }

    pub fn get_peers(&self) -> Vec<PeerInfo> {
        let now = Utc::now();
        let peers = self.peers.read().unwrap();
        let mut list: Vec<PeerInfo> = peers
            .values()
            .map(|p| {
                let mut peer = p.clone();
                if (now - peer.last_seen).num_seconds() > 16 {
                    peer.is_online = false;
                }
                peer
            })
            .collect();

        list.sort_by(|a, b| {
            b.is_online
                .cmp(&a.is_online)
                .then_with(|| a.username.to_lowercase().cmp(&b.username.to_lowercase()))
        });
        list
    }

    pub fn get_typing_users(&self, recipient: &str) -> Vec<String> {
        let mut typ = self.typing_status.write().unwrap();
        let now = Instant::now();
        typ.retain(|_, (_, exp)| *exp > now);

        if recipient == "all" {
            typ.values().map(|(name, _)| name.clone()).collect()
        } else {
            if let Some((name, _)) = typ.get(recipient) {
                vec![name.clone()]
            } else {
                Vec::new()
            }
        }
    }

    pub fn clear_history(&self, recipient: Option<&str>) {
        let my_id = self.config.read().unwrap().user_id.clone();
        let mut msgs = self.messages.write().unwrap();

        match recipient {
            None => {
                msgs.clear();
            }
            Some("all") => {
                msgs.retain(|m| m.recipient_id != "all");
            }
            Some(peer_id) => {
                msgs.retain(|m| {
                    !((m.sender_id == my_id && m.recipient_id == peer_id)
                        || (m.sender_id == peer_id && m.recipient_id == my_id))
                });
            }
        }
        drop(msgs);
        self.save_messages();
    }

    // ==========================================
    // LARGE FILE TRANSFER METHODS
    // ==========================================

    pub fn send_file_offer(&self, recipient_id: &str, file_path_str: &str) -> Result<FileTransferInfo, String> {
        let path = PathBuf::from(file_path_str);
        if !path.is_file() {
            return Err("Selected file does not exist on disk.".to_string());
        }

        let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
        let file_size = meta.len();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file.bin".to_string());

        let cfg = self.config.read().unwrap().clone();
        let transfer_id = format!("ft_{}_{}", Utc::now().timestamp_millis(), rand::random::<u32>());
        let my_tcp_port = self.tcp_port.load(Ordering::SeqCst);
        let my_ip = get_local_ip().map(|ip| ip.to_string()).unwrap_or_else(|| "127.0.0.1".to_string());

        let transfer_info = FileTransferInfo {
            transfer_id: transfer_id.clone(),
            file_name: file_name.clone(),
            file_size,
            file_path: Some(file_path_str.to_string()),
            save_path: None,
            sender_id: cfg.user_id.clone(),
            sender_name: cfg.username.clone(),
            sender_host: cfg.hostname.clone(),
            recipient_id: recipient_id.to_string(),
            sender_ip: my_ip,
            transfer_port: my_tcp_port,
            direction: "outgoing".to_string(),
            status: "offered".to_string(),
            bytes_transferred: 0,
            progress: 0.0,
            speed_bps: 0,
            eta_sec: 0,
            error: None,
            created_at: Utc::now(),
            completed_at: None,
        };

        let session = Arc::new(TransferSession {
            info: RwLock::new(transfer_info.clone()),
            cancel_flag: AtomicBool::new(false),
        });
        self.active_transfers.write().unwrap().insert(transfer_id.clone(), session);

        let chat_msg = ChatMessage {
            id: format!("ft-{}", transfer_id),
            sender_id: cfg.user_id.clone(),
            sender_name: cfg.username.clone(),
            sender_host: cfg.hostname.clone(),
            recipient_id: recipient_id.to_string(),
            content: format!("📎 Offered file: {} ({})", file_name, format_size(file_size)),
            timestamp: Utc::now(),
            is_mine: true,
            file_info: Some(transfer_info.clone()),
            buzz_type: None,
        };

        {
            let mut msgs = self.messages.write().unwrap();
            msgs.push(chat_msg);
            if msgs.len() > 1000 {
                let excess = msgs.len() - 1000;
                msgs.drain(0..excess);
            }
        }
        self.save_messages();

        let packet = NetworkPacket::FileOffer {
            transfer_id: transfer_id.clone(),
            sender_id: cfg.user_id,
            sender_name: cfg.username,
            sender_host: cfg.hostname,
            recipient_id: recipient_id.to_string(),
            file_name,
            file_size,
            transfer_port: my_tcp_port,
        };

        let target_ip = {
            let peers = self.peers.read().unwrap();
            peers.get(recipient_id).map(|p| p.ip.clone())
        };

        if let Some(ref ip) = target_ip {
            self.send_packet(&packet, Some(ip));
        }
        self.send_packet(&packet, None);

        Ok(transfer_info)
    }

    pub fn accept_file_transfer(
        self: &Arc<Self>,
        transfer_id: &str,
        custom_save_path: Option<String>,
    ) -> Result<(), String> {
        let session = {
            let guard = self.active_transfers.read().unwrap();
            guard.get(transfer_id).cloned()
        }
        .ok_or_else(|| "Transfer session not found.".to_string())?;

        let (file_name, file_size, sender_ip, transfer_port, sender_id) = {
            let info = session.info.read().unwrap();
            (
                info.file_name.clone(),
                info.file_size,
                info.sender_ip.clone(),
                info.transfer_port,
                info.sender_id.clone(),
            )
        };

        let save_path = if let Some(p) = custom_save_path {
            PathBuf::from(p)
        } else {
            get_default_download_path(&file_name)
        };

        {
            let mut info = session.info.write().unwrap();
            info.save_path = Some(save_path.to_string_lossy().to_string());
            info.status = "transferring".to_string();
        }
        self.update_message_file_status(transfer_id, "transferring", Some(&save_path.to_string_lossy()));

        let cfg = self.config.read().unwrap().clone();
        let packet = NetworkPacket::FileDecision {
            transfer_id: transfer_id.to_string(),
            sender_id: cfg.user_id,
            recipient_id: sender_id,
            accepted: true,
        };
        self.send_packet(&packet, Some(&sender_ip));

        let engine_clone = self.clone();
        let t_id = transfer_id.to_string();
        std::thread::spawn(move || {
            engine_clone.run_download_worker(t_id, sender_ip, transfer_port, file_size, save_path);
        });

        Ok(())
    }

    pub fn decline_file_transfer(&self, transfer_id: &str) -> Result<(), String> {
        let (sender_ip, sender_id) = {
            let guard = self.active_transfers.read().unwrap();
            if let Some(session) = guard.get(transfer_id) {
                let mut info = session.info.write().unwrap();
                info.status = "declined".to_string();
                (info.sender_ip.clone(), info.sender_id.clone())
            } else {
                return Err("Transfer session not found.".to_string());
            }
        };

        self.update_message_file_status(transfer_id, "declined", None);

        let cfg = self.config.read().unwrap().clone();
        let packet = NetworkPacket::FileDecision {
            transfer_id: transfer_id.to_string(),
            sender_id: cfg.user_id,
            recipient_id: sender_id,
            accepted: false,
        };
        self.send_packet(&packet, Some(&sender_ip));

        Ok(())
    }

    pub fn cancel_file_transfer(&self, transfer_id: &str) -> Result<(), String> {
        let (target_ip, target_id) = {
            let guard = self.active_transfers.read().unwrap();
            if let Some(session) = guard.get(transfer_id) {
                session.cancel_flag.store(true, Ordering::Relaxed);
                let mut info = session.info.write().unwrap();
                info.status = "cancelled".to_string();
                let peer_id = if info.direction == "outgoing" {
                    info.recipient_id.clone()
                } else {
                    info.sender_id.clone()
                };
                (info.sender_ip.clone(), peer_id)
            } else {
                return Err("Transfer session not found.".to_string());
            }
        };

        self.update_message_file_status(transfer_id, "cancelled", None);

        let cfg = self.config.read().unwrap().clone();
        let packet = NetworkPacket::FileCancel {
            transfer_id: transfer_id.to_string(),
            sender_id: cfg.user_id,
            recipient_id: target_id,
        };
        self.send_packet(&packet, Some(&target_ip));

        Ok(())
    }

    pub fn get_file_transfers(&self) -> Vec<FileTransferInfo> {
        let guard = self.active_transfers.read().unwrap();
        let mut list: Vec<FileTransferInfo> = guard
            .values()
            .map(|s| s.info.read().unwrap().clone())
            .collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    fn handle_tcp_sender_stream(
        stream: &mut TcpStream,
        transfers: Arc<RwLock<HashMap<String, Arc<TransferSession>>>>,
        engine_opt: Option<Arc<ChatEngine>>,
    ) {
        let mut reader = BufReader::new(&mut *stream);
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }

        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.len() < 2 || parts[0] != "TRANSFER" {
            let _ = stream.write_all(b"ERR Invalid Command\n");
            return;
        }

        let transfer_id = parts[1];
        let offset: u64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

        let session = {
            let guard = transfers.read().unwrap();
            guard.get(transfer_id).cloned()
        };

        let session = match session {
            Some(s) => s,
            None => {
                let _ = stream.write_all(b"ERR Transfer Not Found\n");
                return;
            }
        };

        let file_path = {
            let info = session.info.read().unwrap();
            info.file_path.clone()
        };

        let file_path = match file_path {
            Some(p) => PathBuf::from(p),
            None => {
                let _ = stream.write_all(b"ERR File Path Unavailable\n");
                return;
            }
        };

        let mut file = match fs::File::open(&file_path) {
            Ok(f) => f,
            Err(e) => {
                let _ = stream.write_all(format!("ERR Open Failed: {}\n", e).as_bytes());
                return;
            }
        };

        let total_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        if offset > 0 {
            let _ = file.seek(SeekFrom::Start(offset));
        }

        if stream.write_all(format!("OK {}\n", total_size).as_bytes()).is_err() {
            return;
        }

        {
            let mut info = session.info.write().unwrap();
            info.status = "transferring".to_string();
            info.bytes_transferred = offset;
        }

        let mut buf = vec![0u8; 512 * 1024]; // 512 KB streaming buffer
        let mut transferred = offset;
        let mut last_speed_check = Instant::now();
        let mut bytes_since_check = 0u64;

        loop {
            if session.cancel_flag.load(Ordering::Relaxed) {
                let mut info = session.info.write().unwrap();
                info.status = "cancelled".to_string();
                break;
            }

            let n = match file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    let mut info = session.info.write().unwrap();
                    info.status = "failed".to_string();
                    info.error = Some(e.to_string());
                    break;
                }
            };

            if let Err(e) = stream.write_all(&buf[..n]) {
                let mut info = session.info.write().unwrap();
                info.status = "failed".to_string();
                info.error = Some(e.to_string());
                break;
            }

            transferred += n as u64;
            bytes_since_check += n as u64;

            let elapsed = last_speed_check.elapsed();
            if elapsed >= Duration::from_millis(500) {
                let speed = (bytes_since_check as f64 / elapsed.as_secs_f64()) as u64;
                let rem_bytes = total_size.saturating_sub(transferred);
                let eta = if speed > 0 { rem_bytes / speed } else { 0 };
                let prog = if total_size > 0 {
                    (transferred as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                };

                let mut info = session.info.write().unwrap();
                info.bytes_transferred = transferred;
                info.speed_bps = speed;
                info.eta_sec = eta;
                info.progress = prog;

                last_speed_check = Instant::now();
                bytes_since_check = 0;
            }
        }

        let _ = stream.flush();

        if transferred >= total_size && total_size > 0 {
            {
                let mut info = session.info.write().unwrap();
                info.status = "completed".to_string();
                info.bytes_transferred = total_size;
                info.progress = 100.0;
                info.speed_bps = 0;
                info.eta_sec = 0;
                info.completed_at = Some(Utc::now());
            }
            if let Some(ref engine) = engine_opt {
                engine.update_message_file_status(transfer_id, "completed", None);
                if engine.config.read().unwrap().desktop_notifications {
                    crate::win32::show_desktop_notification(
                        "✔ File Sent Successfully",
                        &format!("Delivered to peer ({}).", format_size(total_size)),
                    );
                }
            }
        }
    }

    fn run_download_worker(
        &self,
        transfer_id: String,
        sender_ip: String,
        transfer_port: u16,
        expected_size: u64,
        save_path: PathBuf,
    ) {
        let session = {
            let guard = self.active_transfers.read().unwrap();
            guard.get(&transfer_id).cloned()
        };

        let session = match session {
            Some(s) => s,
            None => return,
        };

        let part_path = PathBuf::from(format!("{}.part", save_path.to_string_lossy()));

        let res = (|| -> Result<(), std::io::Error> {
            let addr = format!("{}:{}", sender_ip, transfer_port);
            let socket_addr: SocketAddr = addr.parse().map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Invalid address: {}", e))
            })?;

            let mut stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(12))?;
            stream.set_read_timeout(Some(Duration::from_secs(30)))?;

            let req = format!("TRANSFER {} 0\n", transfer_id);
            stream.write_all(req.as_bytes())?;

            let mut reader = BufReader::new(&mut stream);
            let mut resp_line = String::new();
            reader.read_line(&mut resp_line)?;

            if !resp_line.starts_with("OK") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Peer rejected transfer: {}", resp_line.trim()),
                ));
            }

            if let Some(parent) = part_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let file = fs::File::create(&part_path)?;
            let mut writer = BufWriter::with_capacity(1024 * 1024, file);

            let mut buf = vec![0u8; 512 * 1024]; // 512 KB buffer for maximum throughput
            let mut transferred = 0u64;
            let mut last_speed_check = Instant::now();
            let mut bytes_since_check = 0u64;

            loop {
                if session.cancel_flag.load(Ordering::Relaxed) {
                    drop(writer);
                    let _ = fs::remove_file(&part_path);
                    let mut info = session.info.write().unwrap();
                    info.status = "cancelled".to_string();
                    return Ok(());
                }

                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock => {
                        if expected_size > 0 && transferred >= expected_size {
                            break;
                        }
                        return Err(e);
                    }
                    Err(e) => return Err(e),
                };

                writer.write_all(&buf[..n])?;
                transferred += n as u64;
                bytes_since_check += n as u64;

                let elapsed = last_speed_check.elapsed();
                if elapsed >= Duration::from_millis(500) {
                    let speed = (bytes_since_check as f64 / elapsed.as_secs_f64()) as u64;
                    let rem_bytes = expected_size.saturating_sub(transferred);
                    let eta = if speed > 0 { rem_bytes / speed } else { 0 };
                    let prog = if expected_size > 0 {
                        (transferred as f64 / expected_size as f64) * 100.0
                    } else {
                        0.0
                    };

                    let mut info = session.info.write().unwrap();
                    info.bytes_transferred = transferred;
                    info.speed_bps = speed;
                    info.eta_sec = eta;
                    info.progress = prog;

                    last_speed_check = Instant::now();
                    bytes_since_check = 0;
                }

                if expected_size > 0 && transferred >= expected_size {
                    break;
                }
            }

            writer.flush()?;
            drop(writer);

            if fs::rename(&part_path, &save_path).is_err() {
                fs::copy(&part_path, &save_path)?;
                let _ = fs::remove_file(&part_path);
            }

            {
                let mut info = session.info.write().unwrap();
                info.status = "completed".to_string();
                info.bytes_transferred = expected_size;
                info.progress = 100.0;
                info.speed_bps = 0;
                info.eta_sec = 0;
                info.completed_at = Some(Utc::now());
            }

            self.update_message_file_status(&transfer_id, "completed", Some(&save_path.to_string_lossy()));

            if self.config.read().unwrap().desktop_notifications {
                let fname = save_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string());
                crate::win32::show_desktop_notification(
                    "✔ File Download Complete",
                    &format!("Saved {} to Downloads.", fname),
                );
            }

            Ok(())
        })();

        if let Err(e) = res {
            let _ = fs::remove_file(&part_path);
            let mut info = session.info.write().unwrap();
            info.status = "failed".to_string();
            info.error = Some(e.to_string());
            self.update_message_file_status(&transfer_id, "failed", None);

            if self.config.read().unwrap().desktop_notifications {
                crate::win32::show_desktop_notification(
                    "❌ File Download Failed",
                    &format!("Error downloading file: {}", e),
                );
            }
        }
    }
}
