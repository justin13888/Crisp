import type { ReactNode } from "react";
import { NavLink } from "react-router-dom";

/** A 24px stroke icon. */
function Icon({ children }: { children: ReactNode }) {
  return (
    <svg
      className="icon"
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

/** The primary destinations, shared by the top bar and the bottom nav. */
export const NAV_ITEMS: { to: string; label: string; icon: ReactNode }[] = [
  {
    to: "/",
    label: "Library",
    icon: <path d="M4 19V5a2 2 0 0 1 2-2h12v16H6a2 2 0 0 0-2 2Zm0 0h14" />,
  },
  {
    to: "/new",
    label: "New",
    icon: <path d="M12 5v14M5 12h14" />,
  },
  {
    to: "/browse",
    label: "Browse",
    icon: <path d="m21 21-4.3-4.3M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14Z" />,
  },
  {
    to: "/settings",
    label: "Settings",
    icon: (
      <path d="M20 21a8 8 0 0 0-16 0M12 13a5 5 0 1 0 0-10 5 5 0 0 0 0 10Z" />
    ),
  },
];

/** The thumb-reachable bottom navigation shown on narrow screens. */
export function BottomNav() {
  return (
    <nav className="bottomnav" aria-label="Primary">
      {NAV_ITEMS.map((item) => (
        <NavLink
          key={item.to}
          to={item.to}
          end={item.to === "/"}
          className={({ isActive }) => (isActive ? "active" : undefined)}
        >
          <Icon>{item.icon}</Icon>
          <span>{item.label}</span>
        </NavLink>
      ))}
    </nav>
  );
}
