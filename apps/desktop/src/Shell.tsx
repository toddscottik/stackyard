import { useState, type ReactNode } from "react";

const sections = {
  projects: "Projects",
  create: "Create project",
  components: "Components",
  databases: "Databases",
  activity: "Activity",
  settings: "Settings",
};

export type Section = keyof typeof sections;

export function Shell({
  section, onSelect, engineId, children,
}: {
  section: Section;
  onSelect: (section: Section) => void;
  engineId: number | null;
  children: ReactNode;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  return (
    <div className="app">
      <nav id="section-nav" className={menuOpen ? "sidebar open" : "sidebar"} aria-label="Sections">
        <p className="brand">
          <svg className="mark" viewBox="0 0 18 14" aria-hidden="true">
            <rect x="0" y="0" width="18" height="2.5" rx="1" />
            <rect x="0" y="5.75" width="13" height="2.5" rx="1" />
            <rect x="0" y="11.5" width="8" height="2.5" rx="1" />
          </svg>
          Stackyard
        </p>
        <p className="scope">This machine</p>
        {(Object.keys(sections) as Section[]).map((id) => (
          <button
            key={id}
            type="button"
            className="nav-item"
            aria-current={section === id ? "page" : undefined}
            onClick={() => { onSelect(id); setMenuOpen(false); }}
          >
            {sections[id]}
          </button>
        ))}
        <button type="button" className="btn menu-close" onClick={() => setMenuOpen(false)}>Close</button>
        <p className="bottom">{engineId ? <>Engine <span className="mono">{engineId}</span></> : "Engine offline"}</p>
      </nav>
      <header className="topbar">
        <button type="button" className="btn menu-toggle" aria-expanded={menuOpen} aria-controls="section-nav" onClick={() => setMenuOpen(!menuOpen)}>
          Menu
        </button>
        <h1>{sections[section]}</h1>
      </header>
      <div className="sheet"><main id="main">{children}</main></div>
    </div>
  );
}
