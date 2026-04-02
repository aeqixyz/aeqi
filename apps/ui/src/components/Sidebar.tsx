import { NavLink } from "react-router-dom";

const NAV_SECTIONS = [
  {
    items: [
      { to: "/", label: "Chat", end: true },
      { to: "/inbox", label: "Inbox" },
    ],
  },
  {
    header: "Organization",
    items: [
      { to: "/agents", label: "Agents" },
      { to: "/departments", label: "Departments" },
    ],
  },
  {
    header: "Work",
    items: [
      { to: "/tasks", label: "Tasks" },
      { to: "/triggers", label: "Triggers" },
    ],
  },
  {
    header: "Intelligence",
    items: [
      { to: "/memory", label: "Memory" },
      { to: "/blackboard", label: "Blackboard" },
      { to: "/skills", label: "Skills" },
    ],
  },
  {
    header: "System",
    items: [
      { to: "/cost", label: "Cost" },
      { to: "/audit", label: "Audit" },
    ],
  },
];

export default function SecondaryNav() {
  return (
    <nav className="nav">
      {NAV_SECTIONS.map((section, i) => (
        <div key={i} className="nav-section">
          {section.header && (
            <div className="nav-section-header">{section.header}</div>
          )}
          {section.items.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={"end" in item ? (item as any).end : undefined}
              className={({ isActive }) =>
                `nav-item${isActive ? " active" : ""}`
              }
            >
              {item.label}
            </NavLink>
          ))}
        </div>
      ))}
    </nav>
  );
}
