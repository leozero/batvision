import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import {
  Check,
  Copy,
  Cpu,
  Film,
  FolderPlus,
  Link2,
  RefreshCw,
  Trash2,
  Tv,
  UserPlus,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Badge,
  Button,
  ErrorText,
  Field,
  Input,
  Page,
  Select,
  Spinner,
  TabPanel,
  Tabs,
} from "../../components/ui";
import { ApiError, api, call, type Schemas } from "../../lib/api";
import {
  invalidateLibrary,
  invitationsQuery,
  librariesQuery,
  meQuery,
  scansQuery,
  systemQuery,
  usersQuery,
} from "../../lib/queries";
import { Card, errorText } from "../account/AccountPage";

export function AdminPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { data: me } = useQuery(meQuery);
  const { tab = "libraries" } = useSearch({ strict: false }) as { tab?: string };
  const tabs = [
    { value: "libraries", label: t("admin.libraries") },
    { value: "users", label: t("admin.users") },
    { value: "system", label: t("admin.system") },
  ];
  if (me && !me.totp_enabled) {
    return (
      <Page className="mx-auto max-w-3xl px-4 pt-8">
        <Card title={t("admin.title")} description={t("admin.needs2fa")}>
          <Button
            variant="primary"
            onClick={() => navigate({ to: "/account", search: { tab: "security" } })}
          >
            {t("account.enable2fa")}
          </Button>
        </Card>
      </Page>
    );
  }
  return (
    <Page className="mx-auto max-w-4xl px-4 pt-8 sm:px-6">
      <h1 className="mb-6 text-3xl font-bold tracking-tight">{t("admin.title")}</h1>
      <Tabs
        value={tab}
        onValueChange={(v) => navigate({ to: "/admin", search: { tab: v }, replace: true })}
        tabs={tabs}
      >
        <TabPanel value="libraries" className="space-y-6">
          <LibrariesCard />
        </TabPanel>
        <TabPanel value="users" className="space-y-6">
          <InvitationsCard />
          <UsersCard />
        </TabPanel>
        <TabPanel value="system" className="space-y-6">
          <SystemCard />
        </TabPanel>
      </Tabs>
    </Page>
  );
}

function ScanBadge({
  status,
  kind,
}: {
  status?: Schemas["ScanStatus"];
  kind: "movies" | "series";
}) {
  const { t } = useTranslation();
  if (!status) return null;
  if (status.running) {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-accent">
        <Spinner className="size-3" />
        {t("admin.scanning")}
      </span>
    );
  }
  if (status.error) return <span className="text-xs text-danger">{status.error}</span>;
  const r = status.report;
  if (!r) return null;
  return (
    <span className="text-xs text-faint">
      {kind === "movies"
        ? t("admin.scanReportMovies", { count: r.movies })
        : t("admin.scanReportSeries", { series: r.series, episodes: r.episodes })}
      {r.metadata_skipped ? ` · ${t("admin.noTmdb")}` : ""}
      {r.probe_errors ? ` · ${t("admin.probeErrors", { count: r.probe_errors })}` : ""}
    </span>
  );
}

function LibrariesCard() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data: libraries } = useQuery(librariesQuery);
  const { data: scans } = useQuery(scansQuery);
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [kind, setKind] = useState<"movies" | "series">("movies");
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["admin"] });
    invalidateLibrary(qc);
  };
  const create = useMutation({
    mutationFn: () => call(api.POST("/api/admin/libraries", { body: { name, kind, path } })),
    onSuccess: () => {
      setName("");
      setPath("");
      refresh();
    },
  });
  const scan = useMutation({
    mutationFn: (id: number) =>
      call(api.POST("/api/admin/libraries/{id}/scan", { params: { path: { id } } })),
    onSettled: refresh,
  });
  const remove = useMutation({
    mutationFn: (id: number) =>
      call(api.DELETE("/api/admin/libraries/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate();
  };
  return (
    <>
      <Card title={t("admin.libraries")} description={t("admin.librariesHint")}>
        <ul className="divide-y divide-line">
          <AnimatePresence initial={false}>
            {libraries?.map((lib) => {
              const Icon = lib.kind === "movies" ? Film : Tv;
              const status = scans?.find((s) => s.library_id === lib.id);
              return (
                <motion.li
                  key={lib.id}
                  layout
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  className="flex items-center gap-4 py-3"
                >
                  <span className="flex size-10 items-center justify-center rounded-xl bg-surface-2 text-accent">
                    <Icon className="size-5" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="font-medium">{lib.name}</p>
                    <p className="truncate font-mono text-xs text-faint">/media/{lib.path}</p>
                    <ScanBadge status={status} kind={lib.kind} />
                  </div>
                  <Button size="sm" onClick={() => scan.mutate(lib.id)} disabled={status?.running}>
                    <RefreshCw className={status?.running ? "size-4 animate-spin" : "size-4"} />
                    {t("admin.scan")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      confirm(t("admin.confirmRemoveLibrary")) && remove.mutate(lib.id)
                    }
                  >
                    <Trash2 className="size-4" />
                  </Button>
                </motion.li>
              );
            })}
          </AnimatePresence>
        </ul>
        {libraries?.length === 0 ? (
          <p className="text-sm text-muted">{t("admin.noLibraries")}</p>
        ) : null}
      </Card>
      <Card title={t("admin.addLibrary")} description={t("admin.addLibraryHint")}>
        <form onSubmit={submit} className="grid gap-4 sm:grid-cols-[1fr_1fr_auto]">
          <Field label={t("admin.libraryName")}>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("nav.movies")}
              required
            />
          </Field>
          <Field label={t("admin.libraryPath")}>
            <Input
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="Movies"
              className="font-mono"
              required
            />
          </Field>
          <Field label={t("admin.libraryKind")}>
            <Select value={kind} onChange={(e) => setKind(e.target.value as "movies" | "series")}>
              <option value="movies">{t("nav.movies")}</option>
              <option value="series">{t("nav.series")}</option>
            </Select>
          </Field>
          <div className="sm:col-span-3">
            <ErrorText>{create.error ? errorText(t, create.error) : null}</ErrorText>
            <Button type="submit" variant="primary" loading={create.isPending}>
              <FolderPlus className="size-4" />
              {t("admin.addLibrary")}
            </Button>
          </div>
        </form>
      </Card>
    </>
  );
}

function InvitationsCard() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const { data: invitations } = useQuery(invitationsQuery);
  const [link, setLink] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const create = useMutation({
    mutationFn: () => call(api.POST("/api/admin/invitations", { body: { role: "user" } })),
    onSuccess: (inv) => {
      setLink(`${window.location.origin}/invite/${inv.token}`);
      setCopied(false);
      void qc.invalidateQueries({ queryKey: invitationsQuery.queryKey });
    },
  });
  const remove = useMutation({
    mutationFn: (id: number) =>
      call(api.DELETE("/api/admin/invitations/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: invitationsQuery.queryKey }),
  });
  const pending = invitations?.filter((i) => !i.used_at && i.expires_at * 1000 > Date.now()) ?? [];
  return (
    <Card title={t("admin.invitations")} description={t("admin.invitationsHint")}>
      <Button variant="primary" onClick={() => create.mutate()} loading={create.isPending}>
        <Link2 className="size-4" />
        {t("admin.createInvitation")}
      </Button>
      <AnimatePresence>
        {link ? (
          <motion.div
            initial={{ opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            className="mt-4 flex gap-2"
          >
            <Input
              readOnly
              value={link}
              className="font-mono text-xs"
              onFocus={(e) => e.target.select()}
            />
            <Button
              onClick={() => {
                void navigator.clipboard.writeText(link);
                setCopied(true);
              }}
            >
              {copied ? <Check className="size-4 text-success" /> : <Copy className="size-4" />}
            </Button>
          </motion.div>
        ) : null}
      </AnimatePresence>
      {pending.length > 0 ? (
        <ul className="mt-5 divide-y divide-line text-sm">
          {pending.map((inv) => (
            <li key={inv.id} className="flex items-center justify-between py-2">
              <span className="text-muted">
                {t("admin.expires", {
                  date: new Date(inv.expires_at * 1000).toLocaleString(i18n.language, {
                    dateStyle: "medium",
                    timeStyle: "short",
                  }),
                })}
              </span>
              <Button size="sm" variant="ghost" onClick={() => remove.mutate(inv.id)}>
                <Trash2 className="size-4" />
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
    </Card>
  );
}

function UsersCard() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data: me } = useQuery(meQuery);
  const { data: users } = useQuery(usersQuery);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<"user" | "admin">("user");
  const create = useMutation({
    mutationFn: () => call(api.POST("/api/admin/users", { body: { username, password, role } })),
    onSuccess: () => {
      setUsername("");
      setPassword("");
      void qc.invalidateQueries({ queryKey: usersQuery.queryKey });
    },
  });
  const remove = useMutation({
    mutationFn: (id: number) =>
      call(api.DELETE("/api/admin/users/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: usersQuery.queryKey }),
  });
  return (
    <Card title={t("admin.users")}>
      <ul className="mb-6 divide-y divide-line">
        {users?.map((u) => (
          <li key={u.id} className="flex items-center gap-3 py-2.5">
            <span className="flex size-8 items-center justify-center rounded-full bg-surface-3 text-sm font-bold uppercase">
              {u.username.slice(0, 1)}
            </span>
            <span className="flex-1 font-medium">{u.username}</span>
            {u.role === "admin" ? <Badge tone="accent">{t("account.roleAdmin")}</Badge> : null}
            {u.totp_enabled ? <Badge tone="success">2FA</Badge> : null}
            {u.id !== me?.id ? (
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  confirm(t("admin.confirmRemoveUser", { name: u.username })) && remove.mutate(u.id)
                }
              >
                <Trash2 className="size-4" />
              </Button>
            ) : null}
          </li>
        ))}
      </ul>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate();
        }}
        className="grid gap-4 sm:grid-cols-[1fr_1fr_auto]"
      >
        <Field label={t("auth.username")}>
          <Input value={username} onChange={(e) => setUsername(e.target.value)} required />
        </Field>
        <Field label={t("auth.password")}>
          <Input
            type="password"
            autoComplete="new-password"
            minLength={10}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        <Field label={t("admin.role")}>
          <Select value={role} onChange={(e) => setRole(e.target.value as "user" | "admin")}>
            <option value="user">{t("account.roleUser")}</option>
            <option value="admin">{t("account.roleAdmin")}</option>
          </Select>
        </Field>
        <div className="sm:col-span-3">
          <ErrorText>
            {create.error instanceof ApiError ? errorText(t, create.error) : null}
          </ErrorText>
          <Button type="submit" loading={create.isPending}>
            <UserPlus className="size-4" />
            {t("admin.createUser")}
          </Button>
        </div>
      </form>
    </Card>
  );
}

function SystemCard() {
  const { t } = useTranslation();
  const { data } = useQuery(systemQuery);
  const encoders = data?.ffmpeg?.working_encoders ?? [];
  const hardware = encoders.filter((e) => e.includes("nvenc"));
  return (
    <Card title={t("admin.system")}>
      <dl className="grid gap-4 text-sm sm:grid-cols-[10rem_1fr]">
        <dt className="text-muted">{t("admin.version")}</dt>
        <dd className="font-mono">{data?.version}</dd>
        <dt className="text-muted">ffmpeg</dt>
        <dd className="font-mono text-xs">
          {data?.ffmpeg ? (data.ffmpeg.version ?? t("admin.ffmpegMissing")) : t("admin.detecting")}
        </dd>
        <dt className="text-muted">{t("admin.encoders")}</dt>
        <dd className="flex flex-wrap gap-1.5">
          {encoders.map((e) => (
            <Badge key={e} tone={e.includes("nvenc") ? "accent" : "neutral"}>
              {e}
            </Badge>
          ))}
        </dd>
        <dt className="text-muted">{t("admin.gpu")}</dt>
        <dd className="flex items-center gap-2">
          <Cpu className={hardware.length ? "size-4 text-success" : "size-4 text-faint"} />
          {hardware.length ? t("admin.gpuOk") : t("admin.gpuMissing")}
        </dd>
      </dl>
    </Card>
  );
}
