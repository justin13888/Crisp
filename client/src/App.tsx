import {
  BrowserRouter,
  Navigate,
  NavLink,
  Route,
  Routes,
} from "react-router-dom";
import { BottomNav, NAV_ITEMS } from "./components/BottomNav";
import { TimerDock } from "./components/TimerDock";
import { UpdatePrompt } from "./components/UpdatePrompt";
import { TimersProvider } from "./hooks/useTimers";
import { useTranslation } from "./i18n";
import { IdentityProvider } from "./lib/identity";
import { ServerProvider } from "./lib/servers";
import { Browse } from "./routes/Browse";
import { CookPage } from "./routes/cook/CookPage";
import { Editor } from "./routes/editor/Editor";
import { Import } from "./routes/Import";
import { Library } from "./routes/Library";
import { RecipePage } from "./routes/RecipePage";
import { Settings } from "./routes/Settings";

/** The application shell: navigation chrome and the route table. */
export function App() {
  const { t } = useTranslation();
  return (
    <BrowserRouter>
      <IdentityProvider>
        <ServerProvider>
          <TimersProvider>
            <div className="app">
              <header className="topbar">
                <NavLink to="/" className="brand">
                  {t("brand")}
                </NavLink>
                <nav aria-label={t("nav.primary")}>
                  {NAV_ITEMS.map((item) => (
                    <NavLink
                      key={item.to}
                      to={item.to}
                      end={item.to === "/"}
                      className={({ isActive }) =>
                        isActive ? "active" : undefined
                      }
                    >
                      {t(item.label)}
                    </NavLink>
                  ))}
                </nav>
              </header>
              <main>
                <Routes>
                  <Route path="/" element={<Library />} />
                  <Route path="/import" element={<Import />} />
                  <Route path="/recipe/:docId" element={<RecipePage />} />
                  <Route path="/cook/:docId" element={<CookPage />} />
                  <Route path="/new" element={<Editor />} />
                  <Route path="/edit/:docId" element={<Editor />} />
                  <Route path="/browse" element={<Browse />} />
                  <Route path="/settings" element={<Settings />} />
                  <Route
                    path="/identity"
                    element={<Navigate to="/settings" replace />}
                  />
                  <Route path="*" element={<p>{t("notFound")}</p>} />
                </Routes>
              </main>
              <div className="app-bottom">
                <UpdatePrompt />
                <TimerDock />
                <BottomNav />
              </div>
            </div>
          </TimersProvider>
        </ServerProvider>
      </IdentityProvider>
    </BrowserRouter>
  );
}
