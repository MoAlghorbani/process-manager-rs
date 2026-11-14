// ========================================================
// Process Manager (Rust Desktop GUI) — Master Controller
// ========================================================

const invoke = (cmd, args = {}) => {
  if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  console.warn("Tauri core invoke not available for", cmd);
  return Promise.resolve(null);
};

// Global State
let currentTab = "tab-services";
let selectedGroup = "ALL";
let selectedService = "";
let lastLogText = "";
let lastFilterQuery = "";
let isLogUserPaused = false;
let isResizingSplit = false;
let startY = 0;
let startH = 0;
let activeDownloadId = null;

// ========================================================
// 1. INITIALIZATION & TAB SWITCHING
// ========================================================

document.addEventListener("DOMContentLoaded", () => {
  setupNavigation();
  setupSplitHandle();
  setupServices();
  setupPortKillerTab();
  setupJiggler();
  setupVault();
  setupTasks();
  setupDownloader();
  setupStorageManager();
  setupLanChat();
  setupMedia();

  // Background Telemetry Tickers
  setInterval(pollServices, 1500);
  setInterval(pollLogs, 1500);
  setInterval(pollJiggler, 1000);
  setInterval(pollDownloader, 1000);
  setInterval(pollChatMessages, 1200);
  setInterval(pollChatPeers, 3000);
  setInterval(() => {
    if (currentTab === "tab-ports") {
      const chk = document.getElementById("chk-auto-refresh-ports");
      if (!chk || chk.checked) {
        loadPortKillerTab();
      }
    }
  }, 2500);

  // Initial Data Fetch
  loadStorageHeaderInfo();
  pollServices();
  pollJiggler();
  loadCredentials();
  loadClipboard();
  loadTasks();
  loadDownloader();
  loadChatConfig();
  pollChatPeers();
  pollChatMessages();
});

window.switchTab = function (tabId) {
  const btn = document.querySelector(`.nav-tab[data-tab="${tabId}"]`);
  if (btn) {
    btn.click();
  }
};

function setupNavigation() {
  document.querySelectorAll(".nav-tab").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".nav-tab").forEach((b) => b.classList.remove("active"));
      document.querySelectorAll(".tab-pane").forEach((p) => p.classList.remove("active"));

      btn.classList.add("active");
      currentTab = btn.getAttribute("data-tab");
      const targetPane = document.getElementById(currentTab);
      if (targetPane) targetPane.classList.add("active");

      if (currentTab === "tab-services") {
        pollServices();
        pollLogs();
      } else if (currentTab === "tab-ports") {
        loadPortKillerTab();
      } else if (currentTab === "tab-vault") {
        loadCredentials();
        loadClipboard();
      } else if (currentTab === "tab-tasks") {
        loadTasks();
      } else if (currentTab === "tab-downloader") {
        loadDownloader();
      } else if (currentTab === "tab-chat") {
        totalUnreadChatCount = 0;
        updateChatUnreadBadges();
        pollChatPeers();
        pollChatMessages(true);
      } else if (currentTab === "tab-media") {
        loadMediaTab();
      }
    });
  });

  // Vault Subtabs
  document.querySelectorAll(".sub-tab").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".sub-tab").forEach((b) => b.classList.remove("active"));
      document.querySelectorAll(".subtab-pane").forEach((p) => p.classList.remove("active"));

      btn.classList.add("active");
      const target = btn.getAttribute("data-subtab");
      const pane = document.getElementById(target);
      if (pane) pane.classList.add("active");
    });
  });

  // Admin badge elevation click
  const adminBadge = document.getElementById("admin-badge");
  if (adminBadge) {
    adminBadge.addEventListener("click", async () => {
      if (adminBadge.classList.contains("elevated")) return;
      if (confirm("Relaunch Process Manager with Windows Administrator privileges (UAC)?")) {
        await invoke("request_elevation");
      }
    });
  }
}

// ========================================================
// 2. SERVICES SUPERVISOR & RESIZABLE SPLIT
// ========================================================

function setupSplitHandle() {
  const handle = document.getElementById("split-handle");
  const logsPanel = document.getElementById("logs-panel");

  handle.addEventListener("mousedown", (e) => {
    isResizingSplit = true;
    startY = e.clientY;
    startH = logsPanel.offsetHeight;
    document.body.style.cursor = "row-resize";
    e.preventDefault();
  });

  window.addEventListener("mousemove", (e) => {
    if (!isResizingSplit) return;
    const dy = startY - e.clientY;
    const newH = Math.max(80, Math.min(window.innerHeight * 0.85, startH + dy));
    logsPanel.style.height = `${newH}px`;
  });

  window.addEventListener("mouseup", () => {
    if (isResizingSplit) {
      isResizingSplit = false;
      document.body.style.cursor = "";
    }
  });

  // Quick Preset Buttons
  document.getElementById("btn-logs-collapse").addEventListener("click", () => {
    logsPanel.style.height = "85px";
  });
  document.getElementById("btn-logs-half").addEventListener("click", () => {
    logsPanel.style.height = "50%";
  });
  document.getElementById("btn-logs-expand").addEventListener("click", () => {
    logsPanel.style.height = "75%";
  });
}

function setupServices() {
  document.getElementById("btn-start-all").addEventListener("click", () => invoke("start_all"));
  document.getElementById("btn-stop-all").addEventListener("click", () => invoke("stop_all"));
  document.getElementById("btn-restart-all").addEventListener("click", () => invoke("restart_all"));

  // Log Controls
  document.getElementById("btn-copy-logs").addEventListener("click", async () => {
    const text = document.getElementById("logs-pre").textContent;
    if (text) await invoke("copy_to_clipboard", { text });
  });

  document.getElementById("btn-clear-logs").addEventListener("click", async () => {
    if (selectedService) {
      await invoke("clear_logs", { name: selectedService });
      pollLogs();
    }
  });

  document.getElementById("btn-open-live-cmd")?.addEventListener("click", async () => {
    if (selectedService) {
      try {
        await invoke("open_live_cmd", { name: selectedService });
      } catch (err) {
        console.error("Failed to launch Live CMD:", err);
      }
    }
  });

  document.getElementById("input-log-filter")?.addEventListener("input", () => {
    renderCurrentLogs(true);
  });

  document.getElementById("btn-log-bottom").addEventListener("click", () => {
    isLogUserPaused = false;
    document.getElementById("chk-autoscroll").checked = true;
    document.getElementById("logs-status-badge").textContent = "● LIVE";
    document.getElementById("logs-status-badge").className = "badge badge-emerald";
    pollLogs(true);
  });

  document.getElementById("chk-autoscroll").addEventListener("change", (e) => {
    isLogUserPaused = !e.target.checked;
    updateLogStatusBadge();
  });

  // Smart Log Scroll Freeze Guard
  const logPre = document.getElementById("logs-pre");
  logPre.addEventListener("scroll", () => {
    const atBottom = logPre.scrollTop + logPre.clientHeight >= logPre.scrollHeight - 35;
    if (atBottom) {
      if (isLogUserPaused) {
        isLogUserPaused = false;
        updateLogStatusBadge();
      }
    } else {
      if (!isLogUserPaused) {
        isLogUserPaused = true;
        updateLogStatusBadge();
      }
    }
  });

  // Add Service Modal
  const modalService = document.getElementById("modal-service");
  document.getElementById("btn-add-service").addEventListener("click", () => {
    document.getElementById("modal-service-title").textContent = "➕ Add New Service";
    document.getElementById("input-service-oldname").value = "";
    document.getElementById("input-service-name").value = "";
    document.getElementById("input-service-group").value = selectedGroup === "ALL" ? "default" : selectedGroup;
    document.getElementById("input-service-cmd").value = "";
    document.getElementById("input-service-cwd").value = "";
    document.getElementById("input-service-port").value = "";
    document.getElementById("input-service-venv").value = "";
    document.getElementById("input-service-shortcut").value = "";
    document.getElementById("chk-service-autorestart").checked = true;
    modalService.classList.remove("hidden");
  });

  // Interactive Shortcut Recorder in Service Modal
  const scInput = document.getElementById("input-service-shortcut");
  if (scInput) {
    let chordKeys = [];

    scInput.addEventListener("keydown", (e) => {
      if (e.key === "Tab" || e.key === "Escape") return;

      if (e.key === "Backspace" || e.key === "Delete") {
        scInput.value = "";
        chordKeys = [];
        return;
      }

      e.preventDefault();
      e.stopPropagation();

      const k = e.key.toUpperCase();
      if (["CONTROL", "ALT", "SHIFT", "META"].includes(k)) return;

      const mods = [];
      if (e.ctrlKey) mods.push("Ctrl");
      if (e.altKey) mods.push("Alt");
      if (e.shiftKey) mods.push("Shift");
      if (e.metaKey) mods.push("Win");

      if (
        chordKeys.length > 0 &&
        (e.altKey || e.ctrlKey || Date.now() - chordKeys[0].time < 1200)
      ) {
        chordKeys.push({ key: k, time: Date.now() });
      } else {
        chordKeys = [{ key: k, time: Date.now() }];
      }

      const seq = chordKeys.map((item) => item.key).join("+");
      scInput.value = mods.length > 0 ? `${mods.join("+")}+${seq}` : seq;
    });

    scInput.addEventListener("focus", () => {
      chordKeys = [];
    });
  }
  document.getElementById("btn-cancel-service").addEventListener("click", () => modalService.classList.add("hidden"));

  document.getElementById("btn-detect-venv").addEventListener("click", async () => {
    const cwd = document.getElementById("input-service-cwd").value;
    if (cwd) {
      const venv = await invoke("detect_venv", { cwd });
      if (venv) {
        document.getElementById("input-service-venv").value = venv;
        alert(`Detected virtual environment: ${venv}`);
      } else {
        alert("No virtual environment (.venv, venv, env) found in directory.");
      }
    }
  });

  document.getElementById("btn-save-service").addEventListener("click", async () => {
    const oldName = document.getElementById("input-service-oldname").value;
    const name = document.getElementById("input-service-name").value.trim();
    const group = document.getElementById("input-service-group").value.trim() || "default";
    const command = document.getElementById("input-service-cmd").value.trim();
    const cwd = document.getElementById("input-service-cwd").value.trim();
    const portVal = parseInt(document.getElementById("input-service-port").value, 10);
    const port = isNaN(portVal) ? null : portVal;
    const venv = document.getElementById("input-service-venv").value.trim() || null;
    const shortcut = document.getElementById("input-service-shortcut").value.trim() || null;
    const auto_restart = document.getElementById("chk-service-autorestart").checked;

    if (!name || !command) {
      alert("Service name and command are required.");
      return;
    }

    const cfg = { name, group, command, cwd, port, auto_restart, venv, shortcut };

    if (oldName) {
      await invoke("update_service", { oldName, config: cfg, restart: true });
    } else {
      await invoke("add_service", { config: cfg, autoStart: false });
    }

    modalService.classList.add("hidden");
    pollServices();
  });

  document.getElementById("input-service-cmd")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      document.getElementById("btn-save-service")?.click();
    }
  });

  // Manage Groups Modal
  const modalGroups = document.getElementById("modal-groups");
  document.getElementById("btn-manage-groups").addEventListener("click", () => {
    openGroupsModal();
    modalGroups.classList.remove("hidden");
  });
  document.getElementById("btn-close-groups-modal").addEventListener("click", () => modalGroups.classList.add("hidden"));
  document.getElementById("btn-done-groups").addEventListener("click", () => modalGroups.classList.add("hidden"));

  // Open Web Links / Ports Modal
  const modalOpenPorts = document.getElementById("modal-open-ports");
  document.getElementById("btn-open-ports").addEventListener("click", async () => {
    await renderOpenPortsModal();
    modalOpenPorts.classList.remove("hidden");
  });
  document.getElementById("btn-close-open-ports").addEventListener("click", () => modalOpenPorts.classList.add("hidden"));
  document.getElementById("btn-cancel-open-ports").addEventListener("click", () => modalOpenPorts.classList.add("hidden"));

  document.getElementById("btn-open-all-ports").addEventListener("click", async () => {
    const summary = await invoke("get_manager_summary");
    if (!summary || !summary.services) return;
    const runningWithPorts = summary.services.filter((s) => s.port && s.status === "running");
    for (const s of runningWithPorts) {
      await window.openServiceUrl(s.port);
      await new Promise((r) => setTimeout(r, 120));
    }
  });

  // Port Killer Button in Services Toolbar - Switch to Tab
  const btnPortKiller = document.getElementById("btn-port-killer");
  if (btnPortKiller) {
    btnPortKiller.addEventListener("click", () => {
      window.switchTab("tab-ports");
    });
  }

  // Port Killer Modal (fallback support)
  const modalPortKiller = document.getElementById("modal-port-killer");
  if (modalPortKiller) {
    const btnClose = document.getElementById("btn-close-port-killer");
    if (btnClose) btnClose.addEventListener("click", () => modalPortKiller.classList.add("hidden"));
    const btnCancel = document.getElementById("btn-cancel-port-killer");
    if (btnCancel) btnCancel.addEventListener("click", () => modalPortKiller.classList.add("hidden"));
    const btnRefresh = document.getElementById("btn-refresh-ports");
    if (btnRefresh) btnRefresh.addEventListener("click", loadActivePorts);

    const btnExec = document.getElementById("btn-execute-kill-port");
    if (btnExec) {
      btnExec.addEventListener("click", async () => {
        const inputEl = document.getElementById("input-kill-port");
        const portVal = parseInt(inputEl.value, 10);
        const feedback = document.getElementById("port-kill-feedback");
        if (isNaN(portVal) || portVal <= 0 || portVal > 65535) {
          if (feedback) {
            feedback.textContent = "Please enter a valid port number (1-65535).";
            feedback.className = "port-kill-feedback error";
            feedback.classList.remove("hidden");
          }
          return;
        }

        try {
          const killedCount = await invoke("kill_port_task", { port: portVal });
          if (feedback) {
            if (killedCount > 0) {
              feedback.textContent = `⚡ Success! Terminated ${killedCount} process(es) listening on port :${portVal}.`;
              feedback.className = "port-kill-feedback success";
            } else {
              feedback.textContent = `ℹ️ Port :${portVal} is not occupied by any active process.`;
              feedback.className = "port-kill-feedback info";
            }
            feedback.classList.remove("hidden");
          }
          showToast(killedCount > 0 ? `⚡ Freed port :${portVal} (${killedCount} process killed)` : `Port :${portVal} was already free`, killedCount > 0 ? "success" : "info");
          await refreshAllPortViews();
        } catch (err) {
          if (feedback) {
            feedback.textContent = `Error: ${err}`;
            feedback.className = "port-kill-feedback error";
            feedback.classList.remove("hidden");
          }
        }
      });
    }

    const inputSearch = document.getElementById("input-search-ports");
    if (inputSearch) {
      inputSearch.addEventListener("input", (e) => {
        renderActivePortsTable(e.target.value);
      });
    }
  }
}

function updateLogStatusBadge() {
  const badge = document.getElementById("logs-status-badge");
  if (isLogUserPaused) {
    badge.textContent = "⏸ PAUSED";
    badge.className = "badge badge-admin";
  } else {
    badge.textContent = "● LIVE";
    badge.className = "badge badge-emerald";
  }
}

async function pollServices() {
  if (currentTab !== "tab-services") return;
  const summary = await invoke("get_manager_summary");
  if (!summary) return;

  // Elevation Badge
  const adminBadge = document.getElementById("admin-badge");
  if (summary.admin) {
    adminBadge.textContent = "[ADMIN: ELEVATED]";
    adminBadge.className = "badge badge-admin elevated";
  } else {
    adminBadge.textContent = "[USER: STANDARD]";
    adminBadge.className = "badge badge-admin";
  }

  // Group Pills
  renderGroupPills(summary.groups, summary.services.length);

  // Filter Services
  const filtered = selectedGroup === "ALL"
    ? summary.services
    : summary.services.filter((s) => s.group.toLowerCase() === selectedGroup.toLowerCase());

  renderServicesTable(filtered);
}

function renderGroupPills(groups, totalCount) {
  const container = document.getElementById("group-pills");
  let html = `<button class="pill ${selectedGroup === "ALL" ? "active" : ""}" data-group="ALL">🌐 ALL (${totalCount})</button>`;

  groups.forEach((g) => {
    const isAct = selectedGroup.toLowerCase() === g.name.toLowerCase();
    html += `<button class="pill ${isAct ? "active" : ""}" data-group="${g.name}">📁 ${g.name} (${g.running}/${g.total})</button>`;
  });

  container.innerHTML = html;

  container.querySelectorAll(".pill").forEach((btn) => {
    btn.addEventListener("click", () => {
      selectedGroup = btn.getAttribute("data-group");
      pollServices();
    });
  });
}

function renderServicesTable(services) {
  const tbody = document.getElementById("services-tbody");
  if (services.length === 0) {
    tbody.innerHTML = `<tr><td colspan="7" style="text-align: center; color: var(--text-muted); padding: 30px;">No registered services in this group. Click <strong>➕ Add Service</strong> to register one.</td></tr>`;
    return;
  }

  let html = "";
  services.forEach((s) => {
    const isRunning = s.status === "running";
    const statusClass = `status-${s.status}`;
    const statusDot = isRunning ? "● RUNNING" : s.status === "stopped" ? "● STOPPED" : `● ${s.status.toUpperCase()}`;
    const portBadge = s.port
      ? `<button class="port-link-badge" onclick="window.openServiceUrl(${s.port})" title="Open http://localhost:${s.port} in browser">:${s.port} ↗</button>`
      : `<span style="color: var(--text-dim);">-</span>`;
    const portPid = `${portBadge} / ${s.pid ? `<code>${s.pid}</code>` : `<span style="color: var(--text-dim);">-</span>`}`;
    const ramRestarts = `${s.memory_mb > 0 ? `${s.memory_mb} MB` : "-"} (${s.restarts}r)`;
    const uptime = s.uptime_seconds > 0 ? `${s.uptime_seconds}s` : "-";

    html += `
      <tr>
        <td>
          <div class="service-name-cell">
            <span class="service-name">${s.name}</span>
            ${s.shortcut ? `<span class="shortcut-badge">${s.shortcut}</span>` : ""}
          </div>
        </td>
        <td><span class="badge badge-subtle">${s.group}</span></td>
        <td><span class="status-dot ${statusClass}">${statusDot}</span></td>
        <td>${portPid}</td>
        <td><code>${ramRestarts}</code></td>
        <td>${uptime}</td>
        <td style="text-align: right;">
          <div style="display: inline-flex; gap: 4px;">
            ${s.port ? `
              <button class="btn btn-sm btn-sky" onclick="window.openServiceUrl(${s.port})" title="Open http://localhost:${s.port} in browser">
                🌐 Open
              </button>
            ` : ""}
            <button class="btn btn-sm ${isRunning ? "btn-rose" : "btn-emerald"}" onclick="window.toggleService('${s.name}', ${isRunning})">
              ${isRunning ? "⏹ Stop" : "▶ Start"}
            </button>
            <button class="btn btn-sm btn-secondary" onclick="window.restartService('${s.name}')">🔄</button>
            <button class="btn btn-sm btn-secondary" onclick="window.inspectLogs('${s.name}')">📜</button>
            <button class="btn btn-sm btn-secondary" onclick="window.editService('${s.name}')">✏️</button>
            <button class="btn btn-sm btn-rose" onclick="window.deleteService('${s.name}')">🗑️</button>
          </div>
        </td>
      </tr>
    `;
  });

  tbody.innerHTML = html;
}

async function renderOpenPortsModal() {
  const summary = await invoke("get_manager_summary");
  const container = document.getElementById("open-ports-list");
  const btnOpenAll = document.getElementById("btn-open-all-ports");
  if (!summary || !summary.services) return;

  const withPorts = summary.services.filter((s) => s.port);
  const runningWithPorts = withPorts.filter((s) => s.status === "running");

  btnOpenAll.textContent = `🚀 Open All Running (${runningWithPorts.length})`;
  btnOpenAll.disabled = runningWithPorts.length === 0;

  if (withPorts.length === 0) {
    container.innerHTML = `<div style="text-align: center; color: var(--text-muted); padding: 25px;">No services have configured ports. Edit a service to assign a port.</div>`;
    return;
  }

  let html = "";
  withPorts.forEach((s) => {
    const isRunning = s.status === "running";
    const statusClass = `status-${s.status}`;
    const statusText = isRunning ? "RUNNING" : s.status.toUpperCase();

    html += `
      <div class="port-card">
        <div class="port-card-info">
          <div class="port-card-title">
            <span>${s.name}</span>
            <span class="badge badge-subtle">${s.group}</span>
            <span class="status-dot ${statusClass}">● ${statusText}</span>
          </div>
          <div class="port-card-url">http://localhost:${s.port}</div>
        </div>
        <div style="display: flex; gap: 6px;">
          <button class="btn btn-sm btn-sky" onclick="window.openServiceUrl(${s.port})" title="Open in default browser">
            🌐 Open ↗
          </button>
          <button class="btn btn-sm btn-rose" onclick="window.killPortDirectly(${s.port})" title="Kill any process occupying port :${s.port}">
            ⚡ Free Port
          </button>
        </div>
      </div>
    `;
  });

  container.innerHTML = html;
}

// ==========================================
// Port Killer State & Renderers (Dedicated Tab & Modal)
// ==========================================

let activePortsCache = [];
let servicesCacheForPorts = [];
let portFilterMode = "all";
let portSearchQuery = "";

async function refreshAllPortViews() {
  await Promise.allSettled([
    loadActivePorts(),
    loadPortKillerTab(),
    pollServices(),
  ]);
}

function setupPortKillerTab() {
  // 1. Quick Kill on Port
  const inputEl = document.getElementById("tab-input-kill-port");
  const btnKill = document.getElementById("tab-btn-execute-kill");
  const feedback = document.getElementById("tab-port-kill-feedback");

  const executeTabPortKill = async () => {
    if (!inputEl) return;
    const portVal = parseInt(inputEl.value, 10);
    if (isNaN(portVal) || portVal <= 0 || portVal > 65535) {
      if (feedback) {
        feedback.textContent = "Please enter a valid port number (1-65535).";
        feedback.className = "port-kill-feedback error";
        feedback.classList.remove("hidden");
      }
      return;
    }

    try {
      const killedCount = await invoke("kill_port_task", { port: portVal });
      if (feedback) {
        if (killedCount > 0) {
          feedback.textContent = `⚡ Success! Terminated ${killedCount} process(es) listening on port :${portVal}.`;
          feedback.className = "port-kill-feedback success";
        } else {
          feedback.textContent = `ℹ️ Port :${portVal} is not occupied by any active process.`;
          feedback.className = "port-kill-feedback info";
        }
        feedback.classList.remove("hidden");
      }
      showToast(killedCount > 0 ? `⚡ Freed port :${portVal} (${killedCount} process killed)` : `Port :${portVal} was not occupied`, killedCount > 0 ? "success" : "info");
      await refreshAllPortViews();
    } catch (err) {
      if (feedback) {
        feedback.textContent = `Error: ${err}`;
        feedback.className = "port-kill-feedback error";
        feedback.classList.remove("hidden");
      }
      showToast(`Kill failed: ${err}`, "error");
    }
  };

  if (btnKill) {
    btnKill.addEventListener("click", executeTabPortKill);
  }
  if (inputEl) {
    inputEl.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        executeTabPortKill();
      }
    });
  }

  // 2. Filter Pills (All / Services / Web)
  const pillsContainer = document.getElementById("port-filter-pills");
  if (pillsContainer) {
    pillsContainer.querySelectorAll(".pill").forEach((btn) => {
      btn.addEventListener("click", () => {
        pillsContainer.querySelectorAll(".pill").forEach((p) => p.classList.remove("active"));
        btn.classList.add("active");
        portFilterMode = btn.getAttribute("data-filter") || "all";
        renderPortsTabTable();
      });
    });
  }

  // 3. Live Search Input
  const searchInput = document.getElementById("tab-input-search-ports");
  if (searchInput) {
    searchInput.addEventListener("input", (e) => {
      portSearchQuery = e.target.value;
      renderPortsTabTable();
    });
  }

  // 4. Manual Refresh Button
  const btnRefresh = document.getElementById("tab-btn-refresh-ports");
  if (btnRefresh) {
    btnRefresh.addEventListener("click", () => {
      loadPortKillerTab();
    });
  }
}

async function loadPortKillerTab() {
  try {
    const [ports, summary] = await Promise.all([
      invoke("get_active_ports"),
      invoke("get_manager_summary"),
    ]);

    activePortsCache = ports || [];
    servicesCacheForPorts = (summary && summary.services) ? summary.services.filter((s) => s.port > 0) : [];

    // Telemetry Statistics Calculations
    const activePortSet = new Set(activePortsCache.map((p) => p.port));
    const activeServiceCount = servicesCacheForPorts.filter((s) => activePortSet.has(s.port)).length;

    const elTotalPorts = document.getElementById("stat-total-ports");
    if (elTotalPorts) elTotalPorts.textContent = activePortsCache.length;

    const elServicePorts = document.getElementById("stat-service-ports");
    if (elServicePorts) elServicePorts.textContent = servicesCacheForPorts.length;

    const elActiveListeners = document.getElementById("stat-active-listeners");
    if (elActiveListeners) elActiveListeners.textContent = activeServiceCount;

    const countAllPorts = document.getElementById("count-all-ports");
    if (countAllPorts) countAllPorts.textContent = activePortsCache.length;

    const countServicePorts = document.getElementById("count-service-ports");
    if (countServicePorts) countServicePorts.textContent = servicesCacheForPorts.length;

    // Render TCP Listeners Table
    renderPortsTabTable();
  } catch (err) {
    console.error("Failed to load port killer tab data:", err);
  }
}

function renderPortsTabTable() {
  const tbody = document.getElementById("tab-ports-tbody");
  if (!tbody) return;

  const q = portSearchQuery.trim().toLowerCase();
  const servicePortMap = new Map();
  servicesCacheForPorts.forEach((s) => servicePortMap.set(s.port, s));

  const filtered = activePortsCache.filter((item) => {
    const matchedService = servicePortMap.get(item.port);

    // 1. Filter by mode
    if (portFilterMode === "services" && !matchedService) {
      return false;
    }
    if (portFilterMode === "web") {
      const isCommonWeb = item.port === 80 || item.port === 443 || item.port === 8080 || item.port === 8000;
      const isDevPortRange = item.port >= 3000 && item.port <= 9999;
      if (!isCommonWeb && !isDevPortRange) return false;
    }

    // 2. Filter by search query
    if (q) {
      const matchPort = item.port.toString().includes(q);
      const matchName = item.process_name.toLowerCase().includes(q);
      const matchPid = item.pid.toString().includes(q);
      const matchService = matchedService && matchedService.name.toLowerCase().includes(q);
      return matchPort || matchName || matchPid || matchService;
    }

    return true;
  });

  if (filtered.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-muted); padding: 24px;">${q ? "No matching listening ports found." : "No listening TCP ports match current filter."}</td></tr>`;
    return;
  }

  let html = "";
  filtered.forEach((p) => {
    const matchedService = servicePortMap.get(p.port);
    const serviceBadge = matchedService
      ? `<span class="badge badge-sky" title="Mapped to service: ${matchedService.name}">🎯 ${matchedService.name}</span>`
      : `<span style="color: var(--text-muted); font-size: 11px;">—</span>`;

    html += `
      <tr>
        <td><span class="port-link-badge">:${p.port}</span></td>
        <td><strong style="color: #fff;">${p.process_name}</strong></td>
        <td><code>${p.pid}</code></td>
        <td>${serviceBadge}</td>
        <td><span class="badge badge-emerald">LISTENING</span></td>
        <td style="text-align: right;">
          <div style="display: inline-flex; gap: 4px;">
            <button class="btn btn-sm btn-sky" onclick="window.openServiceUrl(${p.port})" title="Open http://localhost:${p.port}">
              🌐 Open
            </button>
            <button class="btn btn-sm btn-rose" onclick="window.killPortDirectly(${p.port})" title="Free port :${p.port}">
              ⚡ Free Port
            </button>
            ${p.pid > 0 ? `
              <button class="btn btn-sm btn-secondary" onclick="window.killPidDirectly(${p.pid}, '${p.process_name}')" title="Kill PID ${p.pid}">
                💀 PID
              </button>
            ` : ""}
          </div>
        </td>
      </tr>
    `;
  });

  tbody.innerHTML = html;
}

// Fallback Port Killer Modal Handlers
async function openPortKillerModal() {
  const feedback = document.getElementById("port-kill-feedback");
  if (feedback) feedback.classList.add("hidden");
  const inputKill = document.getElementById("input-kill-port");
  if (inputKill) inputKill.value = "";
  const inputSearch = document.getElementById("input-search-ports");
  if (inputSearch) inputSearch.value = "";
  await loadActivePorts();
}

async function loadActivePorts() {
  const tbody = document.getElementById("active-ports-tbody");
  if (!tbody) return;
  tbody.innerHTML = `<tr><td colspan="4" style="text-align: center; color: var(--text-muted); padding: 18px;">Scanning active listening ports...</td></tr>`;
  try {
    const ports = await invoke("get_active_ports");
    activePortsCache = ports || [];
    renderActivePortsTable("");
  } catch (err) {
    tbody.innerHTML = `<tr><td colspan="4" style="text-align: center; color: var(--rose); padding: 18px;">Failed to scan ports: ${err}</td></tr>`;
  }
}

function renderActivePortsTable(filterText = "") {
  const tbody = document.getElementById("active-ports-tbody");
  if (!tbody) return;
  const q = filterText.trim().toLowerCase();

  const filtered = activePortsCache.filter((item) => {
    if (!q) return true;
    return (
      item.port.toString().includes(q) ||
      item.process_name.toLowerCase().includes(q) ||
      item.pid.toString().includes(q)
    );
  });

  if (filtered.length === 0) {
    tbody.innerHTML = `<tr><td colspan="4" style="text-align: center; color: var(--text-muted); padding: 18px;">${q ? "No matching listening ports found." : "No listening TCP ports detected."}</td></tr>`;
    return;
  }

  let html = "";
  filtered.forEach((p) => {
    html += `
      <tr>
        <td><span class="port-link-badge">:${p.port}</span></td>
        <td><strong style="color: #fff;">${p.process_name}</strong></td>
        <td><code>${p.pid}</code></td>
        <td style="text-align: right;">
          <div style="display: inline-flex; gap: 4px;">
            <button class="btn btn-sm btn-sky" onclick="window.openServiceUrl(${p.port})" title="Open http://localhost:${p.port}">
              🌐
            </button>
            <button class="btn btn-sm btn-rose" onclick="window.killPortDirectly(${p.port})" title="Kill process tree on port :${p.port}">
              💀 Kill
            </button>
          </div>
        </td>
      </tr>
    `;
  });

  tbody.innerHTML = html;
}

window.killPortDirectly = async (port) => {
  if (!port) return;
  try {
    const count = await invoke("kill_port_task", { port: parseInt(port, 10) });
    if (count > 0) {
      showToast(`⚡ Freed port :${port} (${count} process killed)`, "success");
    } else {
      showToast(`Port :${port} was not occupied`, "info");
    }
    await refreshAllPortViews();
  } catch (err) {
    showToast(`Failed to free port :${port}: ${err}`, "error");
  }
};

window.killPidDirectly = async (pid, procName) => {
  if (!pid) return;
  try {
    const ok = await invoke("kill_process_by_pid", { pid: parseInt(pid, 10) });
    if (ok) {
      showToast(`💀 Terminated PID ${pid} (${procName})`, "success");
    }
    await refreshAllPortViews();
  } catch (err) {
    showToast(`Failed to terminate PID ${pid}: ${err}`, "error");
  }
};

window.toggleServiceByName = async (name) => {
  if (!name) return;
  try {
    await invoke("toggle_service", { name });
    await refreshAllPortViews();
  } catch (err) {
    showToast(`Failed to toggle service ${name}: ${err}`, "error");
  }
};

// ==========================================
// Toast Notification Engine
// ==========================================

function showToast(message, type = "info") {
  const container = document.getElementById("toast-container");
  if (!container) return;

  const el = document.createElement("div");
  el.className = `toast toast-${type}`;
  el.textContent = message;
  container.appendChild(el);

  setTimeout(() => {
    el.style.opacity = "0";
    el.style.transform = "translateY(12px) scale(0.95)";
    setTimeout(() => el.remove(), 260);
  }, 3200);
}

// Note: All global shortcuts & chords (e.g. Alt+C+2, Ctrl+Alt+1, etc.) are handled
// seamlessly at the OS level by the Rust Low-Level Keyboard Engine (hotkeys.rs).

// Global Service Actions
window.openServiceUrl = async (port) => {
  if (!port) return;
  const url = `http://localhost:${port}`;
  try {
    await invoke("open_url", { url });
  } catch (err) {
    console.error("Failed to open URL:", err);
  }
};
window.toggleService = async (name, isRunning) => {
  if (isRunning) {
    await invoke("stop_service", { name });
  } else {
    await invoke("start_service", { name });
  }
  pollServices();
};

window.restartService = async (name) => {
  await invoke("restart_service", { name });
  pollServices();
};

window.inspectLogs = (name) => {
  selectedService = name;
  lastLogText = "";
  document.getElementById("logs-title").textContent = `📜 Logs for '${name}'`;
  isLogUserPaused = false;
  document.getElementById("chk-autoscroll").checked = true;
  pollLogs(true);
};

window.editService = async (name) => {
  const summary = await invoke("get_manager_summary");
  const s = summary.services.find((it) => it.name === name);
  if (!s) return;

  document.getElementById("modal-service-title").textContent = `✏️ Edit Service '${name}'`;
  document.getElementById("input-service-oldname").value = s.name;
  document.getElementById("input-service-name").value = s.name;
  document.getElementById("input-service-group").value = s.group;
  document.getElementById("input-service-cmd").value = s.command;
  document.getElementById("input-service-cwd").value = s.cwd;
  document.getElementById("input-service-port").value = s.port || "";
  document.getElementById("input-service-venv").value = s.venv || "";
  document.getElementById("input-service-shortcut").value = s.shortcut || "";
  document.getElementById("chk-service-autorestart").checked = s.auto_restart;
  document.getElementById("modal-service").classList.remove("hidden");
};

window.deleteService = async (name) => {
  if (confirm(`Are you sure you want to delete service '${name}'?`)) {
    await invoke("delete_service", { name });
    if (selectedService === name) selectedService = "";
    pollServices();
  }
};

function escapeLogHtml(str) {
  return String(str || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function ansiToHtml(str) {
  const ansiRegex = /\x1b\[([0-9;]*)m/g;
  if (!ansiRegex.test(str)) {
    return null;
  }
  ansiRegex.lastIndex = 0;

  let openSpans = 0;
  let result = "";
  let lastIndex = 0;
  let match;

  while ((match = ansiRegex.exec(str)) !== null) {
    const rawText = str.slice(lastIndex, match.index);
    if (rawText) {
      result += escapeLogHtml(rawText);
    }
    lastIndex = match.index + match[0].length;

    const codes = match[1] ? match[1].split(";").map((n) => parseInt(n, 10)) : [0];
    for (let i = 0; i < codes.length; i++) {
      const code = codes[i];
      if (code === 0) {
        while (openSpans > 0) {
          result += "</span>";
          openSpans--;
        }
      } else if (code === 1) {
        result += '<span class="ansi-bold">';
        openSpans++;
      } else if (code === 2) {
        result += '<span class="ansi-dim">';
        openSpans++;
      } else if (code === 4) {
        result += '<span class="ansi-underline">';
        openSpans++;
      } else if (code >= 30 && code <= 37) {
        const colors = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
        result += `<span class="ansi-${colors[code - 30]}">`;
        openSpans++;
      } else if (code >= 90 && code <= 97) {
        const colors = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
        result += `<span class="ansi-bright-${colors[code - 90]}">`;
        openSpans++;
      } else if (code === 38 && codes[i + 1] === 5 && codes[i + 2] !== undefined) {
        const col = codes[i + 2];
        result += `<span style="color: var(--ansi-256-${col}, #38bdf8)">`;
        openSpans++;
        i += 2;
      } else if (code === 38 && codes[i + 1] === 2 && codes[i + 4] !== undefined) {
        const r = codes[i + 2], g = codes[i + 3], b = codes[i + 4];
        result += `<span style="color: rgb(${r},${g},${b})">`;
        openSpans++;
        i += 4;
      }
    }
  }

  const remaining = str.slice(lastIndex);
  if (remaining) {
    result += escapeLogHtml(remaining);
  }
  while (openSpans > 0) {
    result += "</span>";
    openSpans--;
  }

  return result;
}

function highlightPlainLogLine(line) {
  const isErrorLine = /^\s*(?:ERROR|FATAL|CRITICAL)\b/i.test(line);
  const isWarnLine = /^\s*WARN(?:ING)?\b/i.test(line);
  const isInfoLine = /^\s*INFO\b/i.test(line);

  let escaped = escapeLogHtml(line);

  // 1. Durations like [0.00ms] or (15.2ms)
  escaped = escaped.replace(/(\[\s*\d+(?:\.\d+)?(?:ms|s|µs|ns)\s*\])/g, '<span class="log-time">$1</span>');

  // 2. SQL Keywords & backticks & quotes
  if (/\b(SELECT|INSERT|UPDATE|DELETE|FROM|WHERE|JOIN|CREATE|ALTER|DROP|TABLE)\b/i.test(line)) {
    escaped = escaped.replace(/\b(SELECT|FROM|WHERE|ORDER\s+BY|GROUP\s+BY|DESC|ASC|LIMIT|OFFSET|AND|OR|LEFT\s+JOIN|RIGHT\s+JOIN|INNER\s+JOIN|JOIN|INSERT\s+INTO|VALUES|UPDATE|SET|DELETE\s+FROM|NOT\s+IN|IN|LIKE|ILIKE|BETWEEN|IS\s+NULL|IS\s+NOT\s+NULL|COUNT|MAX|MIN|AVG|SUM|HAVING|DISTINCT|ON|AS)\b/gi, '<span class="log-sql-kw">$1</span>');
    escaped = escaped.replace(/(`[^`]+`)/g, '<span class="log-sql-ident">$1</span>');
    escaped = escaped.replace(/('[^']*')/g, '<span class="log-sql-str">$1</span>');
  }

  // 3. Tree characters └ ├ │ ─
  escaped = escaped.replace(/([└├│─]+)/g, '<span class="log-tree">$1</span>');

  // 4. HTTP Methods and URLs
  escaped = escaped.replace(/\b(GET|POST|PUT|DELETE|PATCH|OPTIONS|HEAD)\s+(\/[a-zA-Z0-9_\-\.\/\?=&%#:@\+]*)/g, function(_, m, p) {
    return `<span class="log-http-${m.toLowerCase()}">${m}</span> <span class="log-route">${p}</span>`;
  });

  // 5. HTTP status codes
  escaped = escaped.replace(/\b(HTTP\s+([1-5]\d\d))\b/g, function(_, full, code) {
    const cls = code >= 400 ? 'log-http-status-err' : (code >= 300 ? 'log-http-status-warn' : 'log-http-status-ok');
    return `<span class="${cls}">${full}</span>`;
  });

  // 6. JSON keys: "key":
  escaped = escaped.replace(/("[\w\-]+")\s*:/g, '<span class="log-json-key">$1</span>:');

  // 7. Log levels
  escaped = escaped.replace(/\b(ERROR|FATAL|CRITICAL)\b/g, '<span class="log-level-error">$1</span>');
  escaped = escaped.replace(/\b(WARN|WARNING)\b/g, '<span class="log-level-warn">$1</span>');
  escaped = escaped.replace(/\b(INFO)\b/g, '<span class="log-level-info">$1</span>');
  escaped = escaped.replace(/\b(DEBUG)\b/g, '<span class="log-level-debug">$1</span>');
  escaped = escaped.replace(/\b(TRACE)\b/g, '<span class="log-level-trace">$1</span>');
  escaped = escaped.replace(/\b(SUCCESS)\b/g, '<span class="log-level-success">$1</span>');

  const lineClass = isErrorLine ? 'log-line log-line-error' : (isWarnLine ? 'log-line log-line-warn' : (isInfoLine ? 'log-line log-line-info' : 'log-line'));
  return `<span class="${lineClass}">${escaped}</span>`;
}

function formatTerminalLogs(rawText, filterQuery = "") {
  if (!rawText) return "";
  let lines = rawText.split("\n");
  if (filterQuery && filterQuery.trim()) {
    const q = filterQuery.trim().toLowerCase();
    lines = lines.filter((l) => l.toLowerCase().includes(q));
  }
  return lines.map((line) => {
    const ansiFormatted = ansiToHtml(line);
    if (ansiFormatted !== null) {
      return `<span class="log-line">${ansiFormatted}</span>`;
    }
    return highlightPlainLogLine(line);
  }).join("\n");
}

function renderCurrentLogs(forceRender = false, forceSnap = false) {
  const logPre = document.getElementById("logs-pre");
  if (!logPre) return;
  const filterInput = document.getElementById("input-log-filter");
  const filterQuery = filterInput ? filterInput.value : "";

  if (forceRender || lastFilterQuery !== filterQuery) {
    lastFilterQuery = filterQuery;
    logPre.innerHTML = formatTerminalLogs(lastLogText, filterQuery);
    if (!isLogUserPaused || forceSnap) {
      logPre.scrollTop = logPre.scrollHeight;
    }
  }
}

async function pollLogs(forceSnap = false) {
  if (!selectedService || currentTab !== "tab-services") return;
  const lines = parseInt(document.getElementById("select-log-lines").value, 10) || 250;

  // Text selection guard: don't overwrite if user is actively highlighting text
  if (window.getSelection && window.getSelection().toString()) {
    isLogUserPaused = true;
    updateLogStatusBadge();
    return;
  }

  const logs = await invoke("get_logs", { name: selectedService, lines });
  if (logs === null || logs === undefined) return;

  const logPre = document.getElementById("logs-pre");
  if (!logPre) return;
  const filterInput = document.getElementById("input-log-filter");
  const filterQuery = filterInput ? filterInput.value : "";

  if (logs !== lastLogText || filterQuery !== lastFilterQuery) {
    lastLogText = logs;
    lastFilterQuery = filterQuery;
    logPre.innerHTML = formatTerminalLogs(logs, filterQuery);

    // Update size badge
    const byteSize = new Blob([logs]).size;
    const sizeBadge = document.getElementById("logs-size-badge");
    if (sizeBadge) {
      if (byteSize > 1024 * 1024) {
        sizeBadge.textContent = `(${(byteSize / (1024 * 1024)).toFixed(1)} MB)`;
      } else if (byteSize > 1024) {
        sizeBadge.textContent = `(${(byteSize / 1024).toFixed(1)} KB)`;
      } else {
        sizeBadge.textContent = `(${byteSize} B)`;
      }
    }

    if (!isLogUserPaused || forceSnap) {
      logPre.scrollTop = logPre.scrollHeight;
    }
  }
}

async function openGroupsModal() {
  const summary = await invoke("get_manager_summary");
  const body = document.getElementById("modal-groups-body");

  let html = `<div style="display: flex; flex-direction: column; gap: 14px;">`;
  summary.groups.forEach((g) => {
    html += `
      <div class="card" style="background: rgba(30, 41, 59, 0.4); padding: 14px; border-radius: 8px;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
          <h4 style="font-size: 14px; color: var(--accent-cyan);">📁 ${g.name} (${g.running}/${g.total} Running)</h4>
          <div style="display: flex; gap: 6px;">
            <button class="btn btn-sm btn-emerald" onclick="window.groupAction('${g.name}', 'start')">▶ Start</button>
            <button class="btn btn-sm btn-rose" onclick="window.groupAction('${g.name}', 'stop')">⏹ Stop</button>
            <button class="btn btn-sm btn-secondary" onclick="window.groupAction('${g.name}', 'restart')">🔄</button>
            <button class="btn btn-sm btn-secondary" onclick="window.groupRename('${g.name}')">✏️ Rename</button>
            ${g.name.toLowerCase() !== "default" ? `<button class="btn btn-sm btn-rose" onclick="window.groupDissolve('${g.name}')">Dissolve</button>` : ""}
          </div>
        </div>
        <p style="font-size: 12px; color: var(--text-muted);">Services: <code>${g.services.join(", ") || "none"}</code></p>
      </div>
    `;
  });
  html += `</div>`;
  body.innerHTML = html;
}

window.groupAction = async (group, action) => {
  if (action === "start") await invoke("start_group", { group });
  if (action === "stop") await invoke("stop_group", { group });
  if (action === "restart") await invoke("restart_group", { group });
  openGroupsModal();
  pollServices();
};

window.groupRename = async (oldGroup) => {
  const newName = prompt(`Enter new name for group '${oldGroup}':`, oldGroup);
  if (newName && newName.trim() && newName.trim() !== oldGroup) {
    await invoke("rename_group", { oldGroup, newGroup: newName.trim() });
    openGroupsModal();
    pollServices();
  }
};

window.groupDissolve = async (group) => {
  if (confirm(`Dissolve group '${group}'? Member services will be safely reassigned to 'default'.`)) {
    await invoke("dissolve_group", { group });
    openGroupsModal();
    pollServices();
  }
};

// ========================================================
// 3. MOUSE JIGGLER CONTROLLER
// ========================================================

function setupJiggler() {
  document.getElementById("btn-toggle-jiggler").addEventListener("click", async () => {
    const isNowRunning = await invoke("toggle_jiggler");
    updateJigglerUIState(isNowRunning);
  });

  document.getElementById("btn-save-jiggler-cfg").addEventListener("click", async () => {
    const mode = document.querySelector('input[name="jiggler-mode"]:checked').value;
    const mouse_enabled = document.getElementById("chk-mouse-enabled").checked;
    const mouse_interval_sec = parseInt(document.getElementById("input-mouse-sec").value, 10) || 10;
    const key_enabled = document.getElementById("chk-key-enabled").checked;
    const key_interval_sec = parseInt(document.getElementById("input-key-sec").value, 10) || 30;
    const key_choice = document.getElementById("select-ghost-key").value;
    const idle_guard_enabled = document.getElementById("chk-idle-guard").checked;
    const idle_threshold_sec = parseInt(document.getElementById("input-idle-threshold").value, 10) || 15;
    const keep_awake_enabled = document.getElementById("chk-keep-awake").checked;

    const cfg = {
      shortcut: "Ctrl+Alt+J",
      mouse_enabled,
      mouse_interval_sec,
      mouse_move_mode: mode,
      key_enabled,
      key_interval_sec,
      key_choice,
      idle_guard_enabled,
      idle_threshold_sec,
      keep_awake_enabled,
    };

    await invoke("save_jiggler_config", { config: cfg });
    alert("Jiggler preferences saved.");
  });
}

async function pollJiggler() {
  if (currentTab !== "tab-jiggler") return;
  const stats = await invoke("get_jiggler_stats");
  if (!stats) return;

  document.getElementById("stat-total-moves").textContent = stats.total_mouse_moves;
  document.getElementById("stat-total-keys").textContent = stats.total_key_taps;
  document.getElementById("stat-next-move").textContent = `${stats.next_mouse_move_sec}s`;
  document.getElementById("stat-next-key").textContent = `${stats.next_key_tap_sec}s`;
  document.getElementById("meter-idle-time").textContent = `${stats.current_idle_sec.toFixed(1)}s`;
  document.getElementById("telemetry-msg").textContent = stats.last_action_msg;

  updateJigglerUIState(stats.running);
}

function updateJigglerUIState(isRunning) {
  const btn = document.getElementById("btn-toggle-jiggler");
  const badge = document.getElementById("jiggler-status-badge");

  if (isRunning) {
    btn.textContent = "⏹ STOP MOUSE JIGGLER";
    btn.className = "btn btn-rose btn-large";
    badge.textContent = "● ACTIVE";
    badge.className = "badge badge-emerald";
  } else {
    btn.textContent = "▶ START MOUSE JIGGLER";
    btn.className = "btn btn-primary btn-large";
    badge.textContent = "Standby";
    badge.className = "badge badge-subtle";
  }
}

// ========================================================
// 4. VAULT & CLIPBOARD CONTROLLER
// ========================================================

function setupVault() {
  document.getElementById("input-search-creds").addEventListener("input", loadCredentials);
  document.getElementById("input-search-clips").addEventListener("input", loadClipboard);
  document.getElementById("chk-pinned-clips").addEventListener("change", loadClipboard);

  // Add Credential Modal
  const modalCred = document.getElementById("modal-cred");
  document.getElementById("btn-add-cred").addEventListener("click", () => {
    document.getElementById("input-cred-platform").value = "";
    document.getElementById("input-cred-username").value = "";
    document.getElementById("input-cred-pass").value = "";
    document.getElementById("input-cred-url").value = "";
    document.getElementById("input-cred-notes").value = "";
    modalCred.classList.remove("hidden");
  });
  document.getElementById("btn-close-cred-modal").addEventListener("click", () => modalCred.classList.add("hidden"));
  document.getElementById("btn-cancel-cred").addEventListener("click", () => modalCred.classList.add("hidden"));

  document.getElementById("btn-save-cred").addEventListener("click", async () => {
    const platform = document.getElementById("input-cred-platform").value.trim();
    const username = document.getElementById("input-cred-username").value.trim();
    const password = document.getElementById("input-cred-pass").value;
    const url = document.getElementById("input-cred-url").value.trim() || null;
    const notes = document.getElementById("input-cred-notes").value.trim() || null;

    if (!platform || !username || !password) {
      alert("Platform, username, and password are required.");
      return;
    }

    await invoke("add_credential", { platform, username, password, url, notes });
    modalCred.classList.add("hidden");
    loadCredentials();
  });

  document.getElementById("btn-cred-gen-pass").addEventListener("click", async () => {
    const pass = await invoke("generate_password", { length: 18, upper: true, lower: true, digits: true, symbols: true });
    document.getElementById("input-cred-pass").value = pass;
  });

  // Standalone Generator Modal
  const modalGen = document.getElementById("modal-genpass");
  document.getElementById("btn-gen-pass-modal").addEventListener("click", () => {
    runPasswordGen();
    modalGen.classList.remove("hidden");
  });
  document.getElementById("btn-close-genpass-modal").addEventListener("click", () => modalGen.classList.add("hidden"));
  document.getElementById("btn-done-genpass").addEventListener("click", () => modalGen.classList.add("hidden"));
  document.getElementById("btn-rerun-genpass").addEventListener("click", runPasswordGen);

  document.getElementById("range-genpass-len").addEventListener("input", (e) => {
    document.getElementById("lbl-genpass-len").textContent = e.target.value;
    runPasswordGen();
  });

  document.getElementById("btn-copy-genpass").addEventListener("click", async () => {
    const pass = document.getElementById("input-genpass-result").value;
    if (pass) {
      await invoke("copy_to_clipboard", { text: pass });
      alert("Password copied to clipboard!");
    }
  });

  // Clear Unpinned Clips
  document.getElementById("btn-clear-clips").addEventListener("click", async () => {
    if (confirm("Clear all unpinned clipboard history items?")) {
      await invoke("clear_unpinned_clips");
      loadClipboard();
    }
  });
}

async function runPasswordGen() {
  const length = parseInt(document.getElementById("range-genpass-len").value, 10) || 16;
  const upper = document.getElementById("chk-gen-upper").checked;
  const lower = document.getElementById("chk-gen-lower").checked;
  const digits = document.getElementById("chk-gen-digits").checked;
  const symbols = document.getElementById("chk-gen-symbols").checked;

  const pass = await invoke("generate_password", { length, upper, lower, digits, symbols });
  document.getElementById("input-genpass-result").value = pass;
}

async function loadCredentials() {
  if (currentTab !== "tab-vault") return;
  const search = document.getElementById("input-search-creds").value;
  const items = await invoke("list_credentials", { search }) || [];

  const tbody = document.getElementById("creds-tbody");
  if (items.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-muted); padding: 30px;">No accounts found. Click <strong>➕ Add Account</strong> to create one.</td></tr>`;
    return;
  }

  let html = "";
  items.forEach((it) => {
    html += `
      <tr>
        <td><strong>${it.platform}</strong></td>
        <td><code>${it.username}</code></td>
        <td>
          <span id="pass-${it.id}">••••••••••••</span>
          <button class="btn btn-sm btn-secondary" onclick="window.togglePassVisibility('${it.id}', '${it.password.replace(/'/g, "\\'")}')">👁️</button>
        </td>
        <td>${it.url ? `<a href="${it.url}" target="_blank" style="color: var(--accent-cyan); text-decoration: none;">${it.url}</a>` : "-"}</td>
        <td style="color: var(--text-muted); font-size: 12px;">${it.notes || "-"}</td>
        <td style="text-align: right;">
          <div style="display: inline-flex; gap: 4px;">
            <button class="btn btn-sm btn-secondary" onclick="window.copyToClip('${it.username.replace(/'/g, "\\'")}')">📋 User</button>
            <button class="btn btn-sm btn-secondary" onclick="window.copyToClip('${it.password.replace(/'/g, "\\'")}')">📋 Pass</button>
            <button class="btn btn-sm btn-rose" onclick="window.deleteCredential('${it.id}')">🗑️</button>
          </div>
        </td>
      </tr>
    `;
  });
  tbody.innerHTML = html;
}

window.togglePassVisibility = (id, actualPass) => {
  const el = document.getElementById(`pass-${id}`);
  if (el.textContent === "••••••••••••") {
    el.textContent = actualPass;
    el.style.fontFamily = "var(--font-mono)";
    el.style.color = "var(--accent-red)";
  } else {
    el.textContent = "••••••••••••";
    el.style.fontFamily = "";
    el.style.color = "";
  }
};

window.copyToClip = async (text) => {
  await invoke("copy_to_clipboard", { text });
};

window.deleteCredential = async (id) => {
  if (confirm("Delete this account from the vault?")) {
    await invoke("delete_credential", { id });
    loadCredentials();
  }
};

async function loadClipboard() {
  if (currentTab !== "tab-vault") return;
  const search = document.getElementById("input-search-clips").value;
  const pinnedOnly = document.getElementById("chk-pinned-clips").checked;
  const items = await invoke("list_clipboard", { search, pinnedOnly }) || [];

  const list = document.getElementById("clips-list");
  if (items.length === 0) {
    list.innerHTML = `<div style="text-align: center; color: var(--text-muted); padding: 30px;">No clipboard entries recorded yet. Copy text anywhere in Windows to automatically capture it here.</div>`;
    return;
  }

  let html = "";
  items.forEach((it) => {
    const timeStr = new Date(it.timestamp).toLocaleTimeString();
    html += `
      <div class="clip-card">
        <button class="btn btn-sm btn-secondary" onclick="window.toggleClipPin('${it.id}')" style="font-size: 14px;">
          ${it.pinned ? "⭐" : "☆"}
        </button>
        <div class="clip-info">
          <div class="clip-meta">
            <span>[${timeStr}]</span>
            <span>•</span>
            <span>${it.length} characters</span>
          </div>
          <div class="clip-text">${escapeHtml(it.content)}</div>
        </div>
        <div style="display: flex; gap: 4px;">
          <button class="btn btn-sm btn-primary" onclick="window.copyToClip('${it.content.replace(/'/g, "\\'")}')">📋 Copy</button>
          <button class="btn btn-sm btn-rose" onclick="window.deleteClip('${it.id}')">🗑️</button>
        </div>
      </div>
    `;
  });
  list.innerHTML = html;
}

window.toggleClipPin = async (id) => {
  await invoke("toggle_clip_pin", { id });
  loadClipboard();
};

window.deleteClip = async (id) => {
  await invoke("delete_clip", { id });
  loadClipboard();
};

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text || "";
  return div.innerHTML;
}

function escapeJs(str) {
  return (str || "").replace(/\\/g, "\\\\").replace(/'/g, "\\'").replace(/"/g, '&quot;');
}

// ========================================================
// 5. KANBAN TASK BOARD CONTROLLER
// ========================================================

function setupTasks() {
  document.getElementById("input-search-tasks").addEventListener("input", loadTasks);
  document.getElementById("select-project-filter").addEventListener("change", loadTasks);

  // Add Task Modal
  const modalTask = document.getElementById("modal-task");
  document.getElementById("btn-add-task").addEventListener("click", async () => {
    document.getElementById("input-task-title").value = "";
    document.getElementById("select-task-reminder").value = "";

    // Populate project options from active service groups
    const summary = await invoke("get_manager_summary");
    const projSelect = document.getElementById("select-task-project");
    let opts = `<option value="General">General</option>`;
    if (summary) {
      summary.groups.forEach((g) => {
        opts += `<option value="${g.name}">${g.name}</option>`;
      });
    }
    projSelect.innerHTML = opts;

    modalTask.classList.remove("hidden");
  });

  document.getElementById("btn-close-task-modal").addEventListener("click", () => modalTask.classList.add("hidden"));
  document.getElementById("btn-cancel-task").addEventListener("click", () => modalTask.classList.add("hidden"));

  document.getElementById("btn-save-task").addEventListener("click", async () => {
    const title = document.getElementById("input-task-title").value.trim();
    const project = document.getElementById("select-task-project").value;
    const status = document.getElementById("select-task-status").value;
    const priority = document.getElementById("select-task-prio").value;
    const reminderChoice = document.getElementById("select-task-reminder").value;

    if (!title) {
      alert("Task title is required.");
      return;
    }

    let notify_at = null;
    const now = new Date();
    if (reminderChoice === "5m") {
      notify_at = new Date(now.getTime() + 5 * 60000).toISOString();
    } else if (reminderChoice === "15m") {
      notify_at = new Date(now.getTime() + 15 * 60000).toISOString();
    } else if (reminderChoice === "30m") {
      notify_at = new Date(now.getTime() + 30 * 60000).toISOString();
    } else if (reminderChoice === "1h") {
      notify_at = new Date(now.getTime() + 60 * 60000).toISOString();
    } else if (reminderChoice === "today18") {
      const d = new Date();
      d.setHours(18, 0, 0, 0);
      notify_at = d.toISOString();
    } else if (reminderChoice === "tomorrow9") {
      const d = new Date();
      d.setDate(d.getDate() + 1);
      d.setHours(9, 0, 0, 0);
      notify_at = d.toISOString();
    }

    await invoke("add_todo", { title, project, priority, status, notifyAt: notify_at });
    modalTask.classList.add("hidden");
    loadTasks();
  });

  document.getElementById("btn-clear-completed-tasks").addEventListener("click", async () => {
    if (confirm("Clear all completed tasks in the 'DONE' column?")) {
      await invoke("clear_completed_todos");
      loadTasks();
    }
  });
}

async function loadTasks() {
  if (currentTab !== "tab-tasks") return;
  const search = document.getElementById("input-search-tasks").value;
  const proj = document.getElementById("select-project-filter").value;
  const items = await invoke("list_todos", { search, project: proj === "ALL" ? null : proj, status: null }) || [];

  const todoList = document.getElementById("list-todo");
  const doingList = document.getElementById("list-doing");
  const doneList = document.getElementById("list-done");

  let todoCount = 0;
  let doingCount = 0;
  let doneCount = 0;

  let todoHtml = "";
  let doingHtml = "";
  let doneHtml = "";

  items.forEach((it) => {
    const prioColor = it.priority === "high" ? "rose" : it.priority === "low" ? "subtle" : "emerald";
    const cardHtml = `
      <div class="task-card">
        <div class="task-meta">
          <span class="badge badge-${prioColor}">${it.priority || "NORMAL"}</span>
          <span class="badge badge-subtle">${it.project || "General"}</span>
        </div>
        <div class="task-title">${escapeHtml(it.title)}</div>
        <div class="task-footer">
          <div style="font-size: 10px; color: var(--text-dim);">#${it.id}</div>
          <div style="display: flex; gap: 4px;">
            ${it.status === "todo" ? `
              <button class="btn btn-sm btn-secondary" onclick="window.moveTask('${it.id}', 'in_progress')">Start ➔</button>
              <button class="btn btn-sm btn-emerald" onclick="window.moveTask('${it.id}', 'done')">✓</button>
            ` : it.status === "in_progress" ? `
              <button class="btn btn-sm btn-secondary" onclick="window.moveTask('${it.id}', 'todo')">⬅ Back</button>
              <button class="btn btn-sm btn-emerald" onclick="window.moveTask('${it.id}', 'done')">Done ✓</button>
            ` : `
              <button class="btn btn-sm btn-secondary" onclick="window.moveTask('${it.id}', 'in_progress')">Reopen</button>
            `}
            <button class="btn btn-sm btn-rose" onclick="window.deleteTask('${it.id}')">✕</button>
          </div>
        </div>
      </div>
    `;

    if (it.status === "todo") {
      todoCount++;
      todoHtml += cardHtml;
    } else if (it.status === "in_progress") {
      doingCount++;
      doingHtml += cardHtml;
    } else {
      doneCount++;
      doneHtml += cardHtml;
    }
  });

  document.getElementById("count-todo").textContent = todoCount;
  document.getElementById("count-doing").textContent = doingCount;
  document.getElementById("count-done").textContent = doneCount;

  todoList.innerHTML = todoHtml || `<div style="text-align: center; color: var(--text-dim); padding: 20px;">No tasks</div>`;
  doingList.innerHTML = doingHtml || `<div style="text-align: center; color: var(--text-dim); padding: 20px;">No tasks in progress</div>`;
  doneList.innerHTML = doneHtml || `<div style="text-align: center; color: var(--text-dim); padding: 20px;">No completed tasks</div>`;
}

window.moveTask = async (id, status) => {
  await invoke("update_todo_status", { id, status });
  loadTasks();
};

window.deleteTask = async (id) => {
  await invoke("delete_todo", { id });
  loadTasks();
};

// ========================================================
// 6. YOUTUBE & MEDIA DOWNLOADER CONTROLLER
// ========================================================

function setupDownloader() {
  // Media Type Radio Toggle
  document.querySelectorAll('input[name="media-type"]').forEach((radio) => {
    radio.addEventListener("change", (e) => {
      const isAudio = e.target.value === "audio";
      document.getElementById("pill-media-video").classList.toggle("active", !isAudio);
      document.getElementById("pill-media-audio").classList.toggle("active", isAudio);

      document.getElementById("box-video-format").classList.toggle("hidden", isAudio);
      document.getElementById("box-video-quality").classList.toggle("hidden", isAudio);
      document.getElementById("box-audio-format").classList.toggle("hidden", !isAudio);
      document.getElementById("box-audio-quality").classList.toggle("hidden", !isAudio);
    });
  });

  // Fetch Stream Metadata
  document.getElementById("btn-fetch-meta").addEventListener("click", async () => {
    const url = document.getElementById("input-ytdl-url").value.trim();
    if (!url) return;

    const btn = document.getElementById("btn-fetch-meta");
    btn.textContent = "⏳ Inspecting...";

    try {
      const meta = await invoke("fetch_video_metadata", { url });
      if (meta) {
        document.getElementById("preview-thumb").src = meta.thumbnail;
        document.getElementById("preview-title").textContent = meta.title;
        document.getElementById("preview-uploader").textContent = `${meta.uploader} • ${meta.duration_string}`;
        document.getElementById("preview-resolution").textContent = meta.max_resolution || "Standard";
        document.getElementById("preview-box").classList.remove("hidden");
      }
    } catch (err) {
      alert(`Stream inspection error: ${err}`);
    } finally {
      btn.textContent = "🔍 Inspect Stream";
    }
  });

  // Start Download
  document.getElementById("btn-start-download").addEventListener("click", async () => {
    const url = document.getElementById("input-ytdl-url").value.trim();
    if (!url) {
      alert("Please paste a valid video or playlist URL.");
      return;
    }

    const isAudio = document.querySelector('input[name="media-type"]:checked').value === "audio";
    const media_type = isAudio ? "audio" : "video";
    const container = isAudio
      ? document.getElementById("select-audio-fmt").value
      : document.getElementById("select-video-fmt").value;
    const quality = isAudio
      ? document.getElementById("select-audio-qual").value
      : document.getElementById("select-video-qual").value;
    const is_playlist = document.getElementById("chk-download-playlist").checked;

    try {
      const item = await invoke("start_download", {
        url,
        mediaType: media_type,
        container,
        quality,
        customDir: null,
        isPlaylist: is_playlist,
      });

      if (item) {
        activeDownloadId = item.id;
        document.getElementById("progress-box").classList.remove("hidden");
      }
    } catch (err) {
      alert(`Download start error: ${err}`);
    }
  });

  // Cancel Download
  document.getElementById("btn-cancel-download").addEventListener("click", async () => {
    await invoke("cancel_download");
    document.getElementById("progress-box").classList.add("hidden");
    activeDownloadId = null;
  });
}

async function loadDownloader() {
  if (currentTab !== "tab-downloader") return;
  const cfg = await invoke("get_ytdl_config");
  if (cfg) {
    document.getElementById("lbl-output-dir").textContent = cfg.output_dir;
  }

  const history = await invoke("get_download_history") || [];
  const tbody = document.getElementById("ytdl-history-tbody");

  if (history.length === 0) {
    tbody.innerHTML = `<tr><td colspan="5" style="text-align: center; color: var(--text-muted); padding: 30px;">No downloads recorded yet. Paste a link above to begin.</td></tr>`;
    return;
  }

  let html = "";
  history.forEach((it) => {
    const dateStr = new Date(it.created_at).toLocaleDateString();
    html += `
      <tr>
        <td>
          <div style="font-weight: 600; color: var(--text-main);">${escapeHtml(it.title)}</div>
          <div style="font-size: 11px; color: var(--text-dim);">${it.url}</div>
        </td>
        <td><span class="badge badge-subtle">${it.format_label}</span></td>
        <td><code>${it.file_size || "-"}</code></td>
        <td>${dateStr}</td>
        <td style="text-align: right;">
          <div style="display: inline-flex; gap: 4px;">
            ${it.file_path ? `
              <button class="btn btn-sm btn-primary" onclick="window.openMediaFile('${it.file_path.replace(/\\/g, "\\\\")}')">▶ Play</button>
              <button class="btn btn-sm btn-secondary" onclick="window.openMediaFolder('${it.file_path.replace(/\\/g, "\\\\")}')">📂 Folder</button>
            ` : ""}
          </div>
        </td>
      </tr>
    `;
  });
  tbody.innerHTML = html;
}

window.openMediaFile = async (path) => {
  await invoke("open_file", { path });
};

window.openMediaFolder = async (path) => {
  await invoke("open_folder", { path });
};

async function pollDownloader() {
  if (currentTab !== "tab-downloader" || !activeDownloadId) return;
  const history = await invoke("get_download_history") || [];
  const active = history.find((it) => it.id === activeDownloadId);

  if (active) {
    const progBox = document.getElementById("progress-box");
    progBox.classList.remove("hidden");

    document.getElementById("progress-item-label").textContent = active.is_playlist
      ? `Downloading item ${active.current_item} of ${active.total_items}`
      : active.title;
    document.getElementById("progress-percent").textContent = `${active.progress.toFixed(1)}%`;
    document.getElementById("progress-bar-fill").style.width = `${active.progress}%`;

    document.getElementById("progress-speed").textContent = `Speed: ${active.speed || "--"}`;
    document.getElementById("progress-size").textContent = `Size: ${active.total_size || "--"}`;
    document.getElementById("progress-eta").textContent = `ETA: ${active.eta || "--"}`;

    if (active.status === "completed" || active.status === "error" || active.status === "cancelled") {
      activeDownloadId = null;
      setTimeout(() => {
        progBox.classList.add("hidden");
        loadDownloader();
      }, 2000);
    }
  }
}

// ========================================================
// 8. DATA STORAGE & IMPORT CONTROLLER
// ========================================================

let currentStorageDir = "";

async function loadStorageHeaderInfo() {
  try {
    const info = await invoke("get_storage_info");
    if (info && info.current_dir) {
      currentStorageDir = info.current_dir;
      const parts = info.current_dir.split(/[/\\]/);
      const folderName = parts[parts.length - 1] || info.current_dir;
      const lbl = document.getElementById("lbl-header-storage-path");
      if (lbl) {
        lbl.textContent = `📁 ${folderName}`;
        lbl.title = info.current_dir;
      }
    }
  } catch (e) {
    console.error("Failed to load storage header info:", e);
  }
}

function setupStorageManager() {
  const modal = document.getElementById("modal-storage");
  const openBtn = document.getElementById("btn-open-storage-modal");
  const closeBtn = document.getElementById("btn-close-storage-modal");
  const cancelBtn = document.getElementById("btn-cancel-storage");
  const applyBtn = document.getElementById("btn-apply-storage");
  const browseBtn = document.getElementById("btn-browse-storage-folder");
  const inspectBtn = document.getElementById("btn-inspect-storage-path");
  const openCurrentBtn = document.getElementById("btn-open-current-storage-dir");
  const pathInput = document.getElementById("input-new-storage-path");
  const feedback = document.getElementById("storage-feedback");

  if (!modal || !openBtn) return;

  const showModal = async () => {
    modal.classList.remove("hidden");
    if (feedback) feedback.classList.add("hidden");
    try {
      const info = await invoke("get_storage_info");
      if (info) {
        currentStorageDir = info.current_dir;
        document.getElementById("lbl-modal-current-dir").textContent = info.current_dir;
        if (!pathInput.value) {
          pathInput.value = info.current_dir;
        }
        renderManifest(info);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const closeModal = () => {
    modal.classList.add("hidden");
  };

  openBtn.addEventListener("click", showModal);
  closeBtn?.addEventListener("click", closeModal);
  cancelBtn?.addEventListener("click", closeModal);

  openCurrentBtn?.addEventListener("click", async () => {
    if (currentStorageDir) {
      await invoke("open_folder", { path: currentStorageDir });
    }
  });

  browseBtn?.addEventListener("click", async () => {
    try {
      const selected = await invoke("pick_storage_folder");
      if (selected) {
        pathInput.value = selected;
        await triggerInspection(selected);
      }
    } catch (e) {
      console.error("Failed to browse folder:", e);
    }
  });

  inspectBtn?.addEventListener("click", async () => {
    const p = pathInput.value.trim();
    if (p) {
      await triggerInspection(p);
    }
  });

  pathInput?.addEventListener("change", async () => {
    const p = pathInput.value.trim();
    if (p) {
      await triggerInspection(p);
    }
  });

  async function triggerInspection(path) {
    const statusLbl = document.getElementById("lbl-storage-scan-status");
    if (statusLbl) {
      statusLbl.textContent = "Scanning...";
      statusLbl.className = "badge badge-admin";
    }
    try {
      const info = await invoke("inspect_storage_dir", { path });
      renderManifest(info);
      if (statusLbl) {
        statusLbl.textContent = "Scanned OK";
        statusLbl.className = "badge badge-emerald";
      }
    } catch (err) {
      if (statusLbl) {
        statusLbl.textContent = "Folder Not Found / Empty";
        statusLbl.className = "badge badge-rose";
      }
      renderManifestEmpty();
    }
  }

  function renderManifest(info) {
    updateManifestItem("services", info.has_services, `services.json (${info.services_count} services)`);
    updateManifestItem("tasks", info.has_todos, `todos.json (${info.todos_count} tasks)`);
    updateManifestItem("creds", info.has_credentials, `credentials.json (${info.credentials_count} accounts)`);
    updateManifestItem("clip", info.has_clipboard, `clipboard_history.json (${info.clips_count} clips)`);
    updateManifestItem("jiggler", info.has_jiggler, "jiggler_config.json");
    updateManifestItem("ytdl", info.has_downloader, "ytdl_config.json / downloads.json");
  }

  function renderManifestEmpty() {
    ["services", "tasks", "creds", "clip", "jiggler", "ytdl"].forEach((key) => {
      updateManifestItem(key, false, "Not present in target folder");
    });
  }

  function updateManifestItem(key, found, text) {
    const itemEl = document.getElementById(`item-manifest-${key}`);
    const detailEl = document.getElementById(`lbl-manifest-${key}`);
    const badgeEl = document.getElementById(`badge-manifest-${key}`);
    if (detailEl) detailEl.textContent = text;
    if (itemEl) {
      if (found) itemEl.classList.add("has-data");
      else itemEl.classList.remove("has-data");
    }
    if (badgeEl) {
      if (found) {
        badgeEl.textContent = "Found";
        badgeEl.className = "manifest-status found";
      } else {
        badgeEl.textContent = "Missing";
        badgeEl.className = "manifest-status missing";
      }
    }
  }

  applyBtn?.addEventListener("click", async () => {
    const newDir = pathInput.value.trim();
    if (!newDir) {
      alert("Please enter or browse to a valid storage directory.");
      return;
    }

    const copyCurrent = document.getElementById("chk-copy-storage-data")?.checked ?? true;

    applyBtn.disabled = true;
    applyBtn.innerHTML = `<span>⏳ Switching & Importing...</span>`;
    if (feedback) {
      feedback.className = "storage-feedback";
      feedback.textContent = "Switching folder and reloading data stores...";
      feedback.classList.remove("hidden");
    }

    try {
      const result = await invoke("set_storage_dir", {
        newDir: newDir,
        copyCurrent: copyCurrent,
      });

      currentStorageDir = result.current_dir;
      document.getElementById("lbl-modal-current-dir").textContent = result.current_dir;
      renderManifest(result);
      await loadStorageHeaderInfo();

      if (feedback) {
        feedback.className = "storage-feedback success";
        feedback.textContent = `✅ Successfully imported storage from: ${result.current_dir}! All services, vault items, and tasks reloaded.`;
      }

      // Immediately refresh all UI tabs with the new data
      pollServices();
      loadTasks();
      loadCredentials();
      loadClipboard();
      loadDownloader();
    } catch (err) {
      console.error(err);
      if (feedback) {
        feedback.className = "storage-feedback error";
        feedback.textContent = `❌ Error switching folder: ${err}`;
      }
    } finally {
      applyBtn.disabled = false;
      applyBtn.innerHTML = `<span>🚀 Switch & Import Data</span>`;
    }
  });
}



// ========================================================
// 9. LAN CHAT CONTROLLER
// ========================================================

let activeChatRecipient = "all"; // "all" for General LAN, or peer user_id
let activeChatTitle = "# 🌐 General LAN";
let activeChatDesc = "Public broadcast to all devices on the local subnet";
let chatConfigCache = null;
let chatPeersCache = [];
let knownMessageIds = new Set();
let unreadChatCounts = { all: 0 };
let totalUnreadChatCount = 0;
let lastTypingSentTime = 0;
let peerSearchFilter = "";
let activeTransfersMap = {};
let knownTriggeredBuzzIds = new Set();
let lastBuzzSentTime = 0;

function setupLanChat() {
  const inputMsg = document.getElementById("input-chat-message");
  const btnSend = document.getElementById("btn-send-chat");
  const btnScan = document.getElementById("btn-scan-peers");
  const btnSettings = document.getElementById("btn-chat-settings");
  const btnEditName = document.getElementById("btn-edit-chat-name");
  const selStatus = document.getElementById("sel-my-status");
  const btnClearConvo = document.getElementById("btn-clear-chat-convo");
  const inputSearch = document.getElementById("input-search-chat");
  const channelAll = document.getElementById("channel-item-all");
  const btnAttach = document.getElementById("btn-chat-attach");

  // Attach file button
  btnAttach?.addEventListener("click", handleAttachFileClick);

  // Buzz menu toggle & options
  const btnBuzzToggle = document.getElementById("btn-chat-buzz-toggle");
  const menuBuzz = document.getElementById("menu-chat-buzz");
  const optHard = document.getElementById("opt-buzz-hard");
  const optSoft = document.getElementById("opt-buzz-soft");

  btnBuzzToggle?.addEventListener("click", (e) => {
    e.stopPropagation();
    menuBuzz?.classList.toggle("hidden");
  });

  optHard?.addEventListener("click", () => {
    menuBuzz?.classList.add("hidden");
    handleSendBuzz("hard");
  });

  optSoft?.addEventListener("click", () => {
    menuBuzz?.classList.add("hidden");
    handleSendBuzz("soft");
  });

  document.addEventListener("click", (e) => {
    if (!menuBuzz?.contains(e.target) && e.target !== btnBuzzToggle) {
      menuBuzz?.classList.add("hidden");
    }
  });

  // Send message on click / Enter
  btnSend?.addEventListener("click", handleSendChatMessage);
  inputMsg?.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSendChatMessage();
    }
  });

  // Typing event throttling
  inputMsg?.addEventListener("input", () => {
    const now = Date.now();
    if (now - lastTypingSentTime > 2000) {
      lastTypingSentTime = now;
      invoke("send_chat_typing", { recipientId: activeChatRecipient }).catch(() => {});
    }
  });

  // Switch to General Channel
  channelAll?.addEventListener("click", () => {
    selectChatRecipient("all", "# 🌐 General LAN", "Public broadcast to all devices on the local subnet");
  });

  // Status select
  selStatus?.addEventListener("change", async (e) => {
    if (chatConfigCache) {
      chatConfigCache.status = e.target.value;
      await invoke("save_chat_config", { config: chatConfigCache });
      showToast(`Status set to: ${e.target.value.toUpperCase()}`, "info");
    }
  });

  // Search filter
  inputSearch?.addEventListener("input", (e) => {
    peerSearchFilter = e.target.value.trim().toLowerCase();
    renderChatPeersList();
  });

  // Scan LAN button
  btnScan?.addEventListener("click", async () => {
    showToast("Scanning local subnet for Process Manager peers...", "info");
    await pollChatPeers();
  });

  // Clear convo button
  btnClearConvo?.addEventListener("click", async () => {
    const targetName = activeChatRecipient === "all" ? "General LAN" : "this conversation";
    if (confirm(`Are you sure you want to clear chat history for ${targetName}?`)) {
      await invoke("clear_chat_history", { recipient: activeChatRecipient });
      knownMessageIds.clear();
      await pollChatMessages();
      showToast("Chat history cleared.", "info");
    }
  });

  // Edit Name & Settings Modals
  btnEditName?.addEventListener("click", openChatSettingsModal);
  btnSettings?.addEventListener("click", openChatSettingsModal);

  setupChatSettingsModal();
}

function selectChatRecipient(recipientId, title, desc) {
  activeChatRecipient = recipientId;
  activeChatTitle = title;
  activeChatDesc = desc;

  // Update UI header
  const titleEl = document.getElementById("lbl-active-convo-title");
  const descEl = document.getElementById("lbl-active-convo-desc");
  const inputEl = document.getElementById("input-chat-message");
  if (titleEl) titleEl.textContent = title;
  if (descEl) descEl.textContent = desc;
  if (inputEl) inputEl.placeholder = `Message ${title}... (Press Enter to send)`;

  // Clear unread for this recipient
  if (unreadChatCounts[recipientId]) {
    unreadChatCounts[recipientId] = 0;
    updateChatUnreadBadges();
  }

  // Active styles on sidebar
  document.querySelectorAll(".chat-channel-item, .chat-peer-item").forEach((el) => {
    el.classList.remove("active");
  });

  if (recipientId === "all") {
    document.getElementById("channel-item-all")?.classList.add("active");
  } else {
    document.getElementById(`peer-item-${recipientId}`)?.classList.add("active");
  }

  // Refresh messages
  pollChatMessages(true);
}

async function loadChatConfig() {
  try {
    const cfg = await invoke("get_chat_config");
    if (!cfg) return;
    chatConfigCache = cfg;

    const usernameEl = document.getElementById("lbl-my-username");
    const avatarEl = document.getElementById("lbl-my-avatar");
    const hostnameEl = document.getElementById("lbl-my-hostname");
    const statusSel = document.getElementById("sel-my-status");
    const portEl = document.getElementById("lbl-chat-port");

    if (usernameEl) usernameEl.textContent = cfg.username;
    if (avatarEl) avatarEl.textContent = (cfg.username || "U").substring(0, 2).toUpperCase();
    if (hostnameEl) hostnameEl.textContent = cfg.hostname;
    if (statusSel) statusSel.value = cfg.status;
    if (portEl) portEl.textContent = cfg.port;
  } catch (err) {
    console.error("Failed to load chat config:", err);
  }
}

async function pollChatPeers() {
  try {
    const peers = (await invoke("get_chat_peers")) || [];
    chatPeersCache = peers;

    const onlineCount = peers.filter((p) => p.is_online).length;
    const countEl = document.getElementById("lbl-online-peer-count");
    const sidebarCountEl = document.getElementById("lbl-sidebar-peer-count");

    if (countEl) countEl.textContent = onlineCount;
    if (sidebarCountEl) sidebarCountEl.textContent = onlineCount;

    renderChatPeersList();
  } catch (err) {
    console.error("Failed to poll chat peers:", err);
  }
}

function renderChatPeersList() {
  const container = document.getElementById("chat-peers-list");
  if (!container) return;

  const filtered = chatPeersCache.filter((p) => {
    if (!peerSearchFilter) return true;
    return (
      p.username.toLowerCase().includes(peerSearchFilter) ||
      p.hostname.toLowerCase().includes(peerSearchFilter) ||
      p.ip.includes(peerSearchFilter)
    );
  });

  if (filtered.length === 0) {
    container.innerHTML = `
      <div class="chat-empty-peers">
        ${peerSearchFilter ? "No matching peers found." : "Scanning local network for devices running Process Manager..."}
      </div>
    `;
    return;
  }

  let html = "";
  filtered.forEach((p) => {
    const initials = (p.username || "P").substring(0, 2).toUpperCase();
    const isActive = activeChatRecipient === p.id;
    const dotClass = !p.is_online
      ? "status-dot-offline"
      : p.status === "busy"
      ? "status-dot-busy"
      : p.status === "away"
      ? "status-dot-away"
      : "status-dot-online";

    const unread = unreadChatCounts[p.id] || 0;
    const unreadHtml = unread > 0 ? `<span class="channel-unread">${unread}</span>` : "";

    html += `
      <div class="chat-peer-item ${isActive ? "active" : ""}" id="peer-item-${p.id}" onclick="selectChatRecipient('${p.id}', '${escapeJs(p.username)}', '${escapeJs(p.hostname)} • ${p.ip}')">
        <div class="chat-peer-avatar">
          <span>${initials}</span>
          <span class="chat-status-dot ${dotClass}"></span>
        </div>
        <div class="chat-peer-info">
          <div class="chat-peer-name">${p.username}</div>
          <div class="chat-peer-meta">${p.hostname} • ${p.ip}</div>
        </div>
        ${unreadHtml}
      </div>
    `;
  });

  container.innerHTML = html;
}

async function handleAttachFileClick() {
  if (activeChatRecipient === "all") {
    showToast("Please select a specific peer from the left sidebar to send a file.", "warning");
    return;
  }

  try {
    const file = await invoke("pick_file_to_send");
    if (!file) return;

    const formattedSize = formatBytes(file.size);
    const confirmMsg = `Send "${file.name}" (${formattedSize}) to ${activeChatTitle}?`;
    if (!confirm(confirmMsg)) return;

    showToast(`Offering "${file.name}" to peer...`, "info");
    await invoke("send_file_offer", {
      recipientId: activeChatRecipient,
      filePath: file.path,
    });
    await pollChatMessages(true);
  } catch (err) {
    showToast(`Failed to send file: ${err}`, "error");
  }
}

function formatBytes(bytes) {
  if (bytes === undefined || bytes === null) return "0 B";
  const num = typeof bytes === "number" ? bytes : Number(bytes);
  if (num === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB", "PB"];
  const i = Math.floor(Math.log(num) / Math.log(k));
  return parseFloat((num / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

function formatSpeed(bytesPerSec) {
  if (!bytesPerSec || bytesPerSec <= 0) return "0 B/s";
  return `${formatBytes(bytesPerSec)}/s`;
}

function formatEta(sec) {
  if (!sec || sec <= 0) return "--";
  if (sec < 60) return `${sec}s`;
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  if (m < 60) return `${m}m ${s}s`;
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return `${h}h ${remM}m`;
}

window.acceptTransfer = async (transferId) => {
  try {
    showToast("Starting file download...", "info");
    await invoke("accept_file_transfer", { transferId, savePath: null });
    await pollChatMessages();
  } catch (err) {
    showToast(`Failed to accept transfer: ${err}`, "error");
  }
};

window.saveAsTransfer = async (transferId, defaultName) => {
  try {
    const savePath = await invoke("pick_save_file_path", { defaultName });
    if (!savePath) return;
    showToast("Starting file download...", "info");
    await invoke("accept_file_transfer", { transferId, savePath });
    await pollChatMessages();
  } catch (err) {
    showToast(`Failed to start download: ${err}`, "error");
  }
};

window.declineTransfer = async (transferId) => {
  try {
    await invoke("decline_file_transfer", { transferId });
    await pollChatMessages();
    showToast("Transfer declined.", "info");
  } catch (err) {
    showToast(`Error: ${err}`, "error");
  }
};

window.cancelTransfer = async (transferId) => {
  try {
    await invoke("cancel_file_transfer", { transferId });
    await pollChatMessages();
    showToast("Transfer cancelled.", "info");
  } catch (err) {
    showToast(`Error: ${err}`, "error");
  }
};

window.openDownloadedFile = async (filePath) => {
  if (!filePath) return;
  try {
    await invoke("open_file", { path: filePath });
  } catch (err) {
    showToast(`Could not open file: ${err}`, "error");
  }
};

window.openFolderOfFile = async (filePath) => {
  if (!filePath) return;
  try {
    await invoke("open_folder", { path: filePath });
  } catch (err) {
    showToast(`Could not open folder: ${err}`, "error");
  }
};


function triggerBuzzEffect(buzzType, isMine = false) {
  const animClass = buzzType === "hard" ? "hard-buzz-active" : "soft-buzz-active";
  const duration = buzzType === "hard" ? 750 : 450;

  document.body.classList.remove("hard-buzz-active", "soft-buzz-active");
  void document.body.offsetWidth;
  document.body.classList.add(animClass);
  setTimeout(() => {
    document.body.classList.remove(animClass);
  }, duration);

  if (chatConfigCache?.sound_notifications !== false) {
    if (buzzType === "hard") {
      playHardBuzzSound();
    } else {
      playSoftBuzzSound();
    }
  }

  if (!isMine) {
    const label = buzzType === "hard" ? "⚡ HARD BUZZ" : "🔔 Soft Buzz";
    showToast(`${label} received!`, buzzType === "hard" ? "warning" : "info");
  }
}

function playHardBuzzSound() {
  try {
    const ctx = new (window.AudioContext || window.webkitAudioContext)();
    const now = ctx.currentTime;

    [0, 0.22].forEach((offset) => {
      const osc1 = ctx.createOscillator();
      const osc2 = ctx.createOscillator();
      const gain = ctx.createGain();

      osc1.type = "sawtooth";
      osc1.frequency.setValueAtTime(160, now + offset);
      osc1.frequency.exponentialRampToValueAtTime(130, now + offset + 0.16);

      osc2.type = "square";
      osc2.frequency.setValueAtTime(115, now + offset);
      osc2.frequency.exponentialRampToValueAtTime(95, now + offset + 0.16);

      gain.gain.setValueAtTime(0.35, now + offset);
      gain.gain.exponentialRampToValueAtTime(0.01, now + offset + 0.16);

      osc1.connect(gain);
      osc2.connect(gain);
      gain.connect(ctx.destination);

      osc1.start(now + offset);
      osc2.start(now + offset);
      osc1.stop(now + offset + 0.17);
      osc2.stop(now + offset + 0.17);
    });
  } catch (_) {}
}

function playSoftBuzzSound() {
  try {
    const ctx = new (window.AudioContext || window.webkitAudioContext)();
    const now = ctx.currentTime;

    [0, 0.16].forEach((offset, idx) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();

      osc.type = "sine";
      const startFreq = idx === 0 ? 540 : 720;
      const endFreq = idx === 0 ? 680 : 860;
      osc.frequency.setValueAtTime(startFreq, now + offset);
      osc.frequency.exponentialRampToValueAtTime(endFreq, now + offset + 0.14);

      gain.gain.setValueAtTime(0.2, now + offset);
      gain.gain.exponentialRampToValueAtTime(0.001, now + offset + 0.14);

      osc.connect(gain);
      gain.connect(ctx.destination);

      osc.start(now + offset);
      osc.stop(now + offset + 0.15);
    });
  } catch (_) {}
}

async function handleSendBuzz(buzzType) {
  const now = Date.now();
  const cooldownRemaining = 4000 - (now - lastBuzzSentTime);
  if (cooldownRemaining > 0) {
    const sec = Math.ceil(cooldownRemaining / 1000);
    showToast(`Please wait ${sec}s before sending another buzz.`, "warning");
    return;
  }
  lastBuzzSentTime = now;

  try {
    const bType = buzzType === "hard" ? "hard" : "soft";
    triggerBuzzEffect(bType, true);

    const msg = await invoke("send_chat_buzz", {
      recipientId: activeChatRecipient,
      buzzType: bType,
    });
    if (msg) {
      knownMessageIds.add(msg.id);
      knownTriggeredBuzzIds.add(msg.id);
    }
    await pollChatMessages(true);
    showToast(bType === "hard" ? "⚡ Sent Hard Buzz!" : "🔔 Sent Soft Buzz.", "success");
  } catch (err) {
    showToast(`Failed to send buzz: ${err}`, "error");
  }
}

async function pollChatMessages(forceScroll = false) {
  try {
    // Fetch active transfers for live speed/progress
    try {
      const activeTransfersList = (await invoke("get_file_transfers")) || [];
      activeTransfersMap = {};
      activeTransfersList.forEach((t) => {
        activeTransfersMap[t.transfer_id] = t;
      });
    } catch (_) {}

    const msgs = (await invoke("get_chat_messages", { recipient: activeChatRecipient })) || [];
    renderChatMessages(msgs, forceScroll);

    // Poll typing status
    const typingUsers = (await invoke("get_chat_typing_status", { recipient: activeChatRecipient })) || [];
    const typingBar = document.getElementById("chat-typing-bar");
    const typingText = document.getElementById("chat-typing-text");
    if (typingBar && typingText) {
      if (typingUsers.length > 0) {
        typingText.textContent = `${typingUsers.join(", ")} ${typingUsers.length > 1 ? "are" : "is"} typing...`;
        typingBar.classList.remove("hidden");
      } else {
        typingBar.classList.add("hidden");
      }
    }
  } catch (err) {
    console.error("Failed to poll chat messages:", err);
  }
}

function renderChatMessages(msgs, forceScroll = false) {
  const container = document.getElementById("chat-messages-container");
  if (!container) return;

  const isScrolledNearBottom =
    container.scrollHeight - container.scrollTop - container.clientHeight < 120;

  // Detect new incoming messages for audio chime & unread counting
  let hasNewOtherMsg = false;
  msgs.forEach((m) => {
    if (!knownMessageIds.has(m.id)) {
      knownMessageIds.add(m.id);
      if (!m.is_mine) {
        hasNewOtherMsg = true;
        if (currentTab !== "tab-chat") {
          totalUnreadChatCount++;
          const targetKey = m.recipient_id === "all" ? "all" : m.sender_id;
          unreadChatCounts[targetKey] = (unreadChatCounts[targetKey] || 0) + 1;
          updateChatUnreadBadges();
        }
      }
    }
  });

  if (hasNewOtherMsg && chatConfigCache?.sound_notifications) {
    playChatNotificationSound();
  }

  if (msgs.length === 0) {
    const channelNotice =
      activeChatRecipient === "all"
        ? "Joined # General LAN. All broadcast messages are delivered directly to devices on the same subnet."
        : `Direct conversation with ${activeChatTitle}. End-to-end local network message stream.`;

    container.innerHTML = `
      <div class="chat-system-msg">
        💬 ${channelNotice}
      </div>
    `;
    return;
  }

  let html = `
    <div class="chat-system-msg">
      💬 Messages in this room are exchanged over the local network via UDP. Large files stream directly via TCP.
    </div>
  `;

  msgs.forEach((m) => {
    const initials = (m.sender_name || "U").substring(0, 2).toUpperCase();
    const timeFormatted = formatChatTime(m.timestamp);

    let contentHtml = "";
    if (m.file_info) {
      const fi = m.file_info;
      const live = activeTransfersMap[fi.transfer_id];
      const status = live ? live.status : fi.status;
      const progress = live ? live.progress : (fi.progress || 0);
      const bytesTransferred = live ? live.bytes_transferred : (fi.bytes_transferred || 0);
      const speed = live ? live.speed_bps : 0;
      const eta = live ? live.eta_sec : 0;
      const savePath = live?.save_path || fi.save_path;
      const errorMsg = live?.error || fi.error;

      let actionsHtml = "";
      if (status === "offered") {
        if (m.is_mine) {
          actionsHtml = `
            <div class="file-card-waiting">Waiting for peer to accept...</div>
            <div class="file-card-actions">
              <button class="btn btn-secondary btn-xs" onclick="cancelTransfer('${fi.transfer_id}')">✖ Cancel</button>
            </div>
          `;
        } else {
          actionsHtml = `
            <div class="file-card-actions">
              <button class="btn btn-primary btn-sm" onclick="acceptTransfer('${fi.transfer_id}')">✔ Accept (Downloads)</button>
              <button class="btn btn-secondary btn-sm" onclick="saveAsTransfer('${fi.transfer_id}', '${escapeJs(fi.file_name)}')">📁 Save As...</button>
              <button class="btn btn-secondary btn-sm" onclick="declineTransfer('${fi.transfer_id}')">✖ Decline</button>
            </div>
          `;
        }
      } else if (status === "transferring") {
        actionsHtml = `
          <div class="file-card-progress-wrap">
            <div class="file-card-progress-bar" style="width: ${progress.toFixed(1)}%;"></div>
          </div>
          <div class="file-card-stats">
            <span>${formatBytes(bytesTransferred)} / ${formatBytes(fi.file_size)} (${progress.toFixed(1)}%)</span>
            <span>${formatSpeed(speed)} • ETA: ${formatEta(eta)}</span>
          </div>
          <div class="file-card-actions">
            <button class="btn btn-secondary btn-xs" onclick="cancelTransfer('${fi.transfer_id}')">✖ Cancel</button>
          </div>
        `;
      } else if (status === "completed") {
        if (savePath) {
          actionsHtml = `
            <div class="file-card-done-text">✔ File saved successfully</div>
            <div class="file-card-actions">
              <button class="btn btn-primary btn-sm" onclick="openDownloadedFile('${escapeJs(savePath)}')">📂 Open File</button>
              <button class="btn btn-secondary btn-sm" onclick="openFolderOfFile('${escapeJs(savePath)}')">📁 Show in Folder</button>
            </div>
          `;
        } else {
          actionsHtml = `<div class="file-card-done-text">✔ Sent successfully</div>`;
        }
      } else if (status === "declined") {
        actionsHtml = `<div class="file-card-muted-text">Transfer was declined.</div>`;
      } else if (status === "cancelled") {
        actionsHtml = `<div class="file-card-muted-text">Transfer was cancelled.</div>`;
      } else if (status === "failed") {
        actionsHtml = `<div class="file-card-error-text">Failed: ${escapeHtml(errorMsg || "Connection error")}</div>`;
      }

      contentHtml = `
        <div class="chat-file-card">
          <div class="file-card-header">
            <div class="file-card-icon">📄</div>
            <div class="file-card-info">
              <div class="file-card-name" title="${escapeHtml(fi.file_name)}">${escapeHtml(fi.file_name)}</div>
              <div class="file-card-meta">
                <span class="file-card-size">${formatBytes(fi.file_size)}</span>
                <span class="file-card-dot">•</span>
                <span class="file-card-status status-${status}">${status}</span>
              </div>
            </div>
          </div>
          ${actionsHtml}
        </div>
      `;
    } else if (m.buzz_type) {
      const bType = m.buzz_type === "hard" ? "hard" : "soft";
      contentHtml = `
        <div class="chat-buzz-bubble ${bType}">
          <div class="chat-buzz-icon">${bType === "hard" ? "⚡💥" : "🔔"}</div>
          <div class="chat-buzz-body">
            <div class="chat-buzz-title">${escapeHtml(m.content)}</div>
            <div class="chat-buzz-sub">${bType === "hard" ? "Violent screen shake & electric buzzer" : "Gentle nudge & alert chime"}</div>
          </div>
        </div>
      `;

      if (!knownTriggeredBuzzIds.has(m.id)) {
        knownTriggeredBuzzIds.add(m.id);
        if (!m.is_mine) {
          triggerBuzzEffect(bType, false);
        }
      }
    } else {
      contentHtml = `<div class="chat-msg-bubble">${escapeHtml(m.content)}</div>`;
    }

    html += `
      <div class="chat-msg-row ${m.is_mine ? "mine" : ""}">
        <div class="chat-msg-avatar">${initials}</div>
        <div class="chat-msg-body">
          <div class="chat-msg-meta">
            <span class="sender">${m.is_mine ? "You" : m.sender_name}</span>
            <span class="host">(${m.sender_host})</span>
            <span class="time">${timeFormatted}</span>
          </div>
          ${contentHtml}
        </div>
      </div>
    `;
  });

  container.innerHTML = html;

  if (forceScroll || isScrolledNearBottom) {
    container.scrollTop = container.scrollHeight;
  }
}

async function handleSendChatMessage() {
  const input = document.getElementById("input-chat-message");
  if (!input) return;

  const text = input.value.trim();
  if (!text) return;

  // Slash commands for Buzz
  const lower = text.toLowerCase();
  if (lower === "/hard" || lower === "/buzz") {
    input.value = "";
    handleSendBuzz("hard");
    return;
  }
  if (lower === "/soft" || lower === "/nudge") {
    input.value = "";
    handleSendBuzz("soft");
    return;
  }

  try {
    input.value = "";
    const newMsg = await invoke("send_chat_message", {
      recipientId: activeChatRecipient,
      content: text,
    });
    if (newMsg) {
      knownMessageIds.add(newMsg.id);
    }
    await pollChatMessages(true);
  } catch (err) {
    showToast(`Failed to send message: ${err}`, "error");
  }
}

function updateChatUnreadBadges() {
  const tabBadge = document.getElementById("badge-chat-unread");
  const channelAllBadge = document.getElementById("unread-channel-all");

  if (tabBadge) {
    if (totalUnreadChatCount > 0) {
      tabBadge.textContent = totalUnreadChatCount;
      tabBadge.classList.remove("hidden");
    } else {
      tabBadge.classList.add("hidden");
    }
  }

  if (channelAllBadge) {
    const unreadAll = unreadChatCounts["all"] || 0;
    if (unreadAll > 0) {
      channelAllBadge.textContent = unreadAll;
      channelAllBadge.classList.remove("hidden");
    } else {
      channelAllBadge.classList.add("hidden");
    }
  }

  renderChatPeersList();
}

function playChatNotificationSound() {
  try {
    const AudioContext = window.AudioContext || window.webkitAudioContext;
    if (!AudioContext) return;
    const ctx = new AudioContext();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();

    osc.type = "sine";
    osc.frequency.setValueAtTime(587.33, ctx.currentTime); // D5
    osc.frequency.setValueAtTime(880, ctx.currentTime + 0.08); // A5

    gain.gain.setValueAtTime(0.08, ctx.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.25);

    osc.connect(gain);
    gain.connect(ctx.destination);

    osc.start();
    osc.stop(ctx.currentTime + 0.25);
  } catch (_) {}
}

function formatChatTime(isoString) {
  if (!isoString) return "";
  try {
    const d = new Date(isoString);
    let hours = d.getHours();
    const minutes = d.getMinutes().toString().padStart(2, "0");
    const ampm = hours >= 12 ? "PM" : "AM";
    hours = hours % 12;
    hours = hours ? hours : 12;
    return `${hours}:${minutes} ${ampm}`;
  } catch (_) {
    return "";
  }
}



function setupChatSettingsModal() {
  const modal = document.getElementById("modal-chat-settings");
  const btnClose = document.getElementById("btn-close-chat-settings");
  const btnCancel = document.getElementById("btn-cancel-chat-settings");
  const btnSave = document.getElementById("btn-save-chat-settings");

  const closeModal = () => modal?.classList.add("hidden");

  btnClose?.addEventListener("click", closeModal);
  btnCancel?.addEventListener("click", closeModal);

  btnSave?.addEventListener("click", async () => {
    if (!chatConfigCache) return;

    const inputUser = document.getElementById("input-cfg-chat-username");
    const inputPort = document.getElementById("input-cfg-chat-port");
    const chkBroadcast = document.getElementById("chk-cfg-chat-broadcast");
    const chkSounds = document.getElementById("chk-cfg-chat-sounds");
    const chkDesktop = document.getElementById("chk-cfg-chat-desktop-notifications");

    const newUsername = inputUser?.value.trim();
    const newPort = parseInt(inputPort?.value, 10);

    if (!newUsername) {
      alert("Username cannot be empty.");
      return;
    }

    if (isNaN(newPort) || newPort < 1024 || newPort > 65535) {
      alert("Please enter a valid port between 1024 and 65535.");
      return;
    }

    chatConfigCache.username = newUsername;
    chatConfigCache.port = newPort;
    chatConfigCache.broadcast_enabled = !!chkBroadcast?.checked;
    chatConfigCache.sound_notifications = !!chkSounds?.checked;
    chatConfigCache.desktop_notifications = !!chkDesktop?.checked;

    await invoke("save_chat_config", { config: chatConfigCache });
    await loadChatConfig();
    closeModal();
    showToast("Chat settings updated and presence broadcasted.", "success");
  });

  const btnTest = document.getElementById("btn-test-notification");
  btnTest?.addEventListener("click", async () => {
    try {
      await invoke("send_desktop_notification", {
        title: "Process Manager Notification",
        body: "Windows desktop notifications are active and working smoothly!",
      });
      showToast("Sent test notification to Windows!", "info");
    } catch (err) {
      showToast(`Failed to trigger notification: ${err}`, "error");
    }
  });
}

function openChatSettingsModal() {
  if (!chatConfigCache) return;

  const modal = document.getElementById("modal-chat-settings");
  const inputUser = document.getElementById("input-cfg-chat-username");
  const inputHost = document.getElementById("input-cfg-chat-hostname");
  const inputPort = document.getElementById("input-cfg-chat-port");
  const chkBroadcast = document.getElementById("chk-cfg-chat-broadcast");
  const chkSounds = document.getElementById("chk-cfg-chat-sounds");
  const chkDesktop = document.getElementById("chk-cfg-chat-desktop-notifications");

  if (inputUser) inputUser.value = chatConfigCache.username;
  if (inputHost) inputHost.value = chatConfigCache.hostname;
  if (inputPort) inputPort.value = chatConfigCache.port;
  if (chkBroadcast) chkBroadcast.checked = chatConfigCache.broadcast_enabled;
  if (chkSounds) chkSounds.checked = chatConfigCache.sound_notifications;
  if (chkDesktop) chkDesktop.checked = chatConfigCache.desktop_notifications !== false;

  modal?.classList.remove("hidden");
}

window.selectChatRecipient = selectChatRecipient;

// ========================================================
// 10. EMBEDDED MEDIA PLAYER (VIDEOS & AUDIOS)
// ========================================================

let mediaServerPort = 0;
let mediaWorkspacePath = "";
let mediaScanData = null; // { root_path, total_count, video_count, audio_count, total_size_formatted, items: [...] }
let currentMediaItem = null;
let currentPlaylist = [];
let mediaFilterType = "all"; // "all" | "video" | "audio"
let mediaFilterSearch = "";
let mediaViewMode = "folders"; // "folders" | "flat"
let isMediaLoadedOnce = false;

function setupMedia() {
  const btnBrowse = document.getElementById("btn-browse-media-folder");
  const btnPlaceholderBrowse = document.getElementById("btn-placeholder-browse");
  const btnRescan = document.getElementById("btn-rescan-media");
  const btnOpenFolder = document.getElementById("btn-open-media-folder");
  const inputSearch = document.getElementById("input-media-search");
  const btnClearSearch = document.getElementById("btn-clear-media-search");
  const pillsContainer = document.getElementById("media-type-pills");
  const btnViewFolders = document.getElementById("btn-media-view-folders");
  const btnViewFlat = document.getElementById("btn-media-view-flat");
  const btnPrev = document.getElementById("btn-media-prev");
  const btnNext = document.getElementById("btn-media-next");
  const btnOpenExt = document.getElementById("btn-media-open-ext");
  const selectSpeed = document.getElementById("select-playback-speed");
  const chkLoop = document.getElementById("chk-media-loop");
  const chkAutoplayNext = document.getElementById("chk-media-autoplay-next");
  const btnPip = document.getElementById("btn-media-pip");
  const btnFullscreen = document.getElementById("btn-media-fullscreen");
  const videoEl = document.getElementById("media-video-element");
  const audioEl = document.getElementById("media-audio-element");
  const audioCard = document.getElementById("media-audio-card");

  btnBrowse?.addEventListener("click", () => browseAndSelectMediaWorkspace());
  btnPlaceholderBrowse?.addEventListener("click", () => browseAndSelectMediaWorkspace());

  btnRescan?.addEventListener("click", () => {
    if (mediaWorkspacePath) {
      scanMediaWorkspace(mediaWorkspacePath, true);
    } else {
      browseAndSelectMediaWorkspace();
    }
  });

  btnOpenFolder?.addEventListener("click", () => {
    if (mediaWorkspacePath) {
      invoke("open_folder", { path: mediaWorkspacePath }).catch((err) => {
        showToast(`Failed to open folder: ${err}`, "error");
      });
    } else {
      showToast("Please choose a media workspace folder first.", "info");
    }
  });

  // Search input
  inputSearch?.addEventListener("input", (e) => {
    mediaFilterSearch = (e.target.value || "").trim().toLowerCase();
    if (btnClearSearch) {
      if (mediaFilterSearch.length > 0) {
        btnClearSearch.classList.remove("hidden");
      } else {
        btnClearSearch.classList.add("hidden");
      }
    }
    renderMediaLibrary();
  });

  btnClearSearch?.addEventListener("click", () => {
    if (inputSearch) inputSearch.value = "";
    mediaFilterSearch = "";
    btnClearSearch.classList.add("hidden");
    renderMediaLibrary();
  });

  // Filter Pills (All / Videos / Audios)
  pillsContainer?.querySelectorAll(".pill").forEach((pill) => {
    pill.addEventListener("click", () => {
      pillsContainer.querySelectorAll(".pill").forEach((p) => p.classList.remove("active"));
      pill.classList.add("active");
      mediaFilterType = pill.getAttribute("data-type") || "all";
      renderMediaLibrary();
    });
  });

  // View Mode Toggles (Folders vs Flat)
  btnViewFolders?.addEventListener("click", () => {
    mediaViewMode = "folders";
    btnViewFolders.classList.add("active");
    btnViewFlat?.classList.remove("active");
    renderMediaLibrary();
  });

  btnViewFlat?.addEventListener("click", () => {
    mediaViewMode = "flat";
    btnViewFlat.classList.add("active");
    btnViewFolders?.classList.remove("active");
    renderMediaLibrary();
  });

  // Prev / Next Buttons
  btnPrev?.addEventListener("click", () => playPrevMedia());
  btnNext?.addEventListener("click", () => playNextMedia());

  // Open in OS player
  btnOpenExt?.addEventListener("click", () => {
    if (currentMediaItem) {
      invoke("open_file", { path: currentMediaItem.abs_path }).catch((err) => {
        showToast(`Failed to open in OS player: ${err}`, "error");
      });
    } else {
      showToast("No media item currently selected.", "info");
    }
  });

  // Playback Speed
  selectSpeed?.addEventListener("change", () => {
    const rate = parseFloat(selectSpeed.value) || 1.0;
    if (videoEl) videoEl.playbackRate = rate;
    if (audioEl) audioEl.playbackRate = rate;
  });

  // Loop toggle
  chkLoop?.addEventListener("change", () => {
    const isLoop = !!chkLoop.checked;
    if (videoEl) videoEl.loop = isLoop;
    if (audioEl) audioEl.loop = isLoop;
  });

  // Mini PiP
  btnPip?.addEventListener("click", async () => {
    if (!videoEl || videoEl.classList.contains("hidden")) {
      showToast("PiP is available when playing video.", "info");
      return;
    }
    try {
      if (document.pictureInPictureElement) {
        await document.exitPictureInPicture();
      } else {
        await videoEl.requestPictureInPicture();
      }
    } catch (err) {
      showToast(`PiP error: ${err}`, "error");
    }
  });

  // Fullscreen
  btnFullscreen?.addEventListener("click", () => {
    const stage = document.getElementById("media-stage-screen");
    if (!stage) return;
    if (!document.fullscreenElement) {
      stage.requestFullscreen().catch((err) => {
        showToast(`Fullscreen error: ${err}`, "error");
      });
    } else {
      document.exitFullscreen().catch(console.error);
    }
  });

  // Video element events
  videoEl?.addEventListener("ended", () => {
    if (chkAutoplayNext?.checked && !videoEl.loop) {
      playNextMedia();
    }
  });

  // Audio element events
  audioEl?.addEventListener("play", () => {
    audioCard?.classList.add("playing");
  });
  audioEl?.addEventListener("pause", () => {
    audioCard?.classList.remove("playing");
  });
  audioEl?.addEventListener("ended", () => {
    audioCard?.classList.remove("playing");
    if (chkAutoplayNext?.checked && !audioEl.loop) {
      playNextMedia();
    }
  });

  // Keyboard navigation on player stage
  window.addEventListener("keydown", (e) => {
    if (currentTab !== "tab-media") return;
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA" || e.target.tagName === "SELECT") return;

    if (e.key === " " || e.code === "Space") {
      // Space to toggle play/pause
      if (currentMediaItem) {
        e.preventDefault();
        if (currentMediaItem.media_type === "video" && videoEl) {
          if (videoEl.paused) videoEl.play(); else videoEl.pause();
        } else if (currentMediaItem.media_type === "audio" && audioEl) {
          if (audioEl.paused) audioEl.play(); else audioEl.pause();
        }
      }
    } else if (e.key === "ArrowRight" && e.altKey) {
      e.preventDefault();
      playNextMedia();
    } else if (e.key === "ArrowLeft" && e.altKey) {
      e.preventDefault();
      playPrevMedia();
    }
  });
}

async function ensureMediaServerPort() {
  if (mediaServerPort > 0) return mediaServerPort;
  try {
    const port = await invoke("get_media_server_port");
    mediaServerPort = Number(port) || 0;
    return mediaServerPort;
  } catch (err) {
    console.error("Failed to get media server port:", err);
    return 0;
  }
}

async function loadMediaTab() {
  await ensureMediaServerPort();

  if (isMediaLoadedOnce) return;
  isMediaLoadedOnce = true;

  try {
    const savedPath = await invoke("get_saved_media_workspace");
    if (savedPath && typeof savedPath === "string" && savedPath.trim().length > 0) {
      await scanMediaWorkspace(savedPath.trim(), false);
    }
  } catch (err) {
    console.warn("Could not load saved media workspace:", err);
  }
}

async function browseAndSelectMediaWorkspace() {
  try {
    const selected = await invoke("pick_media_workspace");
    if (selected && typeof selected === "string" && selected.trim().length > 0) {
      const cleanPath = selected.trim();
      await invoke("save_media_workspace", { path: cleanPath });
      await scanMediaWorkspace(cleanPath, true);
    }
  } catch (err) {
    showToast(`Failed to select folder: ${err}`, "error");
  }
}

async function scanMediaWorkspace(path, notifyUser = false) {
  const inputWorkspace = document.getElementById("input-media-workspace");
  const itemsContainer = document.getElementById("media-items-container");

  if (inputWorkspace) {
    inputWorkspace.value = path;
  }

  if (itemsContainer) {
    itemsContainer.innerHTML = `<div class="media-empty-list">🔄 Scanning workspace and all sub-files recursively...</div>`;
  }

  try {
    await ensureMediaServerPort();
    const data = await invoke("scan_media_workspace", { path });
    mediaWorkspacePath = data.root_path;
    mediaScanData = data;

    // Badges update
    const totalBadge = document.getElementById("badge-media-total");
    const videosBadge = document.getElementById("badge-media-videos");
    const audiosBadge = document.getElementById("badge-media-audios");
    const sizeBadge = document.getElementById("badge-media-size");

    const pillAll = document.getElementById("pill-count-all");
    const pillVideos = document.getElementById("pill-count-videos");
    const pillAudios = document.getElementById("pill-count-audios");

    if (totalBadge) totalBadge.textContent = `${data.total_count} Files`;
    if (videosBadge) videosBadge.textContent = `${data.video_count} Videos`;
    if (audiosBadge) audiosBadge.textContent = `${data.audio_count} Audios`;
    if (sizeBadge) sizeBadge.textContent = data.total_size_formatted;

    if (pillAll) pillAll.textContent = data.total_count;
    if (pillVideos) pillVideos.textContent = data.video_count;
    if (pillAudios) pillAudios.textContent = data.audio_count;

    renderMediaLibrary();

    if (notifyUser) {
      showToast(`Scanned ${data.total_count} files (${data.video_count} videos, ${data.audio_count} audios)`, "success");
    }
  } catch (err) {
    console.error("Scan error:", err);
    if (itemsContainer) {
      itemsContainer.innerHTML = `<div class="media-empty-list" style="color: #f87171;">⚠️ Failed to scan directory: ${escapeHtml(String(err))}</div>`;
    }
    showToast(`Media scan failed: ${err}`, "error");
  }
}

function getFilteredMediaItems() {
  if (!mediaScanData || !Array.isArray(mediaScanData.items)) return [];

  return mediaScanData.items.filter((item) => {
    // Type filter
    if (mediaFilterType === "video" && item.media_type !== "video") return false;
    if (mediaFilterType === "audio" && item.media_type !== "audio") return false;

    // Search filter
    if (mediaFilterSearch.length > 0) {
      const matchName = item.name.toLowerCase().includes(mediaFilterSearch);
      const matchRel = item.rel_path.toLowerCase().includes(mediaFilterSearch);
      const matchFolder = item.folder.toLowerCase().includes(mediaFilterSearch);
      if (!matchName && !matchRel && !matchFolder) return false;
    }

    return true;
  });
}

function renderMediaLibrary() {
  const container = document.getElementById("media-items-container");
  if (!container) return;

  currentPlaylist = getFilteredMediaItems();

  if (!mediaScanData || !mediaScanData.items || mediaScanData.items.length === 0) {
    container.innerHTML = `<div class="media-empty-list">No video or audio files found in this workspace.<br/><span style="opacity: 0.7;">Try choosing a different directory.</span></div>`;
    return;
  }

  if (currentPlaylist.length === 0) {
    container.innerHTML = `<div class="media-empty-list">No media matching current filter or search query.<br/><span style="opacity: 0.7;">Check your search terms.</span></div>`;
    return;
  }

  container.innerHTML = "";

  if (mediaViewMode === "flat") {
    // Render flat list
    currentPlaylist.forEach((item) => {
      container.appendChild(createMediaItemElement(item));
    });
  } else {
    // Group by folder
    const groups = new Map();
    currentPlaylist.forEach((item) => {
      const folderKey = item.folder || ".";
      if (!groups.has(folderKey)) {
        groups.set(folderKey, []);
      }
      groups.get(folderKey).push(item);
    });

    groups.forEach((itemsInFolder, folderName) => {
      const groupEl = document.createElement("div");
      groupEl.className = "media-folder-group";

      const displayName = folderName === "." ? "📁 Root Directory" : `📁 ${folderName}`;

      const headerEl = document.createElement("div");
      headerEl.className = "media-folder-header";
      headerEl.innerHTML = `
        <div class="media-folder-header-left">
          <span class="media-folder-chevron">▼</span>
          <span class="media-folder-name" title="${escapeHtml(folderName)}">${escapeHtml(displayName)}</span>
        </div>
        <span class="media-folder-count">${itemsInFolder.length}</span>
      `;

      headerEl.addEventListener("click", () => {
        groupEl.classList.toggle("collapsed");
      });

      const itemsWrap = document.createElement("div");
      itemsWrap.className = "media-folder-items";

      itemsInFolder.forEach((item) => {
        itemsWrap.appendChild(createMediaItemElement(item));
      });

      groupEl.appendChild(headerEl);
      groupEl.appendChild(itemsWrap);
      container.appendChild(groupEl);
    });
  }
}

function createMediaItemElement(item) {
  const row = document.createElement("div");
  const isActive = currentMediaItem && currentMediaItem.abs_path === item.abs_path;

  row.className = `media-item-row ${isActive ? "active" : ""}`;
  row.setAttribute("data-path", item.abs_path);

  const icon = item.media_type === "video" ? "🎬" : "🎵";
  const folderDisplay = item.folder === "." ? "" : `${item.folder}/`;

  row.innerHTML = `
    <span class="media-item-icon">${icon}</span>
    <div class="media-item-info">
      <div class="media-item-name" title="${escapeHtml(item.name)}">${escapeHtml(item.name)}</div>
      <div class="media-item-sub">
        <span>${escapeHtml(item.size_formatted)}</span>
        <span>•</span>
        <span title="${escapeHtml(item.rel_path)}">${escapeHtml(folderDisplay)}</span>
        ${isActive ? '<span class="media-badge-playing">PLAYING</span>' : ""}
      </div>
    </div>
    <div class="media-item-actions">
      <button class="media-mini-btn btn-open-file-os" title="Open file in Windows default app">↗ OS</button>
      <button class="media-mini-btn btn-open-folder-os" title="Show in File Explorer">📂</button>
    </div>
  `;

  row.addEventListener("click", (e) => {
    // If mini buttons clicked, don't trigger row play
    if (e.target.closest(".media-mini-btn")) return;
    playMediaItem(item);
  });

  const btnOpenOs = row.querySelector(".btn-open-file-os");
  btnOpenOs?.addEventListener("click", (e) => {
    e.stopPropagation();
    invoke("open_file", { path: item.abs_path }).catch((err) => {
      showToast(`Error opening file: ${err}`, "error");
    });
  });

  const btnOpenFolder = row.querySelector(".btn-open-folder-os");
  btnOpenFolder?.addEventListener("click", (e) => {
    e.stopPropagation();
    invoke("open_folder", { path: item.abs_path }).catch((err) => {
      showToast(`Error opening folder: ${err}`, "error");
    });
  });

  return row;
}

function updateMediaLibraryActiveItem() {
  const container = document.getElementById("media-items-container");
  if (!container) return;

  container.querySelectorAll(".media-item-row").forEach((row) => {
    const rowPath = row.getAttribute("data-path");
    const isActive = currentMediaItem && currentMediaItem.abs_path === rowPath;

    if (isActive) {
      row.classList.add("active");
      const sub = row.querySelector(".media-item-sub");
      if (sub && !sub.querySelector(".media-badge-playing")) {
        const badge = document.createElement("span");
        badge.className = "media-badge-playing";
        badge.textContent = "PLAYING";
        sub.appendChild(badge);
      }
    } else {
      row.classList.remove("active");
      const badge = row.querySelector(".media-badge-playing");
      if (badge) badge.remove();
    }
  });
}

function playMediaItem(item) {
  if (!item) return;
  currentMediaItem = item;

  const videoEl = document.getElementById("media-video-element");
  const audioEl = document.getElementById("media-audio-element");
  const audioCard = document.getElementById("media-audio-card");
  const placeholder = document.getElementById("media-empty-placeholder");
  const typeBadge = document.getElementById("media-playing-type-badge");
  const titleEl = document.getElementById("media-playing-title");
  const metaEl = document.getElementById("media-playing-meta");
  const speedSelect = document.getElementById("select-playback-speed");
  const chkLoop = document.getElementById("chk-media-loop");

  const speed = parseFloat(speedSelect?.value) || 1.0;
  const isLoop = !!chkLoop?.checked;

  if (typeBadge) {
    typeBadge.textContent = item.media_type.toUpperCase();
    typeBadge.className = `media-type-badge ${item.media_type}`;
  }
  if (titleEl) {
    titleEl.textContent = item.name;
    titleEl.title = item.name;
  }
  if (metaEl) {
    metaEl.textContent = `${item.rel_path} • ${item.size_formatted} • .${item.extension.toUpperCase()}`;
    metaEl.title = item.abs_path;
  }

  placeholder?.classList.add("hidden");

  // Construct streaming URL from the local Tokio HTTP server
  const streamUrl = `http://127.0.0.1:${mediaServerPort}/stream?path=${encodeURIComponent(item.abs_path)}`;

  if (item.media_type === "video") {
    // Hide audio, show video
    if (audioEl) {
      audioEl.pause();
      audioEl.removeAttribute("src");
    }
    audioCard?.classList.add("hidden");
    audioCard?.classList.remove("playing");

    if (videoEl) {
      videoEl.classList.remove("hidden");
      videoEl.src = streamUrl;
      videoEl.playbackRate = speed;
      videoEl.loop = isLoop;
      videoEl.play().catch((err) => {
        console.warn("Video playback autoplay note:", err);
      });
    }
  } else {
    // Hide video, show audio card
    if (videoEl) {
      videoEl.pause();
      videoEl.removeAttribute("src");
      videoEl.classList.add("hidden");
    }

    const audioTitle = document.getElementById("audio-card-title");
    const audioPath = document.getElementById("audio-card-path");
    if (audioTitle) audioTitle.textContent = item.name;
    if (audioPath) audioPath.textContent = item.rel_path;

    if (audioCard) audioCard.classList.remove("hidden");

    if (audioEl) {
      audioEl.src = streamUrl;
      audioEl.playbackRate = speed;
      audioEl.loop = isLoop;
      audioEl.play().catch((err) => {
        console.warn("Audio playback autoplay note:", err);
      });
    }
  }

  updateMediaLibraryActiveItem();
}

function playNextMedia() {
  if (!currentPlaylist || currentPlaylist.length === 0) return;

  let nextIdx = 0;
  if (currentMediaItem) {
    const idx = currentPlaylist.findIndex((x) => x.abs_path === currentMediaItem.abs_path);
    if (idx !== -1 && idx + 1 < currentPlaylist.length) {
      nextIdx = idx + 1;
    } else {
      nextIdx = 0; // Wrap to start
    }
  }

  playMediaItem(currentPlaylist[nextIdx]);
}

function playPrevMedia() {
  if (!currentPlaylist || currentPlaylist.length === 0) return;

  let prevIdx = currentPlaylist.length - 1;
  if (currentMediaItem) {
    const idx = currentPlaylist.findIndex((x) => x.abs_path === currentMediaItem.abs_path);
    if (idx > 0) {
      prevIdx = idx - 1;
    } else {
      prevIdx = currentPlaylist.length - 1; // Wrap to end
    }
  }

  playMediaItem(currentPlaylist[prevIdx]);
}

window.playMediaItem = playMediaItem;
window.browseAndSelectMediaWorkspace = browseAndSelectMediaWorkspace;
window.scanMediaWorkspace = scanMediaWorkspace;
