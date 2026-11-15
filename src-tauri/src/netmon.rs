use crate::manager::ProcessManager;
use crate::win32::get_process_name_map;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::Networking::WinSock::{
    getnameinfo, AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6,
};
use windows_sys::Win32::System::Threading::{
    GetProcessIoCounters, OpenProcess, IO_COUNTERS, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConnectionItem {
    pub id: String,
    pub protocol: String,
    pub local_ip: String,
    pub local_port: u16,
    pub local_addr: String,
    pub remote_ip: String,
    pub remote_port: u16,
    pub remote_addr: String,
    pub remote_domain: String,
    pub state: String,
    pub pid: u32,
    pub process_name: String,
    pub service_name: Option<String>,
    pub category: String, // "Local", "LAN", "Internet"
    pub app_download_speed: u64,
    pub app_upload_speed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppNetworkSummary {
    pub process_name: String,
    pub pids: Vec<u32>,
    pub total_connections: usize,
    pub established_count: usize,
    pub listening_count: usize,
    pub remote_domains: Vec<String>,
    pub service_name: Option<String>,
    pub download_speed: u64,          // Live Bytes/sec ↓
    pub upload_speed: u64,            // Live Bytes/sec ↑
    pub total_session_download: u64,  // Cumulative Session Download Bytes
    pub total_session_upload: u64,    // Cumulative Session Upload Bytes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceNetworkSummary {
    pub service_name: String,
    pub pid: Option<u32>,
    pub status: String,
    pub listening_ports: Vec<u16>,
    pub active_connections: usize,
    pub remote_domains: Vec<String>,
    pub download_speed: u64,          // Live Bytes/sec ↓
    pub upload_speed: u64,            // Live Bytes/sec ↑
    pub total_session_download: u64,  // Cumulative Session Download Bytes
    pub total_session_upload: u64,    // Cumulative Session Upload Bytes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainNetworkSummary {
    pub domain: String,
    pub ips: Vec<String>,
    pub total_connections: usize,
    pub apps: Vec<String>,
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkThroughputInfo {
    pub download_bytes_per_sec: u64,
    pub upload_bytes_per_sec: u64,
    pub total_session_download: u64,
    pub total_session_upload: u64,
    pub active_interfaces_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMonitorSnapshot {
    pub connections: Vec<NetworkConnectionItem>,
    pub apps_summary: Vec<AppNetworkSummary>,
    pub services_summary: Vec<ServiceNetworkSummary>,
    pub domains_summary: Vec<DomainNetworkSummary>,
    pub throughput: NetworkThroughputInfo,
    pub total_established: usize,
    pub total_listening: usize,
    pub total_remote_hosts: usize,
    pub top_consumer_app: Option<String>,
    pub top_consumer_bytes: u64,
}

struct DnsCacheEntry {
    domain: String,
    resolved_at: Instant,
}

#[derive(Debug, Clone, Default)]
struct AppTrafficStats {
    current_download_rate: u64,
    current_upload_rate: u64,
    total_session_download: u64,
    total_session_upload: u64,
}

#[derive(Debug, Clone)]
struct PidTrafficSample {
    last_read: u64,
    last_write: u64,
    last_time: Instant,
}

pub struct NetworkMonitorEngine {
    pm: Arc<ProcessManager>,
    dns_cache: Arc<RwLock<HashMap<String, DnsCacheEntry>>>,
    pending_dns: Arc<RwLock<HashSet<String>>>,
    last_sample_time: Arc<RwLock<Instant>>,
    last_in_octets: Arc<AtomicU64>,
    last_out_octets: Arc<AtomicU64>,
    current_download_rate: Arc<AtomicU64>,
    current_upload_rate: Arc<AtomicU64>,
    total_session_download: Arc<AtomicU64>,
    total_session_upload: Arc<AtomicU64>,
    active_interfaces_count: Arc<AtomicU64>,
    app_traffic: Arc<RwLock<HashMap<String, AppTrafficStats>>>,
    pid_traffic: Arc<RwLock<HashMap<u32, PidTrafficSample>>>,
}

impl NetworkMonitorEngine {
    pub fn new(pm: Arc<ProcessManager>) -> Self {
        Self {
            pm,
            dns_cache: Arc::new(RwLock::new(HashMap::new())),
            pending_dns: Arc::new(RwLock::new(HashSet::new())),
            last_sample_time: Arc::new(RwLock::new(Instant::now())),
            last_in_octets: Arc::new(AtomicU64::new(0)),
            last_out_octets: Arc::new(AtomicU64::new(0)),
            current_download_rate: Arc::new(AtomicU64::new(0)),
            current_upload_rate: Arc::new(AtomicU64::new(0)),
            total_session_download: Arc::new(AtomicU64::new(0)),
            total_session_upload: Arc::new(AtomicU64::new(0)),
            active_interfaces_count: Arc::new(AtomicU64::new(0)),
            app_traffic: Arc::new(RwLock::new(HashMap::new())),
            pid_traffic: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn start(&self) {
        // Initialize initial if_table octets
        let (init_in, init_out, init_count) = sample_interface_octets();
        self.last_in_octets.store(init_in, Ordering::SeqCst);
        self.last_out_octets.store(init_out, Ordering::SeqCst);
        self.active_interfaces_count.store(init_count as u64, Ordering::SeqCst);

        let last_time_clone = self.last_sample_time.clone();
        let last_in_clone = self.last_in_octets.clone();
        let last_out_clone = self.last_out_octets.clone();
        let cur_dl_clone = self.current_download_rate.clone();
        let cur_ul_clone = self.current_upload_rate.clone();
        let tot_dl_clone = self.total_session_download.clone();
        let tot_ul_clone = self.total_session_upload.clone();
        let if_count_clone = self.active_interfaces_count.clone();

        let app_traffic_clone = self.app_traffic.clone();
        let pid_traffic_clone = self.pid_traffic.clone();

        std::thread::Builder::new()
            .name("netmon-throughput-sampler".to_string())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(1000));

                    // 1. Adapter Bandwidth Sample
                    let (now_in, now_out, if_count) = sample_interface_octets();
                    if_count_clone.store(if_count as u64, Ordering::SeqCst);

                    let mut last_t = last_time_clone.write().unwrap();
                    let now = Instant::now();
                    let elapsed_secs = now.duration_since(*last_t).as_secs_f64();
                    *last_t = now;

                    if elapsed_secs > 0.05 {
                        let prev_in = last_in_clone.swap(now_in, Ordering::SeqCst);
                        let prev_out = last_out_clone.swap(now_out, Ordering::SeqCst);

                        if prev_in > 0 && now_in >= prev_in {
                            let delta_in = now_in - prev_in;
                            let rate_in = (delta_in as f64 / elapsed_secs) as u64;
                            cur_dl_clone.store(rate_in, Ordering::SeqCst);
                            tot_dl_clone.fetch_add(delta_in, Ordering::SeqCst);
                        } else if prev_in > 0 && now_in < prev_in {
                            let delta_in = (u32::MAX as u64 - prev_in) + now_in;
                            if delta_in < 100_000_000 {
                                let rate_in = (delta_in as f64 / elapsed_secs) as u64;
                                cur_dl_clone.store(rate_in, Ordering::SeqCst);
                                tot_dl_clone.fetch_add(delta_in, Ordering::SeqCst);
                            }
                        }

                        if prev_out > 0 && now_out >= prev_out {
                            let delta_out = now_out - prev_out;
                            let rate_out = (delta_out as f64 / elapsed_secs) as u64;
                            cur_ul_clone.store(rate_out, Ordering::SeqCst);
                            tot_ul_clone.fetch_add(delta_out, Ordering::SeqCst);
                        } else if prev_out > 0 && now_out < prev_out {
                            let delta_out = (u32::MAX as u64 - prev_out) + now_out;
                            if delta_out < 100_000_000 {
                                let rate_out = (delta_out as f64 / elapsed_secs) as u64;
                                cur_ul_clone.store(rate_out, Ordering::SeqCst);
                                tot_ul_clone.fetch_add(delta_out, Ordering::SeqCst);
                            }
                        }
                    }

                    // 2. Per-Process Network & I/O Consumption Tracking (GlassWire style)
                    sample_active_processes_traffic(&app_traffic_clone, &pid_traffic_clone, now);
                }
            })
            .ok();
    }

    pub fn get_snapshot(&self) -> NetworkMonitorSnapshot {
        let proc_map = get_process_name_map();
        let summary = self.pm.get_summary();
        let services = summary.services;

        // Build PID -> Service mapping and Port -> Service mapping
        let mut pid_to_service: HashMap<u32, String> = HashMap::new();
        let mut port_to_service: HashMap<u16, String> = HashMap::new();

        for s in &services {
            if let Some(pid) = s.pid {
                pid_to_service.insert(pid, s.name.clone());
                // Also map child processes if any
                for child_pid in crate::win32::get_child_pids(pid) {
                    pid_to_service.insert(child_pid, s.name.clone());
                }
            }
            if let Some(port) = s.port {
                if port > 0 {
                    port_to_service.insert(port, s.name.clone());
                }
            }
        }

        let app_traffic_cache = self.app_traffic.read().unwrap().clone();

        let mut connections: Vec<NetworkConnectionItem> = Vec::new();
        let mut unresolved_ips: Vec<IpAddr> = Vec::new();

        // 1. Query IPv4 TCP
        self.collect_tcp_ipv4(&proc_map, &pid_to_service, &port_to_service, &app_traffic_cache, &mut connections, &mut unresolved_ips);

        // 2. Query IPv6 TCP
        self.collect_tcp_ipv6(&proc_map, &pid_to_service, &port_to_service, &app_traffic_cache, &mut connections, &mut unresolved_ips);

        // 3. Query IPv4 UDP
        self.collect_udp_ipv4(&proc_map, &pid_to_service, &port_to_service, &app_traffic_cache, &mut connections);

        // 4. Query IPv6 UDP
        self.collect_udp_ipv6(&proc_map, &pid_to_service, &port_to_service, &app_traffic_cache, &mut connections);

        // Trigger background optimistic reverse DNS for any unresolved external IPs
        self.schedule_async_dns_resolutions(unresolved_ips);

        // Compute Aggregations
        let mut total_established = 0;
        let mut total_listening = 0;
        let mut remote_hosts_set = HashSet::new();

        let mut app_map: HashMap<String, (Vec<u32>, usize, usize, usize, HashSet<String>, Option<String>)> = HashMap::new();
        let mut domain_map: HashMap<String, (HashSet<String>, usize, HashSet<String>, HashSet<u16>)> = HashMap::new();

        for conn in &connections {
            if conn.state == "ESTABLISHED" {
                total_established += 1;
            } else if conn.state == "LISTENING" {
                total_listening += 1;
            }

            if !conn.remote_domain.is_empty() && conn.remote_domain != "*" && conn.remote_domain != "localhost" {
                remote_hosts_set.insert(conn.remote_domain.clone());
            }

            // Aggregate by App
            let entry = app_map.entry(conn.process_name.clone()).or_insert_with(|| {
                (Vec::new(), 0, 0, 0, HashSet::new(), conn.service_name.clone())
            });
            if !entry.0.contains(&conn.pid) && conn.pid > 0 {
                entry.0.push(conn.pid);
            }
            entry.1 += 1; // total
            if conn.state == "ESTABLISHED" {
                entry.2 += 1;
            } else if conn.state == "LISTENING" {
                entry.3 += 1;
            }
            if !conn.remote_domain.is_empty() && conn.remote_domain != "*" {
                entry.4.insert(conn.remote_domain.clone());
            }
            if entry.5.is_none() && conn.service_name.is_some() {
                entry.5 = conn.service_name.clone();
            }

            // Aggregate by Domain
            if !conn.remote_domain.is_empty() && conn.remote_domain != "*" {
                let d_entry = domain_map.entry(conn.remote_domain.clone()).or_insert_with(|| {
                    (HashSet::new(), 0, HashSet::new(), HashSet::new())
                });
                if !conn.remote_ip.is_empty() && conn.remote_ip != "*" {
                    d_entry.0.insert(conn.remote_ip.clone());
                }
                d_entry.1 += 1;
                d_entry.2.insert(conn.process_name.clone());
                if conn.remote_port > 0 {
                    d_entry.3.insert(conn.remote_port);
                }
            }
        }

        // Build Apps Summary list with GlassWire bandwidth & total consumption stats
        let mut apps_summary: Vec<AppNetworkSummary> = app_map
            .into_iter()
            .map(|(proc_name, (pids, total, established, listening, domains, service))| {
                let mut d_list: Vec<String> = domains.into_iter().collect();
                d_list.sort();

                let (dl_speed, ul_speed, tot_dl, tot_ul) = if let Some(stats) = app_traffic_cache.get(&proc_name) {
                    (
                        stats.current_download_rate,
                        stats.current_upload_rate,
                        stats.total_session_download,
                        stats.total_session_upload,
                    )
                } else {
                    (0, 0, 0, 0)
                };

                AppNetworkSummary {
                    process_name: proc_name,
                    pids,
                    total_connections: total,
                    established_count: established,
                    listening_count: listening,
                    remote_domains: d_list,
                    service_name: service,
                    download_speed: dl_speed,
                    upload_speed: ul_speed,
                    total_session_download: tot_dl,
                    total_session_upload: tot_ul,
                }
            })
            .collect();

        // Sort by highest data consumption first (like GlassWire), then by total connections
        apps_summary.sort_by(|a, b| {
            let a_total = a.total_session_download + a.total_session_upload;
            let b_total = b.total_session_download + b.total_session_upload;
            b_total.cmp(&a_total).then_with(|| b.total_connections.cmp(&a.total_connections))
        });

        // Identify Top Consumer App
        let top_consumer = apps_summary.first().filter(|a| (a.total_session_download + a.total_session_upload) > 0);
        let top_consumer_app = top_consumer.map(|a| a.process_name.clone());
        let top_consumer_bytes = top_consumer.map(|a| a.total_session_download + a.total_session_upload).unwrap_or(0);

        // Build Services Summary list with live bandwidth and total data usage
        let mut services_summary: Vec<ServiceNetworkSummary> = Vec::new();
        for s in &services {
            let mut listening_ports = Vec::new();
            let mut active_conns = 0;
            let mut rem_domains = HashSet::new();

            let mut svc_dl_speed = 0u64;
            let mut svc_ul_speed = 0u64;
            let mut svc_tot_dl = 0u64;
            let mut svc_tot_ul = 0u64;

            if let Some(pid) = s.pid {
                if let Some(pname) = proc_map.get(&pid) {
                    if let Some(stats) = app_traffic_cache.get(pname) {
                        svc_dl_speed += stats.current_download_rate;
                        svc_ul_speed += stats.current_upload_rate;
                        svc_tot_dl += stats.total_session_download;
                        svc_tot_ul += stats.total_session_upload;
                    }
                }
            }

            for c in &connections {
                if c.service_name.as_deref() == Some(&s.name) {
                    if c.state == "LISTENING" {
                        if !listening_ports.contains(&c.local_port) {
                            listening_ports.push(c.local_port);
                        }
                    } else if c.state == "ESTABLISHED" {
                        active_conns += 1;
                    }
                    if !c.remote_domain.is_empty() && c.remote_domain != "*" && c.remote_domain != "localhost" {
                        rem_domains.insert(c.remote_domain.clone());
                    }
                }
            }

            services_summary.push(ServiceNetworkSummary {
                service_name: s.name.clone(),
                pid: s.pid,
                status: s.status.clone(),
                listening_ports,
                active_connections: active_conns,
                remote_domains: rem_domains.into_iter().collect(),
                download_speed: svc_dl_speed,
                upload_speed: svc_ul_speed,
                total_session_download: svc_tot_dl,
                total_session_upload: svc_tot_ul,
            });
        }
        services_summary.sort_by(|a, b| b.active_connections.cmp(&a.active_connections));

        // Build Domains Summary list
        let mut domains_summary: Vec<DomainNetworkSummary> = domain_map
            .into_iter()
            .map(|(domain, (ips, total, apps, ports))| {
                let mut ip_list: Vec<String> = ips.into_iter().collect();
                ip_list.sort();
                let mut app_list: Vec<String> = apps.into_iter().collect();
                app_list.sort();
                let mut port_list: Vec<u16> = ports.into_iter().collect();
                port_list.sort();
                DomainNetworkSummary {
                    domain,
                    ips: ip_list,
                    total_connections: total,
                    apps: app_list,
                    ports: port_list,
                }
            })
            .collect();
        domains_summary.sort_by(|a, b| b.total_connections.cmp(&a.total_connections));

        let throughput = NetworkThroughputInfo {
            download_bytes_per_sec: self.current_download_rate.load(Ordering::SeqCst),
            upload_bytes_per_sec: self.current_upload_rate.load(Ordering::SeqCst),
            total_session_download: self.total_session_download.load(Ordering::SeqCst),
            total_session_upload: self.total_session_upload.load(Ordering::SeqCst),
            active_interfaces_count: self.active_interfaces_count.load(Ordering::SeqCst) as usize,
        };

        NetworkMonitorSnapshot {
            connections,
            apps_summary,
            services_summary,
            domains_summary,
            throughput,
            total_established,
            total_listening,
            total_remote_hosts: remote_hosts_set.len(),
            top_consumer_app,
            top_consumer_bytes,
        }
    }

    fn resolve_domain_or_default(&self, ip: &IpAddr, unresolved_out: &mut Vec<IpAddr>) -> (String, String) {
        match ip {
            IpAddr::V4(v4) => {
                if v4.is_unspecified() {
                    return ("*".to_string(), "Local".to_string());
                }
                if v4.is_loopback() {
                    return ("localhost".to_string(), "Local".to_string());
                }
                if v4.is_private() {
                    return (format!("LAN ({})", v4), "LAN".to_string());
                }
                if v4.is_broadcast() {
                    return ("Broadcast".to_string(), "LAN".to_string());
                }
            }
            IpAddr::V6(v6) => {
                if v6.is_unspecified() {
                    return ("*".to_string(), "Local".to_string());
                }
                if v6.is_loopback() {
                    return ("localhost".to_string(), "Local".to_string());
                }
            }
        }

        let ip_str = ip.to_string();
        {
            let cache = self.dns_cache.read().unwrap();
            if let Some(entry) = cache.get(&ip_str) {
                if entry.resolved_at.elapsed() < Duration::from_secs(3600) {
                    return (entry.domain.clone(), "Internet".to_string());
                }
            }
        }

        // Needs background reverse resolution
        unresolved_out.push(*ip);
        (ip_str, "Internet".to_string())
    }

    fn schedule_async_dns_resolutions(&self, ips: Vec<IpAddr>) {
        if ips.is_empty() {
            return;
        }

        let mut to_resolve = Vec::new();
        {
            let mut pending = self.pending_dns.write().unwrap();
            let cache = self.dns_cache.read().unwrap();
            for ip in ips {
                let s = ip.to_string();
                if !pending.contains(&s) && !cache.contains_key(&s) {
                    pending.insert(s);
                    to_resolve.push(ip);
                }
            }
        }

        if to_resolve.is_empty() {
            return;
        }

        let cache_clone = self.dns_cache.clone();
        let pending_clone = self.pending_dns.clone();

        std::thread::Builder::new()
            .name("netmon-dns-resolver".to_string())
            .spawn(move || {
                for ip in to_resolve {
                    let ip_str = ip.to_string();
                    let domain_opt = reverse_dns_lookup(&ip);
                    let final_domain = domain_opt.unwrap_or_else(|| ip_str.clone());

                    {
                        let mut cache = cache_clone.write().unwrap();
                        cache.insert(
                            ip_str.clone(),
                            DnsCacheEntry {
                                domain: final_domain,
                                resolved_at: Instant::now(),
                            },
                        );
                    }

                    {
                        let mut pending = pending_clone.write().unwrap();
                        pending.remove(&ip_str);
                    }
                }
            })
            .ok();
    }

    fn collect_tcp_ipv4(
        &self,
        proc_map: &HashMap<u32, String>,
        pid_to_service: &HashMap<u32, String>,
        port_to_service: &HashMap<u16, String>,
        app_traffic: &HashMap<String, AppTrafficStats>,
        connections: &mut Vec<NetworkConnectionItem>,
        unresolved_ips: &mut Vec<IpAddr>,
    ) {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }
        if size == 0 {
            return;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut _,
                &mut size,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if ret == 0 {
            unsafe {
                let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
                let entries = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);

                for row in entries {
                    let local_ip = IpAddr::V4(Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()));
                    let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
                    let remote_ip = IpAddr::V4(Ipv4Addr::from(row.dwRemoteAddr.to_ne_bytes()));
                    let remote_port = u16::from_be((row.dwRemotePort & 0xFFFF) as u16);
                    let pid = row.dwOwningPid;
                    let state = tcp_state_to_str(row.dwState).to_string();

                    let process_name = proc_map
                        .get(&pid)
                        .cloned()
                        .unwrap_or_else(|| if pid == 0 { "System Idle".to_string() } else if pid == 4 { "System".to_string() } else { "unknown.exe".to_string() });

                    let service_name = pid_to_service
                        .get(&pid)
                        .cloned()
                        .or_else(|| port_to_service.get(&local_port).cloned());

                    let (remote_domain, category) = if state == "LISTENING" {
                        ("*".to_string(), "Local".to_string())
                    } else {
                        self.resolve_domain_or_default(&remote_ip, unresolved_ips)
                    };

                    let remote_addr_str = if state == "LISTENING" || remote_port == 0 {
                        "*:*".to_string()
                    } else {
                        format!("{}:{}", remote_ip, remote_port)
                    };

                    let (dl_speed, ul_speed) = app_traffic
                        .get(&process_name)
                        .map(|s| (s.current_download_rate, s.current_upload_rate))
                        .unwrap_or((0, 0));

                    let id = format!("tcp4-{}:{}-{}:{}-{}", local_ip, local_port, remote_ip, remote_port, pid);

                    connections.push(NetworkConnectionItem {
                        id,
                        protocol: "TCP".to_string(),
                        local_ip: local_ip.to_string(),
                        local_port,
                        local_addr: format!("{}:{}", local_ip, local_port),
                        remote_ip: remote_ip.to_string(),
                        remote_port,
                        remote_addr: remote_addr_str,
                        remote_domain,
                        state,
                        pid,
                        process_name,
                        service_name,
                        category,
                        app_download_speed: dl_speed,
                        app_upload_speed: ul_speed,
                    });
                }
            }
        }
    }

    fn collect_tcp_ipv6(
        &self,
        proc_map: &HashMap<u32, String>,
        pid_to_service: &HashMap<u32, String>,
        port_to_service: &HashMap<u16, String>,
        app_traffic: &HashMap<String, AppTrafficStats>,
        connections: &mut Vec<NetworkConnectionItem>,
        unresolved_ips: &mut Vec<IpAddr>,
    ) {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET6 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }
        if size == 0 {
            return;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut _,
                &mut size,
                0,
                AF_INET6 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if ret == 0 {
            unsafe {
                let table = &*(buf.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID);
                let entries = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);

                for row in entries {
                    let local_ip = IpAddr::V6(Ipv6Addr::from(row.ucLocalAddr));
                    let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
                    let remote_ip = IpAddr::V6(Ipv6Addr::from(row.ucRemoteAddr));
                    let remote_port = u16::from_be((row.dwRemotePort & 0xFFFF) as u16);
                    let pid = row.dwOwningPid;
                    let state = tcp_state_to_str(row.dwState).to_string();

                    let process_name = proc_map
                        .get(&pid)
                        .cloned()
                        .unwrap_or_else(|| if pid == 0 { "System Idle".to_string() } else if pid == 4 { "System".to_string() } else { "unknown.exe".to_string() });

                    let service_name = pid_to_service
                        .get(&pid)
                        .cloned()
                        .or_else(|| port_to_service.get(&local_port).cloned());

                    let (remote_domain, category) = if state == "LISTENING" {
                        ("*".to_string(), "Local".to_string())
                    } else {
                        self.resolve_domain_or_default(&remote_ip, unresolved_ips)
                    };

                    let remote_addr_str = if state == "LISTENING" || remote_port == 0 {
                        "*:*".to_string()
                    } else {
                        format!("[{}]:{}", remote_ip, remote_port)
                    };

                    let (dl_speed, ul_speed) = app_traffic
                        .get(&process_name)
                        .map(|s| (s.current_download_rate, s.current_upload_rate))
                        .unwrap_or((0, 0));

                    let id = format!("tcp6-[{}]:{}-[{}]:{}-{}", local_ip, local_port, remote_ip, remote_port, pid);

                    connections.push(NetworkConnectionItem {
                        id,
                        protocol: "TCPv6".to_string(),
                        local_ip: local_ip.to_string(),
                        local_port,
                        local_addr: format!("[{}]:{}", local_ip, local_port),
                        remote_ip: remote_ip.to_string(),
                        remote_port,
                        remote_addr: remote_addr_str,
                        remote_domain,
                        state,
                        pid,
                        process_name,
                        service_name,
                        category,
                        app_download_speed: dl_speed,
                        app_upload_speed: ul_speed,
                    });
                }
            }
        }
    }

    fn collect_udp_ipv4(
        &self,
        proc_map: &HashMap<u32, String>,
        pid_to_service: &HashMap<u32, String>,
        port_to_service: &HashMap<u16, String>,
        app_traffic: &HashMap<String, AppTrafficStats>,
        connections: &mut Vec<NetworkConnectionItem>,
    ) {
        let mut size = 0u32;
        unsafe {
            GetExtendedUdpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET as u32,
                UDP_TABLE_OWNER_PID,
                0,
            );
        }
        if size == 0 {
            return;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedUdpTable(
                buf.as_mut_ptr() as *mut _,
                &mut size,
                0,
                AF_INET as u32,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };

        if ret == 0 {
            unsafe {
                let table = &*(buf.as_ptr() as *const MIB_UDPTABLE_OWNER_PID);
                let entries = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);

                for row in entries {
                    let local_ip = IpAddr::V4(Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()));
                    let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
                    let pid = row.dwOwningPid;

                    let process_name = proc_map
                        .get(&pid)
                        .cloned()
                        .unwrap_or_else(|| if pid == 0 { "System Idle".to_string() } else if pid == 4 { "System".to_string() } else { "unknown.exe".to_string() });

                    let service_name = pid_to_service
                        .get(&pid)
                        .cloned()
                        .or_else(|| port_to_service.get(&local_port).cloned());

                    let (dl_speed, ul_speed) = app_traffic
                        .get(&process_name)
                        .map(|s| (s.current_download_rate, s.current_upload_rate))
                        .unwrap_or((0, 0));

                    let id = format!("udp4-{}:{}-{}", local_ip, local_port, pid);

                    connections.push(NetworkConnectionItem {
                        id,
                        protocol: "UDP".to_string(),
                        local_ip: local_ip.to_string(),
                        local_port,
                        local_addr: format!("{}:{}", local_ip, local_port),
                        remote_ip: "*".to_string(),
                        remote_port: 0,
                        remote_addr: "*:*".to_string(),
                        remote_domain: "*".to_string(),
                        state: "LISTENING".to_string(),
                        pid,
                        process_name,
                        service_name,
                        category: "Local".to_string(),
                        app_download_speed: dl_speed,
                        app_upload_speed: ul_speed,
                    });
                }
            }
        }
    }

    fn collect_udp_ipv6(
        &self,
        proc_map: &HashMap<u32, String>,
        pid_to_service: &HashMap<u32, String>,
        port_to_service: &HashMap<u16, String>,
        app_traffic: &HashMap<String, AppTrafficStats>,
        connections: &mut Vec<NetworkConnectionItem>,
    ) {
        let mut size = 0u32;
        unsafe {
            GetExtendedUdpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET6 as u32,
                UDP_TABLE_OWNER_PID,
                0,
            );
        }
        if size == 0 {
            return;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedUdpTable(
                buf.as_mut_ptr() as *mut _,
                &mut size,
                0,
                AF_INET6 as u32,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };

        if ret == 0 {
            unsafe {
                let table = &*(buf.as_ptr() as *const MIB_UDP6TABLE_OWNER_PID);
                let entries = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);

                for row in entries {
                    let local_ip = IpAddr::V6(Ipv6Addr::from(row.ucLocalAddr));
                    let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
                    let pid = row.dwOwningPid;

                    let process_name = proc_map
                        .get(&pid)
                        .cloned()
                        .unwrap_or_else(|| if pid == 0 { "System Idle".to_string() } else if pid == 4 { "System".to_string() } else { "unknown.exe".to_string() });

                    let service_name = pid_to_service
                        .get(&pid)
                        .cloned()
                        .or_else(|| port_to_service.get(&local_port).cloned());

                    let (dl_speed, ul_speed) = app_traffic
                        .get(&process_name)
                        .map(|s| (s.current_download_rate, s.current_upload_rate))
                        .unwrap_or((0, 0));

                    let id = format!("udp6-[{}]:{}-{}", local_ip, local_port, pid);

                    connections.push(NetworkConnectionItem {
                        id,
                        protocol: "UDPv6".to_string(),
                        local_ip: local_ip.to_string(),
                        local_port,
                        local_addr: format!("[{}]:{}", local_ip, local_port),
                        remote_ip: "*".to_string(),
                        remote_port: 0,
                        remote_addr: "*:*".to_string(),
                        remote_domain: "*".to_string(),
                        state: "LISTENING".to_string(),
                        pid,
                        process_name,
                        service_name,
                        category: "Local".to_string(),
                        app_download_speed: dl_speed,
                        app_upload_speed: ul_speed,
                    });
                }
            }
        }
    }
}

fn sample_active_processes_traffic(
    app_traffic: &Arc<RwLock<HashMap<String, AppTrafficStats>>>,
    pid_traffic: &Arc<RwLock<HashMap<u32, PidTrafficSample>>>,
    now: Instant,
) {
    let proc_map = get_process_name_map();
    let network_pids = get_active_network_pids();

    let mut current_pid_deltas: HashMap<u32, (u64, u64)> = HashMap::new();

    {
        let mut p_map = pid_traffic.write().unwrap();

        for pid in network_pids {
            if let Some((curr_r, curr_w)) = sample_process_io(pid) {
                if let Some(entry) = p_map.get_mut(&pid) {
                    let elapsed = now.duration_since(entry.last_time).as_secs_f64();
                    if elapsed > 0.1 {
                        let delta_r = curr_r.saturating_sub(entry.last_read);
                        let delta_w = curr_w.saturating_sub(entry.last_write);
                        let r_speed = (delta_r as f64 / elapsed) as u64;
                        let w_speed = (delta_w as f64 / elapsed) as u64;

                        current_pid_deltas.insert(pid, (r_speed, w_speed));
                        entry.last_read = curr_r;
                        entry.last_write = curr_w;
                        entry.last_time = now;
                    }
                } else {
                    p_map.insert(
                        pid,
                        PidTrafficSample {
                            last_read: curr_r,
                            last_write: curr_w,
                            last_time: now,
                        },
                    );
                }
            }
        }
    }

    // Aggregate PID deltas to process names
    let mut app_map = app_traffic.write().unwrap();

    // Reset current active rates for all known apps to 0 first (in case an app went idle)
    for stats in app_map.values_mut() {
        stats.current_download_rate = 0;
        stats.current_upload_rate = 0;
    }

    for (pid, (r_speed, w_speed)) in current_pid_deltas {
        if let Some(proc_name) = proc_map.get(&pid) {
            let stats = app_map.entry(proc_name.clone()).or_insert_with(AppTrafficStats::default);
            stats.current_download_rate += r_speed;
            stats.current_upload_rate += w_speed;
            stats.total_session_download += r_speed; // accumulates 1s slice
            stats.total_session_upload += w_speed;
        }
    }
}

fn sample_process_io(pid: u32) -> Option<(u64, u64)> {
    if pid <= 4 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut counters: IO_COUNTERS = std::mem::zeroed();
        let ret = GetProcessIoCounters(handle, &mut counters);
        CloseHandle(handle);
        if ret != 0 {
            Some((counters.ReadTransferCount, counters.WriteTransferCount))
        } else {
            None
        }
    }
}

fn get_active_network_pids() -> HashSet<u32> {
    let mut pids = HashSet::new();
    let mut size = 0u32;
    unsafe {
        GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_ALL, 0);
        if size > 0 {
            let mut buf = vec![0u8; size as usize];
            if GetExtendedTcpTable(buf.as_mut_ptr() as *mut _, &mut size, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_ALL, 0) == 0 {
                let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
                let entries = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);
                for row in entries {
                    if row.dwOwningPid > 4 {
                        pids.insert(row.dwOwningPid);
                    }
                }
            }
        }

        size = 0;
        GetExtendedUdpTable(std::ptr::null_mut(), &mut size, 0, AF_INET as u32, UDP_TABLE_OWNER_PID, 0);
        if size > 0 {
            let mut buf = vec![0u8; size as usize];
            if GetExtendedUdpTable(buf.as_mut_ptr() as *mut _, &mut size, 0, AF_INET as u32, UDP_TABLE_OWNER_PID, 0) == 0 {
                let table = &*(buf.as_ptr() as *const MIB_UDPTABLE_OWNER_PID);
                let entries = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);
                for row in entries {
                    if row.dwOwningPid > 4 {
                        pids.insert(row.dwOwningPid);
                    }
                }
            }
        }
    }
    pids
}

fn tcp_state_to_str(state: u32) -> &'static str {
    match state {
        1 => "CLOSED",
        2 => "LISTENING",
        3 => "SYN_SENT",
        4 => "SYN_RCVD",
        5 => "ESTABLISHED",
        6 => "FIN_WAIT1",
        7 => "FIN_WAIT2",
        8 => "CLOSE_WAIT",
        9 => "CLOSING",
        10 => "LAST_ACK",
        11 => "TIME_WAIT",
        12 => "DELETE_TCB",
        _ => "UNKNOWN",
    }
}

fn sample_interface_octets() -> (u64, u64, usize) {
    let mut size = 0u32;
    unsafe {
        GetIfTable(std::ptr::null_mut(), &mut size, 0);
    }
    if size == 0 {
        return (0, 0, 0);
    }

    let mut buf = vec![0u8; size as usize];
    let ret = unsafe { GetIfTable(buf.as_mut_ptr() as *mut _, &mut size, 0) };
    if ret != 0 {
        return (0, 0, 0);
    }

    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut if_count: usize = 0;

    unsafe {
        let table = &*(buf.as_ptr() as *const MIB_IFTABLE);
        let entries = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);

        for row in entries {
            if row.dwType != 24 && row.dwOperStatus == 1 {
                total_in += row.dwInOctets as u64;
                total_out += row.dwOutOctets as u64;
                if_count += 1;
            }
        }
    }

    (total_in, total_out, if_count)
}

fn reverse_dns_lookup(ip: &IpAddr) -> Option<String> {
    match ip {
        IpAddr::V4(v4) => {
            let mut host_buf = [0i8; 256];
            let mut sin: SOCKADDR_IN = unsafe { std::mem::zeroed() };
            sin.sin_family = AF_INET as u16;
            sin.sin_addr = windows_sys::Win32::Networking::WinSock::IN_ADDR {
                S_un: windows_sys::Win32::Networking::WinSock::IN_ADDR_0 {
                    S_addr: u32::from_ne_bytes(v4.octets()),
                },
            };

            let res = unsafe {
                getnameinfo(
                    &sin as *const _ as *const _,
                    std::mem::size_of_val(&sin) as i32,
                    host_buf.as_mut_ptr() as *mut _,
                    host_buf.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    0,
                )
            };

            if res == 0 {
                let cstr = unsafe { std::ffi::CStr::from_ptr(host_buf.as_ptr() as *const _) };
                if let Ok(s) = cstr.to_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() && trimmed != v4.to_string() {
                        return Some(trimmed.to_string());
                    }
                }
            }
            None
        }
        IpAddr::V6(v6) => {
            let mut host_buf = [0i8; 256];
            let mut sin6: SOCKADDR_IN6 = unsafe { std::mem::zeroed() };
            sin6.sin6_family = AF_INET6 as u16;
            sin6.sin6_addr = windows_sys::Win32::Networking::WinSock::IN6_ADDR {
                u: windows_sys::Win32::Networking::WinSock::IN6_ADDR_0 {
                    Byte: v6.octets(),
                },
            };

            let res = unsafe {
                getnameinfo(
                    &sin6 as *const _ as *const _,
                    std::mem::size_of_val(&sin6) as i32,
                    host_buf.as_mut_ptr() as *mut _,
                    host_buf.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    0,
                )
            };

            if res == 0 {
                let cstr = unsafe { std::ffi::CStr::from_ptr(host_buf.as_ptr() as *const _) };
                if let Ok(s) = cstr.to_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() && trimmed != v6.to_string() {
                        return Some(trimmed.to_string());
                    }
                }
            }
            None
        }
    }
}
