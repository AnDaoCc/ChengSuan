import React, { useEffect } from "react";
import { createRoot } from "react-dom/client";
import {
  BrowserRouter,
  Routes,
  Route,
  NavLink,
  useNavigate,
  useLocation,
} from "react-router-dom";
import {
  CheckCircle2,
  LoaderCircle,
  AlertCircle,
  X,
  LockKeyhole,
  ArrowUpRight,
} from "lucide-react";
import { StoreProvider, useApp } from "./store";
import { modules } from "./modules";
import { CostPage } from "../features/costs";
import { desktop } from "../infrastructure/desktop";
import { Button } from "../shared/ui/button";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useInvestment } from "../features/investment/hooks";
import "@fontsource-variable/noto-sans-sc";
import "../shared/styles.css";
import "../features/investment/styles.css";
function Shell() {
  const { loading, error, message, settings, saveState, app } = useApp();
  const navigate = useNavigate();
  const location = useLocation();
  const investment = useInvestment();
  useEffect(() => {
    void app.load();
    void investment.service.initialize();
    if (!desktop) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested(async (event) => {
        event.preventDefault();
        if (
          investment.service.hasDrafts() &&
          !confirm("有尚未保存的投资交易，确认放弃并退出？")
        )
          return;
        try {
          await app.flush();
          await investment.service.flush();
          await getCurrentWindow().destroy();
        } catch (e) {
          app.notify(e);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else cleanup = fn;
      });
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [app, investment.service]);
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () =>
      document.documentElement.classList.toggle(
        "dark",
        settings.theme === "dark" ||
          (settings.theme === "system" && media.matches),
      );
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings.theme]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        void app
          .flush()
          .then(() => investment.service.flush())
          .catch(app.notify);
      }
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [app, investment.service]);
  const toast = investment.message || message;
  const dismiss = () => {
    if (investment.message) investment.service.dismiss();
    else app.patch({ message: "" });
  };
  return (
    <div
      className="app-shell"
      onClickCapture={(event) => {
        const anchor = (event.target as HTMLElement).closest("a");
        if (
          !anchor ||
          anchor.target ||
          event.ctrlKey ||
          event.metaKey ||
          event.shiftKey ||
          event.button !== 0
        )
          return;
        const url = new URL(anchor.href);
        if (url.origin !== window.location.origin) return;
        event.preventDefault();
        event.stopPropagation();
        if (
          investment.service.hasDrafts() &&
          !confirm("投资交易尚未保存，确认放弃并离开？")
        )
          return;
        void app
          .flush()
          .then(() => investment.service.flush())
          .then(() => navigate(url.pathname + url.search))
          .catch(app.notify);
      }}
    >
      <aside className="sidebar">
        <div className="brand">
          <img src="/brand.png" alt="澄算软件图标" />
          <div>
            <strong>澄算</strong>
            <span>ChengSuan</span>
          </div>
        </div>
        <div className="nav-caption">工作空间</div>
        <nav>
          {modules.map(({ id, label, route, Icon }) => (
            <NavLink
              end={route === "/"}
              className={({ isActive }) =>
                isActive ||
                (id === "plans" && location.pathname.startsWith("/plans/"))
                  ? "active"
                  : ""
              }
              key={id}
              to={route}
            >
              <Icon size={19} />
              <span>{label}</span>
              {id === "plans" && (
                <ArrowUpRight size={14} className="nav-arrow" />
              )}
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="privacy">
            <LockKeyhole size={19} />
            <div>
              <strong>你的数据，留在本地</strong>
              <small>便携运行 · 无需账户</small>
            </div>
          </div>
          <footer>
            <span>ChengSuan</span>
            <span>v1.1.0</span>
          </footer>
        </div>
      </aside>
      <div className="main-shell">
        <div className="topbar">
          <span>
            澄算 /{" "}
            {location.pathname.startsWith("/investment")
              ? "投资工作空间"
              : "成本工作空间"}
          </span>
          <button
            className={`save-status ${saveState}`}
            onClick={() =>
              void app
                .flush()
                .then(() => investment.service.flush())
                .catch(app.notify)
            }
          >
            {saveState === "saved" && !investment.saving ? (
              <CheckCircle2 size={14} />
            ) : saveState === "saving" || investment.saving ? (
              <LoaderCircle size={14} className="spinning" />
            ) : (
              <AlertCircle size={14} />
            )}{" "}
            {saveState === "saved" && !investment.saving
              ? "已保存的记录均已落盘"
              : saveState === "saving" || investment.saving
                ? "正在保存…"
                : "保存失败，点击重试"}
          </button>
        </div>
        <main>
          {error && (
            <div className="warning">
              <AlertCircle size={17} />
              <span>{error}</span>
              {desktop && (
                <Button variant="secondary" onClick={() => void app.load()}>
                  重试
                </Button>
              )}
            </div>
          )}
          {loading ? (
            <div className="empty-state">
              <LoaderCircle className="spinning" size={30} />
              <p>正在打开本地数据…</p>
            </div>
          ) : (
            <Routes>
              {modules.map(({ id, route, Page }) => (
                <Route key={id} path={route} element={<Page />} />
              ))}
              <Route path="/plans/:id" element={<CostPage />} />
              <Route path="*" element={<div>页面不存在</div>} />
            </Routes>
          )}
        </main>
      </div>
      {toast && (
        <div role="status" className="toast">
          <AlertCircle size={18} />
          <span>{toast}</span>
          <button
            className="icon-button"
            aria-label="关闭提示"
            onClick={dismiss}
          >
            <X size={17} />
          </button>
        </div>
      )}
    </div>
  );
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <StoreProvider>
      <BrowserRouter>
        <Shell />
      </BrowserRouter>
    </StoreProvider>
  </React.StrictMode>,
);
