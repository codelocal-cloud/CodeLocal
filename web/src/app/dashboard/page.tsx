import Link from "next/link";
import { getBackendStatus } from "@/lib/backend";

const modules = [
  { title: "Project Brain", detail: "Context, rules and verified experience" },
  { title: "Workspaces", detail: "Authorized project and device routing" },
  { title: "MCP gateway", detail: "Client protocol and tool execution" },
  { title: "AI agents", detail: "Orchestration and tool workflows" },
  { title: "Security", detail: "OAuth, roles, sessions and approvals" },
  { title: "Plugins", detail: "Extensions and app connections" },
];

export default async function Dashboard() {
  const status = await getBackendStatus();
  return (
    <main className="dashboard">
      <aside className="sidebar">
        <Link href="/" className="brand"><span className="brand-symbol">C</span> CodeLocal</Link>
        <div className="sidebar-group">WORKSPACE</div>
        <span className="sidebar-active">Overview</span>
        <span className="sidebar-disabled">Project Brain</span>
        <span className="sidebar-disabled">Devices</span>
        <span className="sidebar-disabled">MCP</span>
        <span className="sidebar-disabled">Settings</span>
        <div className="sidebar-bottom">BETA · REBUILD</div>
      </aside>
      <div className="dashboard-content">
        <div className="dashboard-top"><span>Control plane / Overview</span><span className="beta-pill">BETA FOUNDATION</span></div>
        <h1>Welcome to the new CodeLocal.</h1>
        <p className="dashboard-lead">A clean foundation for the next-generation development platform. Production modules will appear here as migration gates pass.</p>
        <div className="dashboard-summary">
          <div><small>BACKEND SERVICE</small><strong>{status ? "Connected" : "Offline"}</strong><span>{status?.service ?? "No Rust API connection"}</span></div>
          <div><small>BACKEND VERSION</small><strong>{status ? status.version : "—"}</strong><span>Rust workspace</span></div>
          <div><small>ROLL-OUT STAGE</small><strong>Bootstrap</strong><span>Production features disabled</span></div>
        </div>
        <h2>Migration modules</h2>
        <div className="module-grid">{modules.map((module) => (
          <div className="module" key={module.title}><span className="module-icon">↗</span><h3>{module.title}</h3><p>{module.detail}</p><span className="module-status">Not yet migrated</span></div>
        ))}</div>
      </div>
    </main>
  );
}
