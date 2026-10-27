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
  setupMediaStudio();
  setupStorageManager();

  // Background Telemetry Tickers
  setInterval(pollServices, 1500);
  setInterval(pollLogs, 1500);
  setInterval(pollJiggler, 1000);
  setInterval(pollDownloader, 1000);
  setInterval(pollMediaProgress, 800);
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
  loadMediaStudio();
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
      } else if (currentTab === "tab-media") {
        loadMediaStudio();
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

  document.getElementById("btn-close-service-modal").addEventListener("click", () => modalService.classList.add("hidden"));
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

    const lblServiceCount = document.getElementById("lbl-service-ports-count");
    if (lblServiceCount) lblServiceCount.textContent = `${servicesCacheForPorts.length} Services`;

    const countAllPorts = document.getElementById("count-all-ports");
    if (countAllPorts) countAllPorts.textContent = activePortsCache.length;

    const countServicePorts = document.getElementById("count-service-ports");
    if (countServicePorts) countServicePorts.textContent = servicesCacheForPorts.length;

    // Render Service Matrix & TCP Listeners Table
    renderServicePortsGrid(activePortSet);
    renderPortsTabTable();
  } catch (err) {
    console.error("Failed to load port killer tab data:", err);
  }
}

function renderServicePortsGrid(activePortSet) {
  const container = document.getElementById("service-ports-grid");
  if (!container) return;

  if (servicesCacheForPorts.length === 0) {
    container.innerHTML = `<div style="grid-column: 1 / -1; color: var(--text-muted); font-size: 13px; text-align: center; padding: 24px;">No services with configured web ports found.</div>`;
    return;
  }

  let html = "";
  servicesCacheForPorts.forEach((s) => {
    const isListening = activePortSet.has(s.port);
    const procInfo = activePortsCache.find((p) => p.port === s.port);
    const isRunning = s.status === "running";

    const statusBadge = isRunning
      ? `<span class="badge badge-emerald">RUNNING</span>`
      : `<span class="badge badge-slate">STOPPED</span>`;

    const listenerBadge = isListening
      ? `<span class="stat-pill" style="border-color: rgba(52, 211, 153, 0.4);"><span class="stat-val stat-emerald">:${s.port} Listening</span></span>`
      : `<span class="stat-pill" style="border-color: var(--border-subtle);"><span class="stat-val" style="color: var(--text-muted);">:${s.port} Inactive</span></span>`;

    const occupantText = procInfo
      ? `PID ${procInfo.pid} (${procInfo.process_name})`
      : isListening
      ? "Occupied"
      : "Port Available";

    html += `
      <div class="service-port-card">
        <div class="service-port-info">
          <div class="service-port-title">
            <span>${s.name}</span>
            ${statusBadge}
          </div>
          <div style="margin: 4px 0;">
            ${listenerBadge}
          </div>
          <div class="service-port-meta">
            ${s.group ? `[${s.group}] · ` : ""}${occupantText}
          </div>
        </div>
        <div style="display: flex; flex-direction: column; gap: 6px; align-items: flex-end;">
          <div style="display: flex; gap: 4px;">
            <button class="btn btn-sm btn-sky" onclick="window.openServiceUrl(${s.port})" title="Open http://localhost:${s.port}">
              🌐 Open ↗
            </button>
            <button class="btn btn-sm btn-rose" onclick="window.killPortDirectly(${s.port})" title="Kill process occupying port :${s.port}">
              ⚡ Free Port
            </button>
          </div>
          <button class="btn btn-sm ${isRunning ? 'btn-secondary' : 'btn-emerald'}" style="width: 100%; font-size: 11px; padding: 3px 8px;" onclick="window.toggleServiceByName('${s.name}')">
            ${isRunning ? '⏹ Stop' : '▶ Start'}
          </button>
        </div>
      </div>
    `;
  });

  container.innerHTML = html;
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

// ==========================================
// In-App Keyboard Shortcuts Listener
// ==========================================

window.addEventListener("keydown", async (e) => {
  const activeEl = document.activeElement;
  if (
    activeEl &&
    (activeEl.tagName === "INPUT" ||
      activeEl.tagName === "TEXTAREA" ||
      activeEl.isContentEditable)
  ) {
    return;
  }

  // Construct shortcut key string, e.g. "Ctrl+Alt+1"
  const parts = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Win");

  const key = e.key.toUpperCase();
  if (["CONTROL", "ALT", "SHIFT", "META"].includes(key)) {
    return;
  }
  parts.push(key);
  const shortcutStr = parts.join("+");

  // 1. Check Mouse Jiggler shortcut (e.g. "Ctrl+Alt+J")
  try {
    const jigglerConfig = await invoke("get_jiggler_config");
    if (
      jigglerConfig &&
      jigglerConfig.shortcut &&
      jigglerConfig.shortcut.trim().toUpperCase() === shortcutStr.toUpperCase()
    ) {
      e.preventDefault();
      const stats = await invoke("get_jiggler_stats");
      if (stats.running) {
        await invoke("stop_jiggler");
        showToast("🐭 Mouse Jiggler Stopped [Ctrl+Alt+J]", "info");
      } else {
        await invoke("start_jiggler");
        showToast("🐭 Mouse Jiggler Started [Ctrl+Alt+J]", "success");
      }
      return;
    }
  } catch (_) {}

  // 2. Check Service shortcuts (e.g. "Ctrl+Alt+1" ... "Ctrl+Alt+6")
  try {
    const summary = await invoke("get_manager_summary");
    if (summary && summary.services) {
      const matched = summary.services.find(
        (s) =>
          s.shortcut &&
          s.shortcut.trim().toUpperCase() === shortcutStr.toUpperCase()
      );
      if (matched) {
        e.preventDefault();
        const isRunning = matched.status === "running";
        if (isRunning) {
          await invoke("stop_service", { name: matched.name });
          showToast(`⏹ Stopped '${matched.name}' [${shortcutStr}]`, "info");
        } else {
          await invoke("start_service", { name: matched.name });
          showToast(`▶ Started '${matched.name}' [${shortcutStr}]`, "success");
        }
        pollServices();
      }
    }
  } catch (_) {}
});

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
  if (!logs) return;

  const logPre = document.getElementById("logs-pre");
  if (logs !== lastLogText) {
    lastLogText = logs;
    logPre.textContent = logs;

    // Update size badge
    const byteSize = new Blob([logs]).size;
    const sizeBadge = document.getElementById("logs-size-badge");
    if (byteSize > 1024 * 1024) {
      sizeBadge.textContent = `(${(byteSize / (1024 * 1024)).toFixed(1)} MB)`;
    } else if (byteSize > 1024) {
      sizeBadge.textContent = `(${(byteSize / 1024).toFixed(1)} KB)`;
    } else {
      sizeBadge.textContent = `(${byteSize} B)`;
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
    el.style.color = "var(--accent-cyan)";
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
  div.textContent = text;
  return div.innerHTML;
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
// 7. MEDIA STUDIO (FFMPEG) CONTROLLER
// ========================================================

let currentMediaMode = "video_to_mp3";
let selectedMediaProbe = null;
let lastMediaOutputPath = "";
let isMediaJobRunning = false;

function setupMediaStudio() {
  // Mode selection buttons
  document.querySelectorAll(".media-mode-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".media-mode-btn").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      currentMediaMode = btn.dataset.mode;
      updateMediaModeUI();
    });
  });

  // Browse Media file button
  document.getElementById("btn-browse-media")?.addEventListener("click", async () => {
    let filter = "all";
    if (currentMediaMode === "video_to_mp3" || currentMediaMode === "convert_video") {
      filter = "video";
    } else if (currentMediaMode === "convert_audio") {
      filter = "audio";
    } else if (currentMediaMode === "convert_image") {
      filter = "image";
    }

    const path = await invoke("pick_media_file", { filterType: filter });
    if (path) {
      document.getElementById("input-media-path").value = path;
      inspectMediaFile(path);
    }
  });

  // Drag & drop support on dropzone
  const dropzone = document.getElementById("media-dropzone");
  if (dropzone) {
    dropzone.addEventListener("dragover", (e) => {
      e.preventDefault();
      dropzone.classList.add("dragover");
    });
    dropzone.addEventListener("dragleave", () => {
      dropzone.classList.remove("dragover");
    });
    dropzone.addEventListener("drop", (e) => {
      e.preventDefault();
      dropzone.classList.remove("dragover");
      if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length > 0) {
        const file = e.dataTransfer.files[0];
        const path = file.path || file.name;
        if (path) {
          document.getElementById("input-media-path").value = path;
          inspectMediaFile(path);
        }
      }
    });
  }

  // Path input change
  document.getElementById("input-media-path")?.addEventListener("change", (e) => {
    const p = e.target.value.trim();
    if (p) inspectMediaFile(p);
  });

  // Volume analysis button
  document.getElementById("btn-analyze-volume")?.addEventListener("click", async () => {
    const path = document.getElementById("input-media-path").value.trim();
    if (!path) return;
    const btn = document.getElementById("btn-analyze-volume");
    btn.disabled = true;
    btn.textContent = "⏳ Analyzing...";

    try {
      const vol = await invoke("detect_media_volume", { filePath: path });
      if (vol) {
        document.getElementById("volume-analysis-box").classList.remove("hidden");
        document.getElementById("lbl-vol-max").textContent = `${vol.max_volume_db.toFixed(1)} dB`;
        document.getElementById("lbl-vol-mean").textContent = `${vol.mean_volume_db.toFixed(1)} dB`;
        document.getElementById("lbl-vol-rec").textContent = `+${vol.recommended_boost_db.toFixed(1)} dB`;
        document.getElementById("lbl-vol-rec-factor").textContent = `${vol.recommended_boost_multiplier.toFixed(2)}x`;

        // Store recommended multiplier on button
        const applyBtn = document.getElementById("btn-apply-rec-boost");
        applyBtn.onclick = () => {
          document.querySelector('.media-mode-btn[data-mode="boost_volume"]')?.click();
          const targetPct = Math.round(vol.recommended_boost_multiplier * 100);
          const slider = document.getElementById("range-boost-mult");
          slider.value = Math.min(Math.max(targetPct, 100), 400);
          updateBoostLabel(slider.value);
        };
      }
    } catch (err) {
      alert("Volume analysis failed: " + err);
    } finally {
      btn.disabled = false;
      btn.textContent = "🔍 Analyze Peak Volume";
    }
  });

  // Volume slider & presets
  const boostSlider = document.getElementById("range-boost-mult");
  if (boostSlider) {
    boostSlider.addEventListener("input", (e) => {
      updateBoostLabel(e.target.value);
    });
  }

  document.querySelectorAll(".btn-preset").forEach((b) => {
    b.addEventListener("click", () => {
      document.querySelectorAll(".btn-preset").forEach((btn) => btn.classList.remove("active"));
      b.classList.add("active");
      const mult = b.dataset.mult;
      if (boostSlider) {
        boostSlider.value = mult;
        updateBoostLabel(mult);
      }
    });
  });

  // Image quality slider
  const imgQuality = document.getElementById("range-img-quality");
  if (imgQuality) {
    imgQuality.addEventListener("input", (e) => {
      document.getElementById("lbl-img-quality").textContent = `${e.target.value}%`;
    });
  }

  // Browse Output Directory
  document.getElementById("btn-browse-outdir")?.addEventListener("click", async () => {
    const dir = await invoke("pick_media_directory");
    if (dir) {
      document.getElementById("input-media-outdir").value = dir;
    }
  });

  // Start Conversion Button
  document.getElementById("btn-start-media-task")?.addEventListener("click", handleStartMediaTask);

  // Cancel Conversion Button
  document.getElementById("btn-cancel-media")?.addEventListener("click", async () => {
    await invoke("cancel_media_conversion");
  });

  // Open Result buttons
  document.getElementById("btn-open-result-file")?.addEventListener("click", async () => {
    if (lastMediaOutputPath) {
      await invoke("open_media_path", { path: lastMediaOutputPath });
    }
  });

  document.getElementById("btn-reveal-result-folder")?.addEventListener("click", async () => {
    if (lastMediaOutputPath) {
      await invoke("reveal_media_in_explorer", { filePath: lastMediaOutputPath });
    }
  });

  // Clear History
  document.getElementById("btn-clear-media-hist")?.addEventListener("click", async () => {
    if (confirm("Clear all media conversion history?")) {
      await invoke("clear_media_history");
      renderMediaHistory([]);
    }
  });
}

function updateBoostLabel(val) {
  const mult = parseFloat(val) / 100.0;
  const db = 20.0 * Math.log10(mult);
  const dbFormatted = db >= 0 ? `+${db.toFixed(1)} dB` : `${db.toFixed(1)} dB`;
  const lbl = document.getElementById("lbl-boost-val");
  if (lbl) {
    lbl.textContent = `${val}% (${dbFormatted})`;
  }
}

function updateMediaModeUI() {
  document.getElementById("subform-video-to-mp3")?.classList.add("hidden");
  document.getElementById("subform-boost-volume")?.classList.add("hidden");
  document.getElementById("subform-convert-video")?.classList.add("hidden");
  document.getElementById("subform-convert-audio")?.classList.add("hidden");
  document.getElementById("subform-convert-image")?.classList.add("hidden");

  const titleMap = {
    video_to_mp3: "🎵 Video to MP3 Extraction Settings",
    boost_volume: "🔊 Audio Volume Amplification Settings",
    convert_video: "📹 Video Transcoding Settings",
    convert_audio: "🎧 Audio Transcoding Settings",
    convert_image: "🖼️ Image Transcoding Settings",
  };

  const hintMap = {
    video_to_mp3: "Select any video file (.mp4, .mkv, .avi, .mov, .webm, etc.)",
    boost_volume: "Select any video or audio file to amplify its sound",
    convert_video: "Select any video file to transcode or convert to GIF",
    convert_audio: "Select any audio or video file to convert audio format",
    convert_image: "Select any image file (.png, .jpg, .webp, .bmp, .ico, etc.)",
  };

  document.getElementById("lbl-options-title").textContent = titleMap[currentMediaMode] || "⚙️ Conversion Settings";
  document.getElementById("lbl-file-source-hint").textContent = hintMap[currentMediaMode] || "Select source file";

  const activeSubform = document.getElementById(`subform-${currentMediaMode.replace(/_/g, "-")}`);
  if (activeSubform) {
    activeSubform.classList.remove("hidden");
  }
}

async function inspectMediaFile(filePath) {
  try {
    const probe = await invoke("probe_media_file", { filePath });
    if (!probe) return;
    selectedMediaProbe = probe;

    document.getElementById("probe-badge").classList.remove("hidden");
    document.getElementById("probe-file-name").textContent = probe.file_name;
    document.getElementById("probe-format").textContent = probe.format.split(",")[0].toUpperCase();
    document.getElementById("probe-size").textContent = `${probe.file_size_mb} MB`;
    document.getElementById("probe-duration").textContent = `⏱️ ${probe.duration_formatted}`;

    const streamsContainer = document.getElementById("probe-streams");
    streamsContainer.innerHTML = "";

    probe.streams.forEach((s) => {
      const chip = document.createElement("span");
      chip.className = "stream-chip";
      if (s.codec_type === "video") {
        chip.textContent = `🎬 ${s.codec_name.toUpperCase()} • ${s.resolution || "Unknown"} ${s.fps ? `@ ${s.fps}fps` : ""}`;
      } else if (s.codec_type === "audio") {
        chip.textContent = `🔊 ${s.codec_name.toUpperCase()} • ${s.channels ? `${s.channels}ch` : ""} ${s.sample_rate || ""} ${s.bitrate || ""}`;
      } else {
        chip.textContent = `📄 ${s.codec_type}: ${s.codec_name}`;
      }
      streamsContainer.appendChild(chip);
    });

    const volBtn = document.getElementById("btn-analyze-volume");
    if (probe.has_audio) {
      volBtn.classList.remove("hidden");
    } else {
      volBtn.classList.add("hidden");
    }

    document.getElementById("volume-analysis-box").classList.add("hidden");
  } catch (err) {
    console.error("Probe failed:", err);
  }
}

async function handleStartMediaTask() {
  const inputPath = document.getElementById("input-media-path").value.trim();
  if (!inputPath) {
    alert("Please select or enter a source media file.");
    return;
  }

  const outDir = document.getElementById("input-media-outdir").value.trim();
  const customName = document.getElementById("input-media-custom-name").value.trim();

  const config = {
    task_type: currentMediaMode,
    input_path: inputPath,
    output_dir: outDir || null,
    custom_filename: customName || null,
    target_format: "mp3",
    audio_bitrate: null,
    video_codec: null,
    resolution: null,
    preset: null,
    volume_multiplier: null,
    volume_db: null,
    use_loudnorm: false,
    image_quality: null,
    image_scale: null,
  };

  if (currentMediaMode === "video_to_mp3") {
    config.target_format = "mp3";
    config.audio_bitrate = document.getElementById("sel-v2mp3-bitrate").value;
  } else if (currentMediaMode === "boost_volume") {
    const boostType = document.querySelector('input[name="boost_type"]:checked')?.value || "multiplier";
    if (boostType === "loudnorm") {
      config.use_loudnorm = true;
    } else {
      const multVal = parseFloat(document.getElementById("range-boost-mult").value);
      config.volume_multiplier = multVal / 100.0;
    }
  } else if (currentMediaMode === "convert_video") {
    config.target_format = document.getElementById("sel-video-format").value;
    config.video_codec = document.getElementById("sel-video-codec").value;
    config.resolution = document.getElementById("sel-video-res").value;
    config.preset = document.getElementById("sel-video-preset").value;
  } else if (currentMediaMode === "convert_audio") {
    config.target_format = document.getElementById("sel-audio-format").value;
    config.audio_bitrate = document.getElementById("sel-audio-bitrate").value;
  } else if (currentMediaMode === "convert_image") {
    config.target_format = document.getElementById("sel-image-format").value;
    config.image_quality = parseInt(document.getElementById("range-img-quality").value, 10);
    config.image_scale = document.getElementById("sel-image-scale").value;
  }

  document.getElementById("hud-success-banner").classList.add("hidden");
  document.getElementById("hud-error-banner").classList.add("hidden");
  document.getElementById("btn-cancel-media").classList.remove("hidden");
  document.getElementById("lbl-hud-status").textContent = "STARTING...";
  document.getElementById("dot-hud-status").className = "indicator-dot active";

  try {
    const jobId = await invoke("start_media_conversion", { config });
    isMediaJobRunning = true;
  } catch (err) {
    alert("Conversion failed to start: " + err);
    document.getElementById("lbl-hud-status").textContent = "FAILED";
    document.getElementById("dot-hud-status").className = "indicator-dot";
  }
}

async function pollMediaProgress() {
  if (currentTab !== "tab-media" && !isMediaJobRunning) return;

  const prog = await invoke("get_media_progress");
  if (!prog) return;

  if (prog.status === "running") {
    isMediaJobRunning = true;
    document.getElementById("lbl-hud-status").textContent = "CONVERTING";
    document.getElementById("dot-hud-status").className = "indicator-dot active";
    document.getElementById("btn-cancel-media").classList.remove("hidden");

    document.getElementById("lbl-hud-percent").textContent = `${prog.progress_percent.toFixed(1)}%`;
    document.getElementById("hud-bar-fill").style.width = `${prog.progress_percent}%`;

    document.getElementById("lbl-hud-time").textContent = prog.current_time || "00:00:00";
    document.getElementById("lbl-hud-total").textContent = prog.total_duration || "00:00:00";
    document.getElementById("lbl-hud-speed").textContent = prog.speed || "-";

    const mins = Math.floor(prog.elapsed_sec / 60);
    const secs = prog.elapsed_sec % 60;
    document.getElementById("lbl-hud-elapsed").textContent = `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  } else if (prog.status === "completed") {
    if (isMediaJobRunning) {
      isMediaJobRunning = false;
      lastMediaOutputPath = prog.output_path;
      document.getElementById("lbl-hud-status").textContent = "COMPLETED";
      document.getElementById("dot-hud-status").className = "indicator-dot active";
      document.getElementById("btn-cancel-media").classList.add("hidden");

      document.getElementById("lbl-hud-percent").textContent = "100.0%";
      document.getElementById("hud-bar-fill").style.width = "100%";

      const banner = document.getElementById("hud-success-banner");
      banner.classList.remove("hidden");
      document.getElementById("lbl-hud-output-path").textContent = prog.output_path;

      loadMediaStudio();
    }
  } else if (prog.status === "failed") {
    if (isMediaJobRunning) {
      isMediaJobRunning = false;
      document.getElementById("lbl-hud-status").textContent = "ERROR";
      document.getElementById("dot-hud-status").className = "indicator-dot";
      document.getElementById("btn-cancel-media").classList.add("hidden");

      const errBanner = document.getElementById("hud-error-banner");
      errBanner.classList.remove("hidden");
      document.getElementById("lbl-hud-error-msg").textContent = prog.error_msg || "Unknown conversion error";
    }
  } else if (prog.status === "cancelled") {
    isMediaJobRunning = false;
    document.getElementById("lbl-hud-status").textContent = "CANCELLED";
    document.getElementById("dot-hud-status").className = "indicator-dot";
    document.getElementById("btn-cancel-media").classList.add("hidden");
  }
}

async function loadMediaStudio() {
  try {
    const info = await invoke("get_ffmpeg_info");
    if (info) {
      const lbl = document.getElementById("lbl-engine-status");
      const pill = document.getElementById("media-engine-pill");
      if (info.available) {
        lbl.textContent = info.is_embedded
          ? "Embedded FFmpeg Active (v9.0)"
          : "System FFmpeg Active";
        pill.title = `Path: ${info.path}\nGPU Acceleration: ${info.has_nvenc ? "NVENC/CUDA " : ""}${info.has_qsv ? "QSV " : ""}${info.has_amf ? "AMF" : ""}`;
      } else {
        lbl.textContent = "FFmpeg Engine Not Found";
        pill.classList.remove("pill-engine");
        pill.style.background = "rgba(244, 63, 94, 0.15)";
        pill.style.color = "#fda4af";
      }
    }

    const history = await invoke("get_media_history") || [];
    renderMediaHistory(history);
  } catch (err) {
    console.error("Failed to load media studio info:", err);
  }
}

function renderMediaHistory(list) {
  const tbody = document.getElementById("media-history-tbody");
  if (!tbody) return;
  tbody.innerHTML = "";

  if (list.length === 0) {
    tbody.innerHTML = `<tr><td colspan="5" class="empty-state">No media conversions yet.</td></tr>`;
    return;
  }

  const taskLabels = {
    video_to_mp3: "🎵 Video → MP3",
    boost_volume: "🔊 Audio Boost",
    convert_video: "📹 Video Transcode",
    convert_audio: "🎧 Audio Transcode",
    convert_image: "🖼️ Image Convert",
  };

  list.forEach((item) => {
    const tr = document.createElement("tr");
    const dateStr = item.completed_at ? new Date(item.completed_at).toLocaleTimeString() : "-";
    const taskName = taskLabels[item.task_type] || item.task_type;

    tr.innerHTML = `
      <td><span class="badge badge-group">${taskName}</span></td>
      <td>
        <strong style="color: var(--text-main); font-size: 12px;">${item.output_file}</strong>
        <div class="text-dim" style="font-size: 11px;">from ${item.input_file}</div>
      </td>
      <td><span class="badge">${item.output_size_mb.toFixed(1)} MB</span></td>
      <td><span class="text-dim">${dateStr}</span></td>
      <td style="text-align: right;">
        <button class="btn btn-xs btn-cyan" onclick="openMediaItemFile('${escapeJs(item.output_file)}')">▶ Open</button>
        <button class="btn btn-xs btn-secondary" onclick="revealMediaItemFolder('${escapeJs(item.output_file)}')">📂 Folder</button>
      </td>
    `;
    tbody.appendChild(tr);
  });
}

window.openMediaItemFile = async (fileName) => {
  await invoke("open_media_path", { path: fileName });
};

window.revealMediaItemFolder = async (fileName) => {
  await invoke("reveal_media_in_explorer", { filePath: fileName });
};

function escapeJs(str) {
  return (str || "").replace(/\\/g, "\\\\").replace(/'/g, "\\'");
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
      await invoke("open_media_path", { path: currentStorageDir });
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
    updateManifestItem("media", info.has_media, "media_history.json");
  }

  function renderManifestEmpty() {
    ["services", "tasks", "creds", "clip", "jiggler", "ytdl", "media"].forEach((key) => {
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
      loadMediaStudio();
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

