const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;

let isConnected = false;

window.addEventListener("DOMContentLoaded", () => {
  const toggleBtn = document.getElementById("toggle-btn");
  const ipInput = document.getElementById("server-ip");
  const statusText = document.getElementById("status-text");
  const logText = document.getElementById("log-text");

  // Load saved IP from Rust backend
  invoke("get_saved_ip").then((savedIp) => {
    if (savedIp) ipInput.value = savedIp;
  });

  // Listen for backend log events
  listen("vpn-log", (event) => {
    logText.textContent = event.payload;
  });

  toggleBtn.addEventListener("click", async () => {
    if (isConnected) {
      // Disconnect
      toggleBtn.disabled = true;
      statusText.textContent = "Disconnecting...";
      logText.textContent = "Cleaning up routes...";
      
      try {
        await invoke("disconnect_vpn");
        isConnected = false;
        toggleBtn.textContent = "Connect";
        toggleBtn.className = "btn-connect";
        statusText.textContent = "Disconnected";
        statusText.className = "status-disconnected";
        logText.textContent = "";
        ipInput.disabled = false;
      } catch (e) {
        logText.textContent = "Error disconnecting: " + e;
      } finally {
        toggleBtn.disabled = false;
      }
    } else {
      // Connect
      const ip = ipInput.value.trim() || "13.235.67.162";
      if (!ip) {
        logText.textContent = "Please enter a server IP!";
        return;
      }

      toggleBtn.disabled = true;
      statusText.textContent = "Connecting...";
      statusText.className = "status-connecting";
      ipInput.disabled = true;

      try {
        await invoke("connect_vpn", { ip });
        isConnected = true;
        toggleBtn.textContent = "Disconnect";
        toggleBtn.className = "btn-connect btn-disconnect";
        statusText.textContent = "Connected";
        statusText.className = "status-connected";
        logText.textContent = "Traffic is secured";
      } catch (e) {
        isConnected = false;
        statusText.textContent = "Disconnected";
        statusText.className = "status-disconnected";
        logText.textContent = "Error: " + e;
        ipInput.disabled = false;
      } finally {
        toggleBtn.disabled = false;
      }
    }
  });
});
