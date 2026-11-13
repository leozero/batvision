import { useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { ArrowLeft, KeyRound } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { LogoMark } from "../components/Logo";
import { Button, ErrorText, Field, Input } from "../components/ui";
import { ApiError, api, call, thisDevice } from "../lib/api";

function useErrorMessage() {
  const { t } = useTranslation();
  return (e: unknown) => {
    if (e instanceof ApiError) {
      if (e.code === "too_many_attempts")
        return t("errors.tooManyAttempts", { seconds: e.retryAfter ?? 30 });
      const key = `errors.${e.code}`;
      return t(key, { defaultValue: e.message });
    }
    return t("errors.network");
  };
}

/** Full-screen backdrop with an animated glow, shared by the sign-in pages. */
export function AuthShell({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  return (
    <div className="relative flex min-h-dvh items-center justify-center overflow-hidden px-4 py-12">
      <motion.div
        aria-hidden
        className="absolute -top-1/3 left-1/2 -z-10 size-[60rem] -translate-x-1/2 rounded-full bg-accent/10 blur-3xl"
        animate={{ scale: [1, 1.08, 1], opacity: [0.6, 0.9, 0.6] }}
        transition={{ duration: 8, repeat: Number.POSITIVE_INFINITY, ease: "easeInOut" }}
      />
      <motion.div
        initial={{ opacity: 0, y: 24, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1] }}
        className="w-full max-w-sm"
      >
        <div className="mb-8 flex flex-col items-center text-center">
          <motion.div
            initial={{ rotate: -20, scale: 0.6, opacity: 0 }}
            animate={{ rotate: 0, scale: 1, opacity: 1 }}
            transition={{ type: "spring", stiffness: 200, damping: 14, delay: 0.1 }}
          >
            <LogoMark className="size-14 drop-shadow-[0_0_30px_rgba(244,183,64,0.45)]" />
          </motion.div>
          <h1 className="mt-5 text-2xl font-bold tracking-tight">{title}</h1>
          {subtitle ? <p className="mt-2 text-sm text-muted">{subtitle}</p> : null}
        </div>
        <div className="rounded-2xl border border-line bg-surface/80 p-6 shadow-2xl backdrop-blur-xl">
          {children}
        </div>
      </motion.div>
    </div>
  );
}

export function LoginPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const errorMessage = useErrorMessage();
  const { next } = useSearch({ strict: false }) as { next?: string };
  const [step, setStep] = useState<{ kind: "password" } | { kind: "totp"; challenge: string }>({
    kind: "password",
  });
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const done = async () => {
    // Signed-out data (including "me = null") must not survive the sign-in.
    qc.clear();
    await navigate({ to: next?.startsWith("/") && !next.startsWith("//") ? next : "/" });
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (step.kind === "password") {
        const res = await call(
          api.POST("/api/auth/login", { body: { username, password, device: thisDevice() } }),
        );
        if (res.status === "totp_required") setStep({ kind: "totp", challenge: res.challenge });
        else await done();
      } else {
        await call(api.POST("/api/auth/login/totp", { body: { challenge: step.challenge, code } }));
        await done();
      }
    } catch (err) {
      setError(errorMessage(err));
      if (err instanceof ApiError && err.code === "challenge_expired")
        setStep({ kind: "password" });
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthShell
      title={t("auth.welcome")}
      subtitle={step.kind === "totp" ? t("auth.totpPrompt") : t("auth.signInPrompt")}
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <AnimatePresence mode="wait" initial={false}>
          {step.kind === "password" ? (
            <motion.div
              key="password"
              initial={{ opacity: 0, x: -20 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -20 }}
              className="flex flex-col gap-4"
            >
              <Field label={t("auth.username")}>
                <Input
                  autoFocus
                  autoComplete="username"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  required
                />
              </Field>
              <Field label={t("auth.password")}>
                <Input
                  type="password"
                  autoComplete="current-password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                />
              </Field>
            </motion.div>
          ) : (
            <motion.div
              key="totp"
              initial={{ opacity: 0, x: 20 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 20 }}
            >
              <Field label={t("auth.code")} hint={t("auth.codeHint")}>
                <Input
                  autoFocus
                  inputMode="text"
                  autoComplete="one-time-code"
                  value={code}
                  onChange={(e) => setCode(e.target.value)}
                  className="text-center font-mono text-xl tracking-[0.4em]"
                  required
                />
              </Field>
            </motion.div>
          )}
        </AnimatePresence>
        <ErrorText>{error}</ErrorText>
        <Button type="submit" variant="primary" size="lg" loading={busy}>
          {step.kind === "password" ? t("auth.signIn") : t("auth.verify")}
        </Button>
        {step.kind === "totp" ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setStep({ kind: "password" })}
          >
            <ArrowLeft className="size-4" />
            {t("common.back")}
          </Button>
        ) : null}
      </form>
    </AuthShell>
  );
}

function NewAccountForm({
  submitLabel,
  extra,
  onSubmit,
}: {
  submitLabel: string;
  extra?: ReactNode;
  onSubmit: (username: string, password: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const errorMessage = useErrorMessage();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (password !== confirm) {
      setError(t("auth.passwordsDiffer"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onSubmit(username, password);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      {extra}
      <Field label={t("auth.username")} hint={t("auth.usernameHint")}>
        <Input
          autoComplete="username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          required
        />
      </Field>
      <Field label={t("auth.password")} hint={t("auth.passwordHint")}>
        <Input
          type="password"
          autoComplete="new-password"
          minLength={10}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
        />
      </Field>
      <Field label={t("auth.confirmPassword")}>
        <Input
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          required
        />
      </Field>
      <ErrorText>{error}</ErrorText>
      <Button type="submit" variant="primary" size="lg" loading={busy}>
        {submitLabel}
      </Button>
    </form>
  );
}

export function SetupPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [token, setToken] = useState("");
  return (
    <AuthShell title={t("setup.title")} subtitle={t("setup.subtitle")}>
      <NewAccountForm
        submitLabel={t("setup.submit")}
        extra={
          <Field label={t("setup.token")} hint={t("setup.tokenHint")}>
            <div className="relative">
              <KeyRound className="absolute left-3.5 top-1/2 size-4 -translate-y-1/2 text-faint" />
              <Input
                value={token}
                onChange={(e) => setToken(e.target.value)}
                className="pl-10 font-mono"
                required
              />
            </div>
          </Field>
        }
        onSubmit={async (username, password) => {
          await call(
            api.POST("/api/setup", {
              body: { setup_token: token, username, password, device: thisDevice() },
            }),
          );
          qc.clear();
          await navigate({ to: "/account", search: { tab: "security" } });
        }}
      />
    </AuthShell>
  );
}

export function InvitePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { token } = useParams({ strict: false }) as { token: string };
  return (
    <AuthShell title={t("invite.title")} subtitle={t("invite.subtitle")}>
      <NewAccountForm
        submitLabel={t("invite.submit")}
        onSubmit={async (username, password) => {
          await call(
            api.POST("/api/auth/invitations/accept", {
              body: { token, username, password, device: thisDevice() },
            }),
          );
          qc.clear();
          await navigate({ to: "/" });
        }}
      />
    </AuthShell>
  );
}
