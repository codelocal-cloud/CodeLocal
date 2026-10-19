import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import postcss from "postcss";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const css = (path) => postcss.parse(read(path));
function declarations(path, selector, property) {
  const values = [];
  css(path).walkRules(selector, (rule) => {
    rule.walkDecls(property, (decl) => values.push(decl.value));
  });
  return values;
}

// Source-level guards only; these do not replace browser layout checks.
function luminance(hex) {
  const channels = hex.match(/[a-f\d]{2}/gi).map((channel) => {
    const value = parseInt(channel, 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return channels.reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
}
function contrast(foreground, background) {
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}
const tokens = new Map();
css("./globals.css").walkDecls(/^--/, (decl) => tokens.set(decl.prop, decl.value));
function token(name) {
  const value = tokens.get(name);
  assert.ok(value, `Missing token ${name}`);
  return value.startsWith("var(") ? token(value.slice(4, -1)) : value;
}
for (const foreground of ["--text", "--text-secondary", "--text-tertiary"]) {
  for (const background of ["--canvas", "--surface", "--surface-raised"]) {
    assert.ok(contrast(token(foreground), token(background)) >= 4.5, `${foreground} on ${background}`);
  }
}
for (const background of ["--accent", "--accent-strong"]) {
  assert.ok(contrast("#102019", token(background)) >= 4.5, `Primary button on ${background}`);
}

assert.ok(declarations("./dashboard/neural-graph-stage.module.css", ".stage", "min-height").every((value) => value === "0"));
assert.ok(declarations("./dashboard/dashboard-controls.module.css", ".overviewState", "flex-wrap").includes("wrap"));
assert.ok(declarations("./dashboard/design/design.module.css", ".embedShell", "grid-template-rows").includes("auto minmax(0, 1fr)"));
assert.deepEqual(declarations("./dashboard/shots/shots.module.css", ".dropZone > button:hover:not(:disabled)", "background"), ["var(--accent-strong)"]);
assert.ok(!read("./dashboard/code-graph/page.tsx").includes("styles.brainContent"));
assert.ok(read("./dashboard/code-graph/code-graph-live.tsx").includes("styles.codeGraphStage"));
const connect = read("./dashboard/connect/page.tsx");
assert.ok(!connect.includes("<LiveOverview"));
assert.match(connect, /<details[^>]*\bopen\b/);
for (const stylesheet of ["./dashboard/blogs/blog-editor.module.css", "./dashboard/blogs/series-editor.module.css"]) {
  assert.ok(read(stylesheet).includes(":focus-within"));
}
const dashboardSidebar = read("./dashboard/dashboard-sidebar.tsx");
const dashboardChrome = "./dashboard/dashboard-chrome.module.css";
for (const href of [
  "/dashboard", "/chat", "/dashboard/workspaces", "/dashboard/connect",
  "/dashboard/plugins", "/dashboard/skills", "/dashboard/settings",
]) {
  assert.ok(dashboardSidebar.includes(`href: "${href}"`), `Missing quick navigation link: ${href}`);
}
assert.match(dashboardSidebar, /aria-current=\{active \? "page" : undefined\}/);
assert.match(dashboardSidebar, /<DashboardNav onNavigate=/);
assert.ok(declarations(dashboardChrome, ".rail", "display").includes("none"), "Mobile rail should be hidden");
assert.ok(declarations(dashboardChrome, ".sidebarPanel", "overflow-y").includes("auto"), "Sidebar menu must remain scrollable");
assert.ok(declarations(dashboardChrome, ".railLink", "min-height").includes("50px"), "Quick links need touch-sized targets");
assert.ok(read("./dashboard/plugins/plugins-hub.tsx").includes('t("Extend CodeLocal with reusable tools and external data.")'));
// Regression contract: a visual redesign must keep the original authenticated flows.
const plugins = read("./dashboard/plugins/plugins-hub.tsx");
for (const endpoint of [
  'fetch("/api/v1/plugins"',
  'fetch("/api/v1/account"',
  'fetch("/api/v1/workspaces"',
  '/install',
  '/oauth',
  '/connections',
]) {
  assert.ok(plugins.includes(endpoint), `Visual redesign removed Plugin API flow: ${endpoint}`);
}
for (const behavior of [
  'onClick={openDialog}',
  '<PluginConnectDialog',
  'install={install}',
  'uninstall={uninstall}',
  'connect={connect}',
  'disconnect={disconnect}',
  'data-ready={connected ? "true" : undefined}',
]) {
  assert.ok(plugins.includes(behavior), `Visual redesign changed Plugin interaction: ${behavior}`);
}
const nav = read("./dashboard/dashboard-nav.tsx");
for (const behavior of [
  'readyAccount?.isAdmin',
  'href="/dashboard/account"',
  'method="post" action="/logout"',
  'name="csrf" value={readyAccount.csrf}',
  'onClick={onNavigate}',
]) {
  assert.ok(nav.includes(behavior), `Dashboard interaction missing: ${behavior}`);
}
for (const behavior of [
  'event.key === "Escape"',
  'event.key !== "Tab"',
  'document.body.style.overflow = "hidden"',
  'desktop.addEventListener("change", handleResize)',
  'aria-modal={open || undefined}',
  'onClick={close}',
]) {
  assert.ok(dashboardSidebar.includes(behavior), `Mobile drawer behavior missing: ${behavior}`);
}
const pluginBrand = read("./dashboard/plugins/plugin-brand-icon.tsx");
assert.ok(pluginBrand.includes('key === "github"'), "Recognized icon names must match exactly");
assert.ok(!pluginBrand.includes('key.includes('), "Do not show trusted vendor marks on similarly named custom Plugins");
assert.ok(pluginBrand.includes("name.slice(0, 2).toUpperCase()"), "Unknown Plugins must retain their original monogram fallback");
const landing = read("./page.tsx");
assert.match(landing, /src="\/codelocal-icon.png"/);
assert.match(landing, /href="\/dashboard"/);
assert.match(landing, /href="\/security"/);
console.log("Interface source checks passed: contrast, graphs, navigation, Plugin CRUD, sessions, drawer keyboard and branded icons.");
