import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { LogOut, Search, Settings, ShieldAlert, ShieldCheck, User as UserIcon } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import { meQuery } from "../lib/queries";
import { Logo } from "./Logo";
import { cx, IconButton, MenuContent, MenuItem, MenuRoot, MenuSeparator, MenuTrigger } from "./ui";

function NavLink({ to, children }: { to: "/" | "/movies" | "/series"; children: string }) {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const active = to === "/" ? pathname === "/" : pathname.startsWith(to);
  return (
    <Link
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
          "sticky top-0 z-40 flex h-16 items-center gap-6 px-4 transition-all duration-300 sm:px-10",
          scrolled
            ? "bg-bg/85 shadow-lg shadow-black/30 backdrop-blur-xl"
            : "bg-gradient-to-b from-black/70 to-transparent",
        )}
      >
        <Link to="/" className="shrink-0 transition-opacity hover:opacity-80">
          <Logo />
        </Link>
        <nav className="flex items-center gap-5">
          <NavLink to="/">{t("nav.home")}</NavLink>
          <NavLink to="/movies">{t("nav.movies")}</NavLink>
          <NavLink to="/series">{t("nav.series")}</NavLink>
        </nav>
        <div className="ml-auto flex items-center gap-1">
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
        batvision
      </footer>
    </div>
  );
}
