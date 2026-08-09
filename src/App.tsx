import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import tidyIcon from "./assets/tidy-icon.png";
import "./App.css";

type Page = "Dashboard" | "Scan" | "Review" | "History";

type HealthStatus = {
  app: "ok";
  database: {
    status: "ok";
    path: string;
    schema_version: number;
  };
};

const pages: { name: Page; label: string; icon: Page }[] = [
  { name: "Dashboard", label: "Overview", icon: "Dashboard" },
  { name: "Scan", label: "Drive scan", icon: "Scan" },
  { name: "Review", label: "Review queue", icon: "Review" },
  { name: "History", label: "History", icon: "History" },
];

function App() {
  const [activePage, setActivePage] = useState<Page>("Dashboard");
  const [health, setHealth] = useState<HealthStatus | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);

  useEffect(() => {
    invoke<HealthStatus>("health_check")
      .then(setHealth)
      .catch((error: unknown) => setHealthError(String(error)));
  }, []);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark"><img src={tidyIcon} alt="" /></span>
          <div>
            <strong>Tidy</strong>
            <span>your drive, calmly</span>
          </div>
        </div>

        <nav aria-label="Main navigation">
          <span className="nav-label">Workspace</span>
          {pages.map((page) => (
            <button
              className={`nav-item ${activePage === page.name ? "active" : ""}`}
              key={page.name}
              onClick={() => setActivePage(page.name)}
              type="button"
            >
              <NavIcon name={page.icon} />
              {page.label}
            </button>
          ))}
        </nav>

        <div className="sidebar-note">
          <span className="status-dot" />
          <div>
            <strong>Safety first</strong>
            <span>Nothing changes without your approval.</span>
          </div>
        </div>
      </aside>

      <main className="main-content">
        <header className="topbar">
          <span className="eyebrow">{activePage}</span>
          <span className="account-state">No account connected</span>
        </header>

        {activePage === "Dashboard" ? (
          <Dashboard health={health} healthError={healthError} />
        ) : (
          <section className="placeholder-panel">
            <span className="eyebrow">Workspace</span>
            <h1>{activePage}</h1>
            <p>This workspace is not available yet.</p>
          </section>
        )}
      </main>
    </div>
  );
}

function NavIcon({ name }: { name: Page }) {
  if (name === "Dashboard") {
    return (
      <svg className="nav-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none">
        <rect x="4" y="4" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.8" />
        <rect x="14" y="4" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.8" />
        <rect x="4" y="14" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.8" />
        <rect x="14" y="14" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.8" />
      </svg>
    );
  }

  if (name === "Scan") {
    return (
      <svg className="nav-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none">
        <circle cx="10.8" cy="10.8" r="5.8" stroke="currentColor" strokeWidth="1.8" />
        <path d="m15.2 15.2 4.5 4.5M4 5V3.5h1.5M19 5V3.5h-1.5M4 19v1.5h1.5M19 19v1.5h-1.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
      </svg>
    );
  }

  if (name === "Review") {
    return (
      <svg className="nav-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none">
        <circle cx="12" cy="12" r="8" stroke="currentColor" strokeWidth="1.8" />
        <path d="m8.5 12 2.3 2.3 4.8-5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  }

  return (
    <svg className="nav-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none">
      <path d="M12 6v6l4 2M20 12a8 8 0 1 1-2.3-5.7M20 5v4h-4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function Dashboard({
  health,
  healthError,
}: {
  health: HealthStatus | null;
  healthError: string | null;
}) {
  return (
    <div className="dashboard">
      <section className="hero">
        <div>
          <span className="eyebrow">A clearer place to start</span>
          <h1>Your Drive,<br /><em>tidied gently.</em></h1>
          <p>Connect your account to find duplicates, reclaim space, and make organization feel manageable.</p>
          <button className="primary-action" type="button" disabled>
            Connect Google Drive <span>→</span>
          </button>
        </div>
        <div className="hero-orbit" aria-hidden="true">
          <div className="orbit orbit-one" />
          <div className="orbit orbit-two" />
          <div className="orbit-core"><img src={tidyIcon} alt="" /></div>
          <span className="orbit-card card-one">duplicates</span>
          <span className="orbit-card card-two">unused files</span>
          <span className="orbit-card card-three">your approval</span>
        </div>
      </section>

      <section className="status-section">
        <div className="section-heading">
          <div>
            <span className="eyebrow">Foundation</span>
            <h2>Ready when you are</h2>
          </div>
          <span className={`health-pill ${health ? "ready" : healthError ? "error" : "checking"}`}>
            <span className="status-dot" />
            {health ? "Local systems ready" : healthError ? "Local systems unavailable" : "Checking local systems"}
          </span>
        </div>
        <div className="status-grid">
          <article className="status-card">
            <span className="card-index">01 / LOCAL</span>
            <h3>Private by design</h3>
            <p>Tidy starts with metadata only. Your files stay in Drive.</p>
          </article>
          <article className="status-card">
            <span className="card-index">02 / DATABASE</span>
            <h3>Local foundation</h3>
            <p>{health ? `SQLite schema v${health.database.schema_version} is initialized.` : healthError ?? "Initializing the local database."}</p>
          </article>
          <article className="status-card accent-card">
            <span className="card-index">03 / ACCOUNT</span>
            <h3>Connect your Drive</h3>
            <p>Connect an account to begin organizing your Drive.</p>
          </article>
        </div>
      </section>
    </div>
  );
}

export default App;
