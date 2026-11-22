import Link from "next/link";
import { getBackendStatus } from "@/lib/backend";

const milestones = [
  {
    number: "01",
    title: "Rust-native core",
    description: "Backend, CLI and local runtime rebuilt with explicit security boundaries.",
  },
  {
    number: "02",
    title: "MCP & Project Brain",
    description: "Protocol compatibility, agent context, workspaces, permissions and memory.",
  },
  {
    number: "03",
    title: "Next.js control plane",
    description: "A redesigned, responsive interface for every CodeLocal workflow.",
  },
];

export default async function Home() {
  const status = await getBackendStatus();

  return (
    <main className="site">
      <header className="nav shell">
        <Link className="brand" href="/" aria-label="CodeLocal home">
          <span className="brand-symbol">C</span>
          <span>CodeLocal <span className="brand-suffix">/ beta</span></span>
        </Link>
        <nav aria-label="Main navigation">
          <Link className="nav-link" href="#architecture">Architecture</Link>
          <Link className="nav-button" href="/dashboard">Open dashboard <span aria-hidden="true">↗</span></Link>
        </nav>
      </header>
      <section className="hero shell">
        <div className="hero-copy">
          <div className="eyebrow"><span className="pulse" /> The next chapter of CodeLocal</div>
          <h1>Your code.<br /><span>Your intelligence.</span><br />One place.</h1>
          <p>Reimagining the AI development control plane with a native Rust core and a completely new Next.js experience.</p>
          <div className="hero-actions">
            <Link href="/dashboard" className="primary-button">Explore beta foundation <span aria-hidden="true">↗</span></Link>
            <a href="#architecture" className="secondary-button">Explore architecture</a>
          </div>
          <div className="status-line">
            <span className={status ? "status-led online" : "status-led"} />
            {status
              ? `Rust API connected · ${status.state} · v${status.version}`
              : "Rust API offline · start with cargo run -p codelocal-backend"}
          </div>
        </div>
        <div className="architecture-art" aria-label="New CodeLocal architecture overview">
          <div className="art-header"><span>CODELOCAL</span><span>RUST / NEXT.JS</span></div>
          <div className="art-main">
            <div className="central-orbit">
              <div className="central-core"><span className="core-mark">C</span><span>CORE</span></div>
              <div className="orb orb-a" />
              <div className="orb orb-b" />
              <div className="orb orb-c" />
            </div>
            <div className="art-node left-node">CLIENT<br /><strong>AI AGENTS</strong></div>
            <div className="art-node right-node">RUNTIME<br /><strong>RUST</strong></div>
            <div className="art-node bottom-node">CONTROL PLANE<br /><strong>NEXT.JS</strong></div>
          </div>
          <div className="art-footer"><span>LOCAL-FIRST BY DESIGN</span><span>01 / 03</span></div>
        </div>
      </section>
      <section className="lower" id="architecture">
        <div className="shell">
          <div className="section-intro"><span className="section-number">01 — BLUEPRINT</span><h2>Rebuilt at the core.<br /><span>Designed for what comes next.</span></h2></div>
          <div className="feature-grid">
            {milestones.map((item) => (
              <article className="feature" key={item.number}>
                <span className="feature-number">{item.number} / FOUNDATION</span>
                <h3>{item.title}</h3>
                <p>{item.description}</p>
              </article>
            ))}
          </div>
          <p className="note">Beta foundation only. Production features and security-sensitive operations are not yet enabled.</p>
        </div>
      </section>
    </main>
  );
}
