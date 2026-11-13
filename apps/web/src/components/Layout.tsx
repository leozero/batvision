import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Download,
  LogOut,
  RefreshCw,
  Search,
  Settings,
  ShieldAlert,
  ShieldCheck,
  User as UserIcon,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import { desktop, useUpdateState } from "../lib/desktop";
import { meQuery } from "../lib/queries";
import { Logo } from "./Logo";
import { cx, IconButton, MenuContent, MenuItem, MenuRoot, MenuSeparator, MenuTrigger } from "./ui";

function NavLink({
  to,
  children,
}: {
  to: "/" | "/movies" | "/series" | "/watchlist" | "/downloads";
  children: string;
}) {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const active = to === "/" ? pathname === "/" : pathname.startsWith(to);
  const link = useRef<HTMLAnchorElement>(null);
  // Narrow screens scroll the links: the current one stays in view.
  useEffect(() => {
    if (active) link.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active]);
  return (
    <Link
      ref={link}
      to={to}
      className={cx(
        "relative px-1 py-2 text-sm font-medium transition-colors",
        active ? "text-fg" : "text-muted hover:text-fg",
      )}
    >
      {children}
      {active ? (
        <motion.span
          layoutId="nav-underline"
          className="absolute inset-x-0 -bottom-0.5 h-0.5 rounded-full bg-accent"
        />
      ) : null}
    </Link>
  );
}

export function AppLayout() {
  const { t } = useTranslation();
  const { data: me } = useQuery(meQuery);
  const navigate = useNavigate();
  const qc = useQueryClient();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 24);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  const logout = async () => {
    await api.POST("/api/auth/logout");
    qc.clear();
    await navigate({ to: "/login" });
  };

  const needs2fa = me?.role === "admin" && !me.totp_enabled;

  return (
    <div className="min-h-dvh">
      <header
        className={cx(
          "sticky top-0 z-40 flex h-16 items-center gap-3 px-4 transition-all duration-300 sm:gap-6 sm:px-10",
          scrolled
            ? "bg-bg/85 shadow-lg shadow-black/30 backdrop-blur-xl"
            : "bg-gradient-to-b from-black/70 to-transparent",
        )}
      >
        <Link to="/" className="shrink-0 transition-opacity hover:opacity-80">
          <Logo />
        </Link>
        {/* Narrow screens: the links scroll, search and the account stay in view. */}
        <nav className="no-scrollbar flex h-full min-w-0 items-center gap-5 overflow-x-auto">
          <NavLink to="/">{t("nav.home")}</NavLink>
          <NavLink to="/movies">{t("nav.movies")}</NavLink>
          <NavLink to="/series">{t("nav.series")}</NavLink>
          <NavLink to="/watchlist">{t("nav.watchlist")}</NavLink>
          <NavLink to="/downloads">{t("nav.downloads")}</NavLink>
        </nav>
        <div className="ml-auto flex shrink-0 items-center gap-1">
          <Link to="/search" aria-label={t("nav.search")}>
            <IconButton label={t("nav.search")} tabIndex={-1}>
              <Search className="size-5" />
            </IconButton>
          </Link>
          <MenuRoot>
            <MenuTrigger asChild>
              <button
                type="button"
                className="ml-1 flex size-9 cursor-pointer items-center justify-center rounded-full bg-gradient-to-br from-accent to-amber-600 text-sm font-bold uppercase text-accent-ink ring-2 ring-transparent transition hover:ring-accent/50"
                aria-label={t("nav.account")}
              >
                {me?.username.slice(0, 1)}
              </button>
            </MenuTrigger>
            <MenuContent>
              <div className="px-3 py-2">
                <p className="text-sm font-semibold">{me?.username}</p>
                <p className="text-xs text-faint">
                  {me?.role === "admin" ? t("account.roleAdmin") : t("account.roleUser")}
                </p>
              </div>
              <MenuSeparator />
              <MenuItem onSelect={() => navigate({ to: "/account" })}>
                <UserIcon className="size-4" />
                {t("nav.account")}
              </MenuItem>
              {me?.role === "admin" ? (
                <MenuItem onSelect={() => navigate({ to: "/admin" })}>
                  <Settings className="size-4" />
                  {t("nav.admin")}
                </MenuItem>
              ) : null}
              <MenuSeparator />
              <MenuItem danger onSelect={logout}>
                <LogOut className="size-4" />
                {t("nav.logout")}
              </MenuItem>
            </MenuContent>
          </MenuRoot>
        </div>
      </header>

      <AnimatePresence>
        {needs2fa && !pathname.startsWith("/account") ? (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            className="relative z-30 overflow-hidden"
          >
            <Link
              to="/account"
              search={{ tab: "security" }}
              className="flex items-center justify-center gap-2 bg-accent/15 px-4 py-2.5 text-sm text-accent transition hover:bg-accent/20"
            >
              <ShieldAlert className="size-4" />
              {t("account.enable2faBanner")}
            </Link>
          </motion.div>
        ) : null}
      </AnimatePresence>

      <UpdateBanner />

      <AnimatePresence mode="wait">
        <motion.div
          key={pathname.split("/")[1] ?? ""}
          exit={{ opacity: 0, transition: { duration: 0.15 } }}
        >
          <Outlet />
        </motion.div>
      </AnimatePresence>

      <footer className="mt-20 flex items-center justify-center gap-2 px-4 pb-10 text-xs text-faint">
        <ShieldCheck className="size-3.5" />
        BatVision
      </footer>
    </div>
  );
}

/** Desktop app only: a newer version is ready to install or to download. */
function UpdateBanner() {
  const { t } = useTranslation();
  const update = useUpdateState();
  const show = update?.status === "ready" || update?.status === "available";
  return (
    <AnimatePresence>
      {show ? (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          className="relative z-30 overflow-hidden"
        >
          <button
            type="button"
            onClick={() => void desktop?.installUpdate()}
            className="flex w-full cursor-pointer items-center justify-center gap-2 bg-accent/15 px-4 py-2.5 text-sm text-accent transition hover:bg-accent/20"
          >
            {update.status === "ready" ? (
              <RefreshCw className="size-4" />
            ) : (
              <Download className="size-4" />
            )}
            {update.status === "ready"
              ? t("desktop.updateReady", { version: update.version })
              : t("desktop.updateAvailable", { version: update.version })}
          </button>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
