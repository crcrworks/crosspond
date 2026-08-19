const HOST_NAME = "com.crosspond.chrome";
const GROUP_TITLE = "Crosspond";
const RETRY_MS = 2500;

let port = null;
let connecting = false;
let retryTimer = null;

function scheduleReconnect() {
  if (retryTimer) {
    return;
  }
  retryTimer = setTimeout(() => {
    retryTimer = null;
    connectNative();
  }, RETRY_MS);
}

function connectNative() {
  if (port || connecting) {
    return;
  }
  connecting = true;
  let next;
  try {
    next = chrome.runtime.connectNative(HOST_NAME);
  } catch {
    connecting = false;
    scheduleReconnect();
    return;
  }
  port = next;
  connecting = false;
  port.onMessage.addListener((message) => {
    void handleHostMessage(message);
  });
  port.onDisconnect.addListener(() => {
    // Must read lastError or Chrome shows "Unchecked runtime.lastError:
    // Specified native messaging host not found" when Crosspond has not
    // written the host manifest yet.
    const _ignored = chrome.runtime.lastError;
    port = null;
    scheduleReconnect();
  });
}

async function handleHostMessage(message) {
  const id = message && message.id;
  if (!port) {
    return;
  }
  try {
    const result = await dispatch(message || {});
    port.postMessage({ id, ok: true, result });
  } catch (error) {
    const text = error && error.message ? error.message : String(error);
    port.postMessage({ id, ok: false, error: text });
  }
}

async function dispatch(message) {
  const op = message.op;
  switch (op) {
    case "status":
      return { connected: true };
    case "list_tabs":
      return listTabs();
    case "attach":
      await attach(requiredTabId(message));
      return {};
    case "cdp":
      return sendCdp(requiredTabId(message), message.method, message.params || {});
    case "navigate":
      return navigate(requiredTabId(message), message.action, message.url);
    case "new_tab":
      return newTab(message.url);
    case "activate":
      await chrome.tabs.update(requiredTabId(message), { active: true });
      return {};
    default:
      throw new Error(`unknown op: ${op || "(missing)"}`);
  }
}

function requiredTabId(message) {
  const tabId = message.tabId;
  if (typeof tabId !== "number") {
    throw new Error("tabId is required");
  }
  return tabId;
}

async function listTabs() {
  const tabs = await chrome.tabs.query({});
  let attached = new Set();
  try {
    const targets = await chrome.debugger.getTargets();
    attached = new Set(
      targets.filter((target) => target.attached && typeof target.tabId === "number").map((target) => target.tabId)
    );
  } catch {
    attached = new Set();
  }
  return {
    tabs: tabs
      .filter((tab) => typeof tab.id === "number")
      .filter((tab) => !(tab.url || "").startsWith("chrome-extension://"))
      .map((tab) => ({
        id: tab.id,
        title: tab.title || "",
        url: tab.url || "",
        active: Boolean(tab.active),
        attached: attached.has(tab.id)
      }))
  };
}

async function attach(tabId) {
  try {
    await chrome.debugger.attach({ tabId }, "1.3");
  } catch (error) {
    const text = error && error.message ? error.message : String(error);
    if (!text.includes("already attached")) {
      throw error;
    }
  }
}

async function sendCdp(tabId, method, params) {
  if (!method) {
    throw new Error("CDP method is required");
  }
  await attach(tabId);
  return chrome.debugger.sendCommand({ tabId }, method, params || {});
}

function waitComplete(tabId, timeoutMs) {
  const timeout = timeoutMs || 15000;
  return new Promise((resolve) => {
    let settled = false;
    const finish = () => {
      if (settled) {
        return;
      }
      settled = true;
      chrome.tabs.onUpdated.removeListener(listener);
      resolve();
    };
    const timer = setTimeout(finish, timeout);
    function listener(id, info) {
      if (id === tabId && info.status === "complete") {
        clearTimeout(timer);
        finish();
      }
    }
    chrome.tabs.onUpdated.addListener(listener);
  });
}

async function navigate(tabId, action, url) {
  if (action === "goto") {
    if (!url) {
      throw new Error("url is required for goto");
    }
    await chrome.tabs.update(tabId, { url });
  } else if (action === "back") {
    await chrome.tabs.goBack(tabId);
  } else if (action === "forward") {
    await chrome.tabs.goForward(tabId);
  } else if (action === "reload") {
    await chrome.tabs.reload(tabId);
  } else {
    throw new Error("action must be goto, back, forward, or reload");
  }
  await waitComplete(tabId);
  const tab = await chrome.tabs.get(tabId);
  return { tabId, url: tab.url || "", title: tab.title || "" };
}

async function newTab(url) {
  const tab = await chrome.tabs.create({ url: url || "about:blank" });
  if (typeof tab.id === "number") {
    await groupTab(tab.id);
    await waitComplete(tab.id);
  }
  const fresh = typeof tab.id === "number" ? await chrome.tabs.get(tab.id) : tab;
  return {
    tabId: fresh.id,
    url: fresh.url || url || "about:blank",
    title: fresh.title || ""
  };
}

async function groupTab(tabId) {
  try {
    const groupId = await chrome.tabs.group({ tabIds: [tabId] });
    await chrome.tabGroups.update(groupId, { title: GROUP_TITLE, color: "cyan" });
  } catch {
    // Some chrome:// pages cannot join a group.
  }
}

connectNative();
chrome.runtime.onStartup.addListener(connectNative);
chrome.runtime.onInstalled.addListener(connectNative);
