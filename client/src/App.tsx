import { BrowserRouter, NavLink, Route, Routes } from "react-router-dom";
import { BottomNav } from "./components/BottomNav";
import { TimerDock } from "./components/TimerDock";
import { UpdatePrompt } from "./components/UpdatePrompt";
import { TimersProvider } from "./hooks/useTimers";
import { IdentityProvider } from "./lib/identity";
import { Browse } from "./routes/Browse";
import { Editor } from "./routes/editor/Editor";
import { IdentityPage } from "./routes/IdentityPage";
import { Import } from "./routes/Import";
import { Library } from "./routes/Library";
import { RecipePage } from "./routes/RecipePage";

/** A top-bar navigation link with an active-route style. */
function Nav({ to, label }: { to: string; label: string }) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) => (isActive ? "active" : undefined)}
      end
    >
      {label}
    </NavLink>
  );
}

/** The application shell: navigation chrome and the route table. */
export function App() {
  return (
    <BrowserRouter>
      <IdentityProvider>
        <TimersProvider>
          <div className="app">
            <header className="topbar">
              <NavLink to="/" className="brand">
                Crisp
              </NavLink>
              <nav aria-label="Primary">
                <Nav to="/" label="Library" />
                <Nav to="/import" label="Import" />
                <Nav to="/new" label="New recipe" />
                <Nav to="/browse" label="Browse" />
                <Nav to="/identity" label="Identity" />
              </nav>
            </header>
            <main>
              <Routes>
                <Route path="/" element={<Library />} />
                <Route path="/import" element={<Import />} />
                <Route path="/new" element={<Editor />} />
                <Route path="/browse" element={<Browse />} />
                <Route path="/identity" element={<IdentityPage />} />
                <Route path="/recipe/:docId" element={<RecipePage />} />
                <Route path="*" element={<p>Page not found.</p>} />
              </Routes>
            </main>
            <div className="app-bottom">
              <UpdatePrompt />
              <TimerDock />
              <BottomNav />
            </div>
          </div>
        </TimersProvider>
      </IdentityProvider>
    </BrowserRouter>
  );
}
